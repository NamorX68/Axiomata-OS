#!/usr/bin/env bash
# Vendors the file tree's icons (docs/plans/editor-look.md, LK4, K5): for every
# kind of file the tree tells apart (FILE_ICON_KEYS below), one icon from each of
# three sets, written as src/ui/icons/fileIcons.ts with the licences beside it:
#
#   catppuccin  Catppuccin's VS Code icons, from @iconify-json/catppuccin (MIT)
#   octicons    GitHub's Octicons ("Git"), from @iconify-json/octicon (MIT)
#   jetbrains   JetBrains' "Expui" file-type icons, light and dark, from
#               intellij-community at a pinned commit (Apache 2.0)
#
# The fourth style, monochrome, is drawn from the Lucide icons already vendored
# (`src/core/fileIcons.ts`). Everything is pinned — package versions with their
# registry checksums, the commit — and checked in; nothing is fetched at runtime.
#
# Usage: scripts/vendor-file-icons.sh
set -euo pipefail

CATPPUCCIN="@iconify-json/catppuccin@1.2.17"
CATPPUCCIN_SHASUM="3a163a4b198bb8363327bad9e48227ae958cb6b2"
OCTICON="@iconify-json/octicon@1.2.36"
OCTICON_SHASUM="c83e41581a76e8015316dbd701e3dd874ef70597"
# Only for Octicons' licence text, which the Iconify package does not carry.
PRIMER="@primer/octicons@19.38.0"
PRIMER_SHASUM="dfb239d9cb5e0e40cade3d5d9058d34ae9316f22"
CATPPUCCIN_LICENSE="https://raw.githubusercontent.com/catppuccin/vscode-icons/v1.26.0/LICENSE"
INTELLIJ_COMMIT="4be4f52c4e4eaebee455598ad6cfcae182d0b962"
INTELLIJ="https://raw.githubusercontent.com/JetBrains/intellij-community/$INTELLIJ_COMMIT"

HERE="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$HERE/src/ui/icons"
WORK="${TMPDIR:-/tmp}/axiomata-file-icons"

rm -rf "$WORK"
mkdir -p "$WORK/jetbrains" "$OUT"
cd "$WORK"

fetch_package() {
  local spec="$1" sum="$2"
  local tarball
  tarball="$(npm pack --silent "$spec")"
  if [ "$(shasum -a 1 "$tarball" | cut -d' ' -f1)" != "$sum" ]; then
    echo "checksum mismatch for $spec" >&2
    exit 1
  fi
  mkdir -p "${tarball%.tgz}"
  tar -xzf "$tarball" -C "${tarball%.tgz}"
  echo "$WORK/${tarball%.tgz}/package"
}

CAT_DIR="$(fetch_package "$CATPPUCCIN" "$CATPPUCCIN_SHASUM")"
OCT_DIR="$(fetch_package "$OCTICON" "$OCTICON_SHASUM")"
PRIMER_DIR="$(fetch_package "$PRIMER" "$PRIMER_SHASUM")"

# Kind of file | Catppuccin | Octicons | JetBrains (under platform/icons/src/expui/; `anyType` where it has none)
MAP=(
  "rust|rust|file-code-16|fileTypes/anyType"
  "typescript|typescript|file-code-16|fileTypes/typeScript"
  "javascript|javascript|file-code-16|fileTypes/javaScript"
  "react|typescript-react|file-code-16|fileTypes/javaScript"
  "json|json|file-code-16|fileTypes/json"
  "markdown|markdown|markdown-16|fileTypes/markdown"
  "python|python|file-code-16|fileTypes/anyType"
  "css|css|file-code-16|fileTypes/css"
  "html|html|file-code-16|fileTypes/html"
  "svelte|svelte|file-code-16|fileTypes/anyType"
  "toml|toml|gear-16|fileTypes/toml"
  "yaml|yaml|gear-16|fileTypes/yaml"
  "lua|lua|file-code-16|fileTypes/anyType"
  "swift|swift|file-code-16|fileTypes/swiftLang"
  "sql|database|database-16|fileTypes/sql"
  "bash|bash|file-code-16|fileTypes/shell"
  "image|image|file-media-16|fileTypes/image"
  "git|git|git-branch-16|fileTypes/gitignore"
  "docker|docker|container-16|fileTypes/docker"
  "lock|lock|lock-16|fileTypes/anyType"
  "text|text|file-16|fileTypes/text"
  "env|env|gear-16|fileTypes/properties"
  "license|license|law-16|fileTypes/text"
  "readme|readme|book-16|fileTypes/markdown"
  "cargo|cargo|package-16|fileTypes/toml"
  "npm|package-json|package-16|fileTypes/json"
  "file|file|file-16|fileTypes/anyType"
  "folder|folder|file-directory-fill-16|nodes/folder"
  "folder-open|folder-open|file-directory-open-fill-16|nodes/folder"
)

