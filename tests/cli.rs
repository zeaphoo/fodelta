use std::process::Command;

fn fodelta() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fodelta"))
}

#[test]
fn version_flag_reports_package_version() {
    let output = fodelta().arg("--version").output().expect("run fodelta");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("fodelta 0.1.0"),
        "unexpected version: {stdout}"
    );
}

#[test]
fn help_lists_every_command() {
    let output = fodelta().arg("--help").output().expect("run fodelta");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for command in [
        "init", "status", "commit", "log", "show", "diff", "restore", "rollback", "safety",
        "ignore", "gc", "fsck", "config",
    ] {
        assert!(
            stdout.contains(command),
            "help is missing {command}:\n{stdout}"
        );
    }
}

#[test]
fn unknown_command_exits_with_usage_error() {
    let output = fodelta()
        .arg("not-a-command")
        .output()
        .expect("run fodelta");
    assert_eq!(output.status.code(), Some(2));
}
