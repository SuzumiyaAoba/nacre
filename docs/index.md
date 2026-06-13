# Nacre Documentation

<img class="nacre-cover" src="assets/nacre-compiler.png" alt="Structured Nacre source flowing through pearlescent compiler layers into a shell script" />

<p class="nacre-lede">
Nacre is a Rust-implemented compiler that translates a compact `.ncr` source
file into a standalone Bash script. It combines typed, structured expressions
with externally reviewed command and filesystem capabilities.
</p>

The current implementation is intentionally small and verifiable. It supports
immutable and mutable variables, primitive and structured expressions,
statement-level control flow, policy-approved command execution, and a CLI
compiler. Source syntax is parsed from a declarative PEG grammar using
`rust-peg`. `$sh`, pipelines, redirects, and raw Bash blocks are rejected by the
safe language profile.

## What to Read

<div class="nacre-links">
  <a href="tutorial.html">Start the tutorial</a>
  <a href="language-reference.html">Browse the language reference</a>
  <a href="examples.html">Read verified examples</a>
  <a href="cli.html">Use the compiler CLI</a>
</div>

## Current Scope

Implemented:

- `const`, `let`, and reassignment.
- `Int`, `Float`, `Bool`, `String`, `Path`, `ExitCode`, and `Unit`.
- Options, results, arrays, maps, records, tuples, type aliases, generic type
  aliases, function types, structured-payload sum types, newtypes, type
  annotations, identifiers, `env.NAME`, and `env.NAME ?? "default"`.
- Option `.map(mapper)`, `.ap(value)`, and `.flatMap(mapper)` for
  one-parameter functions and lambdas, plus lazy
  `.orElse(fallback)` / `<|>` selection.
- Result `.map(mapper)`, `.ap(value)`, and `.flatMap(mapper)`, preserving and
  short-circuiting error values.
- `<$>`, `<*>`, and `>>=` aliases for `map`, `ap`, and `flatMap`.
- `do { ... }` expressions with `<-` bindings, local declarations, and
  context-directed `pure(value)` for Option and Result workflows.
- Array, tuple, and record destructuring for `const` and `let` bindings.
- Array and map `.len()` / `.isEmpty()`.
- Array `.first()`, `.last()`, `.reverse()`, `.sort()`, `.unique()`, `.map(mapper)`,
  `.contains(value)`, `.indexOf(value)`, `.slice(start, end)`, `.take(count)`,
  `.drop(count)`, `.push(value)`, `.pop()`, and `.join(sep)` for `[String]`
  and `[Path]`.
- Map `.keys()`, `.values()`, `.has(key)`, `.set(key, value)`, and
  `.remove(key)`.
- String and Path `.len()` / `.isEmpty()` / `.slice(start, end)` /
  `.contains(needle)` / `.indexOf(needle)` / `.trim()` / `.trimStart()` /
  `.trimEnd()` / `.repeat(count)` / `.split(sep)` /
  `.replace(search, replacement)` / `.isAbsolute()` / `.basename()` /
  `.dirname()` / `.stem()` / `.extname()`.
- Built-in `args: [String]` for script command-line arguments.
- Single-line and triple-quoted multi-line string literals.
- Explicit `as` casts for `Path`/`String` and newtypes.
- `+`, `-`, `*`, `/`, `%`, `++`, `&`, `|`, `^`, `~`, `<<`, `>>`,
  `==`, `!=`, `<`, `<=`, `>`, `>=`, `&&`, `||`, and `!`, with
  parenthesized grouping.
- `if`, `else if`, `else`, `while`, `for`, `break`, and `continue`.
- Bare `{ ... }` blocks for statement grouping and static scope boundaries.
- `if condition { then } else if other { other } else { else }` expressions.
- `match` expressions with guards, structured patterns, and exhaustiveness
  checks for `Bool`, Option, Result, and sum types.
- `fn`, typed parameters, default parameters, rest parameters, generic
  functions, `return`, and function calls.
- Expression lambdas with contextual function types and by-value primitive or
  structured captures.
- UFCS-style method calls for functions, where `value.method(arg)` compiles as
  `method(value, arg)`.
- `trait` method declarations, `impl` method definitions, and generic function
  bounds such as `fn id[T: Show](value: T): T`.
- `use` module imports with namespaced function calls.
- Bundled `std.cli`, `std.fs`, `std.io`, `std.json`, `std.log`,
  `std.path`, `std.process`, `std.str`, and `std.test` modules.
- Policy-approved `run.<group>.<command>(...)` calls with fixed executable
  paths and individually quoted arguments.
- Runtime enforcement of read and write roots for filesystem operations and
  policy-designated command arguments.
- `require("cmd")`, `require("cmd", version = ">= 1")`, and
  `requireOneOf(["cmd1", "cmd2"])`.
- Rejection of `$sh`, raw Bash, pipelines, and redirects.
- `##` comments, blank lines, and shebang lines.
- A static checker for the implemented expression and block subset.
- CLI output to stdout or a file.
- Self-compilation check for `bootstrap/self.ncr`.

The broader design is documented separately in the
[language design draft](syntax.md).

## Development Attribution

All code in this repository was developed by a Coding Agent. See
[Development Attribution](development-attribution.md) for details.
