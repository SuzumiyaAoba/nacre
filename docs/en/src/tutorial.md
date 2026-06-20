# Getting Started

This guide compiles and runs a small Nacre program from the repository checkout.
Nacre source uses the `.ncr` extension and compiles to Bash.

## Prerequisites

Use either a local Rust toolchain or the Nix development environment:

```bash
nix develop path:.
```

The repository pins the tools needed to build, test, and document the project.

## 1. Write a Pure Program

Create `hello.ncr`:

```nacre
fn greet(name: String): String {
    return "Hello, ${name}"
}

const names = ["Ada", "Grace"]
const message = greet(names.join(" and "))
```

Compile it:

```bash
cargo run -- hello.ncr hello.sh
bash hello.sh
```

Pure computation needs no policy. The generated script starts with
`set -euo pipefail` and contains no dependency on the compiler.

## 2. Call an Approved Command

Nacre does not permit the source file to choose an executable. Add a static
command call:

```nacre
fn greet(name: String): String {
    return "Hello, ${name}"
}

run.output.echo(greet("Nacre"))
```

Create `policy.toml`:

```toml
[command_groups.output.commands.echo]
program = "bin/echo"
args = 1
```

The executable path is resolved relative to the policy file and canonicalized.
Compile and run:

```bash
cargo run -- --policy policy.toml hello.ncr hello.sh
bash hello.sh
```

Each expression passed to `run.output.echo(...)` becomes one Bash argument.
Shell metacharacters inside values are data, not executable syntax.

## 3. Grant Filesystem Access

Filesystem operations also require an external policy:

```toml
[filesystem]
read = ["workspace"]
write = ["workspace"]
```

The directories must exist when the policy is loaded. The program can then use
structured operations:

```nacre
const output: Path = "workspace/message.txt"
fs.writeLines(output, ["first", "second"])

const lines = fs.readLines(output)
run.output.echo(lines.join(", "))
```

Generated runtime guards reject paths outside the configured roots.

## 4. Inspect Compilation Errors

Errors include a source line and a concrete explanation:

```nacre
const count: Int = "three"
```

```text
line 1: expected Int, found String
```

The exact wording can evolve, but parse, type, policy, and file errors all
produce a non-zero CLI exit status.

## Next Steps

- Learn expressions and declarations in the
  [Language Reference](language-reference.md).
- Review authority boundaries in [Execution Policy](security-policy.md).
- Run the complete [Verified Examples](examples.md).
