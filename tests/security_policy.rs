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
fn environment_access_requires_an_allowed_name() {
    let denied = compile_source(r#"const home = env.HOME ?? "/tmp""#).unwrap_err();
    assert!(denied
        .to_string()
        .contains("environment variable `HOME` is not allowed by the execution policy"));

    let policy = ExecutionPolicy::from_toml(
        "[environment]\nread = [\"HOME\"]\n",
        std::path::Path::new("."),
    )
    .unwrap();
    compile_source_with_policy(r#"const home = process.env("HOME")"#, &policy).unwrap();

    let dynamic = compile_source_with_policy(
        r#"const key = "HOME"
const home = process.env(key)
"#,
        &policy,
    )
    .unwrap_err();
    assert!(dynamic
        .to_string()
        .contains("process.env requires a static string literal"));
}

#[test]
fn process_arguments_require_policy_access() {
    let cases = [
        "const count = args.len()",
        "const values = process.args()",
        "const parsed = cli.parse()",
        "const rendered = \"${args}\"",
    ];

    for source in cases {
        let error = compile_source(source).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("process arguments are not allowed by the execution policy"),
            "{error}"
        );
    }

    let policy =
        ExecutionPolicy::from_toml("[process]\nargs = true\n", std::path::Path::new(".")).unwrap();
    compile_source_with_policy(
        r#"
const count = args.len()
const values = process.args()
const parsed = cli.parse()
const rendered = args.join(",")
"#,
        &policy,
    )
    .unwrap();
}

#[test]
fn args_binding_name_is_reserved_for_process_arguments() {
    let error = compile_source("const args = []").unwrap_err();
    assert!(error
        .to_string()
        .contains("`args` is reserved for process arguments"));
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
            "[command_groups.inspect.commands.probe]\nprogram = {:?}\nargs = 1\n",
            executable
        ),
    )
    .unwrap();

    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();
    let bash =
        compile_source_with_policy(r#"const value = run.inspect.probe("hello")"#, &policy).unwrap();
    assert!(bash.contains(&executable.to_string_lossy().to_string()));
    assert!(bash.contains("__nacre_run_arg_0='hello'"));

    let wrong_arity =
        compile_source_with_policy(r#"const value = run.inspect.probe()"#, &policy).unwrap_err();
    assert!(wrong_arity
        .to_string()
        .contains("expects 1 argument by policy, found 0"));

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
    assert!(bash.contains("__nacre_checked_path read"));

    let denied = compile_source(r#"const lines = fs.readLines("/tmp/input.txt")"#).unwrap_err();
    assert!(denied
        .to_string()
        .contains("requires at least one allowed filesystem read root"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn approved_command_result_captures_stdout_stderr_and_status() {
    let dir = unique_dir();
    fs::create_dir_all(&dir).unwrap();
    let command = dir.join("probe");
    write_executable(
        &command,
        "#!/usr/bin/env bash\nset -euo pipefail\nprintf 'out:%s\\n' \"$1\"\nprintf 'err:%s\\n' \"$1\" >&2\nexit 7\n",
    );
    let policy_path = dir.join("policy.toml");
    fs::write(
        &policy_path,
        "[command_groups.inspect.commands.probe]\nprogram = \"probe\"\nargs = 1\n",
    )
    .unwrap();
    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();
    let mut bash = compile_source_with_policy(
        r#"
const result: CommandOutput = run.result.inspect.probe("value")
const stdout = result.stdout
const stderr = result.stderr
const status = result.status
const success = result.success
"#,
        &policy,
    )
    .unwrap();
    bash.push_str("\nprintf '%s|%s|%s|%s\\n' \"$stdout\" \"$stderr\" \"$status\" \"$success\"\n");

    let output = run_bash(&bash);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "out:value|err:value|7|false\n"
    );
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
        "[command_groups.inspect.commands.echo]\nprogram = \"echo-args\"\nargs = 3\n",
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
fn async_approved_commands_capture_order_and_failures() {
    let dir = unique_dir();
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("worker");
    write_executable(
        &script,
        "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == fail ]]; then printf 'bad\\n' >&2; exit 7; fi\nsleep \"$2\"\nprintf '%s\\n' \"$1\"\n",
    );
    let policy_path = dir.join("policy.toml");
    fs::write(
        &policy_path,
        "[command_groups.jobs.commands.worker]\nprogram = \"worker\"\nargs = 2\n",
    )
    .unwrap();
    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();
    let mut bash = compile_source_with_policy(
        r#"
const slow = async run.jobs.worker("slow", "0.05")
const fast = async run.jobs.worker("fast", "0")
const first = await slow
const second = await fast
const again = await fast
"#,
        &policy,
    )
    .unwrap();
    bash.push_str("\nprintf '%s|%s|%s\\n' \"$first\" \"$second\" \"$again\"\n");
    let output = run_bash(&bash);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "slow|fast|fast\n"
    );

    let bash = compile_source_with_policy(
        r#"
const job = async run.jobs.worker("fail", "0")
const value = await job
"#,
        &policy,
    )
    .unwrap();
    let output = run_bash(&bash);
    assert_eq!(output.status.code(), Some(7));

    let unsafe_async = compile_source("const job = async $sh\"printf unsafe\"").unwrap_err();
    assert!(unsafe_async.message().contains("unsafe shell execution"));

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
#[cfg(unix)]
fn runtime_guards_reject_symlink_components() {
    let dir = unique_dir();
    let allowed = dir.join("allowed");
    let real = allowed.join("real");
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join("data.txt"), "secret\n").unwrap();
    std::os::unix::fs::symlink(&real, allowed.join("linked")).unwrap();
    let policy_path = dir.join("policy.toml");
    fs::write(&policy_path, "[filesystem]\nread = [\"allowed\"]\n").unwrap();
    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();
    let bash = compile_source_with_policy(
        &format!(
            "const lines = fs.readLines({:?})",
            allowed.join("linked/data.txt")
        ),
        &policy,
    )
    .unwrap();

    let output = run_bash(&bash);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("denied read path"));
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
        "[filesystem]\nread = [\"allowed\"]\n\n[command_groups.read.commands.file]\nprogram = \"reader\"\nargs = 1\nread_args = [0]\n",
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
fn command_path_arguments_are_canonicalized_before_execution() {
    let dir = unique_dir();
    let allowed = dir.join("allowed");
    let nested = allowed.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(allowed.join("input.txt"), "safe\n").unwrap();
    let script = dir.join("reader");
    write_executable(
        &script,
        "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$1\"\n",
    );
    let policy_path = dir.join("policy.toml");
    fs::write(
        &policy_path,
        "[filesystem]\nread = [\"allowed\"]\n\n[command_groups.read.commands.file]\nprogram = \"reader\"\nargs = 1\nread_args = [0]\n",
    )
    .unwrap();
    let policy = ExecutionPolicy::from_file(&policy_path).unwrap();
    let source_path = nested.join("../input.txt");
    let mut bash = compile_source_with_policy(
        &format!("const output = run.read.file({source_path:?})"),
        &policy,
    )
    .unwrap();
    bash.push_str("\nprintf '%s\\n' \"$output\"\n");

    let output = run_bash(&bash);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "{}\n",
            allowed.join("input.txt").canonicalize().unwrap().display()
        )
    );
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
        "[command_groups.test.commands.command]\nprogram = \"command\"\nargs = 1\nread_args = [0]\nwrite_args = [0]\n",
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

    let invalid_env = dir.join("invalid-env.toml");
    fs::write(&invalid_env, "[environment]\nread = [\"BAD-NAME\"]\n").unwrap();
    assert!(ExecutionPolicy::from_file(&invalid_env)
        .unwrap_err()
        .to_string()
        .contains("invalid environment variable name"));

    let missing_args = dir.join("missing-args.toml");
    fs::write(
        &missing_args,
        "[command_groups.test.commands.command]\nprogram = \"command\"\n",
    )
    .unwrap();
    assert!(ExecutionPolicy::from_file(&missing_args)
        .unwrap_err()
        .to_string()
        .contains("must declare exact `args` count"));

    let out_of_range_arg = dir.join("out-of-range-arg.toml");
    fs::write(
        &out_of_range_arg,
        "[command_groups.test.commands.command]\nprogram = \"command\"\nargs = 1\nread_args = [1]\n",
    )
    .unwrap();
    assert!(ExecutionPolicy::from_file(&out_of_range_arg)
        .unwrap_err()
        .to_string()
        .contains("outside declared args count"));

    let writable_program = dir.join("writable-program.toml");
    fs::write(
        &writable_program,
        "[filesystem]\nwrite = [\".\"]\n\n[command_groups.test.commands.command]\nprogram = \"command\"\nargs = 0\n",
    )
    .unwrap();
    let error = ExecutionPolicy::from_file(&writable_program).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("inside writable filesystem root"),
        "{error}"
    );
    fs::remove_dir_all(dir).unwrap();
}
