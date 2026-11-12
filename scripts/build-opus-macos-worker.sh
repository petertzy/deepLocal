#!/usr/bin/env bash
# Build a standalone macOS OPUS-MT worker with an embedded Python interpreter.
# Model weights are intentionally excluded from the app bundle.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_DIR="$ROOT_DIR/.tools"
BUILD_VENV="$TOOLS_DIR/opus-package-venv"
OUTPUT_DIR="$ROOT_DIR/apps/desktop/src-tauri/resources/opus-runtime"
STAGING_DIR="$TOOLS_DIR/opus-package-build"

[[ "$(uname -s)" == "Darwin" ]] || { echo "macOS is required." >&2; exit 1; }
command -v python3 >/dev/null || { echo "Python 3 is required to build the package." >&2; exit 1; }

mkdir -p "$TOOLS_DIR"
if [[ ! -x "$BUILD_VENV/bin/python" ]]; then
  python3 -m venv "$BUILD_VENV"
fi
"$BUILD_VENV/bin/python" -m pip install \
  'pyinstaller==6.16.0' 'ctranslate2==4.9.0' 'sentencepiece==0.2.3'

rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"
"$BUILD_VENV/bin/python" -m PyInstaller \
  --noconfirm --clean --onedir --name opus-mt-worker \
  --distpath "$STAGING_DIR/dist" --workpath "$STAGING_DIR/build" \
  --specpath "$STAGING_DIR" \
  --collect-all ctranslate2 --collect-all sentencepiece \
  "$ROOT_DIR/scripts/opus_mt_translate.py"

WORKER_DIR="$STAGING_DIR/dist/opus-mt-worker"
"$WORKER_DIR/opus-mt-worker" --self-test
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"
cp -R "$WORKER_DIR/." "$OUTPUT_DIR/"
chmod +x "$OUTPUT_DIR/opus-mt-worker"
"$OUTPUT_DIR/opus-mt-worker" --self-test
echo "Standalone OPUS-MT worker ready: $OUTPUT_DIR/opus-mt-worker"
