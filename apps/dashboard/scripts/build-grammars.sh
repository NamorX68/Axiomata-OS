#!/usr/bin/env bash
# Builds the editor's tree-sitter grammars to WebAssembly (docs/plans/editor.md, G1).
#
# Every grammar is pinned to a release tag and built with a pinned tree-sitter
# CLI; the CLI fetches the WASI SDK and wasm-opt into ~/.cache/tree-sitter by
# itself, so neither emscripten nor Docker is needed. The output is checked in
# (G9): public/grammars/<name>.wasm plus the grammar's own queries under
# public/grammars/<name>/. Our own query additions live beside them as
# *.local.scm and are never touched here.
#
# Usage: scripts/build-grammars.sh [name ...]   (no names = all)
set -euo pipefail

CLI="tree-sitter-cli@0.27.0"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$HERE/public/grammars"
WORK="${TMPDIR:-/tmp}/axiomata-grammars"

# name | repository | tag | subdirectory holding grammar.js/src ("." = root) | queries directory | commit
#
# The commit is checked after cloning: a tag can be moved upstream, and building runs the
# grammar's own grammar.js, so a moved tag must stop the build rather than slip through.
GRAMMARS=(
  "rust|tree-sitter/tree-sitter-rust|v0.24.2|.|queries|77a3747266f4d621d0757825e6b11edcbf991ca5"
  "javascript|tree-sitter/tree-sitter-javascript|v0.25.0|.|queries|44c892e0be055ac465d5eeddae6d3e194424e7de"
  "typescript|tree-sitter/tree-sitter-typescript|v0.23.2|typescript|queries|f975a621f4e7f532fe322e13c4f79495e0a7b2e7"
  "tsx|tree-sitter/tree-sitter-typescript|v0.23.2|tsx|queries|f975a621f4e7f532fe322e13c4f79495e0a7b2e7"
  "json|tree-sitter/tree-sitter-json|v0.24.8|.|queries|ee35a6ebefcef0c5c416c0d1ccec7370cfca5a24"
  "css|tree-sitter/tree-sitter-css|v0.25.0|.|queries|dda5cfc5722c429eaba1c910ca32c2c0c5bb1a3f"
  "html|tree-sitter/tree-sitter-html|v0.23.2|.|queries|5a5ca8551a179998360b4a4ca2c0f366a35acc03"
  "python|tree-sitter/tree-sitter-python|v0.25.0|.|queries|293fdc02038ee2bf0e2e206711b69c90ac0d413f"
  "bash|tree-sitter/tree-sitter-bash|v0.25.1|.|queries|a06c2e4415e9bc0346c6b86d401879ffb44058f7"
  "markdown|tree-sitter-grammars/tree-sitter-markdown|v0.5.3|tree-sitter-markdown|tree-sitter-markdown/queries|f969cd3ae3f9fbd4e43205431d0ae286014c05b5"
  "markdown_inline|tree-sitter-grammars/tree-sitter-markdown|v0.5.3|tree-sitter-markdown-inline|tree-sitter-markdown-inline/queries|f969cd3ae3f9fbd4e43205431d0ae286014c05b5"
  "toml|tree-sitter-grammars/tree-sitter-toml|v0.7.0|.|queries|64b56832c2cffe41758f28e05c756a3a98d16f41"
  "yaml|tree-sitter-grammars/tree-sitter-yaml|v0.7.2|.|queries|7708026449bed86239b1cd5bce6e3c34dbca6415"
  "lua|tree-sitter-grammars/tree-sitter-lua|v0.5.0|.|queries|10fe0054734eec83049514ea2e718b2a56acd0c9"
  "svelte|tree-sitter-grammars/tree-sitter-svelte|v1.0.2|.|queries|774a65aea563accc35f5d45fafa4d96ec5761f57"
  "swift|alex-pinkus/tree-sitter-swift|0.7.3-with-generated-files|.|queries|31d17fe7e818a2048c808b5c6fdc2dc792f4f5b5"
  "sql|DerekStride/tree-sitter-sql|v0.3.11|.|queries|7b51ecda191d36b92f5a90a8d1bc3faef1c7b8b8"
)

wanted() {
  [ "$#" -eq 0 ] && return 0
  local name="$1"; shift
  for w in "${SELECTED[@]}"; do [ "$w" = "$name" ] && return 0; done
  return 1
}

SELECTED=("$@")
mkdir -p "$OUT" "$WORK"
: > "$OUT/.versions.tmp"

for entry in "${GRAMMARS[@]}"; do
  IFS='|' read -r name repo tag subdir queries commit <<< "$entry"
  if [ "${#SELECTED[@]}" -gt 0 ] && ! wanted "$name" "${SELECTED[@]}"; then
    grep "^$name " "$OUT/VERSIONS" >> "$OUT/.versions.tmp" 2>/dev/null || true
    continue
  fi
  src="$WORK/$(echo "$repo" | tr '/' '_')@$tag"
  if [ ! -d "$src" ]; then
    echo "== fetching $repo@$tag"
    git clone -q --depth 1 --branch "$tag" "https://github.com/$repo" "$src"
  fi
  actual="$(git -C "$src" rev-parse HEAD)"
  if [ "$actual" != "$commit" ]; then
    echo "!! $repo@$tag is $actual, expected $commit — tag moved? Not building." >&2
    exit 1
  fi
  dir="$src/$subdir"
  if [ ! -f "$dir/src/parser.c" ]; then
    echo "== generating $name (no parser.c at $tag)"
    (cd "$dir" && npx -y "$CLI" generate)
  fi
  echo "== building $name"
  (cd "$dir" && npx -y "$CLI" build --wasm -o "$OUT/$name.wasm")
  mkdir -p "$OUT/$name"
  for q in highlights injections; do
    [ -f "$src/$queries/$q.scm" ] && cp "$src/$queries/$q.scm" "$OUT/$name/$q.scm"
  done
  # JavaScript ships its JSX rules separately; TSX needs them.
  [ -f "$src/$queries/highlights-jsx.scm" ] && cp "$src/$queries/highlights-jsx.scm" "$OUT/$name/highlights-jsx.scm"
  for license in LICENSE LICENSE.md LICENSE.txt; do
    [ -f "$src/$license" ] && cp "$src/$license" "$OUT/$name/LICENSE" && break
  done
  echo "$name $repo $tag $commit" >> "$OUT/.versions.tmp"
done

sort "$OUT/.versions.tmp" > "$OUT/VERSIONS"
rm "$OUT/.versions.tmp"
echo "== done: $(ls "$OUT"/*.wasm | wc -l | tr -d ' ') grammars in $OUT"
