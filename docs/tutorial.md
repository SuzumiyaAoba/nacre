# Tutorial

Nacre compiles typed `.ncr` source into Bash. The default execution policy is
deny-all: pure computation works without configuration, while commands and
filesystem access require a separately reviewed policy.

## Pure Programs

```nacre
fn greet(name: String): String {
return "Hello, ${name}"
}

const message = greet("Nacre")
const names = ["Ada", "Grace"]
const joined = names.join(", ")
```

Compile it with:

```bash
cargo run -- input.ncr output.sh
bash output.sh
```

## Approved Commands

Commands use a static group and alias:

```nacre
const version = run.inspect.version()
run.output.echo("version: ${version}")
```

The source cannot choose an executable. A separate TOML policy maps each alias
to a reviewed program or script:

```toml
[command_groups.inspect.commands.version]
program = "bin/version"

[command_groups.output.commands.echo]
program = "bin/echo"
```

Relative program paths are resolved from the policy file directory and
canonicalized. Compile with:

```bash
cargo run -- --policy policy.toml input.ncr output.sh
```

Arguments are evaluated separately and passed as individual Bash arguments.
They are never concatenated into a shell command.

## Filesystem Access

Declare canonical read and write roots:

```toml
[filesystem]
read = ["input"]
write = ["output"]
```

Then use structured filesystem operations:

```nacre
const lines = fs.readLines("input/data.txt")
fs.writeLines("output/result.txt", lines)
```

Runtime guards reject paths outside the configured roots. Commands can apply
the same guards to selected arguments with `read_args` and `write_args`.

## Removed Shell Features

`$sh`, raw Bash blocks, shell pipelines, redirects, async shell commands, and
`require` are rejected. Add a narrowly scoped reviewed script to the policy
instead of embedding shell syntax in Nacre source.

All code in this repository was developed by a Coding Agent.
