# Nacre

Nacre is a small, self-hosting language experiment that compiles a typed,
script-oriented syntax into Bash.

This repository currently contains:

- A Rust compiler library and CLI.
- A grammar-driven parser built with `rust-peg`.
- A verified bootstrap source at `bootstrap/self.ncr`.
- Documentation for the implemented language subset.
- An mdBook site deployed through GitHub Pages.
- Tests and coverage gates for the compiler.

## Quick Start

```bash
cargo run -- docs/examples/hello.ncr
cargo run -- docs/examples/hello.ncr /tmp/hello.sh
bash /tmp/hello.sh
```

## Documentation

- Published site: [suzumiyaaoba.com/nacre](https://suzumiyaaoba.com/nacre/)
- Start here: [docs/index.md](docs/index.md)
- Tutorial: [docs/tutorial.md](docs/tutorial.md)
- Language reference: [docs/language-reference.md](docs/language-reference.md)
- CLI reference: [docs/cli.md](docs/cli.md)
- Self-compilation: [docs/self-compilation.md](docs/self-compilation.md)
- Testing and coverage: [docs/testing-and-coverage.md](docs/testing-and-coverage.md)
- Current limitations: [docs/limitations.md](docs/limitations.md)

The long-form syntax design draft is kept at [docs/syntax.md](docs/syntax.md).
It describes planned language features beyond the current implementation.

## Development Attribution

All code in this repository was developed by a Coding Agent.

## Verification

```bash
nix develop path:. -c cargo test
nix develop path:. -c scripts/verify-docs.sh
nix run .#mdbook -- build
nix develop path:. -c scripts/coverage.sh
```

`scripts/coverage.sh` enforces non-regression floors of 75% line coverage and
90% function coverage. `nix develop path:.` provides the pinned nightly Rust
toolchain, `llvm-tools-preview`, and `cargo-llvm-cov`.
