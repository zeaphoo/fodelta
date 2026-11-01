mod common;

use std::process::Output;

use common::TestRepo;

fn run(repo: &TestRepo, args: &[&str]) -> Output {
    repo.command().args(args).output().expect("run fodelta")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn init(repo: &TestRepo) {
    let output = run(repo, &["init"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
}

fn seed(repo: &TestRepo) {
    init(repo);
    repo.write("a.txt", b"a1");
    assert!(run(repo, &["commit", "-m", "first"]).status.success());
    repo.write("a.txt", b"a2");
    repo.write("b.txt", b"b");
    assert!(run(repo, &["commit", "-m", "second"]).status.success());
}

#[test]
fn restore_worktree_from_head() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"dirty");
    let output = run(&repo, &["restore", "--", "a.txt"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("restored 1 file(s)"));
    assert_eq!(repo.read("a.txt"), b"a2");
    assert!(stdout(&run(&repo, &["status"])).contains("working tree clean"));
}

#[test]
fn restore_from_older_revision() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["restore", "HEAD~1", "--", "a.txt"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(repo.read("a.txt"), b"a1");
    assert!(stdout(&run(&repo, &["status"])).contains("modified: a.txt"));
}

#[test]
fn restore_delete_removes_extra_files() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["restore", "HEAD~1", "--delete"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(!repo.exists("b.txt"));
    let text = stdout(&run(&repo, &["status"]));
    assert!(text.contains("modified: a.txt"), "{text}");
    assert!(text.contains("deleted:  b.txt"), "{text}");
}

#[test]
fn restore_rejects_path_traversal() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["restore", "HEAD", "--", "../outside.txt"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("escapes"), "{}", stderr(&output));
}

#[test]
fn restore_creates_a_safety_snapshot() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"dirty");
    let output = run(&repo, &["restore", "--", "a.txt"]);
    let text = stdout(&output);
    assert!(text.contains("safety snapshot"), "{text}");
    let safety = text
        .lines()
        .find_map(|line| line.strip_prefix("safety snapshot "))
        .unwrap()
        .to_string();

    let list = run(&repo, &["safety"]);
    assert!(stdout(&list).contains(&safety), "{}", stdout(&list));

    // The safety snapshot can be restored, bringing the dirty content back.
    let output = run(&repo, &["restore", &safety, "--delete"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(repo.read("a.txt"), b"dirty");
}

#[test]
fn rollback_appends_a_commit() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["rollback", "HEAD~1"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(repo.read("a.txt"), b"a1");
    assert!(!repo.exists("b.txt"));
    assert!(stdout(&run(&repo, &["status"])).contains("working tree clean"));

    let log = stdout(&run(&repo, &["log", "--oneline"]));
    assert_eq!(log.lines().count(), 3, "{log}");
    assert!(log.lines().next().unwrap().contains("rollback to"));
}

#[test]
fn rollback_to_head_is_a_noop_when_clean() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["rollback", "HEAD"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("nothing to rollback"));
    let log = stdout(&run(&repo, &["log", "--oneline"]));
    assert_eq!(log.lines().count(), 2);
}

#[test]
fn rollback_to_head_synchronizes_a_dirty_tree() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"dirty");
    let output = run(&repo, &["rollback", "HEAD"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(repo.read("a.txt"), b"a2");
    let log = stdout(&run(&repo, &["log", "--oneline"]));
    assert_eq!(log.lines().count(), 2);
    assert!(stdout(&run(&repo, &["status"])).contains("working tree clean"));
}

#[test]
fn rollback_reports_safety_snapshot() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"dirty");
    let output = run(&repo, &["rollback", "HEAD~1"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("safety snapshot"));
    assert_eq!(repo.read("a.txt"), b"a1");
}

#[test]
fn no_safety_flag_warns() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"dirty");
    let output = run(&repo, &["rollback", "HEAD~1", "--no-safety"]);
    assert!(output.status.success());
    assert!(stderr(&output).contains("no safety snapshot"));
}
