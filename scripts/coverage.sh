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

minimum_line_coverage="${COVERAGE_MIN_LINES:-75}"
minimum_function_coverage="${COVERAGE_MIN_FUNCTIONS:-90}"

cargo "${cargo_args[@]}" llvm-cov \
  --all-targets \
  --ignore-filename-regex '/tests/' \
  --summary-only \
  --fail-under-lines "$minimum_line_coverage" \
  --fail-under-functions "$minimum_function_coverage"
