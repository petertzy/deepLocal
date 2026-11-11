#!/usr/bin/env bash
set -euo pipefail

REPO="petertzy/deepLocal"
APP_NAME="deepLocal"
VERSION="${DEEPLOCAL_VERSION:-latest}"
INSTALL_DIR="${DEEPLOCAL_INSTALL_DIR:-/Applications}"
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

# Resolve the ZIP asset from release metadata instead of assuming a fixed name.
# Current releases use versioned assets (deepLocal_0.1.15_macos.zip), while
# older releases used deepLocal-macos.zip.
if [[ "$VERSION" == "latest" ]]; then
  RELEASE_API="https://api.github.com/repos/$REPO/releases/latest"
else
  RELEASE_API="https://api.github.com/repos/$REPO/releases/tags/$VERSION"
fi

command -v python3 >/dev/null 2>&1 || {
  echo "python3 is required to resolve the latest deepLocal release asset." >&2
  exit 1
}

echo "Checking deepLocal release assets..."
curl -fsSL --retry 3 -H "Accept: application/vnd.github+json" \
  "$RELEASE_API" -o "$TMP_DIR/release.json" || {
  echo "Could not fetch release metadata from $RELEASE_API" >&2
  exit 1
}

DOWNLOAD_URL="$(python3 - "$TMP_DIR/release.json" <<'PY'
import json
import re
import sys

with open(sys.argv[1], encoding="utf-8") as release_file:
    release = json.load(release_file)

assets = release.get("assets", [])
matches = [
    asset for asset in assets
    if asset.get("name") == "deepLocal-macos.zip"
    or re.fullmatch(r"deepLocal_[0-9]+\.[0-9]+\.[0-9]+_macos\.zip", asset.get("name", ""))
]
if len(matches) != 1:
    sys.exit("Expected exactly one macOS ZIP asset in the selected deepLocal release.")
url = matches[0].get("browser_download_url", "")
if not url.startswith("https://github.com/petertzy/deepLocal/releases/download/"):
    sys.exit("Unexpected deepLocal release asset URL.")
print(url)
PY
)"

echo "Downloading deepLocal from:"
echo "  $DOWNLOAD_URL"
curl -fL --retry 3 --progress-bar "$DOWNLOAD_URL" -o "$TMP_DIR/deepLocal-macos.zip"

echo "Extracting deepLocal..."
unzip -q "$TMP_DIR/deepLocal-macos.zip" -d "$TMP_DIR"

APP_PATH="$(find "$TMP_DIR" -maxdepth 2 -name "$APP_NAME.app" -type d -print -quit)"
if [[ -z "$APP_PATH" ]]; then
  echo "Could not find $APP_NAME.app in the downloaded archive." >&2
  exit 1
fi

DEST_PATH="$INSTALL_DIR/$APP_NAME.app"

install_app() {
  mkdir -p "$INSTALL_DIR"

  if [[ -d "$DEST_PATH" ]]; then
    echo "Replacing existing $DEST_PATH"
    rm -rf "$DEST_PATH"
  fi

  cp -R "$APP_PATH" "$DEST_PATH"
}

install_app_with_admin() {
  local source_path
  source_path="$(printf "%q" "$APP_PATH")"
  local install_dir
  install_dir="$(printf "%q" "$INSTALL_DIR")"
  local destination_path
  destination_path="$(printf "%q" "$DEST_PATH")"

  osascript -e "do shell script \"mkdir -p $install_dir && rm -rf $destination_path && cp -R $source_path $destination_path\" with administrator privileges"
}

if ! install_app 2>/dev/null; then
  if [[ "$INSTALL_DIR" == "/Applications" ]] && command -v osascript >/dev/null 2>&1; then
    echo "Administrator permission is required to install to /Applications."
    install_app_with_admin
  else
    echo "Could not install deepLocal to $INSTALL_DIR." >&2
    echo "Try setting DEEPLOCAL_INSTALL_DIR to a writable folder." >&2
    exit 1
  fi
fi

if command -v xattr >/dev/null 2>&1; then
  xattr -dr com.apple.quarantine "$DEST_PATH" 2>/dev/null || {
    if [[ "$INSTALL_DIR" == "/Applications" ]] && command -v osascript >/dev/null 2>&1; then
      osascript -e "do shell script \"xattr -dr com.apple.quarantine $(printf "%q" "$DEST_PATH")\" with administrator privileges" >/dev/null 2>&1 || true
    fi
  }
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
