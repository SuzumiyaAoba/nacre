# Execution Policy

Nacre separates source code from authority. Source files describe typed
computation and static capability calls; an external TOML policy decides which
executables and filesystem roots those calls may access.

Without `--policy`, the compiler uses a deny-all policy.

## Command Capabilities

Source names a static group and command:

```nacre
const status = run.inspect.git("--version")
```

The policy maps that name to one executable:

```toml
[command_groups.inspect.commands.git]
program = "/usr/bin/git"
read_args = []
write_args = []
```

The source cannot substitute another group, command, or executable at runtime.
Relative `program` paths are resolved from the policy file directory and
canonicalized when the policy is loaded.

## Argument Handling

Each source expression becomes one Bash argument. Nacre does not concatenate
arguments into a command string or evaluate them as shell syntax.

For example, a value containing `$(command)`, backticks, whitespace, or a
semicolon remains ordinary data when passed to an approved executable.

## Filesystem Roots

Grant canonical roots by access mode:

```toml
[filesystem]
read = ["input"]
write = ["output"]
```

Roots must exist when the policy is loaded. Runtime guards resolve the requested
path and require it to be the root itself or a descendant. A new write target
is allowed when its parent directory exists beneath an allowed write root.

## Guard Command Arguments

Approved commands can identify path arguments:

```toml
[filesystem]
read = ["input"]
write = ["output"]

[command_groups.convert.commands.document]
program = "bin/convert-document"
read_args = [0]
write_args = [1]
```

`read_args` and `write_args` use zero-based positions. Guards run before the
executable. The same position cannot appear in both lists.

## Policy Validation

Policy loading rejects:

- Unknown fields.
- Invalid group or command identifiers.
- Missing or non-file executables.
- Missing filesystem roots.
- Overlapping read and write argument positions.
- Paths that cannot be canonicalized.

## Trust Boundary

Treat the following as trusted inputs:

- The policy file.
- Every executable or script named by the policy.
- The compiler and generated runtime helpers.

Treat Nacre source, environment values, script arguments, and data files as
potentially untrusted.

The runtime guards reduce path escape risk but cannot eliminate filesystem
time-of-check/time-of-use races against a concurrently malicious process.

## Operational Guidance

- Keep policies narrow and under human review.
- Prefer a dedicated wrapper script over granting a general-purpose shell.
- Separate read-only and write-capable commands into clear groups.
- Pin or control reviewed executables outside source-writable directories.
- Test denial paths as well as successful execution.
