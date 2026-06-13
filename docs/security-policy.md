# Execution Policy

Nacre uses an external TOML policy to grant command and filesystem access.
Source files cannot grant capabilities to themselves. Without `--policy`, the
compiler uses a deny-all policy.

All code in this repository was developed by a Coding Agent.

## Commands

Source code invokes a statically named command:

```nacre
const status = run.inspect.git("--version")
```

The policy maps the group and alias to one canonical executable:

```toml
[command_groups.inspect.commands.git]
program = "/usr/bin/git"
read_args = []
write_args = []
```

Nacre passes every argument directly as one Bash argument. Command names,
pipelines, redirects, `$sh` expressions, and raw Bash blocks cannot be supplied
by source code.

Path arguments can be constrained:

```toml
[filesystem]
read = ["docs"]
write = ["generated"]

[command_groups.inspect.commands.cat]
program = "/bin/cat"
read_args = [0]
```

`read_args` and `write_args` contain zero-based argument positions. At runtime,
those arguments must resolve beneath a corresponding allowed root. Policy paths
relative to the TOML file are canonicalized when the policy is loaded.

## CLI

```bash
nacre --policy nacre-policy.toml input.ncr output.sh
```

Keep policy files under human review. A Coding Agent may edit Nacre source, but
must not be allowed to broaden the policy used to compile or run it.

