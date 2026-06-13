# Testing and Coverage

## Test Suite

Run:

```bash
cargo test
```

The suite covers:

- Parser success and error cases.
- Bash emission for variables, expressions, commands, and raw blocks.
- CLI stdout, file output, usage errors, read errors, and write errors.
- Public API accessors.
- Self-compilation of `bootstrap/self.ncr`.

## Documentation Examples

Run:

```bash
scripts/verify-docs.sh
```

This compiles and executes every `.ncr` file in `docs/examples/`.

## Documentation Site

Build the GitHub Pages site locally:

```bash
nix run .#mdbook -- build
```

The generated site is written to `site/`. GitHub Actions runs the same command
and deploys the artifact whenever the repository's default branch is updated.

## Coverage Gate

Run:

```bash
scripts/coverage.sh
```

For a complete Nix-provided environment, run:

```bash
nix develop path:.
scripts/coverage.sh
```

Or run the gate directly:

```bash
nix develop path:. -c scripts/coverage.sh
```

The development shell provides nightly Rust, `llvm-tools-preview`, and
`cargo-llvm-cov`. The script also supports a rustup-managed nightly toolchain
outside Nix. It enforces:

- 100% line coverage.
- 100% function coverage.

`flake.lock` pins the Nix inputs. `rust-toolchain.toml` provides the equivalent
rustup configuration.
