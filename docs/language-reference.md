# Language Reference

This page describes the language supported by the current Rust implementation.
The larger planned design is in `docs/syntax.md`.

## Files

Nacre source files use the `.ncr` extension.

The compiler ignores:

- Blank lines.
- Lines starting with `##`, and `##` trailing comments outside strings and
  `$sh{ ... }` shell fragments.
- Lines starting with `#!`.

Statements can appear at the top level and inside supported block statements.

## Identifiers

Variable names must start with `_` or an ASCII letter. The rest of the name may
contain `_`, ASCII letters, or ASCII digits.
Function names that coincide with Bash reserved words, such as `select`, are
automatically assigned a safe generated Bash name.

Valid:

```nacre
const name = "Nacre"
let count_1 = 1
```

Invalid:

```nacre
const bad-name = 1
```

## Statements

### `const`

```nacre
const name = "Nacre"
```

Emits:

```bash
readonly name='Nacre'
```

`const` bindings are immutable. Assigning to a `const` name is a compile error.

### `let`

```nacre
let count = 1
```

Emits:

```bash
count=1
```

`let` bindings are mutable when reassigned with the same inferred type.

### Reassignment

```nacre
count = count + 1
```

Emits:

```bash
count=$(awk -v __nacre_0="$count" 'BEGIN { print ((__nacre_0 + 1)) }')
```

The target variable must already exist, must be mutable, and the assigned
expression must have the same inferred type.

### Command

```nacre
$sh'echo ok'
$sh{ printf '%s\n' "ok" }
const message = $sh"cat <<EOF
hello ${name}
EOF"
const optional = $sh"hostname"?
const fallback = $sh"hostname" ?? "unknown"
```

Emits:

```bash
echo ok
printf '%s\n' "ok"
```

Quoted `$sh"..."` and `$sh'...'` commands are convenient for compact commands.
They may span physical lines, including Bash heredocs. Heredoc body lines are
preserved verbatim, including lines beginning with `##`, and Nacre
interpolation such as `${name}` remains available. The quote matching the
`$sh` opener terminates the command, so escape that quote when it is part of
the shell fragment.
Braced `$sh{ ... }` commands accept a raw Bash fragment with nested quotes,
braces, and shell syntax without Nacre string escaping.
Use postfix `?` to convert command output to `String?`, returning `None` when
the command fails. Use `??` to replace command failure with a fallback string.

### Script Arguments

```nacre
const first = args[0]
const count = args.len()
const [command, ...rest] = args
const processArgs = process.args()
const home = process.env("HOME")
const hasGit = process.hasCommand("git")
const uname = process.exec("uname -s")
const here = process.cwd()
process.chdir("/tmp")
```

`args` is a built-in `[String]` array initialized from the script's command-line
arguments (`"$@"` in Bash). `process.args()` returns the same array when
`std.process` is imported. `process.env(name)` reads an environment variable by
runtime name and returns an empty string for missing or invalid names.
`process.hasCommand(command)` checks whether a command is available on `PATH`.
`process.exec(command)` runs a dynamic Bash command and returns stdout.
`process.cwd()` returns the current working directory, and
`process.chdir(path)` changes it for subsequent commands.

### Checked Command

```nacre
try $sh"test -n ${HOME}"
try $sh{ test -n "${HOME:-}" }
const filtered = try ("alpha\nbeta\n" |> $sh"grep beta")
try ("alpha\nbeta\n" |> $sh"grep beta")
const stored: String \/ CmdError = try $sh"false"
fn fetch(): String \/ CmdError {
try $sh"test -n ${HOME}"
return "ready"
}
```

Emits:

```bash
test -n ${HOME} || exit $?
```

At top level and in `Unit` functions, `try` exits when the command fails. In a
function returning a result whose error type accepts `CmdError`, a failing
`try` returns `Err({ code, stderr })` from that function instead.
When a checked command or pipeline is explicitly bound or returned as
`String \/ CmdError`, failures are stored as `Err({ code, stderr })` values
instead of exiting.
Checked pipelines may start with a `$sh` stage, or with a `String`/`Path`
expression in parentheses as standard input for the first command. They can be
bound as checked `String` expressions or emitted as statements.

### Pipeline

```nacre
const result = $sh"printf pipeline" |> $sh"cat"
const rawResult = $sh{ printf '%s' "pipeline" } |> $sh{ cat }
const filtered = "alpha\nbeta\n" |> $sh"grep beta"
$sh"printf direct" |> $sh"cat"
```

