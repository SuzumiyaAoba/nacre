# Nacre

> 日本語で読む: [Nacre ドキュメント](ja/index.html)

<img class="nacre-cover" src="assets/nacre-compiler.png" alt="Structured Nacre source flowing through compiler layers into a shell script" />

<p class="nacre-lede">
Nacre is a typed, script-oriented language that compiles to a standalone Bash
script. It keeps executable and filesystem authority in a separately reviewed
policy instead of granting that authority to source code.
</p>

## Choose a Path

<div class="nacre-links">
  <a href="tutorial.html">Build your first program</a>
  <a href="language-reference.html">Browse the language reference</a>
  <a href="examples.html">Explore verified examples</a>
  <a href="security-policy.html">Understand the security model</a>
</div>

## A Small Example

```nacre
fn greet(name: String): String {
    return "Hello, ${name}"
}

const message = greet("Nacre")
run.output.echo(message)
```

The command is not selected by this source. A policy maps
`run.output.echo` to one reviewed executable:

```toml
[command_groups.output.commands.echo]
program = "bin/echo"
args = 1
```

Compile the program with:

```bash
cargo run -- --policy policy.toml input.ncr output.sh
bash output.sh
```

## What Is Implemented

The current compiler supports:

- Immutable and mutable bindings with static type checking.
- Primitive values, options, results, arrays, maps, tuples, and records.
- Functions, generics, traits, lambdas, newtypes, and sum types.
- `if`, `while`, `for`, and exhaustive `match` expressions.
- Modules and the bundled `std` modules.
- Policy-approved commands and guarded filesystem operations.
- A Rust library API and command-line compiler.

Arbitrary shell commands, pipelines, redirects, raw Bash, background commands,
and `require` statements are rejected by the safe profile. See
[Current Limitations](limitations.md) for other constraints.

## Documentation Status

Every program on the [Verified Examples](examples.md) page is compiled and
executed by `scripts/verify-docs.sh`. The English and Japanese books are built
from separate sources and indexed together with language-aware search, so each
has correct navigation, search results, and document language.

All code in this repository was developed by a Coding Agent. See
[Development Attribution](development-attribution.md).
