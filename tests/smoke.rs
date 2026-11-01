mod common;

#[test]
fn version_runs_through_test_helper() {
    let output = common::fodelta_cmd()
        .arg("--version")
        .output()
        .expect("run fodelta");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("fodelta 0.1.0"));
}
