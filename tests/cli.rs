use std::process::Command;

fn rtrace() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rtrace"))
}

#[test]
fn no_arguments_prints_usage_and_fails() {
    let output = rtrace().output().expect("failed to run rtrace");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage: rtrace"), "stderr was: {stderr}");
}

#[test]
fn a_command_exits_zero_for_now() {
    let status = rtrace().arg("true").status().expect("failed to run rtrace");
    assert!(status.success());
}

#[test]
fn false_exists_one() {
    let status = rtrace().arg("false").status().expect("failed to run rtrace");
    assert_eq!(status.code(), Some(1));
}
