#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$ROOT_DIR/apps/desktop"
RUNTIME_DIR="$DESKTOP_DIR/src-tauri/resources/llama-runtime"
DIST_DIR="$ROOT_DIR/dist"
TEMP_CONFIG="$(mktemp --suffix=.json)"
trap 'rm -f "$TEMP_CONFIG"' EXIT

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "This script must run on Linux." >&2
  exit 1
fi
for tool in cargo npm node; do
  command -v "$tool" >/dev/null || { echo "Missing required tool: $tool" >&2; exit 1; }
done

SERVER="${DEEPLOCAL_LLAMA_SERVER:-${LLAMA_SERVER:-}}"
if [[ -z "$SERVER" ]]; then
  SERVER="$(command -v llama-server || true)"
fi
if [[ -z "$SERVER" ]]; then
  SERVER="$(find "$ROOT_DIR/.tools/llama.cpp" -type f -name llama-server -print -quit 2>/dev/null || true)"
fi
if [[ ! -x "$SERVER" ]]; then
  echo "llama-server is required. Run scripts/setup-unix.sh first." >&2
  exit 1
fi

mkdir -p "$RUNTIME_DIR"
rm -f "$RUNTIME_DIR"/*
cp -L "$SERVER" "$RUNTIME_DIR/llama-server"
chmod +x "$RUNTIME_DIR/llama-server"
SOURCE_DIR="$(dirname "$(readlink -f "$SERVER")")"
if command -v ldd >/dev/null; then
  while IFS= read -r dependency; do
    case "$dependency" in
      "$SOURCE_DIR"/*) cp -L "$dependency" "$RUNTIME_DIR/$(basename "$dependency")" ;;
    esac
  done < <(ldd "$SERVER" | grep -oE '/[^ ]+' || true)
fi
find "$SOURCE_DIR" -maxdepth 1 -type f \( -name 'libllama*.so*' -o -name 'libggml*.so*' \) -exec cp -L {} "$RUNTIME_DIR/" \;
node - "$DESKTOP_DIR/src-tauri/tauri.conf.json" "$TEMP_CONFIG" <<'NODE'
const fs = require('fs');
const [source, target] = process.argv.slice(2);
const config = JSON.parse(fs.readFileSync(source, 'utf8'));
config.bundle.targets = ['deb', 'appimage'];
if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
  config.bundle.createUpdaterArtifacts = false;
  delete config.plugins?.updater;
}
fs.writeFileSync(target, JSON.stringify(config));
NODE
cd "$DESKTOP_DIR"
[[ -d node_modules ]] || npm ci
npm run tauri:build -- --config "$TEMP_CONFIG"
mkdir -p "$DIST_DIR"
find "$ROOT_DIR/target/release/bundle" -type f \( -name '*.deb' -o -name '*.AppImage' \) -exec cp {} "$DIST_DIR/" \;
echo "Linux packages are available in $DIST_DIR"
