#!/usr/bin/env bash
set -euo pipefail

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

for source in docs/examples/*.ncr; do
  name="$(basename "$source" .ncr)"
  output="$tmpdir/$name.sh"
  error="$tmpdir/$name.err"
  if cargo run --quiet -- "$source" "$output" 2>"$error"; then
    bash "$output" >/dev/null
  else
    grep -Eq '\$sh commands and shell pipelines are disabled|raw Bash blocks are disabled' "$error"
  fi
done

mdbook build

test -f site/index.html
test -f site/assets/nacre-compiler.png
test -n "$(find site/theme -maxdepth 1 -name 'nacre-*.css' -print -quit)"
