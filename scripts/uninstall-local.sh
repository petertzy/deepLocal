#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BACKEND_PORT="${DEELOCAL_BACKEND_PORT:-14567}"
FRONTEND_PORT="${DEELOCAL_FRONTEND_PORT:-5173}"
REMOVE_LLAMA=0
YES=0

usage() {
  cat <<'EOF'
Usage: ./scripts/uninstall-local.sh [--yes] [--remove-llama]

Removes local deepLocal runtime artifacts from this checkout:
  - running dev servers on the configured frontend/backend ports
  - downloaded models and partial downloads under ./models
  - local SQLite databases in the project root
  - Rust build output under ./target
  - desktop node_modules and dist output

Options:
  --yes           Run without an interactive confirmation prompt.
  --remove-llama  Also uninstall Homebrew llama.cpp when it appears to be installed.

This script is intentionally scoped to this project directory by default.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --yes|-y)
      YES=1
      ;;
    --remove-llama)
      REMOVE_LLAMA=1
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
  shift
done

stop_port() {
  local port="$1"
  local pids
  pids="$(lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)"
  if [[ -n "$pids" ]]; then
    echo "Stopping process on port $port..."
    kill $pids 2>/dev/null || true
    sleep 1
  fi
}

remove_path() {
  local path="$1"
  if [[ -e "$path" || -L "$path" ]]; then
    echo "Removing ${path#$ROOT_DIR/}"
    rm -rf "$path"
  fi
}

remove_glob() {
  local pattern="$1"
  local match
  shopt -s nullglob
  for match in $pattern; do
    remove_path "$match"
  done
  shopt -u nullglob
}

echo "deepLocal local cleanup"
echo
echo "Project directory:"
echo "  $ROOT_DIR"
echo
echo "This will remove generated local files from this checkout."
echo "Downloaded models under ./models will be deleted."
if [[ "$REMOVE_LLAMA" -eq 1 ]]; then
  echo "Homebrew llama.cpp will also be removed if it is installed."
fi
echo

if [[ "$YES" -ne 1 ]]; then
  read -r -p "Continue? Type 'yes' to proceed: " answer
  if [[ "$answer" != "yes" ]]; then
    echo "Cancelled."
    exit 0
  fi
fi

stop_port "$FRONTEND_PORT"
stop_port "$BACKEND_PORT"

remove_path "$ROOT_DIR/models"
remove_path "$ROOT_DIR/target"
remove_path "$ROOT_DIR/apps/desktop/node_modules"
remove_path "$ROOT_DIR/apps/desktop/dist"
remove_glob "$ROOT_DIR"/*.db
remove_glob "$ROOT_DIR"/*.db-shm
remove_glob "$ROOT_DIR"/*.db-wal
remove_glob "$ROOT_DIR"/*.sqlite
remove_glob "$ROOT_DIR"/*.sqlite-shm
remove_glob "$ROOT_DIR"/*.sqlite-wal
remove_glob "$ROOT_DIR"/*.sqlite3
remove_glob "$ROOT_DIR"/*.sqlite3-shm
remove_glob "$ROOT_DIR"/*.sqlite3-wal

if [[ "$REMOVE_LLAMA" -eq 1 ]]; then
  if [[ "$(uname -s)" == "Darwin" ]] && command -v brew >/dev/null 2>&1; then
    if brew list llama.cpp >/dev/null 2>&1; then
      echo "Uninstalling Homebrew llama.cpp..."
      brew uninstall llama.cpp
    else
      echo "Homebrew llama.cpp is not installed."
    fi
  else
    echo "Skipping llama.cpp uninstall: Homebrew on macOS was not detected."
  fi
fi

echo
echo "deepLocal local cleanup complete."
