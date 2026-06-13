use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use nacre::{compile_source, compile_source_with_policy, ExecutionPolicy};

fn unique_dir() -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nacre-policy-{unique}"))
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
