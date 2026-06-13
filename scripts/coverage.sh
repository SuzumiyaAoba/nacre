#!/usr/bin/env bash
set -euo pipefail

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  echo "coverage requires cargo-llvm-cov" >&2
  exit 1
fi

cargo_args=()
if command -v rustup >/dev/null 2>&1; then
  cargo_args+=(+nightly)
elif [[ "$(rustc --version)" != *nightly* ]]; then
  echo "coverage requires a nightly Rust toolchain" >&2
  exit 1
fi

cargo "${cargo_args[@]}" llvm-cov \
  --all-targets \
  --ignore-filename-regex '/tests/' \
  --summary-only \
  --fail-under-lines 100 \
  --fail-under-functions 100
