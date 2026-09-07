#!/usr/bin/env bash
set -euo pipefail

REPO="petertzy/deepLocal"
APP_NAME="deepLocal"
VERSION="${DEEPLOCAL_VERSION:-latest}"
INSTALL_DIR="${DEEPLOCAL_INSTALL_DIR:-$HOME/Applications}"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "deepLocal macOS installer must run on macOS." >&2
  exit 1
fi

command -v curl >/dev/null 2>&1 || {
  echo "curl is required to download deepLocal." >&2
  exit 1
}

command -v unzip >/dev/null 2>&1 || {
  echo "unzip is required to install deepLocal." >&2
  exit 1
}

if [[ "$VERSION" == "latest" ]]; then
  DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/deepLocal-macos.zip"
else
  DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/deepLocal-macos.zip"
fi

echo "Downloading deepLocal from:"
echo "  $DOWNLOAD_URL"
curl -fL --progress-bar "$DOWNLOAD_URL" -o "$TMP_DIR/deepLocal-macos.zip"

echo "Extracting deepLocal..."
unzip -q "$TMP_DIR/deepLocal-macos.zip" -d "$TMP_DIR"

APP_PATH="$(find "$TMP_DIR" -maxdepth 2 -name "$APP_NAME.app" -type d -print -quit)"
if [[ -z "$APP_PATH" ]]; then
  echo "Could not find $APP_NAME.app in the downloaded archive." >&2
  exit 1
fi

mkdir -p "$INSTALL_DIR"
DEST_PATH="$INSTALL_DIR/$APP_NAME.app"

if [[ -d "$DEST_PATH" ]]; then
  echo "Replacing existing $DEST_PATH"
  rm -rf "$DEST_PATH"
fi

cp -R "$APP_PATH" "$DEST_PATH"

if command -v xattr >/dev/null 2>&1; then
  xattr -dr com.apple.quarantine "$DEST_PATH" 2>/dev/null || true
fi

echo "Installed deepLocal:"
echo "  $DEST_PATH"

if ! command -v llama-server >/dev/null 2>&1 && [[ ! -x /opt/homebrew/bin/llama-server && ! -x /usr/local/bin/llama-server ]]; then
  echo
  echo "Note: llama-server was not found. deepLocal will open, but loading GGUF models requires llama.cpp."
  echo "Install it later with Homebrew:"
  echo "  brew install llama.cpp"
fi

echo "Opening deepLocal..."
open "$DEST_PATH"
