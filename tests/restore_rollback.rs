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

#[cfg(unix)]
#[test]
fn restore_recreates_symlinks() {
    let repo = TestRepo::new();
    init(&repo);
    std::os::unix::fs::symlink("target", repo.path_of("link")).unwrap();
    assert!(run(&repo, &["commit", "-m", "link"]).status.success());
    std::fs::remove_file(repo.path_of("link")).unwrap();
    let output = run(&repo, &["restore", "--", "link"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let metadata = std::fs::symlink_metadata(repo.path_of("link")).unwrap();
    assert!(metadata.file_type().is_symlink());
    assert_eq!(
        std::fs::read_link(repo.path_of("link")).unwrap(),
        std::path::Path::new("target")
    );
}

#[test]
fn rollback_uses_a_custom_message() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["rollback", "HEAD~1", "-m", "back to v1"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("back to v1"));
    let log = stdout(&run(&repo, &["log", "--oneline", "-n", "1"]));
    assert!(log.contains("back to v1"), "{log}");
}

fn latest_safety(repo: &TestRepo) -> String {
    let output = run(repo, &["safety"]);
    stdout(&output)
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .expect("safety entry")
        .to_string()
}

#[test]
fn ignored_untracked_file_is_captured_by_rollback_safety() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a1");
    repo.write("secret.env", b"secret-v1");
    assert!(run(&repo, &["commit", "-m", "c1"]).status.success());

    std::fs::remove_file(repo.path_of("secret.env")).unwrap();
    repo.write(".gitignore", b"secret.env\n");
    assert!(run(&repo, &["commit", "-m", "c2"]).status.success());

    repo.write("secret.env", b"secret-v2");
    repo.write("a.txt", b"a2");
    assert!(run(&repo, &["rollback", "HEAD~1"]).status.success());

    let safety = latest_safety(&repo);
    let output = run(&repo, &["show", &format!("{safety}:secret.env")]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(output.stdout, b"secret-v2");

    assert!(
        run(&repo, &["restore", &safety, "--delete"])
            .status
            .success()
    );
    assert_eq!(repo.read("secret.env"), b"secret-v2");
}

#[test]
fn untracked_files_are_recoverable_after_rollback() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("new.txt", b"new");
    repo.write("a.txt", b"dirty");

    assert!(run(&repo, &["rollback", "HEAD~1"]).status.success());
    assert!(!repo.exists("new.txt"));

    let safety = latest_safety(&repo);
    assert!(
        run(&repo, &["restore", &safety, "--delete"])
            .status
            .success()
    );
    assert_eq!(repo.read("new.txt"), b"new");
    assert_eq!(repo.read("a.txt"), b"dirty");
}

#[test]
fn rollback_refuses_to_replace_a_directory_with_a_file() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("cfg", b"file");
    assert!(run(&repo, &["commit", "-m", "file"]).status.success());

    std::fs::remove_file(repo.path_of("cfg")).unwrap();
    repo.write("cfg/child", b"child");
    let output = run(&repo, &["commit", "-m", "dir"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let output = run(&repo, &["rollback", "HEAD~1"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("cannot replace directory"),
        "{}",
        stderr(&output)
    );
    assert_eq!(repo.read("cfg/child"), b"child");
}

#[test]
fn restore_captures_ignored_collision_in_safety() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a1");
    repo.write("secret.env", b"secret-v1");
    assert!(run(&repo, &["commit", "-m", "c1"]).status.success());

    std::fs::remove_file(repo.path_of("secret.env")).unwrap();
    repo.write(".gitignore", b"secret.env\n");
    assert!(run(&repo, &["commit", "-m", "c2"]).status.success());

    repo.write("secret.env", b"secret-v2");
    let output = run(&repo, &["restore", "HEAD~1", "--", "secret.env"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    let safety = text
        .lines()
        .find_map(|line| line.strip_prefix("safety snapshot "))
        .expect("safety id")
        .to_string();
    let output = run(&repo, &["show", &format!("{safety}:secret.env")]);
    assert_eq!(output.stdout, b"secret-v2");
    assert_eq!(repo.read("secret.env"), b"secret-v1");
}

#[test]
fn rollback_then_commit_keeps_linear_history() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"v1");
    assert!(run(&repo, &["commit", "-m", "c1"]).status.success());
    repo.write("a.txt", b"v2");
    assert!(run(&repo, &["commit", "-m", "c2"]).status.success());
    repo.write("a.txt", b"v3");
    repo.write("b.txt", b"b3");
    assert!(run(&repo, &["commit", "-m", "c3"]).status.success());

    let output = run(&repo, &["rollback", "HEAD~2"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("previous tip is HEAD~1"));

    repo.write("a.txt", b"v1-edited");
    repo.write("c.txt", b"new");
    assert!(
        run(&repo, &["commit", "-m", "after rollback"])
            .status
            .success()
    );

    let log = stdout(&run(&repo, &["log", "--oneline"]));
    let lines: Vec<&str> = log.lines().collect();
    assert_eq!(lines.len(), 5, "{log}");
    assert!(lines[0].contains("after rollback"), "{log}");
    assert!(lines[1].contains("rollback to"), "{log}");
    assert!(lines[2].contains("c3"), "{log}");
    assert!(lines[3].contains("c2"), "{log}");
    assert!(lines[4].contains("c1"), "{log}");

    assert!(stdout(&run(&repo, &["show", "HEAD~2"])).contains("c3"));
    assert!(stdout(&run(&repo, &["show", "HEAD~4"])).contains("c1"));

    let diff = stdout(&run(&repo, &["diff", "HEAD~4", "HEAD"]));
    assert!(diff.contains("+v1-edited"), "{diff}");
    assert!(diff.contains("+new"), "{diff}");
    assert!(!diff.contains("b/b.txt"), "{diff}");
}
