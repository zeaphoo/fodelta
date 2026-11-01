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

#[test]
fn git_style_headers_for_added_and_deleted_files() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("gone.txt", b"gone\n");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    std::fs::remove_file(repo.path_of("gone.txt")).unwrap();
    repo.write("new.txt", b"new\n");

    let output = run(&repo, &["diff", "--", "gone.txt"]);
    let text = stdout(&output);
    assert!(text.contains("diff --git a/gone.txt b/gone.txt"), "{text}");
    assert!(text.contains("deleted file mode 100644"), "{text}");
    assert!(text.contains("--- a/gone.txt"), "{text}");
    assert!(text.contains("+++ /dev/null"), "{text}");
    assert!(text.contains("index "), "{text}");

    let output = run(&repo, &["diff", "--", "new.txt"]);
    let text = stdout(&output);
    assert!(text.contains("new file mode 100644"), "{text}");
    assert!(text.contains("--- /dev/null"), "{text}");
    assert!(text.contains("+++ b/new.txt"), "{text}");
}

#[test]
fn no_newline_at_end_of_file_marker() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"old");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    repo.write("a.txt", b"new");
    let output = run(&repo, &["diff"]);
    let text = stdout(&output);
    assert!(text.contains("\\ No newline at end of file"), "{text}");
}

#[test]
fn single_line_hunk_header_omits_count() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"old");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    repo.write("a.txt", b"new");
    let output = run(&repo, &["diff"]);
    assert!(stdout(&output).contains("@@ -1 +1 @@"));
}

#[test]
fn exact_renames_are_detected() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("old.txt", b"same\n");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    std::fs::rename(repo.path_of("old.txt"), repo.path_of("new.txt")).unwrap();

    let output = run(&repo, &["diff", "--name-status"]);
    assert_eq!(stdout(&output).trim(), "R100\told.txt\tnew.txt");

    let output = run(&repo, &["diff"]);
    let text = stdout(&output);
    assert!(text.contains("diff --git a/old.txt b/new.txt"), "{text}");
    assert!(text.contains("similarity index 100%"), "{text}");
    assert!(text.contains("rename from old.txt"), "{text}");
    assert!(text.contains("rename to new.txt"), "{text}");
    assert!(!text.contains("--- a/old.txt"), "{text}");

    let output = run(&repo, &["diff", "--stat"]);
    let text = stdout(&output);
    assert!(text.contains("old.txt => new.txt | 0"), "{text}");
    assert!(
        text.contains("1 file changed, 0 insertions(+), 0 deletions(-)"),
        "{text}"
    );
}

#[test]
fn whitespace_options_ignore_only_whitespace_changes() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"one two\n");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    repo.write("a.txt", b"one  two\n");

    let output = run(&repo, &["diff", "-w"]);
    assert!(output.status.success());
    assert!(stdout(&output).is_empty());

    let output = run(&repo, &["diff", "--stat", "-w"]);
    assert!(stdout(&output).is_empty());

    let output = run(&repo, &["diff", "-w", "--exit-code"]);
    assert_eq!(output.status.code(), Some(0));

    let output = run(&repo, &["diff", "--name-status", "-w"]);
    assert!(stdout(&output).contains("M\ta.txt"));
}

#[test]
fn histogram_algorithm_is_selectable() {
    let repo = TestRepo::new();
    seed(&repo);
    repo.write("a.txt", b"one\nTHREE\n");
    let output = run(&repo, &["diff", "--diff-algorithm", "histogram"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("+THREE"));
}

#[test]
fn rename_name_only_and_path_filter_use_new_path() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write(
        "old.txt", b"same
",
    );
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    std::fs::rename(repo.path_of("old.txt"), repo.path_of("new.txt")).unwrap();

    let output = run(&repo, &["diff", "--name-only"]);
    assert_eq!(stdout(&output).trim(), "new.txt");

    let output = run(&repo, &["diff", "--", "old.txt"]);
    assert!(stdout(&output).contains("rename from old.txt"));
}

#[test]
fn space_change_and_eol_options() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"one two\n");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    repo.write("a.txt", b"one   two\n");

    let output = run(&repo, &["diff", "-b"]);
    assert!(stdout(&output).is_empty());

    repo.write("a.txt", b"one two   \n");
    let output = run(&repo, &["diff", "--ignore-space-at-eol"]);
    assert!(stdout(&output).is_empty());
}

#[test]
fn histogram_can_be_selected_through_config() {
    let repo = TestRepo::new();
    seed(&repo);
    assert!(
        run(&repo, &["config", "diff.algorithm", "histogram"])
            .status
            .success()
    );
    repo.write("a.txt", b"one\nTHREE\n");
    let output = run(&repo, &["diff"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("+THREE"));
}

fn seed_renamable(repo: &TestRepo) {
    let mut contents = String::new();
    for index in 0..10 {
        contents.push_str(&format!("line {index}\n"));
    }
    repo.write("old.txt", contents.as_bytes());
    assert!(run(repo, &["commit", "-m", "seed"]).status.success());
    std::fs::rename(repo.path_of("old.txt"), repo.path_of("new.txt")).unwrap();
    let edited = contents.replace("line 5\n", "CHANGED\n");
    repo.write("new.txt", edited.as_bytes());
}

#[test]
fn similarity_renames_and_threshold_flags() {
    let repo = TestRepo::new();
    init(&repo);
    seed_renamable(&repo);

    let output = run(&repo, &["diff", "--name-status"]);
    assert_eq!(stdout(&output).trim(), "R90\told.txt\tnew.txt");

    let output = run(&repo, &["diff"]);
    let text = stdout(&output);
    assert!(text.contains("similarity index 90%"), "{text}");
    assert!(text.contains("rename from old.txt"), "{text}");
    assert!(text.contains("rename to new.txt"), "{text}");
    assert!(text.contains("-line 5"), "{text}");
    assert!(text.contains("+CHANGED"), "{text}");

    let output = run(&repo, &["diff", "--no-renames", "--name-status"]);
    let text = stdout(&output);
    assert!(text.contains("D\told.txt"), "{text}");
    assert!(text.contains("A\tnew.txt"), "{text}");

    let output = run(
        &repo,
        &["diff", "--rename-threshold", "95", "--name-status"],
    );
    let text = stdout(&output);
    assert!(text.contains("D\told.txt"), "{text}");
    assert!(text.contains("A\tnew.txt"), "{text}");

    let output = run(&repo, &["diff", "--diff-filter=R", "--name-status"]);
    assert_eq!(stdout(&output).trim(), "R90\told.txt\tnew.txt");
}

#[test]
fn diff_filter_rejects_unknown_letters() {
    let repo = TestRepo::new();
    seed(&repo);
    let output = run(&repo, &["diff", "--diff-filter=Z"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--diff-filter"));
}