Pipeline command stages must be `$sh` commands. The first stage may also be a
`String` or `Path` expression, which is written to the command's standard input.
Bound pipelines capture stdout, while top-level command pipelines emit a direct
Bash pipeline.

### Redirect

```nacre
$sh"printf write" >> write("/tmp/out.txt")
$sh"printf append" >> append("/tmp/out.txt")
$sh"cmd" >> write("/tmp/out.txt", stderr = "/tmp/err.txt")
```

`write` emits `>`, and `append` emits `>>`. Add `stderr = "path"` to redirect
standard error with `2>` or `2>>`. The redirect source must be a `$sh` command
or pipeline.

### Raw Bash

```nacre
raw {
echo raw
}
```

Everything between `raw {` and the closing `}` is copied to the generated Bash.
Nested `raw { ... }` delimiters are matched before the outer raw block closes.

### Blocks

```nacre
{
const label = "inside"
$sh"printf ${label}"
}
```

Bare `{ ... }` blocks group statements and create a static scope boundary.
Bindings declared inside the block are not available after the block.

### `if`

```nacre
if count > 0 {
$sh'echo positive'
} else if count == 0 {
$sh'echo zero'
} else {
$sh'echo negative'
}
```

Conditions must have type `Bool`.

### `if` Expression

```nacre
const status = if count > 0 { "positive" } else { "zero" }
const label = if count == 0 { "zero" } else if count == 1 { "one" } else { "many" }
```

All branches must have matching types. The final `else` branch is required.

### `match` Expression

```nacre
const status = match code { 200 => "ok", _ => "other" }
const name = match maybeName { Some(value) => value, None => "unknown", _ => "fallback" }
const result = match response { Ok(value) => value, Err(error) => error, _ => "fallback" }
const exitCode = match $sh"false" { Err(error) => error.code, _ => 0 as ExitCode }
const stderr = match $sh"printf error >&2; exit 1" { Err({ stderr }) => stderr, _ => "" }
const stored: String \/ CmdError = $sh"false"
const storedCode = match stored { Err(error) => error.code, _ => 0 as ExitCode }
fn fetch(): String \/ CmdError {
try $sh"test -n ${HOME}"
$sh"printf body"
}
const fetched = fetch() ?? ""
const size = match Some(count) { Some(value) if value > 10 => "large", _ => "small" }
const route = match (code, method) { (200, "GET") => "ok", (_, "DELETE") => "delete", _ => "other" }
const matchedMethod = match (code, method) { (200, name) => name, _ => "unknown" }
const pair = (code, method)
const pairMethod = match pair { (200, name) => name, _ => "unknown" }
const userName = match user { { name } => name, _ => "unknown" }
const body = match response { Ok({ status, body }) if status == 200 => body, _ => "" }
const tupleBody = match maybePair { Some((200, body)) => body, _ => "" }
```

Literal patterns are compared with equality against the matched value.
`Some(value)`, `Ok(value)`, and `Err(value)` constructor patterns bind the
payload for that arm. `Err(error)` on a direct command, pipeline match, or a
command value annotated as `String \/ CmdError` exposes `error.code` and
`error.stderr`. Functions returning `String \/ CmdError` can use a final command
or pipeline as the return value, and a plain success value is wrapped as `Ok`.
`try $sh"..."` inside those functions returns `Err({ code, stderr })` on
failure. `None` matches an empty option. Tuple expression values and
tuple variables can be matched with tuple patterns; identifiers bind tuple
elements, and `_` ignores a tuple element. Record values can be matched with
record patterns; `{ name }` binds the field, and `{ name: "Ada" }` compares it.
Tuple and record patterns can also appear inside `Some`, `Ok`, and `Err`
constructor patterns.
Guards can be added with `if condition`, and all arm expressions must have
matching types. A wildcard `_` arm is required for open-ended types such as
`Int` and `String`. It may be omitted when all cases of `Bool`, `Option`,
`Result`, or a user-defined sum type are covered by unguarded arms. Guarded arms
do not count toward exhaustiveness because their conditions may be false.
Non-exhaustive diagnostics list the missing cases.

### `while`, `break`, and `continue`

```nacre
while count > 0 {
count = count - 1
continue
}
```

`break` and `continue` emit the matching Bash loop control statements.

### `for`

```nacre
const names = ["alice", "bob"]
for name in names {
$sh"echo ${name}"
}
```

The iterable expression must be an array. The loop variable is scoped to the
loop body and must not shadow an existing name.

### `require` and `requireOneOf`

```nacre
require("git")
require("git", version = ">= 2.25")
requireOneOf(["curl", "wget"])
```

