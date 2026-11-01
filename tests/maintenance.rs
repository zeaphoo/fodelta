mod common;

use std::process::Output;

use common::TestRepo;
use fodelta::repo::Repository;

fn run(repo: &TestRepo, args: &[&str]) -> Output {
    repo.command().args(args).output().expect("run fodelta")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn seed(repo: &TestRepo) {
    assert!(run(repo, &["init"]).status.success());
    repo.write("a.txt", b"a");
    assert!(run(repo, &["commit", "-m", "seed"]).status.success());
}

#[test]
fn gc_dry_run_reports_reachable_objects() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["gc", "--dry-run"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("reachable objects:"), "{text}");
    assert!(text.contains("would remove:"), "{text}");
}

#[test]
fn fsck_reports_a_healthy_repository() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["fsck"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("checked "), "{}", stdout(&output));
}

#[test]
fn fsck_full_detects_a_corrupt_object() {
    let repo = TestRepo::new();
    seed(&repo);
    let repository = Repository::open(repo.path()).unwrap();
    let oid = repository.store().write_blob(b"orphan").unwrap();
    let path = repository.store().path_for(oid);
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    std::fs::write(path, bytes).unwrap();

    let output = run(&repo, &["fsck", "--full"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("corrupt") || stderr(&output).contains("object"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn fsck_reports_dangling_objects_without_failing() {
    let repo = TestRepo::new();
    seed(&repo);
    let repository = Repository::open(repo.path()).unwrap();
    repository.store().write_blob(b"orphan").unwrap();

    let output = run(&repo, &["fsck"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("1 dangling"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn gc_keeps_safety_snapshots() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"dirty");
    let output = run(&repo, &["restore", "--", "a.txt"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let safety = stdout(&output)
        .lines()
        .find_map(|line| line.strip_prefix("safety snapshot "))
        .unwrap()
        .to_string();

    let output = run(&repo, &["gc"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let output = run(&repo, &["show", &safety]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
}

#[test]
fn gc_prunes_expired_safety_log_entries() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"dirty");
    assert!(run(&repo, &["restore", "--", "a.txt"]).status.success());

    let path = repo.path_of(".fodelta/logs/safety");
    let text = std::fs::read_to_string(&path).unwrap();
    let rewritten: String = text
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, ' ');
            let oid = parts.next()?;
            let _timestamp = parts.next()?;
            let rest = parts.next()?;
            let (_offset, reason) = rest.split_once('\t')?;
            Some(format!("{oid} 0 +0000\t{reason}\n"))
        })
        .collect();
    repo.write(".fodelta/logs/safety", rewritten);

    assert!(run(&repo, &["gc"]).status.success());
    let output = run(&repo, &["safety"]);
    assert!(
        stdout(&output).contains("no safety snapshots"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn full_fsck_passes_on_a_healthy_repository() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["fsck", "--full"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("checked "));
}
