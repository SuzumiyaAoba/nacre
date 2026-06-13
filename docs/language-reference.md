# Language Reference

## Bindings and Types

```nacre
const name: String = "Nacre"
let count: Int = 1
count = count + 1

const enabled: Bool = true
const ratio: Float = 1.5
const path: Path = "input/data.txt"
const names: [String] = ["Ada", "Grace"]
const ports: Map[String, Int] = { "http": 80 }
const pair: (String, Int) = ("localhost", 8080)
const user: { name: String, age: Int } = { name: "Ada", age: 36 }
```

Nacre also supports options, results, aliases, newtypes, sum types, generic
functions, traits, lambdas, arrays, maps, records, and tuples.

## Functions and Control Flow

```nacre
fn classify(value: Int): String {
if value > 0 {
return "positive"
} else {
return "zero"
}
}

for name in ["Ada", "Grace"] {
const label = classify(name.len())
}
```

Supported control flow includes `if`, `while`, `for`, `match`, `break`, and
`continue`.

## Modules

```nacre
use std.path
const base = path.basename("/tmp/nacre.txt")
```

Module declarations are namespaced when imported. Modules containing command or
filesystem operations require the corresponding policy capabilities even when
the module wraps those operations in functions.

## Approved Commands

```nacre
const output = run.inspect.status("--short")
run.output.echo(output)
```

The command name must have exactly three static segments:
`run.<group>.<command>`. The checker resolves it through the external execution
policy. Dynamic command names are not supported.

An approved command returns captured standard output as `String`. A non-zero
exit terminates the generated script under `set -euo pipefail`.

Policy command entries:

```toml
[command_groups.inspect.commands.status]
program = "/usr/bin/git"
read_args = []
write_args = []
```

`read_args` and `write_args` are zero-based argument positions checked against
the filesystem roots before execution.

## Filesystem Operations

Read operations:

- `pathExists(path)`
- `fs.isFile(path)`
- `fs.isDir(path)`
- `fs.size(path)`
- `fs.readLines(path)`
- `fs.list(path)`

Write operations:

- `fs.writeLines(path, lines)`
- `fs.appendLines(path, lines)`

The checker requires at least one root for the corresponding access mode. The
generated Bash resolves each runtime path and rejects paths outside the
canonical roots.

## Environment and Arguments

```nacre
const shell = env.SHELL ?? "/bin/sh"
const home = process.env("HOME")
const arguments = args
```

Environment access does not execute a command. Treat environment values and
script arguments as untrusted data when passing them to approved commands.

## Disabled Syntax

The safe profile rejects:

- `$sh"..."`, `$sh'...'`, and `$sh{ ... }`
- shell pipelines and redirects
- raw Bash blocks
- async or spawned shell commands
- `require(...)` and `requireOneOf(...)`

All external execution must use a policy-approved static command call.

All code in this repository was developed by a Coding Agent.