These statements emit command availability checks and exit with status `127` if
the requirement is not met. `require(..., version = "...")` runs
`command --version` and compares the first version-like number. Version
requirements support `>=`, `<=`, `>`, `<`, `=`, and `==`; a requirement without
an operator is matched as a substring of the version output.

### `fn` and `return`

```nacre
fn greet(name: String, prefix: String = "Hello"): String {
return "${prefix}, ${name}"
}

const message = greet("Nacre")
greet("Nacre")

fn join(label: String, parts: ...String): String {
return "${label}:${parts.len()}"
}

const joined = join("items", "a", "b")
```

Function parameters require type annotations. Parameters with defaults are
optional at call sites, and required parameters cannot follow default
parameters. A final `name: ...Type` parameter accepts the remaining call
arguments and is available inside the function as `[Type]`. Rest parameters
cannot have defaults or follow default parameters. Non-`Unit` functions must use
`return`, except that functions returning `String \/ CmdError` may end with a
command or pipeline as an implicit return. Function calls can be bound as values
or emitted as statements.

### Method Calls

```nacre
fn exclaim(value: String): String {
return "${value}!"
}

const name = "Nacre"
const loud = name.exclaim()
```

Method calls are UFCS-style syntax for ordinary functions. `value.method(arg)`
is checked and emitted as `method(value, arg)`. Qualified function calls from
modules, such as `fs.exists("/tmp")`, are preserved when the qualified function
exists.

### `trait` and `impl`

```nacre
trait Show[T] {
fn show(value: T): String
}

impl Show[Int] {
fn show(value: Int): String {
return "int ${value}"
}
}

fn identityShown[T: Show](value: T): T {
return value
}

const value = identityShown(7)
const shown = value.show()
const explicit = Show.show(value)
```

Traits can declare method signatures whose first parameter is the receiver type
parameter. Impl blocks must define every declared method with the concrete
receiver type. Generic function type parameters can require one or more traits
with `T: Trait` or `T: TraitA + TraitB`.

Impl methods emit with type-specific Bash function names, so the same method
name can be implemented for different receiver types. If multiple traits
provide the same method name for the same receiver type, use
`TraitName.method(value, ...)` to disambiguate.

### `use`

```nacre
use lib.utils
const label = utils.label("ok")
```

`use a.b.c` resolves `a/b/c.ncr`, `a/b/c.d.ncr`, or `a/b/c/index.ncr` relative
to the importing file. Directories in `NACRE_PATH` are also searched. Imported functions and
top-level bindings are inlined into the generated Bash with the final path
component as a namespace, so `fn label(...)` from `lib/utils.ncr` is called as
`utils.label(...)` and a top-level `const prefix = ...` becomes `utils_prefix`.
Imported `type`, `newtype`, and `trait` declarations use the same namespace in
type annotations, constructors, bounds, and scoped trait calls, such as
`utils.User`, `utils.UserId(1)`, and `utils.Show.show(value)`.
Top-level module declarations whose names start with `_` are private to that
module: public functions in the same module may use them, but importers cannot
call them through the module namespace.

Definition modules can declare external Bash functions without emitting their
bodies:

```nacre
export fn echo(value: String): String
```

External functions cannot declare type parameters or default parameter values.
Calls are type-checked and emitted as calls to the namespaced Bash function.

Standard modules bundled with the compiler can be imported the same way:

```nacre
use std.cli
use std.fs
use std.io
use std.json
use std.log
use std.path
use std.process
use std.str
use std.test
const options = cli.parse()
const data = json.parse("{\"name\":\"Ada\"}")
const ok = fs.exists("/tmp")
const answer = io.prompt("Continue? ")
const name = path.basename("/tmp/nacre.txt")
const clean = str.trim(" nacre ")
log.info("checked /tmp")
test.assert(ok)
```

Implemented standard modules:

- `std.cli`: `parse`
- `std.fs`: `exists`, `isFile`, `isDir`, `size`, `mkdirP`, `remove`, `copy`,
  `move`, `touch`, `createTempDir`, `readText`, `readLines`, `list`,
  `basename`, `dirname`, `stem`, `extname`, `writeText`, `appendText`,
  `writeLines`, `appendLines`
- `std.io`: `prompt`, `confirm`, `promptPassword`
- `std.json`: `parse`, `stringify`
- `std.log`: `info`, `warn`, `error`, `debug`
- `std.path`: `join`, `isAbsolute`, `basename`, `dirname`, `stem`, `extname`
- `std.process`: `args`, `env`, `hasCommand`, `exec`, `cwd`, `chdir`,
  `exit`, `onExit`, `onSignal`
