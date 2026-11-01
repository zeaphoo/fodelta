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
    repo.write("a.txt", b"one\ntwo\n");
    assert!(run(repo, &["commit", "-m", "first"]).status.success());
    repo.write("a.txt", b"one\nTWO\n");
    repo.write("b.txt", b"b\n");
    assert!(run(repo, &["commit", "-m", "second"]).status.success());
}

#[test]
fn default_diff_compares_worktree_with_head() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"one\nTHREE\n");
    std::fs::remove_file(repo.path_of("b.txt")).unwrap();
    repo.write("c.txt", b"c\n");

    let output = run(&repo, &["diff"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("--- a/a.txt"), "{text}");
    assert!(text.contains("-TWO"), "{text}");
    assert!(text.contains("+THREE"), "{text}");
    assert!(text.contains("deleted file mode 100644"), "{text}");
    assert!(text.contains("new file mode 100644"), "{text}");
}

#[test]
fn name_status_and_name_only() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"one\nTHREE\n");
    std::fs::remove_file(repo.path_of("b.txt")).unwrap();
    repo.write("c.txt", b"c\n");

    let output = run(&repo, &["diff", "--name-status"]);
    let text = stdout(&output);
    assert!(text.contains("M\ta.txt"), "{text}");
    assert!(text.contains("D\tb.txt"), "{text}");
    assert!(text.contains("A\tc.txt"), "{text}");

    let output = run(&repo, &["diff", "--name-only"]);
    let lines: Vec<&str> = stdout(&output)
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>()
        .leak()
        .iter()
        .map(|s| s.as_str())
        .collect();
    assert_eq!(lines, ["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn stat_summary_and_exit_code() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"one\nTHREE\n");
    repo.write("c.txt", b"c\n");

    let output = run(&repo, &["diff", "--stat"]);
    let text = stdout(&output);
    assert!(text.contains("2 files changed"), "{text}");
    assert!(text.contains("insertions(+)"), "{text}");

    let output = run(&repo, &["diff", "--exit-code"]);
    assert_eq!(output.status.code(), Some(1));

    let output = run(&repo, &["diff", "HEAD", "HEAD"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).is_empty());

    let output = run(&repo, &["diff", "HEAD", "HEAD", "--exit-code"]);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn compares_two_revisions() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["diff", "HEAD~1", "HEAD", "--name-status"]);
    let text = stdout(&output);
    assert!(text.contains("M\ta.txt"), "{text}");
    assert!(text.contains("A\tb.txt"), "{text}");
}

#[test]
fn compares_a_revision_with_the_worktree() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"one\nTHREE\n");
    let output = run(&repo, &["diff", "HEAD~1", "--", "a.txt"]);
    let text = stdout(&output);
    assert!(text.contains("-two"), "{text}");
    assert!(text.contains("+THREE"), "{text}");
}

#[test]
fn binary_files_are_not_dumped() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("logo.png", b"\x89PNG\0first");
    assert!(run(&repo, &["commit", "-m", "png"]).status.success());
    repo.write("logo.png", b"\x89PNG\0second");
    let output = run(&repo, &["diff"]);
    let text = stdout(&output);
    assert!(text.contains("Binary files"), "{text}");
    assert!(!text.contains("second"), "{text}");
}

#[test]
fn path_filter_limits_output() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"one\nTHREE\n");
    repo.write("b.txt", b"b2\n");
    let output = run(&repo, &["diff", "--", "a.txt"]);
    let text = stdout(&output);
    assert!(text.contains("a/a.txt"), "{text}");
    assert!(!text.contains("b/b.txt"), "{text}");
}

#[test]
fn zero_context_omits_unchanged_lines() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"one\nTHREE\n");
    let output = run(&repo, &["diff", "-U0"]);
    let text = stdout(&output);
    assert!(text.contains("-TWO"), "{text}");
    assert!(text.contains("+THREE"), "{text}");
    assert!(!text.lines().any(|line| line == " one"), "{text}");
}

#[cfg(unix)]
#[test]
fn type_change_is_reported_as_t() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("cfg", b"data\n");
    assert!(run(&repo, &["commit", "-m", "file"]).status.success());
    std::fs::remove_file(repo.path_of("cfg")).unwrap();
    std::os::unix::fs::symlink("target", repo.path_of("cfg")).unwrap();

    let output = run(&repo, &["diff", "--name-status"]);
    assert!(stdout(&output).contains("T\tcfg"), "{}", stdout(&output));

    let output = run(&repo, &["diff"]);
    let text = stdout(&output);
    assert!(text.contains("deleted file mode 100644"), "{text}");
    assert!(text.contains("new file mode 120000"), "{text}");
}
