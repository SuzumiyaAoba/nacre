#!/usr/bin/env bash
set -euo pipefail

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

for source in docs/examples/*.ncr; do
  name="$(basename "$source" .ncr)"
  output="$tmpdir/$name.sh"
  printf 'Verifying %s\n' "$source"
  cargo run --quiet -- --policy docs/examples/policy.toml "$source" "$output"
  bash "$output" >/dev/null
done
rm -f docs/examples/workspace/output.txt

scripts/build-docs.sh
node scripts/verify-docs.mjs
node scripts/verify-search.mjs