- `std.str`: `split`, `join`, `len`, `isEmpty`, `slice`, `trim`,
  `trimStart`, `trimEnd`, `contains`, `indexOf`, `startsWith`, `endsWith`,
  `toUpper`, `toLower`, `repeat`, `replace`
- `std.test`: `assert`

`cli.parse()` reads the script's command-line arguments and returns
`Map[String, String]`. It supports `--name value`, `--name=value`, and boolean
flags such as `--verbose`, stored as `"true"`.

`json.parse(value)` reads a flat JSON object into `Map[String, String]`.
`json.stringify(map)` writes a `Map[String, String]` as a JSON object string.
It accepts named maps, map literals, and direct `json.parse(...)` results.

`log.info(message)` writes an `INFO` line to stdout. `log.warn(message)` and
`log.error(message)` write `WARN` and `ERROR` lines to stderr.
`log.debug(message)` writes a `DEBUG` line to stderr only when `NACRE_DEBUG=1`.

### `async` and `await`

```nacre
const future = async $sh"printf async"
const output = await future
const job = spawn $sh"printf job"
const jobOutput = job.wait()
```

`async $sh"..."` and `spawn $sh"..."` start a command in the background and
store its captured stdout in a temporary file. `await name` and `name.wait()`
wait for that command, return stdout, remove the temporary file, and exit with
the command status if the background command failed.

## Expressions

### Integers

```nacre
const answer = 42
```

Integers are parsed as signed 64-bit decimal values.

Hexadecimal (`0xFF`) and binary (`0b1010`) integer literals are also supported.

### Floats

```nacre
const pi = 3.14
const doubled = pi * 2
```

Floating-point arithmetic and numeric comparisons are emitted through `awk` so
the generated Bash remains runnable.

### Booleans

```nacre
const yes = true
const no = false
```

Booleans emit `true` or `false`.

### Strings

Both quote styles are supported:

```nacre
const a = "double quoted"
const b = 'single quoted'
```

The compiler strips the Nacre quotes and emits a shell-safe single-quoted
string. Escapes such as `\n`, `\r`, and `\t` are decoded in normal strings.

Triple-quoted multi-line strings are also supported:

```nacre
const message = """
line one
line "two"
"""
```

The opening and closing `"""` delimiters are not part of the value. Newlines
inside the delimiters are preserved.

### Identifiers

```nacre
const copied = answer
```

Identifier expressions emit shell variable references such as `"$answer"`.

### String Methods

```nacre
const text = "nacre"
const count = text.len()
const commandCount = try $sh"printf nacre".len()
const empty = text.isEmpty()
const valueEmpty = ("").isEmpty()
const middle = text.slice(1, 4)
const valueMiddle = ("nacre").slice(1, 4)
const commandMiddle = try $sh"printf nacre".slice(1, 4)
const hasAc = text.contains("ac")
const hasCommandAc = try $sh"printf nacre".contains("ac")
const acIndex = text.indexOf("ac")
const valueIndex = ("nacre").indexOf("cr")
const starts = text.startsWith("na")
const ends = text.endsWith("re")
const clean = text.trim()
const cleanValue = ("  nacre  ").trim()
const cleanCommand = try $sh"printf '  nacre  '".trim()
const cleanLeft = text.trimStart()
const cleanRight = text.trimEnd()
const loud = text.toUpper()
const loudCommand = try $sh"printf nacre".toUpper()
const quiet = loud.toLower()
const tripled = text.repeat(3)
const valueTripled = ("na").repeat(3)
const commandTripled = try $sh"printf na".repeat(3)
const parts = text.split("c")
const commandParts = try $sh"printf 'a\nb\n'".split("\n")
const renamed = text.replace("na", "Na")
const renamedValue = ("nacre").replace("na", "Na")
const renamedCommand = try $sh"printf nacre".replace("na", "Na")
const absolute = text.isAbsolute()
const valueAbsolute = ("/tmp/nacre").isAbsolute()
const commandAbsolute = try $sh"printf /tmp/nacre".isAbsolute()
const base = text.basename()
const valueBase = ("/tmp/nacre.txt").basename()
const commandBase = try $sh"printf /tmp/nacre.txt".basename()
const dir = text.dirname()
const stem = text.stem()
const ext = text.extname()
```

