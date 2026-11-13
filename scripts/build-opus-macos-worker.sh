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
# Prefer a supported interpreter with prebuilt Apple Silicon wheels.
# A previously created Python 3.9 venv must not be reused.
PACKAGE_PYTHON=""
if [[ -n "${DEEPLOCAL_OPUS_PACKAGE_PYTHON:-}" ]]; then
  PACKAGE_PYTHON="$DEEPLOCAL_OPUS_PACKAGE_PYTHON"
else
  for candidate in python3.14 python3.13 python3.12 python3.11 python3.10 python3; do
    if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import sys; assert (3, 10) <= sys.version_info[:2] < (3, 15)' >/dev/null 2>&1; then
      PACKAGE_PYTHON="$(command -v "$candidate")"
      break
    fi
  done
fi
if [[ -z "$PACKAGE_PYTHON" ]] || ! "$PACKAGE_PYTHON" -c 'import sys; assert (3, 10) <= sys.version_info[:2] < (3, 15)' >/dev/null 2>&1; then
  echo "A Python version from 3.10 through 3.14 is required for OPUS-MT packaging." >&2
  echo "Set DEEPLOCAL_OPUS_PACKAGE_PYTHON to a supported Python executable." >&2
  exit 1
fi

mkdir -p "$TOOLS_DIR"
if [[ -x "$BUILD_VENV/bin/python" ]]; then
  if ! "$BUILD_VENV/bin/python" -c 'import sys; assert (3, 10) <= sys.version_info[:2] < (3, 15)' >/dev/null 2>&1; then
    echo "Replacing an incompatible OPUS-MT packaging environment."
    rm -rf "$BUILD_VENV"
  fi
fi
if [[ ! -x "$BUILD_VENV/bin/python" ]]; then
  "$PACKAGE_PYTHON" -m venv "$BUILD_VENV"
fi
echo "OPUS-MT packaging Python: $("$BUILD_VENV/bin/python" --version)"
"$BUILD_VENV/bin/python" -m pip install --upgrade pip
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
