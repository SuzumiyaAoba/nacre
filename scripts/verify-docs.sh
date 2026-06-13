#!/usr/bin/env bash
set -euo pipefail

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

for source in docs/examples/*.ncr; do
  name="$(basename "$source" .ncr)"
  output="$tmpdir/$name.sh"
  cargo run --quiet -- "$source" "$output"
  bash "$output" >/dev/null
done

test -f docs/site/index.html
test -f docs/site/styles.css
