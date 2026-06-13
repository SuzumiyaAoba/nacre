use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn cli_writes_to_stdout_and_file_and_reports_usage() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir();
    let input = dir.join(format!("nacre-cli-{unique}.ncr"));
    let output = dir.join(format!("nacre-cli-{unique}.sh"));
    fs::write(
        &input,
        r#"
const answer = 42
const no = false
const text = "a\"b"
const home = env.HOME ?? "/tmp"
let count = 1
count = count + 2 * 3
const cmp = count >= 7
"#,
    )
    .unwrap();

    let stdout_run = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg(&input)
        .output()
        .unwrap();
    assert!(stdout_run.status.success());
    assert!(String::from_utf8(stdout_run.stdout)
        .unwrap()
        .contains("readonly answer=42"));

    let file_run = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert!(file_run.status.success());
    assert!(fs::read_to_string(&output)
        .unwrap()
        .contains("readonly answer=42"));

    let usage_run = Command::new(env!("CARGO_BIN_EXE_nacre")).output().unwrap();
    assert!(!usage_run.status.success());
    assert!(String::from_utf8(usage_run.stderr)
        .unwrap()
        .contains("usage: nacre"));

    let missing_run = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg(dir.join(format!("missing-{unique}.ncr")))
        .output()
        .unwrap();
    assert!(!missing_run.status.success());
    assert!(String::from_utf8(missing_run.stderr)
        .unwrap()
        .contains("failed to read"));

    let missing_write_run = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg(dir.join(format!("missing-write-{unique}.ncr")))
        .arg(&output)
        .output()
        .unwrap();
    assert!(!missing_write_run.status.success());
    assert!(String::from_utf8(missing_write_run.stderr)
        .unwrap()
        .contains("failed to read"));

    let write_error_run = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg(&input)
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!write_error_run.status.success());
    assert!(String::from_utf8(write_error_run.stderr)
        .unwrap()
        .contains("failed to write"));

    fs::remove_file(input).unwrap();
    fs::remove_file(output).unwrap();
}
