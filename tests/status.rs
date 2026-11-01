mod common;

use std::process::Output;

use common::TestRepo;
use fodelta::history::{TreeEntryInfo, TreeMap, build_trees};
use fodelta::odb::{Commit, MODE_FILE, ObjectKind};
use fodelta::repo::{MAIN_REF, Repository, refs};

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

fn seed(repo: &TestRepo, files: &[(&str, &[u8])]) {
    let repository = Repository::open(repo.path()).expect("open repository");
    let store = repository.store();
    let mut map = TreeMap::new();
    for (name, contents) in files {
        let path = repository.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, contents).unwrap();
        let oid = store.write_blob(contents).unwrap();
        map.insert(
            name.as_bytes().to_vec(),
            TreeEntryInfo {
                mode: MODE_FILE,
                oid,
            },
        );
    }
    let tree = build_trees(&store, &map).unwrap();
    let commit = Commit::new(tree, None, repository.author().unwrap(), "seed".to_string());
    let oid = store
        .write(ObjectKind::Commit, &commit.encode().unwrap())
        .unwrap();
    refs::update_ref(&repository.fodelta, MAIN_REF, None, oid, false).unwrap();
}

#[test]
fn status_reports_new_modified_and_deleted() {
    let repo = TestRepo::new();
    init(&repo);
    seed(&repo, &[("a.txt", b"a"), ("b.txt", b"b")]);

    repo.write("a.txt", b"a changed");
    std::fs::remove_file(repo.path_of("b.txt")).unwrap();
    repo.write("c.txt", b"c");

    let output = run(&repo, &["status"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("new file: c.txt"), "{text}");
    assert!(text.contains("modified: a.txt"), "{text}");
    assert!(text.contains("deleted:  b.txt"), "{text}");
    assert!(text.contains("3 change(s)"), "{text}");
}

#[test]
fn status_is_clean_after_seed() {
    let repo = TestRepo::new();
    init(&repo);
    seed(&repo, &[("a.txt", b"a")]);
    let output = run(&repo, &["status"]);
    assert!(output.status.success());
    assert!(stdout(&output).contains("working tree clean"));
}

#[test]
fn status_on_unborn_repository() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write("new.txt", b"x");
    let output = run(&repo, &["status"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("no commits yet"));
    assert!(text.contains("new file: new.txt"));
}

#[test]
fn ignored_files_are_listed_with_flag() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write(".gitignore", b"*.log\n");
    repo.write("build.log", b"x");

    let output = run(&repo, &["status"]);
    assert!(!stdout(&output).contains("build.log"));

    let output = run(&repo, &["status", "--ignored"]);
    let text = stdout(&output);
    assert!(text.contains("Ignored files:"), "{text}");
    assert!(text.contains("build.log"), "{text}");
}

#[test]
fn ignore_check_explains_the_matching_rule() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write(".gitignore", b"*.log\n");
    repo.write("x.log", b"x");
    repo.write("src/main.rs", b"x");

    let output = run(&repo, &["ignore", "check", "x.log"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("ignored"), "{text}");
    assert!(text.contains("pattern: *.log"), "{text}");
    assert!(text.contains("source: .gitignore"), "{text}");

    let output = run(&repo, &["ignore", "check", "src/main.rs"]);
    let text = stdout(&output);
    assert_eq!(text.trim(), "not ignored");
}

#[test]
fn ignore_list_shows_nested_files() {
    let repo = TestRepo::new();
    init(&repo);
    repo.write(".gitignore", b"*.log\n");
    repo.write("src/.gitignore", b"*.tmp\n");

    let output = run(&repo, &["ignore", "list"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains(".gitignore:"), "{text}");
    assert!(text.contains("src/.gitignore:"), "{text}");
    assert!(text.contains("*.tmp"), "{text}");
}

#[test]
fn ignore_add_is_not_a_command() {
    let repo = TestRepo::new();
    init(&repo);
    let output = run(&repo, &["ignore", "add", "*.bak"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn rewriting_same_content_does_not_create_a_change() {
    let repo = TestRepo::new();
    init(&repo);
    seed(&repo, &[("a.txt", b"same")]);
    repo.write("a.txt", b"same");
    let output = run(&repo, &["status"]);
    assert!(stdout(&output).contains("working tree clean"));
}

#[cfg(unix)]
#[test]
fn executable_bit_is_reported_as_a_modification() {
    use std::os::unix::fs::PermissionsExt;

    let repo = TestRepo::new();
    init(&repo);
    seed(&repo, &[("run.sh", b"#!/bin/sh\n")]);
    let path = repo.path_of("run.sh");
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();

    let output = run(&repo, &["status"]);
    assert!(stdout(&output).contains("modified: run.sh"));
}
