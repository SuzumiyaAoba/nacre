use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use nacre::{compile_source, compile_source_with_policy, ExecutionPolicy};

fn unique_dir() -> std::path::PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nacre-policy-{}-{unique}-{sequence}",
        std::process::id()
    ))
}

fn write_executable(path: &std::path::Path, source: &str) {
    fs::write(path, source).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }
}

fn run_bash(source: &str) -> std::process::Output {
    Command::new("bash").arg("-c").arg(source).output().unwrap()
}

#[test]
fn shell_and_raw_bash_are_rejected() {
    let shell = compile_source(r#"const value = $sh"echo unsafe""#).unwrap_err();
    assert!(shell.to_string().contains("$sh commands"));

    let raw = compile_source("raw {\necho unsafe\n}\n").unwrap_err();
    assert!(raw.to_string().contains("raw Bash blocks are disabled"));
}

#[test]
fn approved_command_is_resolved_to_the_policy_executable() {
    let dir = unique_dir();
    fs::create_dir_all(&dir).unwrap();
    let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
    let policy_path = dir.join("policy.toml");
    fs::write(
        &policy_path,
        format!(
            "[command_groups.inspect.commands.probe]\nprogram = {:?}\n",
            executable
        ),
    )
    .unwrap();

    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();
    let bash =
        compile_source_with_policy(r#"const value = run.inspect.probe("hello")"#, &policy).unwrap();
    assert!(bash.contains(&executable.to_string_lossy().to_string()));
    assert!(bash.contains("__nacre_run_arg_0='hello'"));

    let denied = compile_source(r#"const value = run.inspect.probe("hello")"#).unwrap_err();
    assert!(denied
        .to_string()
        .contains("not allowed by the execution policy"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn filesystem_access_requires_an_allowed_root() {
    let dir = unique_dir();
    let root = dir.join("readable");
    fs::create_dir_all(&root).unwrap();
    let policy_path = dir.join("policy.toml");
    fs::write(&policy_path, "[filesystem]\nread = [\"readable\"]\n").unwrap();
    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();

    let bash = compile_source_with_policy(
        &format!(
            r#"const lines = fs.readLines({:?})"#,
            root.join("input.txt")
        ),
        &policy,
    )
    .unwrap();
    assert!(bash.contains("__nacre_assert_path_in_roots read"));

    let denied = compile_source(r#"const lines = fs.readLines("/tmp/input.txt")"#).unwrap_err();
    assert!(denied
        .to_string()
        .contains("requires at least one allowed filesystem read root"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn approved_command_arguments_are_not_evaluated_as_shell() {
    let dir = unique_dir();
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("echo-args");
    write_executable(
        &script,
        "#!/usr/bin/env bash\nset -euo pipefail\nprintf '<%s>\\n' \"$@\"\n",
    );
    let policy_path = dir.join("policy.toml");
    fs::write(
        &policy_path,
        "[command_groups.inspect.commands.echo]\nprogram = \"echo-args\"\n",
    )
    .unwrap();
    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();
    let marker = dir.join("must-not-exist");
    let source = format!(
        "const prefix = \"safe\"\nconst output = run.inspect.echo(\"${{prefix}} $(touch {})\", \"`touch {}`\", \"semi;colon\")",
        marker.display(),
        marker.display()
    );
    let mut bash = compile_source_with_policy(&source, &policy).unwrap();
    bash.push_str("\nprintf '%s\\n' \"$output\"\n");

    let output = run_bash(&bash);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("$(touch "));
    assert!(stdout.contains("`touch "));
    assert!(stdout.contains("<semi;colon>"));
    assert!(!marker.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn runtime_guards_reject_paths_outside_roots() {
    let dir = unique_dir();
    let allowed = dir.join("allowed");
    let outside = dir.join("outside");
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.txt"), "secret\n").unwrap();
    let policy_path = dir.join("policy.toml");
    fs::write(
        &policy_path,
        "[filesystem]\nread = [\"allowed\"]\nwrite = [\"allowed\"]\n",
    )
    .unwrap();
    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();

    let read = compile_source_with_policy(
        &format!(
            "const lines = fs.readLines({:?})",
            outside.join("secret.txt")
        ),
        &policy,
    )
    .unwrap();
    let read_output = run_bash(&read);
    assert!(!read_output.status.success());
    assert!(String::from_utf8_lossy(&read_output.stderr).contains("denied read path"));

    let write = compile_source_with_policy(
        &format!(
            "fs.writeLines({:?}, [\"blocked\"])",
            outside.join("written.txt")
        ),
        &policy,
    )
    .unwrap();
    let write_output = run_bash(&write);
    assert!(!write_output.status.success());
    assert!(String::from_utf8_lossy(&write_output.stderr).contains("denied write path"));
    assert!(!outside.join("written.txt").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn command_path_arguments_are_guarded_before_execution() {
    let dir = unique_dir();
    let allowed = dir.join("allowed");
    let outside = dir.join("outside");
    fs::create_dir_all(&allowed).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let invocation_log = dir.join("invoked");
    let script = dir.join("reader");
    write_executable(
        &script,
        &format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf invoked > {:?}\nprintf '%s\\n' \"$1\"\n",
            invocation_log
        ),
    );
    let policy_path = dir.join("policy.toml");
    fs::write(
        &policy_path,
        "[filesystem]\nread = [\"allowed\"]\n\n[command_groups.read.commands.file]\nprogram = \"reader\"\nread_args = [0]\n",
    )
    .unwrap();
    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();
    let bash = compile_source_with_policy(
        &format!(
            "const output = run.read.file({:?})",
            outside.join("secret.txt")
        ),
        &policy,
    )
    .unwrap();

    let output = run_bash(&bash);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("denied read path"));
    assert!(!invocation_log.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn malformed_policies_are_rejected() {
    let dir = unique_dir();
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("command");
    write_executable(&script, "#!/usr/bin/env bash\nexit 0\n");

    let overlap = dir.join("overlap.toml");
    fs::write(
        &overlap,
        "[command_groups.test.commands.command]\nprogram = \"command\"\nread_args = [0]\nwrite_args = [0]\n",
    )
    .unwrap();
    assert!(ExecutionPolicy::from_file(&overlap)
        .unwrap_err()
        .to_string()
        .contains("both read and write"));

    let unknown = dir.join("unknown.toml");
    fs::write(&unknown, "unexpected = true\n").unwrap();
    assert!(ExecutionPolicy::from_file(&unknown)
        .unwrap_err()
        .to_string()
        .contains("failed to parse policy"));
    fs::remove_dir_all(dir).unwrap();
}