`len()` returns the character count reported by Bash for `String` and `Path`.
`isEmpty()`, `contains(needle)`, `startsWith(prefix)`, and `endsWith(suffix)`
are available for `String` and `Path` values and return `Bool`. String size
checks and predicates can receive named values, parenthesized `String`/`Path`
expressions, and command expressions.
`slice(start, end)` returns the substring for the half-open range from `start`
up to but not including `end` and supports the same receivers as string size
checks. `indexOf(needle)` returns the first matching
zero-based index, or `-1` if the value is not present. `indexOf()` supports the
same receivers as string predicates. `trim()`, `toUpper()`,
`toLower()`, `trimStart()`, and `trimEnd()` return `String`. These unary string
transforms can receive named values, parenthesized `String`/`Path` expressions,
and command expressions.
`repeat(count)` returns the value repeated `count` times and supports the same
receivers as unary string transforms. `split(separator)`
returns `[String]` and can split a named value or a parenthesized `String`/`Path`
expression. Command expressions can also be split directly. `replace(search,
replacement)` returns `String` and supports the same receivers as unary string
transforms.
`isAbsolute()` returns whether the value starts with `/`.
`basename()`, `dirname()`, `stem()`, and `extname()` return `String` path
components for `String` and `Path` values. Path methods can receive named
values, parenthesized `String`/`Path` expressions, and command expressions.

### Environment Defaults

```nacre
const home = env.HOME ?? "/tmp"
```

Rules:

- Environment names must be uppercase ASCII letters, digits, or `_`.
- A default value is required.
- The default value must be a quoted string.

Output:

```bash
readonly home="${HOME:-/tmp}"
```

`env.NAME` without a default is also supported and emits `"${NAME}"`.

### Arrays

```nacre
const names: [String] = ["alice", "bob"]
const first = names[0]
const literalIndexed = (["alice", "bob"])[1]
const count = names.len()
const empty = names.isEmpty()
const literalCount = (["alice", "bob"]).len()
const literalEmpty = ([]).isEmpty()
const csv = names.join(",")
const literalCsv = (["alice", "bob"]).join(",")
const firstName = names.first()
const lastName = names.last()
const literalFirst = (["alice", "bob"]).first()
const literalLast = (["alice", "bob"]).last()
const reversed = names.reverse()
const literalReversed = (["alice", "bob"]).reverse()
const sorted = names.sort()
const literalSorted = (["bob", "alice"]).sort()
const unique = names.unique()
const literalUnique = (["alice", "alice"]).unique()
const upper = names.map(name => name.toUpper())
const literalLengths = (["alice", "bob"]).map(name => name.len())
const hasAlice = names.contains("alice")
const literalHasAlice = (["alice", "bob"]).contains("alice")
const bobIndex = names.indexOf("bob")
const literalBobIndex = (["alice", "bob"]).indexOf("bob")
const subset = names.slice(0, 1)
const literalSubset = (["alice", "bob", "carol"]).slice(1, 3)
const firstTwo = names.take(2)
const literalFirstTwo = (["alice", "bob", "carol"]).take(2)
const afterFirst = names.drop(1)
const literalAfterFirst = (["alice", "bob", "carol"]).drop(1)
const [head, ...tail] = names
let mutableNames = ["alice"]
mutableNames.push("bob")
mutableNames.pop()
```

Array literals must contain values of one compatible element type. Empty arrays
need a type annotation when a concrete element type is required. Indexing can
receive a named array or a parenthesized array literal. `len()` and
`isEmpty()` can receive a named array or a parenthesized array literal. `reverse()`
returns a new array with the elements in reverse order. `sort()` returns a new
lexically sorted array for `[String]` and `[Path]`. `unique()` returns a new
array with duplicate elements removed while preserving first-seen order.
`reverse()`, `sort()`, and `unique()` can receive a named array or a
parenthesized array literal. `map(mapper)` returns a new array by applying a
one-parameter function or lambda to every element. It accepts a
named array or a non-empty parenthesized array literal. `<$>` is an alias for
`map`, such as `names <$> (name => name.toUpper())`. `push(value)` appends to a
mutable array.
`pop()` removes the last element from a mutable array. Both are valid only as
statements.
Array destructuring binds indexed elements from an array literal or variable.
The optional `...rest` binding must appear last and captures the remaining
elements as an array. `contains(value)` checks whether an array contains a
compatible value. `indexOf(value)` returns the first matching index, or `-1` if
the value is not present. `contains(value)` and `indexOf(value)` can receive a
named array or a parenthesized array literal. `first()` and `last()` return the
first or last array element and can receive a named array or a parenthesized
non-empty array literal. `join` is available for `[String]` and `[Path]` arrays
and returns a `String`. `join(separator)` can receive a named array or a
parenthesized array literal. `slice(start, end)` returns a new array for the
half-open range from `start` up to but not including `end`. `take(count)`
returns the first `count` elements, and `drop(count)` returns the remaining
elements after removing the first `count`. `slice(start, end)`, `take(count)`,
and `drop(count)` can receive a named array or a parenthesized array literal.

