#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$ROOT_DIR/apps/desktop"
DIST_DIR="$ROOT_DIR/dist"
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS app packaging must run on macOS." >&2
  exit 1
fi

command -v cargo >/dev/null 2>&1 || {
  echo "cargo is required. Run ./scripts/start-dev.sh once to install prerequisites." >&2
  exit 1
}

command -v npm >/dev/null 2>&1 || {
  echo "npm is required. Install Node.js or run ./scripts/start-dev.sh once." >&2
  exit 1
}

echo "Building Tauri app..."
(
  cd "$DESKTOP_DIR"
  if [[ ! -d node_modules ]]; then
    npm ci
  fi
  npm run tauri:build
)

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

APP_PATH="$(find "$ROOT_DIR/target/release/bundle/macos" -maxdepth 1 -name 'deepLocal.app' -print -quit)"
DMG_PATH="$(find "$ROOT_DIR/target/release/bundle/dmg" -maxdepth 1 -name '*.dmg' -print -quit 2>/dev/null || true)"

if [[ -n "$APP_PATH" ]]; then
  cp -R "$APP_PATH" "$DIST_DIR/"
fi

if [[ -d "$DIST_DIR/deepLocal.app" ]]; then
  if ! hdiutil create \
    -volname "deepLocal" \
    -srcfolder "$DIST_DIR/deepLocal.app" \
    -ov \
    -format UDZO \
    "$DIST_DIR/deepLocal-macos.dmg" >/dev/null; then
    echo "DMG creation failed; continuing with the app bundle and zip archive."
  fi
elif [[ -n "$DMG_PATH" ]]; then
  cp "$DMG_PATH" "$DIST_DIR/deepLocal-macos.dmg"
fi

if [[ -d "$DIST_DIR/deepLocal.app" ]]; then
  (
    cd "$DIST_DIR"
    ditto -c -k --sequesterRsrc --keepParent "deepLocal.app" "deepLocal-macos.zip"
  )
fi

echo "Packaged app artifacts:"
find "$DIST_DIR" -maxdepth 1 \( -name 'deepLocal.app' -o -name 'deepLocal-macos.*' \) -print
