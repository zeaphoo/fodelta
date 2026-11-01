mod common;

use std::process::Output;
use std::time::{Duration, SystemTime};

use common::TestRepo;

fn run(repo: &TestRepo, args: &[&str]) -> Output {
    repo.command().args(args).output().expect("run fodelta")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn gc_removes_old_temporary_files() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    repo.write("a.txt", b"a");
    assert!(run(&repo, &["commit", "-m", "seed"]).status.success());

    let temp = repo.path_of(".fodelta/objects/.fodelta-tmp-crash");
    repo.write(".fodelta/objects/.fodelta-tmp-crash", b"junk");
    let file = std::fs::OpenOptions::new().write(true).open(&temp).unwrap();
    file.set_times(
        std::fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(2 * 86_400)),
    )
    .unwrap();
    drop(file);

    let output = run(&repo, &["gc"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(!temp.exists());
}

#[test]
fn rollback_can_be_replayed_after_a_partial_sync() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    repo.write("a.txt", b"a1");
    assert!(run(&repo, &["commit", "-m", "first"]).status.success());
    repo.write("a.txt", b"a2");
    repo.write("b.txt", b"b");
    assert!(run(&repo, &["commit", "-m", "second"]).status.success());

    assert!(run(&repo, &["rollback", "HEAD~1"]).status.success());
    // Simulate a rollback that moved HEAD but failed before finishing the
    // working tree synchronization.
    std::fs::remove_file(repo.path_of("a.txt")).unwrap();
    repo.write("partial.txt", b"x");

    let output = run(&repo, &["rollback", "HEAD~2"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(repo.read("a.txt"), b"a1");
    assert!(!repo.exists("partial.txt"));
    assert!(run(&repo, &["fsck"]).status.success());
}

#[test]
fn lock_is_released_after_each_command() {
    let repo = TestRepo::new();
    assert!(run(&repo, &["init"]).status.success());
    repo.write("a.txt", b"a");
    assert!(run(&repo, &["commit", "-m", "first"]).status.success());
    repo.write("a.txt", b"b");
    assert!(run(&repo, &["commit", "-m", "second"]).status.success());
}
