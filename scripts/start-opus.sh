#!/usr/bin/env bash
# Start the source-checkout UI with an automatically managed OPUS-MT Python environment.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANAGED_DIR="$ROOT_DIR/.tools/opus-runtime"
VENV_DIR="${DEEPLOCAL_OPUS_MT_VENV:-$MANAGED_DIR}"
PYTHON="${DEEPLOCAL_OPUS_MT_PYTHON:-$VENV_DIR/bin/python}"
BACKEND_PORT="${DEEPLOCAL_BACKEND_PORT:-${DEELOCAL_BACKEND_PORT:-14567}}"
RESTART=0
REMOVE_RUNTIME=0
SETUP_ONLY=0

usage() {
  cat <<'EOF'
Usage: bash scripts/start-opus.sh [--setup] [--restart] [--stop] [--remove-runtime] [--help]

Normal startup automatically prepares an isolated Python environment on first
use and reuses it on subsequent runs. Python 3 must already be installed.
Package downloads require internet access only during initial setup.

  --setup           Prepare the runtime without starting the servers.
  --restart         Restart the development servers.
  --stop            Stop the development servers.
  --remove-runtime  Remove only the script-managed runtime after stopping servers.
  --help            Show this message.

Existing OPUS-MT model weights and custom Python environments are not modified.
EOF
}

for arg in "$@"; do
  case "$arg" in
    --setup) SETUP_ONLY=1 ;;
    --restart) RESTART=1 ;;
    --stop) exec "$ROOT_DIR/scripts/start-dev.sh" --stop ;;
    --remove-runtime) REMOVE_RUNTIME=1 ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Unknown option: $arg" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ "$REMOVE_RUNTIME" -eq 1 ]]; then
  if [[ "$SETUP_ONLY" -eq 1 || "$RESTART" -eq 1 ]]; then
    echo "--remove-runtime cannot be combined with other options." >&2
    exit 2
  fi
  if [[ -n "${DEEPLOCAL_OPUS_MT_PYTHON:-}" || -n "${DEEPLOCAL_OPUS_MT_VENV:-}" ]]; then
    echo "Refusing to remove a custom Python environment." >&2
    exit 2
  fi
  if [[ -L "$MANAGED_DIR" ]]; then
    echo "Refusing to remove a symlinked runtime directory." >&2
    exit 2
  fi
  if [[ ! -f "$MANAGED_DIR/.deeplocal-opus-managed" ]]; then
    echo "No script-managed OPUS-MT runtime to remove." >&2
    exit 2
  fi
  echo "Stop the development servers before removing the managed runtime."
  "$ROOT_DIR/scripts/start-dev.sh" --stop
  rm -rf -- "$MANAGED_DIR"
  echo "Removed the script-managed runtime. Translation models were preserved."
  exit 0
fi

if [[ -n "${DEEPLOCAL_OPUS_MT_PYTHON:-}" ]]; then
  if [[ ! -x "$PYTHON" ]] || ! "$PYTHON" -c 'import ctranslate2, sentencepiece' >/dev/null 2>&1; then
    echo "Custom OPUS-MT Python is missing or lacks required dependencies: $PYTHON" >&2
    exit 1
  fi
else
  if [[ ! -x "$PYTHON" ]]; then
    command -v python3 >/dev/null 2>&1 || {
      echo "Python 3 is required. Install Python 3 and retry." >&2
      exit 1
    }
    echo "Creating an isolated OPUS-MT runtime at $VENV_DIR ..."
    mkdir -p "$(dirname "$VENV_DIR")"
    python3 -m venv "$VENV_DIR"
    if [[ "$VENV_DIR" == "$MANAGED_DIR" ]]; then
      touch "$MANAGED_DIR/.deeplocal-opus-managed"
    fi
  fi
  if ! "$PYTHON" -c 'import ctranslate2, sentencepiece' >/dev/null 2>&1; then
    echo "Installing OPUS-MT dependencies (first run only)..."
    if ! "$PYTHON" -m pip install 'ctranslate2==4.9.0' sentencepiece; then
      echo "Dependency installation failed; retry this command when online." >&2
      exit 1
    fi
  fi
fi

if ! "$PYTHON" -c 'import ctranslate2, sentencepiece' >/dev/null 2>&1; then
  echo "OPUS-MT runtime verification failed: $PYTHON" >&2
  exit 1
fi

echo "OPUS-MT runtime ready: $PYTHON"
if [[ "$SETUP_ONLY" -eq 1 ]]; then
  exit 0
fi

if command -v curl >/dev/null 2>&1 &&
   curl -fsS --max-time 2 "http://127.0.0.1:$BACKEND_PORT/health" >/dev/null 2>&1 &&
   [[ "$RESTART" -eq 0 ]]; then
  echo "The backend is already running. Use --restart to apply this runtime." >&2
  exit 1
fi

export DEEPLOCAL_OPUS_MT_PYTHON="$PYTHON"
export DEEPLOCAL_SKIP_LLAMA_INSTALL="${DEEPLOCAL_SKIP_LLAMA_INSTALL:-1}"
echo "Translation models remain in the existing deepLocal models directory."
if [[ "$RESTART" -eq 1 ]]; then
  exec "$ROOT_DIR/scripts/start-dev.sh" --restart
fi
exec "$ROOT_DIR/scripts/start-dev.sh"
