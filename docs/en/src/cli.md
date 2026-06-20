# CLI Reference

The `nacre` binary parses, type-checks, and compiles one `.ncr` file to Bash.

## Usage

```text
nacre [--policy policy.toml] <input.ncr> [output.sh]
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
Source modules are resolved from the importing file's directory; only `std.*`
imports use the bundled standard library.

## Exit Status

The CLI exits successfully after compilation and optional file writing.
It exits unsuccessfully for:

- Invalid arguments.
- Policy loading or validation errors.
- Input read errors.
- Parse, type, or capability errors.
- Output write errors.

Errors include the source line when one is available.

## Library API

Rust callers can use:

```rust
use nacre::{compile_file, compile_file_with_policy};
```

`compile_source` and `compile_source_with_policy` provide the corresponding
in-memory source APIs. All return `Result<String, CompileError>`.
