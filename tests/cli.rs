use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn cli_writes_to_stdout_and_file_and_reports_usage() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("nacre-cli-{unique}"));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join(format!("nacre-cli-{unique}.ncr"));
    let output = dir.join(format!("nacre-cli-{unique}.sh"));
    let command_input = dir.join("command.ncr");
    let command_output = dir.join("command.sh");
    let command_script = dir.join("approved-command");
    let invalid_input = dir.join("invalid.ncr");
    let policy = dir.join("policy.toml");
    fs::write(
        &input,
        r#"
const answer = 42
const no = false
const text = "a\"b"
let count = 1
count = count + 2 * 3
const cmp = count >= 7
"#,
    )
    .unwrap();
    fs::write(
        &command_input,
        "const output = run.inspect.command(\"safe\")\n",
    )
    .unwrap();
    fs::write(&invalid_input, "const value: Bool = 1\n").unwrap();
    fs::write(
        &command_script,
        "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$@\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&command_script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&command_script, permissions).unwrap();
    }
    fs::write(
        &policy,
        "[command_groups.inspect.commands.command]\nprogram = \"approved-command\"\nargs = 1\n",
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

    let denied_command = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg(&command_input)
        .output()
        .unwrap();
    assert!(!denied_command.status.success());
    assert!(String::from_utf8(denied_command.stderr)
        .unwrap()
        .contains("not allowed by the execution policy"));

    let allowed_command = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg("--policy")
        .arg(&policy)
        .arg(&command_input)
        .arg(&command_output)
        .output()
        .unwrap();
    assert!(allowed_command.status.success());
    assert!(fs::read_to_string(&command_output).unwrap().contains(
        &command_script
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string()
    ));

    let usage_run = Command::new(env!("CARGO_BIN_EXE_nacre")).output().unwrap();
    assert!(!usage_run.status.success());
    assert!(String::from_utf8(usage_run.stderr)
        .unwrap()
        .contains("usage: nacre"));

    let invalid_run = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg(&invalid_input)
        .output()
        .unwrap();
    assert!(!invalid_run.status.success());
    let invalid_stderr = String::from_utf8(invalid_run.stderr).unwrap();
    assert!(invalid_stderr.contains("line 1:1"));
    assert!(invalid_stderr.contains("const value: Bool = 1"));
    assert!(invalid_stderr.contains("^"));

    let json_invalid_run = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg("--diagnostic-format")
        .arg("json")
        .arg(&invalid_input)
        .output()
        .unwrap();
    assert!(!json_invalid_run.status.success());
    let json_stderr = String::from_utf8(json_invalid_run.stderr).unwrap();
    assert!(json_stderr.contains("\"line\":1"));
    assert!(json_stderr.contains("\"message\":\"type annotation mismatch"));
    assert!(json_stderr.contains("\"source_line\":\"const value: Bool = 1\""));

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

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cli_writes_and_validates_lockfile() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("nacre-lock-{unique}"));
    let app = dir.join("app");
    let tools = dir.join("tools");
    fs::create_dir_all(&app).unwrap();
    fs::create_dir_all(&tools).unwrap();
    fs::write(
        app.join("nacre.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies.tools]\npath = \"../tools\"\n",
    )
    .unwrap();
    fs::write(
        tools.join("format.ncr"),
        "fn label(value: String): String {\nreturn \"tool:${value}\"\n}\n",
    )
    .unwrap();
    let input = app.join("main.ncr");
    fs::write(
        &input,
        "use tools.format\nconst result = format.label(\"ok\")\n",
    )
    .unwrap();

    let write_lock = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg("--write-lock")
        .arg(&input)
        .output()
        .unwrap();
    assert!(write_lock.status.success());
    let lock = fs::read_to_string(app.join("nacre.lock")).unwrap();
    assert!(lock.contains("[[package]]"));
    assert!(lock.contains("name = \"tools\""));

    fs::write(
        tools.join("format.ncr"),
        "fn label(value: String): String {\nreturn \"changed:${value}\"\n}\n",
    )
    .unwrap();
    let stale_lock = Command::new(env!("CARGO_BIN_EXE_nacre"))
        .arg(&input)
        .output()
        .unwrap();
    assert!(!stale_lock.status.success());
    assert!(String::from_utf8(stale_lock.stderr)
        .unwrap()
        .contains("does not match"));

    fs::remove_dir_all(dir).unwrap();
}
