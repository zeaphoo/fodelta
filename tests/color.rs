mod common;

use std::process::Output;

use common::TestRepo;

fn run(repo: &TestRepo, args: &[&str]) -> Output {
    repo.command().args(args).output().expect("run fodelta")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn init(repo: &TestRepo) {
    assert!(run(repo, &["init"]).status.success());
}

#[test]
fn color_always_emits_ansi_escapes() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a");
    let output = run(&repo, &["status", "--color", "always"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("\u{1b}["), "{:?}", stdout(&output));
}

#[test]
fn color_never_is_plain_even_with_no_color_unset() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a");
    let output = run(&repo, &["status", "--color", "never"]);
    assert!(output.status.success());
    assert!(!stdout(&output).contains("\u{1b}["));
}

#[test]
fn diff_colors_patch_lines() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"old\n");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    repo.write("a.txt", b"new\n");
    let output = run(&repo, &["diff", "--color", "always"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("\u{1b}[32m"), "{text:?}");
    assert!(text.contains("\u{1b}[31m"), "{text:?}");
}

#[test]
fn color_auto_is_plain_when_not_a_terminal() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a");
    let output = run(&repo, &["status", "--color", "auto"]);
    assert!(output.status.success());
    assert!(!stdout(&output).contains("\u{1b}["));
}
