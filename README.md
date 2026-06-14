# Nacre

Nacre is a small, self-hosting language experiment that compiles a typed,
script-oriented syntax into Bash.

This repository currently contains:

- A Rust compiler library and CLI.
- A grammar-driven parser built with `rust-peg`.
- Documentation for the implemented language subset.
- An mdBook site deployed through GitHub Pages.
- Tests and coverage gates for the compiler.

## Quick Start

```bash
cargo run -- --policy docs/examples/policy.toml docs/examples/hello.ncr /tmp/hello.sh
bash /tmp/hello.sh
```

Commands and filesystem operations require an externally reviewed TOML policy:

```bash
cargo run -- --policy nacre-policy.toml input.ncr output.sh
```

## Documentation

- Published site: [suzumiyaaoba.com/nacre](https://suzumiyaaoba.com/nacre/)
- English: [docs/en/src/index.md](docs/en/src/index.md)
- 日本語: [docs/ja/src/index.md](docs/ja/src/index.md)
- Verified examples: [docs/examples](docs/examples)

The English and Japanese books contain equivalent guides, references, security
documentation, and project information.

## Development Attribution

All code in this repository was developed by a Coding Agent.

## Verification

```bash
nix develop path:. -c scripts/check.sh
nix develop path:. -c scripts/verify-docs.sh
nix develop path:. -c scripts/build-docs.sh
nix develop path:. -c scripts/coverage.sh
```

`scripts/check.sh` enforces formatting, Clippy, and the full test suite.
`scripts/verify-docs.sh` compiles and runs every documentation example, builds
both languages, and validates generated links and code formatting.
`scripts/coverage.sh` enforces non-regression floors of 48% line coverage and
66% function coverage. `nix develop path:.` provides the pinned nightly Rust
toolchain, `llvm-tools-preview`, and `cargo-llvm-cov`.
