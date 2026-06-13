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

mdbook build

test -f site/index.html
test -f site/assets/nacre-compiler.png
test -n "$(find site/theme -maxdepth 1 -name 'nacre-*.css' -print -quit)"
