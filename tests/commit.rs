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

#[test]
fn jobs_option_is_accepted() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a");
    let output = run(&repo, &["--jobs", "2", "commit", "-m", "seed"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
}

#[cfg(unix)]
#[test]
fn unchanged_files_are_not_read_again() {
    use std::os::unix::fs::PermissionsExt;

    let repo = TestRepo::new();
    init(&repo);
    repo.write("a.txt", b"a");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());

    let path = repo.path_of("a.txt");
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o000);
    std::fs::set_permissions(&path, permissions).unwrap();

    let output = run(&repo, &["commit", "-m", "noop"]);

    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o644);
    std::fs::set_permissions(&path, permissions).unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("nothing to commit"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn jobs_zero_is_rejected() {
    let repo = TestRepo::new();
    init(&repo);
    let output = run(&repo, &["--jobs", "0", "status"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--jobs"), "{}", stderr(&output));
}

#[test]
fn directory_prefix_commit_leaves_other_changes() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("src/a.rs", b"a1");
    repo.write("src/b.rs", b"b1");
    repo.write("docs/x.md", b"x1");
    assert!(run(&repo, &["commit", "-m", "initial"]).status.success());

    repo.write("src/a.rs", b"a2");
    repo.write("src/b.rs", b"b2");
    repo.write("docs/x.md", b"x2");
    let output = run(&repo, &["commit", "-m", "src", "--", "src/"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("2 file(s) changed"));

    let status = stdout(&run(&repo, &["status"]));
    assert!(status.contains("modified: docs/x.md"), "{status}");
    assert!(!status.contains("src/a.rs"), "{status}");
}