for entry in "${MAP[@]}"; do
  jb="${entry##*|}"
  light="$WORK/jetbrains/${jb//\//_}.svg"
  dark="$WORK/jetbrains/${jb//\//_}_dark.svg"
  [ -f "$light" ] || curl -fsSL "$INTELLIJ/platform/icons/src/expui/$jb.svg" -o "$light"
  # A few icons (gitignore) have no dark variant: the light one is used there too.
  [ -f "$dark" ] || curl -fsSL "$INTELLIJ/platform/icons/src/expui/${jb}_dark.svg" -o "$dark" || cp "$light" "$dark"
done

node - "$CAT_DIR/icons.json" "$OCT_DIR/icons.json" "$WORK/jetbrains" "$OUT/fileIcons.ts" "${MAP[@]}" <<'NODE'
const fs = require("fs");
const [catFile, octFile, jbDir, out, ...map] = process.argv.slice(2);
const cat = JSON.parse(fs.readFileSync(catFile, "utf8"));
const oct = JSON.parse(fs.readFileSync(octFile, "utf8"));

const safe = (body, what) => {
  if (!body || /<script|<style|on\w+=|href=|<foreignObject|@import|url\(/i.test(body)) {
    throw new Error(`unexpected content in ${what}`);
  }
  return body;
};
const iconify = (set, name) => {
  const icon = set.icons[name];
  if (!icon) throw new Error(`no ${name} in ${set.prefix}`);
  return { viewBox: `0 0 ${icon.width ?? set.width ?? 16} ${icon.height ?? set.height ?? 16}`, body: safe(icon.body, name) };
};
const svgFile = (path) => {
  const svg = fs.readFileSync(path, "utf8").replace(/<!--[\s\S]*?-->/g, "");
  const viewBox = /viewBox="([^"]+)"/.exec(svg)?.[1] ?? "0 0 16 16";
  const body = svg.replace(/^[\s\S]*?<svg[^>]*>/, "").replace(/<\/svg>[\s\S]*$/, "").replace(/\s*\n\s*/g, "").trim();
  return { viewBox, body: safe(body, path) };
};

const keys = [];
const sets = { catppuccin: {}, octicons: {}, jetbrains: {} };
for (const line of map) {
  const [key, catName, octName, jb] = line.split("|");
  keys.push(key);
  sets.catppuccin[key] = iconify(cat, catName);
  sets.octicons[key] = iconify(oct, octName);
  const file = `${jbDir}/${jb.replace("/", "_")}`;
  sets.jetbrains[key] = { ...svgFile(`${file}.svg`), dark: svgFile(`${file}_dark.svg`) };
}

fs.writeFileSync(
  out,
  [
    "// Generated by scripts/vendor-file-icons.sh — do not edit. Catppuccin (MIT, LICENSE-catppuccin),",
    "// Octicons (MIT, LICENSE-octicons), JetBrains Expui (Apache 2.0, LICENSE-jetbrains, LICENSE-jetbrains-NOTICE).",
    "",
    "export const FILE_ICON_KEYS = " + JSON.stringify(keys) + " as const;",
    "",
    "export type FileIconKey = (typeof FILE_ICON_KEYS)[number];",
    "",
    "/** One icon: an SVG's inside and its view box; `dark` where the set draws dark themes differently. */",
    "export interface FileIconGlyph {",
    "  viewBox: string;",
    "  body: string;",
    "  dark?: { viewBox: string; body: string };",
    "}",
    "",
    "export const VENDORED_FILE_ICONS: Record<\"catppuccin\" | \"octicons\" | \"jetbrains\", Record<FileIconKey, FileIconGlyph>> = " +
      JSON.stringify(sets, null, 2) +
      ";",
    "",
  ].join("\n"),
);
NODE

curl -fsSL "$CATPPUCCIN_LICENSE" -o "$OUT/LICENSE-catppuccin"
cp "$PRIMER_DIR/LICENSE" "$OUT/LICENSE-octicons"
curl -fsSL "$INTELLIJ/LICENSE.txt" -o "$OUT/LICENSE-jetbrains"
# Apache 2.0 §4(d): the NOTICE travels with what is redistributed.
curl -fsSL "$INTELLIJ/NOTICE.txt" -o "$OUT/LICENSE-jetbrains-NOTICE"
echo "wrote ${#MAP[@]} kinds × 3 sets to $OUT/fileIcons.ts"