### Maps

```nacre
const envs: Map[String, String] = { "PORT": "8080" }
const port = envs["PORT"]
const literalPort = ({ "PORT": "8080" })["PORT"]
const envCount = envs.len()
const noEnv = envs.isEmpty()
const literalCount = ({ "PORT": "8080" }).len()
const literalEmpty = ({}).isEmpty()
const envKeys = envs.keys()
const envValues = envs.values()
const hasPort = envs.has("PORT")
const literalKeys = ({ "PORT": "8080" }).keys()
const literalValues = ({ "PORT": "8080" }).values()
const literalHasPort = ({ "PORT": "8080" }).has("PORT")
let mutableEnvs: Map[String, String] = {}
mutableEnvs.set("PORT", "8080")
mutableEnvs.remove("PORT")
```

Map keys and values must each have compatible types. Indexing, `len()`, and
`isEmpty()` can receive a named map or a parenthesized map literal. `keys()`
returns `[K]`, `values()` returns `[V]`, and `has(key)` returns whether the key
exists. These methods can also receive a named map or a parenthesized map
literal. `set(key, value)` inserts or replaces an entry in a mutable map, and
`remove(key)` removes an entry when present. Both mutation methods are valid
only as statements.

### Records

```nacre
const user: { name: String, age: Int } = { name: "Ada", age: 36 }
const name = user.name
const literalName = ({ name: "Grace", age: 37 }).name
const { age } = user
```

Record literals use bare field names. Field names must be unique, and record
annotations must match the available fields and their types.
Field access can receive a named record or a parenthesized record literal.

Record bindings emit one Bash variable per field using the binding name as a
prefix.

Record destructuring binds fields with matching names from a record literal or
record variable.

### Sum Types

```nacre
type LogLevel = Info | Warn | Error
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Label(String)

fn describe(shape: Shape): String {
return match shape {
Circle(radius) => "circle ${radius}",
Rect(width, height) => "rect ${width}x${height}",
Label(text) => text
}
}
```

`type Name = Variant | Variant(T, U)` declares a sum type. Nullary variants are
values, while variants with fields are constructor calls. Variant fields are
statically checked and may contain primitive or structured types such as
arrays, maps, records, tuples, and structured Option or Result values.

`match` patterns bind positional variant fields. The same exhaustiveness rules
used for `Bool`, `Option`, and `Result` apply to sum types. Sum types and their
constructors can be exported through modules.

Generated Bash stores a sum value as one quoted scalar containing the variant
tag and length-prefixed declaration snapshots for its fields. This preserves
structured Bash values, spaces, and newlines while allowing sum values to pass
through function arguments and return values without word splitting.

### Tuples

```nacre
const pair: (String, Int) = ("localhost", 8080)
const host = pair._1
const literalHost = ("localhost", 8080)._1
const (tupleHost, tuplePort) = pair
```

Tuple field access is one-based and uses `._1`, `._2`, and so on. It can receive
a named tuple or a parenthesized tuple literal.
Tuple destructuring binds all tuple elements from a tuple literal or tuple
variable, and the number of names must match the tuple size.

### Options

```nacre
const present: String? = Some("Ada")
const missing: String? = None
const name = present ?? "unknown"
const upper = present.map(value => value.toUpper())
const stillMissing = missing.map(value => value.toUpper())
const validated = present.flatMap(value => if value.isEmpty() { None } else { Some(value) })
const selected = missing.orElse(Some("fallback"))
const selectedAlias = missing <|> Some("fallback")
const transform: Option[String => String] = Some(value => value.toUpper())
const applied = transform.ap(present)
const appliedAlias = transform <*> present
```

`T?` is an optional value. `Some(value)` stores a present value, and `None`
stores no value. The `??` operator unwraps an option with a fallback expression
of the same value type. `map(mapper)` applies a one-parameter function or
lambda to a present value and returns `None` without calling the
mapper when the option is empty. A bare `None` receiver needs an Option type
annotation so the mapper parameter type is known. Lambdas may capture scalar
values from their surrounding scope. `flatMap(mapper)` also
short-circuits `None`, but its mapper returns an Option directly, avoiding a
nested optional value. `orElse(fallback)` keeps a present Option and lazily
evaluates another Option only when the receiver is `None`. `<|>` is its operator
alias. `ap(value)` applies a wrapped one-parameter function to another Option;
it returns `None` without evaluating later work when either side is empty.
`<$>`, `<*>`, and `>>=` are aliases for `map`, `ap`, and `flatMap`.

