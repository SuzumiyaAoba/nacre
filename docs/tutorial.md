# Tutorial

This tutorial walks through the implemented Nacre subset. Each example is also
available under `docs/examples/` and is checked by `scripts/verify-docs.sh`.

## 1. Compile a Script

Create `hello.ncr`:

```nacre
const name = "Nacre"
try $sh"echo Hello from ${name}"
```

Compile to stdout:

```bash
cargo run -- docs/examples/hello.ncr
```

Compile to a file and run it:

```bash
cargo run -- docs/examples/hello.ncr /tmp/hello.sh
bash /tmp/hello.sh
```

The generated Bash starts with:

```bash
#!/usr/bin/env bash
set -euo pipefail
```

## 2. Use Variables

```nacre
const project = "nacre"
let count = 1
count = count + 2 * 3
try $sh"echo ${project}: ${count}"
```

`const` emits `readonly`, while `let` and reassignment emit normal shell
assignments. The compiler checks that reassignment keeps the original inferred
type.

## 3. Read Environment Variables

```nacre
const shell = env.SHELL ?? "/bin/sh"
try $sh"echo Shell: ${shell}"
```

Environment access must use a default value. `env.SHELL ?? "/bin/sh"` compiles
to Bash parameter expansion:

```bash
"${SHELL:-/bin/sh}"
```

## 4. Run Commands

Use `$sh` for direct command emission:

```nacre
$sh'echo direct command'
```

Quoted commands can span lines, so Bash heredocs can be written directly:

```nacre
const message = $sh"cat <<EOF
hello ${name}
EOF"
```

Use `try $sh` when a non-zero exit should stop the generated script:

```nacre
try $sh"test -n ${SHELL}"
```

## 5. Use Pipelines

```nacre
const captured = $sh"printf pipeline" |> $sh"cat"
try $sh"echo ${captured}"
```

Pipeline stages are written as `$sh` commands and compile to Bash pipelines.

## 6. Redirect Output

```nacre
$sh"printf write" >> write("/tmp/out.txt")
$sh"printf append" >> append("/tmp/out.txt")
$sh"sh -c 'printf err >&2'" >> append("/tmp/out.txt", stderr = "/tmp/err.txt")
```

Use `write` for truncating output and `append` for appending output. Optional
`stderr` redirects standard error separately.

## 7. Use Raw Bash When Needed

The current implementation intentionally keeps the language small. Use `raw`
for Bash that should be copied into the output verbatim:

```nacre
raw {
echo "inside raw bash"
}
```

`raw` blocks are also how `bootstrap/self.ncr` implements the generated
self-compiler.

## 8. Use Control Flow

```nacre
const names: [String] = ["alice", "bob"]
for name in names {
try $sh"echo ${name}"
}

let count = 2
const label = if count > 0 { "positive" } else { "zero" }
try $sh"echo ${label}"
const matched = match label { "positive" => "matched", _ => "fallback" }
try $sh"echo ${matched}"
while count > 0 {
count = count - 1
}
```

`if`, `while`, and `for` conditions and iterables are checked statically. `if`
and `match` can also be used as expressions when all branches return the same
type.

## 9. Define Functions

```nacre
fn greet(name: String, prefix: String = "Hello"): String {
return "${prefix}, ${name}"
}

const message = greet("Nacre")
try $sh"echo ${message}"
```

Function arguments and return values are checked against the declared types.

## 10. Verify the Tutorial Examples

Run:

```bash
scripts/verify-docs.sh
```

This compiles every `.ncr` file under `docs/examples/` and runs the generated
Bash scripts.
