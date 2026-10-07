#!/usr/bin/env bash
# Builds the installable Axiomata-OS.app and, with --install, copies it to /Applications.
#
# The app starts `axiomata-cli` (the agents' MCP server, the assistant's module actions) from next to its own
# executable, so the CLI is bundled as a sidecar: Tauri takes `binaries/axiomata-cli-<target triple>` and puts it into
# Contents/MacOS/axiomata-cli. That setting lives in `tauri.bundle.conf.json`, merged in only here: in the base config
# every ordinary build (`cargo tauri dev`, clippy) would fail while the copied binary is missing. The bundle is signed ad
# hoc (`signingIdentity: "-"`): there is no Developer ID, so it runs on this Mac only and is not notarized.
set -euo pipefail

cd "$(dirname "$0")/.."
root="$(cd ../.. && pwd)"
triple="$(rustc -vV | sed -n 's/^host: //p')"

cargo build --release -p axiomata-cli --manifest-path "$root/Cargo.toml"
mkdir -p src-tauri/binaries
cp "$root/target/release/axiomata-cli" "src-tauri/binaries/axiomata-cli-$triple"

cargo tauri build --bundles app --config src-tauri/tauri.bundle.conf.json

app="$root/target/release/bundle/macos/Axiomata-OS.app"
echo "built: $app"

if [[ "${1:-}" == "--install" ]]; then
  if pgrep -f "/Applications/Axiomata-OS.app/" >/dev/null; then
    echo "Axiomata-OS is running from /Applications; quit it first." >&2
    exit 1
  fi
  rm -rf /Applications/Axiomata-OS.app
  cp -R "$app" /Applications/Axiomata-OS.app
  echo "installed: /Applications/Axiomata-OS.app"
fi