### Results

```nacre
const ok: String \/ String = Ok("ready")
const err: String \/ String = Err("failed")
const value = ok ?? "fallback"
const optional: String? = ok?
const upper = ok.map(value => value.toUpper())
const next = ok.flatMap(value => Ok("${value}-next"))
const transform: Result[String => String, String] = Ok(value => value.toUpper())
const applied = transform.ap(ok)
const appliedAlias = transform <*> ok
fn runStep(step: String \/ String): String \/ String {
try step
return "done"
}
fn readStep(step: String \/ String): String \/ String {
const value = try step
return "${value}-done"
}
fn readStepWithBang(step: String \/ String): String \/ String {
const value = step!
return "${value}-done"
}
fn decorateStep(step: String \/ String): String \/ String {
return decorate("value: ", step!)
}
fn addStep(step: Int \/ String): Int \/ String {
return step! + 1
}
```

`T \/ E` is a result value. `Ok(value)` stores a successful value, and
`Err(error)` stores an error value. The `??` operator unwraps a result with a
fallback expression of the same success type. The postfix `?` operator converts a
result into an option, preserving `Ok(value)` as `Some(value)` and converting
`Err(error)` to `None`. As a statement inside a Result-returning function,
`try resultValue` continues on `Ok` and returns the same `Err` from the current
function or lambda on failure. In binding and `return` expressions, it unwraps
`Ok(value)` to the success value. Postfix `!` is equivalent to `try` for Result
values and commands. It is valid as a standalone statement, as a binding or
`return` value, and inside eagerly evaluated expressions such as function
arguments, constructors, collection elements, and arithmetic. It also
preserves lazy evaluation inside the right side of `&&` and `||`, `??` and
`<|>` fallbacks, `if` and `match` result branches, and `match` guards. An
unselected branch or guard is not evaluated, and an `Err` from a selected guard
is propagated from the enclosing `match`.
`map(mapper)` transforms only the `Ok` payload while
preserving `Err`. `flatMap(mapper)` expects the mapper to return a Result and
short-circuits an existing `Err` without calling it. `ap(value)` applies a
wrapped one-parameter function to another Result and preserves the first `Err`
encountered. `<$>`, `<*>`, and `>>=` are the corresponding operator aliases.

### Do Expressions

```nacre
const total = do {
left <- Some(2)
const offset: Int = left + 1
right <- Some(3)
pure(offset + right)
}
```

`do { ... }` is expression syntax for a sequence of Option or Result
`flatMap` operations. `name <- expression` unwraps the successful payload for
the remaining steps and short-circuits on `None` or `Err`. The block may contain
`const` or `let` declarations between bindings. These local expressions are
evaluated once and may store primitive or structured values such as arrays,
maps, records, tuples, and Option or Result wrappers.

The final line must be an Option or Result expression. `pure(value)` can be used
as the final line when an earlier `<-` binding establishes whether the block is
an Option or Result expression. All `<-` steps in one block must use the same
container kind. Structured arrays, maps, records, and tuples cannot currently
be stored in an intermediate `const` or `let` declaration.

### Union and Intersection Types

```nacre
const label: String | Int = "ready"
const pathText: String & Path = "/tmp/nacre"
```

`A | B` accepts a value assignable to either member type. `A & B` accepts a
value assignable to every member type. These are type-checking constructs only;
they do not change the generated Bash representation.

### Newtypes

```nacre
newtype UserId = Int
const uid: UserId = UserId(42)
const raw: Int = uid.value
```

Newtypes wrap an existing type and require explicit construction.

The implicit constructor can be overridden in the same module with `fn!`:

```nacre
newtype UserId = Int
fn! UserId(value: Int): UserId \/ String {
if value < 0 {
return Err("negative")
}
return value as UserId
}
const uid = UserId(7) ?? (0 as UserId)
```

`fn!` must name an existing newtype and cannot declare type parameters. Calls to
that newtype constructor use the overriding function's return type.

`as` can also be used for explicit newtype wrapping and unwrapping:

```nacre
const rawId = 42
const uid: UserId = rawId as UserId
const again: Int = uid as Int
```

`as` is also supported between `Path` and `String`, which share the same Bash
representation:

```nacre
const path: Path = "/tmp"
const text: String = path as String
```

### Type Aliases

```nacre
type User = { name: String, age: Int }
const user: User = { name: "Ada", age: 36 }

type Box[T] = { item: T }
const boxed: Box[Int] = { item: 7 }
```

