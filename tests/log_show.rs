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

fn seed_history(repo: &TestRepo) {
    init(repo);
    repo.write("a.txt", b"a1");
    assert!(run(repo, &["commit", "-m", "first"]).status.success());
    repo.write("a.txt", b"a2");
    repo.write("b.txt", b"b");
    assert!(run(repo, &["commit", "-m", "second"]).status.success());
}

#[test]
fn log_lists_newest_first_and_honors_limit() {
    let repo = TestRepo::new();
    seed_history(&repo);

    let output = run(&repo, &["log", "--oneline"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("second"), "{lines:?}");
    assert!(lines[1].contains("first"), "{lines:?}");

    let output = run(&repo, &["log", "--oneline", "-n", "1"]);
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("second"));
}

#[test]
fn log_filters_by_path() {
    let repo = TestRepo::new();
    seed_history(&repo);

    let output = run(&repo, &["log", "--oneline", "--", "b.txt"]);
    let text = stdout(&output);
    assert!(text.contains("second"), "{text}");
    assert!(!text.contains("first"), "{text}");
}

#[test]
fn show_prints_metadata_and_changes() {
    let repo = TestRepo::new();
    seed_history(&repo);

    let output = run(&repo, &["show", "HEAD"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("commit "), "{text}");
    assert!(text.contains("Author: "), "{text}");
    assert!(text.contains("second"), "{text}");
    assert!(text.contains("--- a/a.txt"), "{text}");
    assert!(text.contains("-a1"), "{text}");
    assert!(text.contains("+a2"), "{text}");
    assert!(text.contains("new file mode 100644"), "{text}");
    assert!(text.contains("+b"), "{text}");
}

#[test]
fn show_outputs_blob_bytes() {
    let repo = TestRepo::new();
    seed_history(&repo);

    let output = run(&repo, &["show", "HEAD:a.txt"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(output.stdout, b"a2");

    let output = run(&repo, &["show", "HEAD~1:a.txt"]);
    assert_eq!(output.stdout, b"a1");
}

#[test]
fn show_missing_path_is_an_error() {
    let repo = TestRepo::new();
    seed_history(&repo);
    let output = run(&repo, &["show", "HEAD:missing.txt"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("does not exist"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn log_on_empty_repository_is_an_error() {
    let repo = TestRepo::new();
    init(&repo);
    let output = run(&repo, &["log"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("no commits"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn walking_past_the_root_is_an_error() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a");
    assert!(run(&repo, &["commit", "-m", "first"]).status.success());
    let output = run(&repo, &["show", "HEAD~2"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("past the root"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn short_hash_resolves_to_a_commit() {
    let repo = TestRepo::new();
    seed_history(&repo);
    let output = run(&repo, &["log", "--oneline"]);
    let text = stdout(&output);
    let short = text
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();
    let output = run(&repo, &["show", short]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("second"));
}

#[test]
fn log_stat_reports_line_counts() {
    let repo = TestRepo::new();
    seed_history(&repo);
    let output = run(&repo, &["log", "-n", "1", "--stat"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("M a.txt | 1 +, 1 -"), "{text}");
    assert!(text.contains("A b.txt | 1 +, 0 -"), "{text}");
    assert!(
        text.contains("2 files changed, 2 insertions(+), 1 deletion(-)"),
        "{text}"
    );
}

#[test]
fn log_reverse_shows_oldest_first() {
    let repo = TestRepo::new();
    seed_history(&repo);
    let output = run(&repo, &["log", "--oneline", "--reverse"]);
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].contains("first"), "{text}");
    assert!(lines[1].contains("second"), "{text}");
}

#[test]
fn show_root_commit_is_all_added() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a\n");
    assert!(run(&repo, &["commit", "-m", "first"]).status.success());
    let output = run(&repo, &["show", "HEAD"]);
    let text = stdout(&output);
    assert!(text.contains("new file mode 100644"), "{text}");
    assert!(text.contains("+a"), "{text}");
}

#[test]
fn show_and_log_stat_report_renames() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("old.txt", b"same\n");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    std::fs::rename(repo.path_of("old.txt"), repo.path_of("new.txt")).unwrap();
    assert!(run(&repo, &["commit", "-m", "rename"]).status.success());

    let output = run(&repo, &["show", "HEAD"]);
    assert!(stdout(&output).contains("rename from old.txt"));
    assert!(stdout(&output).contains("rename to new.txt"));

    let output = run(&repo, &["log", "-n", "1", "--stat"]);
    assert!(stdout(&output).contains("old.txt => new.txt | 0"));
}

#[test]
fn show_similarity_rename_with_edits() {
    let repo = TestRepo::new();
    init(&repo);
    let mut contents = String::new();
    for index in 0..10 {
        contents.push_str(&format!("line {index}\n"));
    }
    repo.write("old.txt", contents.as_bytes());
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    std::fs::rename(repo.path_of("old.txt"), repo.path_of("new.txt")).unwrap();
    repo.write(
        "new.txt",
        contents.replace("line 5\n", "CHANGED\n").as_bytes(),
    );
    assert!(run(&repo, &["commit", "-m", "rename"]).status.success());

    let output = run(&repo, &["show", "HEAD"]);
    let text = stdout(&output);
    assert!(text.contains("similarity index 90%"), "{text}");
    assert!(text.contains("rename from old.txt"), "{text}");
    assert!(text.contains("-line 5"), "{text}");
    assert!(text.contains("+CHANGED"), "{text}");

    let output = run(&repo, &["log", "-n", "1", "--stat"]);
    let text = stdout(&output);
    assert!(text.contains("old.txt => new.txt | 1 +, 1 -"), "{text}");
}
