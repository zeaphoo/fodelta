mod common;

use std::process::Output;

use common::TestRepo;
use fodelta::history::{TreeEntryInfo, TreeMap, build_trees};
use fodelta::odb::{Commit, MODE_FILE, ObjectKind};
use fodelta::repo::{MAIN_REF, Repository, refs};

fn run(repo: &TestRepo, args: &[&str]) -> Output {
    repo.command().args(args).output().expect("run fodelta")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn commit_rejects_parent_paths() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    repo.write("a.txt", b"a");
    let output = run(&repo, &["commit", "-m", "x", "--", "../outside.txt"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("escapes"), "{}", stderr(&output));
}

#[test]
fn restore_rejects_absolute_paths() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    repo.write("a.txt", b"a");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());
    let output = run(&repo, &["restore", "HEAD", "--", "/tmp/fodelta-escape"]);
    assert_eq!(output.status.code(), Some(2));
}

#[cfg(unix)]
#[test]
fn restore_does_not_write_through_a_symlinked_parent() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());

    // Create a commit containing link/secret.txt without creating that path in
    // the working tree.
    let repository = Repository::open(repo.path()).unwrap();
    let store = repository.store();
    let oid = store.write_blob(b"secret").unwrap();
    let mut map = TreeMap::new();
    map.insert(
        b"link/secret.txt".to_vec(),
        TreeEntryInfo {
            mode: MODE_FILE,
            oid,
        },
    );
    let tree = build_trees(&store, &map).unwrap();
    let commit = Commit::new(tree, None, repository.author().unwrap(), "seed".to_string());
    let commit_oid = store
        .write(ObjectKind::Commit, &commit.encode().unwrap())
        .unwrap();
    refs::update_ref(&repository.fodelta, MAIN_REF, None, commit_oid, false).unwrap();

    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), repo.path_of("link")).unwrap();

    let output = run(&repo, &["restore", "HEAD", "--", "link/secret.txt"]);
    assert_eq!(output.status.code(), Some(2), "stdout: {:?}", output.stdout);
    assert!(!outside.path().join("secret.txt").exists());
}
