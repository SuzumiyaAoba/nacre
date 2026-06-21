# CLI Reference

The `nacre` binary parses, type-checks, and compiles one `.ncr` file to Bash.

## Usage

```text
nacre [--policy policy.toml] [--diagnostic-format text|json] [--write-lock] <input.ncr> [output.sh]
```

From the repository:

```bash
cargo run -- [--policy policy.toml] <input.ncr> [output.sh]
```

Without `--policy`, the compiler uses a deny-all execution policy. Pure
programs compile normally; command and filesystem capabilities do not.

## Write to Standard Output

Omit the output path:

```bash
cargo run -- hello.ncr
```

The generated Bash is written to standard output. Diagnostics are written to
standard error.

## Write to a File

```bash
cargo run -- hello.ncr /tmp/hello.sh
bash /tmp/hello.sh
```

The compiler writes the generated text but does not mark the file executable.
Run it through `bash` or set its permissions explicitly.

## Compile with a Policy

```bash
cargo run -- \
  --policy docs/examples/policy.toml \
  docs/examples/hello.ncr \
  /tmp/hello.sh
```

`--policy` must precede the positional paths. Relative executable and
filesystem paths inside the policy are resolved from the policy file directory.
Source modules are resolved from the importing file's directory, and `std.*`
imports use the bundled standard library. The compiler walks upward from the
input file to find `nacre.toml`; `[dependencies.<name>] path = "..."` entries
resolve local package paths relative to that manifest directory.

## Write a Lockfile

```bash
cargo run -- --write-lock app/main.ncr
```

`--write-lock` writes `nacre.lock` next to `nacre.toml`. When a lockfile exists,
compilation validates path dependency roots and content fingerprints.

## Exit Status

The CLI exits successfully after compilation and optional file writing.
It exits unsuccessfully for:

- Invalid arguments.
- Policy loading or validation errors.
- Input read errors.
- Parse, type, or capability errors.
- Output write errors.

When available, errors include the file name, line, column, source line, and a
caret range. `--diagnostic-format json` writes a single diagnostic as a JSON
object to standard error.

## Library API

Rust callers can use:

```rust
use nacre::{compile_file, compile_file_with_policy};
```

`compile_source` and `compile_source_with_policy` provide the corresponding
in-memory source APIs. All return `Result<String, CompileError>`.
`CompileError` keeps the existing `line()` and `message()` accessors and also
provides `column()`, `end_line()`, `end_column()`, `source_name()`, and
`source_line()`, and `to_json()`.
