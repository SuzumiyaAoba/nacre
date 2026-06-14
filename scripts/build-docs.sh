#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

rm -rf site
mdbook build docs/en --dest-dir "$repo_root/site"
mdbook build docs/ja --dest-dir "$repo_root/site/ja"

install -d site/assets site/examples
cp docs/assets/nacre-compiler.png site/assets/
find docs/examples -maxdepth 1 -type f \
  \( -name '*.ncr' -o -name '*.toml' \) \
  -exec cp {} site/examples/ \;
touch site/.nojekyll

while IFS= read -r -d '' page; do
  temporary="${page}.tmp"
  awk '{ sub("<main>", "<main data-pagefind-body>"); print }' "$page" >"$temporary"
  mv "$temporary" "$page"
done < <(find site -type f -name '*.html' -print0)

pagefind \
  --site site \
  --glob "**/{index,tutorial,language-reference,cli,security-policy,examples,testing-and-coverage,limitations,syntax,development-attribution}.html"
