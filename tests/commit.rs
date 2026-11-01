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

#[test]
fn full_commit_cleans_the_tree() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a");
    repo.write("src/b.txt", b"b");

    let output = run(&repo, &["commit", "-m", "initial"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("[main "), "{text}");
    assert!(text.contains("2 file(s) changed"), "{text}");

    let status = run(&repo, &["status"]);
    assert!(stdout(&status).contains("working tree clean"));
}

#[test]
fn selective_commit_leaves_other_changes() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a");
    repo.write("b.txt", b"b");
    assert!(run(&repo, &["commit", "-m", "initial"]).status.success());

    repo.write("a.txt", b"a2");
    repo.write("b.txt", b"b2");
    let output = run(&repo, &["commit", "-m", "only a", "--", "a.txt"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("1 file(s) changed"), "{text}");
    assert!(text.contains("1 change(s) remain"), "{text}");

    let status = run(&repo, &["status"]);
    let text = stdout(&status);
    assert!(text.contains("modified: b.txt"), "{text}");
    assert!(!text.contains("a.txt"), "{text}");

    assert!(run(&repo, &["commit", "-m", "everything"]).status.success());
    let status = run(&repo, &["status"]);
    assert!(stdout(&status).contains("working tree clean"));
}

#[test]
fn ignored_file_cannot_be_committed_explicitly() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write(".gitignore", b"*.log\n");
    repo.write("build.log", b"x");
    let output = run(&repo, &["commit", "-m", "x", "--", "build.log"]);
    assert_eq!(output.status.code(), Some(2));
    let text = stderr(&output);
    assert!(text.contains("ignored"), "{text}");
    assert!(text.contains("!build.log"), "{text}");
}

#[test]
fn empty_commit_requires_the_flag() {
    let repo = TestRepo::new();
    init(&repo);
    let output = run(&repo, &["commit", "-m", "empty"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("nothing to commit"));

    let output = run(&repo, &["commit", "-m", "empty", "--allow-empty"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
}

#[test]
fn commit_paths_are_relative_to_dash_c_directory() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("sub/a.txt", b"a");
    repo.write("sub/b.txt", b"b");

    let output = repo
        .command()
        .args(["-C", "sub", "commit", "-m", "sub", "--", "a.txt"])
        .output()
        .expect("run fodelta");
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let status = run(&repo, &["status"]);
    let text = stdout(&status);
    assert!(text.contains("new file: sub/b.txt"), "{text}");
    assert!(!text.contains("sub/a.txt"), "{text}");
}

#[test]
fn empty_message_is_rejected() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a");
    let output = run(&repo, &["commit", "-m", "   "]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("must not be empty"));
}
