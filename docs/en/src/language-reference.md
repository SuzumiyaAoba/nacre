# Language Reference

This page describes the currently implemented safe language profile. The
[Language Design](syntax.md) page explains the broader design principles.

## Source Files

Nacre source uses UTF-8 text and the `.ncr` extension. Newlines terminate most
statements. Blank lines, shebang lines, and comments beginning with `##` are
accepted.

```nacre
#!/usr/bin/env nacre

## Values are immutable unless declared with `let`.
const project = "Nacre"
let revision = 1
revision = revision + 1
```

Blocks use braces. Indentation is not syntactically significant, but four
spaces per level is the documented style.

## Bindings and Primitive Types

`const` creates an immutable binding. `let` creates a mutable binding.
Annotations are optional when the type can be inferred.

```nacre
const name: String = "Nacre"
let count: Int = 1
count = count + 1

const enabled: Bool = true
const ratio: Float = 1.5
const path: Path = "input/data.txt"
const status: ExitCode = 0
const nothing: Unit = ()
```

Explicit casts support `String`/`Path` conversion, numeric widening where
allowed, and newtype conversion.

## Structured Values

```nacre
const names: [String] = ["Ada", "Grace"]
const ports: Map[String, Int] = { "http": 80, "https": 443 }
const endpoint: (String, Int) = ("localhost", 8080)
const user: { name: String, age: Int } = {
    name: "Ada",
    age: 36
}
```

Arrays, tuples, and records can be destructured:

```nacre
const (host, port) = endpoint
const { name, age } = user
const [first, second, ...rest] = names
```

Common collection operations include `.len()`, `.isEmpty()`, `.map(...)`,
`.contains(...)`, `.slice(...)`, `.keys()`, `.values()`, `.set(...)`, and
`.remove(...)`. Availability depends on the receiver type.

## Strings and Paths

Strings support interpolation and triple-quoted multiline values:

```nacre
const name = "Nacre"
const greeting = "Hello, ${name}"
const message = """
first line
second line
"""
const literal = r"backslashes stay \n literal"
```

String and Path values provide operations such as `.len()`, `.isEmpty()`,
`.slice(...)`, `.contains(...)`, `.trim()`, `.replace(...)`, `.basename()`,
`.dirname()`, `.stem()`, and `.extname()`.

## Functions

```nacre
fn greet(name: String, prefix: String = "Hello"): String {
    return "${prefix}, ${name}"
}

fn firstLabel(prefix: String, values: ...String): String {
    return "${prefix}: ${values[0]}"
}

const defaultGreeting = greet("Nacre")
const customGreeting = greet("Nacre", "Hi")
```

Nacre supports generic functions, trait bounds, function values, expression
lambdas, and UFCS-style calls:

```nacre
fn identity[T](value: T): T {
    return value
}

fn decorate(value: String): String {
    return "[${value}]"
}

const names = ["Ada", "Grace"]
const decorated = names.map(decorate)
```

## Options and Results

Options use `T?` or `Option[T]`. Results use `Result[T, E]` or `T \/ E`.

```nacre
const present: String? = Some("value")
const missing: String? = None
const fallback = missing.orElse(Some("fallback"))

const ok: Result[Int, String] = Ok(7)
const error: Result[Int, String] = Err("invalid")
const incremented = ok.map(value => value + 1)
```

`.map(...)`, `.ap(...)`, `.flatMap(...)`, and lazy `.orElse(...)` are
available where their types apply. `do { ... }` expressions support `<-`
bindings and context-directed `pure(...)`.

## Control Flow

```nacre
let count = 3

while count > 0 {
    count = count - 1
}

for name in ["Ada", "Grace"] {
    const length = name.len()
}

const label = if count == 0 {
    "done"
} else {
    "pending"
}
```

`if`, `else if`, `else`, `while`, `for`, `break`, and `continue` are
implemented. Bare blocks create a static scope. `break` and `continue` are
valid only inside loops, and statements after an unconditional control-flow
exit are rejected as unreachable.

Every path through a non-`Unit` function must return a value. A final expression
is an implicit return, including when earlier branches return explicitly.

## Pattern Matching

`match` supports literal, wildcard, tuple, record, option, result, and sum-type
patterns. The checker verifies exhaustiveness for supported closed types.

```nacre
type Message = Text(String) | Pair(Int, Int) | Empty

fn describe(message: Message): String {
    return match message {
        Text(text) if !text.isEmpty() => text,
        Pair(left, right) => "${left}:${right}",
        Empty => "empty",
        _ => "blank"
    }
}
```

## Types, Traits, and Modules

```nacre
type Identifier = Int
newtype UserId = Int

trait Show[T] {
    fn show(value: T): String
}

impl Show[Int] {
    fn show(value: Int): String {
        return "Int(${value})"
    }
}
```

Import modules with `use`:

```nacre
use std.path

const file: Path = "/tmp/archive.tar.gz"
const extension = path.extname(file)
```

Imported declarations are namespaced. Non-`std` modules resolve only relative to
the importing file. Bundled modules include `std.cli`, `std.fs`, `std.io`,
`std.json`, `std.log`, `std.path`, `std.process`, `std.str`, and `std.test`.

## Environment and Arguments

```nacre
const shell = env.SHELL ?? "/bin/sh"
const home = process.env("HOME")
const arguments: [String] = args
```

Environment values and command-line arguments are untrusted data. They remain
single arguments when passed to an approved command. Environment variable names
must be listed in the execution policy; `process.env(...)` accepts only a static
string literal name. Command-line arguments require `[process] args = true` in
the execution policy.

## Approved Commands

```nacre
const version = run.inspect.version()
run.output.echo("version: ${version}")

const inspected: CommandOutput = run.result.inspect.version()
const status: ExitCode = inspected.status
const stderr: String = inspected.stderr
```

The name must have the static form `run.<group>.<command>`. The compiler
resolves it through an [Execution Policy](security-policy.md). Commands return
captured standard output as `String`. Use `run.result.<group>.<command>` when
failure should be handled as data. That form returns a `CommandOutput` record
with `stdout: String`, `stderr: String`, `status: ExitCode`, and
`success: Bool`.

## Operators

Implemented operators include:

- Arithmetic: `+`, `-`, `*`, `/`, `%`
- Concatenation: `++`
- Comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
- Boolean: `!`, `&&`, `||`
- Bitwise: `&`, `|`, `^`, `~`, `<<`, `>>`
- Applicative/monadic aliases: `<$>`, `<*>`, `>>=`, `<|>`
- Default extraction: `??`

Parentheses control grouping.

## Rejected Syntax

The safe profile rejects:

- `$sh"..."`, `$sh'...'`, and `$sh{ ... }`
- Raw Bash blocks
- Shell pipelines and redirects
- Background, asynchronous, or spawned shell commands
- `hasCommand(...)`, `require(...)`, and `requireOneOf(...)`

Use a narrowly scoped, reviewed executable in the policy instead.
