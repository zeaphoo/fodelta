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

#[test]
fn init_creates_expected_layout() {
    let repo = TestRepo::new();
    let output = run(&repo, &["init"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Initialized empty fodelta repository"));

    for path in [
        ".fodelta/HEAD",
        ".fodelta/config.toml",
        ".fodelta/objects",
        ".fodelta/refs/heads",
        ".fodelta/logs/safety",
    ] {
        assert!(repo.exists(path), "missing {path}");
    }
    assert!(!repo.exists(".fodelta/info/exclude"));
    assert_eq!(repo.read(".fodelta/HEAD"), b"ref: refs/heads/main\n");
    let config = String::from_utf8(repo.read(".fodelta/config.toml")).unwrap();
    assert!(config.contains("format_version = 1"));
}

#[test]
fn double_init_is_rejected() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    let output = run(&repo, &["init"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("already exists"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn init_inside_repository_is_rejected() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    std::fs::create_dir_all(repo.path_of("nested")).unwrap();
    let output = run(&repo, &["init", "nested"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("inside"), "{}", stderr(&output));
}

#[test]
fn config_set_and_get() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    let set = run(&repo, &["config", "user.name", "Ada"]);
    assert!(set.status.success(), "stderr: {}", stderr(&set));

    let get = run(&repo, &["config", "user.name"]);
    assert!(get.status.success());
    assert_eq!(stdout(&get), "Ada\n");
}

#[test]
fn config_rejects_unknown_keys() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    let output = run(&repo, &["config", "core.surprise", "1"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("unknown configuration key"));
}

#[test]
fn config_rejects_unknown_fields() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    let mut config = String::from_utf8(repo.read(".fodelta/config.toml")).unwrap();
    config.push_str("\n[core]\nsurprise = true\n");
    repo.write(".fodelta/config.toml", config);
    let output = run(&repo, &["config", "user.name"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn higher_repository_version_is_rejected() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    repo.write(".fodelta/config.toml", "format_version = 2\n");
    let output = run(&repo, &["config", "user.name"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("unsupported repository format version 2"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn discovery_works_from_a_subdirectory_with_dash_c() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    assert!(
        run(&repo, &["config", "user.name", "Grace"])
            .status
            .success()
    );
    std::fs::create_dir_all(repo.path_of("sub/dir")).unwrap();

    let mut command = repo.command();
    let output = command
        .args(["-C", "sub/dir", "config", "user.name"])
        .output()
        .expect("run fodelta");
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output), "Grace\n");
}
