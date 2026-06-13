#!/usr/bin/env bash
set -euo pipefail

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

for source in docs/examples/*.ncr; do
  name="$(basename "$source" .ncr)"
  output="$tmpdir/$name.sh"
  cargo run --quiet -- --policy docs/examples/policy.toml "$source" "$output"
  bash "$output" >/dev/null
done
rm -f docs/examples/workspace/output.txt

mdbook build

test -f site/index.html
test -f site/assets/nacre-compiler.png
test -n "$(find site/theme -maxdepth 1 -name 'nacre-*.css' -print -quit)"
