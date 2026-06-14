# Testing and Coverage

The repository has separate gates for compiler behavior, documentation, and
coverage.

## Compiler Checks

```bash
nix develop path:. -c scripts/check.sh
```

This command runs:

1. `cargo fmt -- --check`
2. Clippy for all targets and features with warnings denied
3. The complete all-target test suite

The tests cover parsing, type checking, Bash emission, public APIs, CLI
behavior, policy validation, runtime path guards, modules, and representative
language programs.

## Documentation Verification

```bash
nix develop path:. -c scripts/verify-docs.sh
```

The script:

1. Compiles every `.ncr` file under `docs/examples/`.
2. Executes each generated Bash script with the example policy.
3. Builds the English and Japanese books.
4. Builds a language-aware Pagefind index.
5. Checks required pages, shared assets, language metadata, links, and
   formatted code.
6. Executes representative English and Japanese searches against the index.

The generated site is written to `site/`.

To build the books without executing examples:

```bash
nix develop path:. -c scripts/build-docs.sh
```

## Coverage Gate

```bash
nix develop path:. -c scripts/coverage.sh
```

The default non-regression floors are:

- 48% line coverage.
- 66% function coverage.

Override them locally with `COVERAGE_MIN_LINES` and
`COVERAGE_MIN_FUNCTIONS`.

## Toolchain

`flake.lock` pins the Nix inputs. `rust-toolchain.toml` provides the equivalent
rustup configuration with Rustfmt, Clippy, and LLVM tools. The Nix development
shell also provides mdBook, Node.js, and Pagefind Extended for documentation
verification.

GitHub Actions builds the same bilingual site before publishing GitHub Pages.
