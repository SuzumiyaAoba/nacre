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

Environment variable reads, commands, and filesystem operations require an
externally reviewed TOML policy:

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

## Compiler Structure

`src/lib.rs` coordinates parsing, module expansion, type checking and lowering,
and Bash generation. The public library API is re-exported from this file.

- `src/parser/` separates the PEG grammar, AST construction, validation, and
  diagnostics. `src/ast.rs` defines the shared syntax and types.
- `src/module_loader/` handles package manifests and lockfiles separately from
  imported symbol namespacing.
- `src/checker/` groups semantic checks and lowering by language feature. The
  parent module owns checker state and statement dispatch.
- `src/lowering.rs` resolves method-call syntax before `src/emitter/` generates
  Bash using modules for control flow, values, collections, commands, and runtime
  support.
- `src/symbols.rs` and `src/value_layout.rs` keep naming and shell storage rules
  consistent between checking and generation. `src/policy.rs` validates execution
  capabilities, and `src/error.rs` formats diagnostics.

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
