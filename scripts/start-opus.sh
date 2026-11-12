#!/usr/bin/env bash
# Start the source-checkout development UI with the local OPUS-MT worker.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV_DIR="${DEEPLOCAL_OPUS_MT_VENV:-$ROOT_DIR/.venv-opus}"
PYTHON="${DEEPLOCAL_OPUS_MT_PYTHON:-$VENV_DIR/bin/python}"
BACKEND_PORT="${DEEPLOCAL_BACKEND_PORT:-${DEELOCAL_BACKEND_PORT:-14567}}"
SETUP=0
RESTART=0
PASSTHROUGH=()

usage() {
  cat <<'EOF'
Usage: ./scripts/start-opus.sh [--setup] [--restart] [--stop] [--help]

Start the deepLocal source-checkout UI with a local OPUS-MT Python runtime.
The default interpreter is .venv-opus/bin/python; an explicit
DEEPLOCAL_OPUS_MT_PYTHON takes precedence.

  --setup     Create a local virtual environment and install translation
              dependencies if needed (requires internet access).
  --restart   Restart the frontend and backend to apply the interpreter.
  --stop      Stop the existing deepLocal development servers.
  --help      Show this message.

Existing model weights under models/opus-mt are never modified.
Open http://127.0.0.1:5173/ after the server starts.
EOF
}

for arg in "$@"; do
  case "$arg" in
    --setup) SETUP=1 ;;
    --restart) RESTART=1; PASSTHROUGH+=(--restart) ;;
    --stop) exec "$ROOT_DIR/scripts/start-dev.sh" --stop ;;
    --help|-h) usage; exit 0 ;;
    *) echo "Unknown option: $arg" >&2; usage >&2; exit 2 ;;
  esac
done

if [[ "$SETUP" -eq 1 ]]; then
  if [[ -n "${DEEPLOCAL_OPUS_MT_PYTHON:-}" ]]; then
    echo "--setup cannot modify a custom DEEPLOCAL_OPUS_MT_PYTHON environment." >&2
    echo "Install ctranslate2 and sentencepiece in that environment yourself." >&2
    exit 2
  fi
  if [[ ! -x "$PYTHON" ]]; then
    command -v python3 >/dev/null 2>&1 || {
      echo "Python 3 is required for --setup." >&2
      exit 1
    }
    echo "Creating local OPUS-MT virtual environment at $VENV_DIR ..."
    python3 -m venv "$VENV_DIR"
  fi
  if ! "$PYTHON" -c 'import ctranslate2, sentencepiece' >/dev/null 2>&1; then
    echo "Installing CTranslate2 and SentencePiece into $VENV_DIR ..."
    "$PYTHON" -m pip install ctranslate2 sentencepiece
  fi
fi

if [[ ! -x "$PYTHON" ]]; then
  echo "OPUS-MT Python interpreter not found: $PYTHON" >&2
  echo "Run ./scripts/start-opus.sh --setup once, or set DEEPLOCAL_OPUS_MT_PYTHON." >&2
  exit 1
fi

if ! "$PYTHON" -c 'import ctranslate2, sentencepiece' >/dev/null 2>&1; then
  echo "Missing CTranslate2 or SentencePiece in: $PYTHON" >&2
  echo "Run ./scripts/start-opus.sh --setup once, or install the packages manually." >&2
  exit 1
fi

if command -v curl >/dev/null 2>&1 &&
   curl -fsS --max-time 2 "http://127.0.0.1:$BACKEND_PORT/health" >/dev/null 2>&1 &&
   [[ "$RESTART" -eq 0 ]]; then
  echo "An API server is already running on port $BACKEND_PORT." >&2
  echo "Its Python runtime cannot be changed without restarting it." >&2
  echo "Use ./scripts/start-opus.sh --restart to apply the selected interpreter." >&2
  exit 1
fi

export DEEPLOCAL_OPUS_MT_PYTHON="$PYTHON"
echo "Using OPUS-MT Python: $DEEPLOCAL_OPUS_MT_PYTHON"
echo "Model weights are loaded from the existing deepLocal models directory."
exec "$ROOT_DIR/scripts/start-dev.sh" "${PASSTHROUGH[@]}"