Type aliases name an existing type and can have type parameters. They do not
emit Bash by themselves and do not create a distinct type; use `newtype` when
values must not be mixed with the underlying type.

### Function Types

```nacre
type Unary = String => String

fn exclaim(value: String): String {
return "${value}!"
}

fn applyString(f: Unary, value: String): String {
return f(value)
}

const result = applyString(exclaim, "Hi")
```

Function types use `A => B` for one parameter and `(A, B) => C` for multiple
parameters. Function names can be passed as values and called through a
function-typed parameter or binding.

Expression lambdas use `value => expression` or
`(left, right) => expression`. Their parameter and return types are inferred
from a function type annotation or a typed function parameter:

```nacre
const double: Int => Int = value => value * 2

fn applyInt(f: Int => Int, value: Int): Int {
return f(value)
}

const result = applyInt(value => value + 1, 4)
```

Lambdas capture surrounding scalar values when the lambda is created:

```nacre
fn makeAdder(amount: Int): Int => Int {
return value => value + amount
}

const addTwo = makeAdder(2)
const result = addTwo(5)
```

Captured values remain available after the defining function returns. Capture
is by value, so later reassignment does not alter an existing closure. Arrays,
maps, records, tuples, and Option or Result wrappers with structured payloads
are captured as declaration snapshots and restored for each invocation.

Result-returning lambdas may use `try` and postfix `!` in their bodies. An
`Err` returns from the lambda invocation, not from the function that created or
called the lambda. Mapper lambdas may infer a Result return type from a body
containing `!`.

### Generic Functions

```nacre
fn identity[T](value: T): T {
return value
}

const text = identity("generic")
const number = identity(7)
```

Generic functions declare type parameters in square brackets after the function
name. The checker infers concrete type arguments from call arguments and uses
them for the return type.

## Operators

Arithmetic operators:

```nacre
const value = 1 + 2 * 3
const grouped = (1 + 2) * 3
```

Supported:

- `+`
- `-`
- `*`
- `/`
- `%`

Parentheses group expressions and do not create tuples unless they contain at
least two comma-separated elements.

String concatenation:

```nacre
const label = "na" ++ "cre"
```

`++` accepts `String` and `Path` operands and returns `String`.

Bitwise integer operators:

```nacre
const mask = 6 & 3
const flags = mask | 8
const shifted = flags << 1
const inverted = ~shifted
```

Supported:

- `&`
- `|`
- `^`
- `~`
- `<<`
- `>>`

Bitwise operators require `Int` or `ExitCode` operands and return `Int`.

Comparison operators:

```nacre
const ok = value >= 7
```

Supported:

- `==`
- `!=`
- `<`
- `<=`
- `>`
- `>=`

Logical operators:

```nacre
const ok = hasCommand("git") && !hasCommand("missing-command")
const fallback = ok || pathExists("/tmp")
```

Supported:

- `&&`
- `||`
- `!`

Operator detection ignores operators inside quoted strings.

## Static Checking

The compiler infers one of these implemented types for expressions:

- `Int`
- `Float`
- `Bool`
- `String`
- `Path`
- `ExitCode`
- `CmdError`
- `Unit`
- Options, arrays, maps, records, tuples, type aliases, generic type aliases, function
  types, generic function type parameters, futures, and newtypes

The checker verifies:

- Variables are defined before identifier use.
- Names are not defined twice in the same file, except `_`, which is discarded.
- `const` variables are not reassigned.
- `let` reassignment keeps the original inferred type.
- Arithmetic operators require numeric operands; `%` requires integer operands.
- `++` requires `String` or `Path` operands.
- Bitwise operators require integer operands.
- `==` and `!=` require comparable operand types.
- `<`, `<=`, `>`, and `>=` require numeric operands.
- `&&`, `||`, and `!` require `Bool` operands.
- Block conditions are `Bool`.
- `for` loop iterables are arrays.
- Function arguments and return expressions match the declared types.
- Method-call receivers are checked as the first function argument.
- Generic function bounds reference known marker traits, and calls use types
  with matching `impl` declarations.
- `pathExists(...)` receives `String` or `Path` and returns `Bool`.

Examples:

```nacre
let count = 1
count = count + 1
```

```nacre
const name = "Nacre"
name = "Other" ## compile error
```

## Generated Bash

Every generated file starts with:

```bash
#!/usr/bin/env bash
set -euo pipefail
```

The compiler emits a blank line before each generated statement for readability.

## Errors

Compiler errors include a line number and message:

```text
line 1: invalid variable name `bad-name`
```

Quoted string and command parsing errors report the source line that contained
the invalid syntax.
