# CLI Reference

The `nacre` binary compiles `.ncr` source to Bash.

## Usage

```bash
nacre <input.ncr> [output.sh]
```

In this repository, run it through Cargo:

```bash
cargo run -- <input.ncr>
cargo run -- <input.ncr> <output.sh>
```

## Compile to Stdout

```bash
cargo run -- docs/examples/hello.ncr
```

## Compile to a File

```bash
cargo run -- docs/examples/hello.ncr /tmp/hello.sh
bash /tmp/hello.sh
```

## Exit Behavior

The CLI exits successfully when compilation and optional file writing succeed.

It exits with failure when:

- The argument count is not `1` or `2`.
- The input file cannot be read.
- The source cannot be parsed.
- The output file cannot be written.

Examples:

```bash
cargo run --
cargo run -- missing.ncr
```
