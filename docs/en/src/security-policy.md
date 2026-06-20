# Execution Policy

Nacre separates source code from authority. Source files describe typed
computation and static capability calls; an external TOML policy decides which
environment variables, process arguments, executables, and filesystem roots
those calls may access.

Without `--policy`, the compiler uses a deny-all policy.

## Environment Variables

Grant individual environment variable names explicitly:

```toml
[environment]
read = ["HOME", "SHELL"]
```

Both `env.NAME` and `process.env("NAME")` require the name to appear in this
list. `process.env(...)` accepts only a static string literal name.

## Process Arguments

Grant access to script arguments explicitly:

```toml
[process]
args = true
```

Bare `args`, `process.args()`, and `cli.parse()` all require this permission.
Without it, generated programs cannot read command-line arguments supplied to
the script.

## Command Capabilities

Source names a static group and command:

```nacre
const status = run.inspect.git("--version")
```

The policy maps that name to one executable:

```toml
[command_groups.inspect.commands.git]
program = "/usr/bin/git"
args = 1
read_args = []
write_args = []
```

The source cannot substitute another group, command, or executable at runtime.
The policy also fixes the exact argument count accepted by the source call.
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
Filesystem operations and guarded command path arguments use the resolved
physical path after the guard succeeds.

## Guard Command Arguments

Approved commands can identify path arguments:

```toml
[filesystem]
read = ["input"]
write = ["output"]

[command_groups.convert.commands.document]
program = "bin/convert-document"
args = 2
read_args = [0]
write_args = [1]
```

`read_args` and `write_args` use zero-based positions. Guards run before the
executable. The same position cannot appear in both lists.

## Policy Validation

Policy loading rejects:

- Unknown fields.
- Invalid environment variable names.
- Invalid group or command identifiers.
- Missing command `args` counts.
- Command path-argument positions outside the declared `args` count.
- Missing or non-file executables.
- Command executables located inside an allowed write root.
- Missing filesystem roots.
- Overlapping read and write argument positions.
- Paths that cannot be canonicalized.

Keeping command executables outside write roots is enforced because otherwise
source code could replace an approved executable and invoke the replacement
with the executable's authority.

## Trust Boundary

Treat the following as trusted inputs:

- The policy file.
- Every executable or script named by the policy.
- The compiler and generated runtime helpers.

Treat Nacre source, Nacre packages loaded through local path dependencies,
environment values, script arguments, and data files as potentially untrusted.
Dependency packages do not carry separate authority; capability calls in them
are checked against the same external policy as the importing program.

The runtime guards reduce path escape risk but cannot eliminate filesystem
time-of-check/time-of-use races against a concurrently malicious process.

## Operational Guidance

- Keep policies narrow and under human review.
- Prefer a dedicated wrapper script over granting a general-purpose shell.
- Separate read-only and write-capable commands into clear groups.
- Pin or control reviewed executables outside every configured write root.
- Test denial paths as well as successful execution.
