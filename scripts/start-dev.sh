#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$ROOT_DIR/apps/desktop"
BACKEND_PORT="${DEELOCAL_BACKEND_PORT:-14567}"
FRONTEND_PORT="${DEELOCAL_FRONTEND_PORT:-5173}"
BACKEND_PID=""
BACKEND_ALREADY_RUNNING=0
FRONTEND_ALREADY_RUNNING=0
RESTART=0
STOP=0
BUILD=0

cleanup() {
  if [[ -n "$BACKEND_PID" ]] && kill -0 "$BACKEND_PID" 2>/dev/null; then
    echo "Stopping backend runtime..."
    kill "$BACKEND_PID" 2>/dev/null || true
  fi
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1"
    echo "Please install it, then run this script again."
    exit 1
  fi
}

port_in_use() {
  lsof -nP -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1
}

service_healthy() {
  curl -fsS "$1" >/dev/null 2>&1
}

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

configure_llama_server() {
  if [[ -n "${DEEPLOCAL_LLAMA_SERVER:-}" || -n "${DEELOCAL_LLAMA_SERVER:-}" || -n "${LLAMA_SERVER:-}" ]]; then
    return 0
  fi

  if command -v llama-server >/dev/null 2>&1; then
    export DEEPLOCAL_LLAMA_SERVER="$(command -v llama-server)"
    echo "Using system llama-server: $DEEPLOCAL_LLAMA_SERVER"
  else
    echo "llama-server was not found in PATH."
    if [[ "${DEEPLOCAL_SKIP_LLAMA_INSTALL:-}" == "1" ]]; then
      echo "Skipping llama.cpp installation because DEEPLOCAL_SKIP_LLAMA_INSTALL=1."
      echo "The app can still start, but real GGUF chat needs llama.cpp installed."
      return 0
    fi
    install_llama_cpp
    export DEEPLOCAL_LLAMA_SERVER="$(command -v llama-server)"
    echo "Using system llama-server: $DEEPLOCAL_LLAMA_SERVER"
  fi
}

install_llama_cpp() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "Automatic llama.cpp installation is currently supported on macOS with Homebrew."
    echo "Install llama.cpp manually, then run this script again."
    return 0
  fi

  if ! command -v brew >/dev/null 2>&1; then
    echo "Homebrew was not found, so llama.cpp cannot be installed automatically."
    echo "Install Homebrew and llama.cpp manually, then run this script again."
    return 0
  fi

  echo "Installing llama.cpp with Homebrew..."
  brew install llama.cpp

  if ! command -v llama-server >/dev/null 2>&1; then
    echo "llama.cpp was installed, but llama-server is still not available in PATH."
    echo "Check your Homebrew shell setup, then run this script again."
    exit 1
  fi
}

trap cleanup EXIT INT TERM

if [[ "${1:-}" == "--restart" ]]; then
  RESTART=1
elif [[ "${1:-}" == "--stop" ]]; then
  STOP=1
elif [[ "${1:-}" == "--build" ]]; then
  BUILD=1
elif [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  echo "Usage: ./scripts/start-dev.sh [--restart|--stop|--build]"
  echo
  echo "Starts the deepLocal backend and frontend development servers."
  echo "Installs llama.cpp automatically on macOS with Homebrew if llama-server is missing."
  echo "Use --restart to stop existing processes on ports $BACKEND_PORT and $FRONTEND_PORT first."
  echo "Use --stop to stop processes on ports $BACKEND_PORT and $FRONTEND_PORT."
  echo "Use --build to run the desktop frontend production build from the project root."
  echo "Set DEEPLOCAL_SKIP_LLAMA_INSTALL=1 to skip automatic llama.cpp installation."
  exit 0
fi

require_command npm

if [[ "$BUILD" -eq 1 ]]; then
  cd "$DESKTOP_DIR"
  if [[ ! -d node_modules ]]; then
    echo "Installing frontend dependencies..."
    npm install
  fi
  echo "Building deepLocal desktop UI..."
  npm run build
  exit 0
fi

require_command cargo
require_command lsof
require_command curl

configure_llama_server

if [[ "$RESTART" -eq 1 ]]; then
  stop_port "$FRONTEND_PORT"
  stop_port "$BACKEND_PORT"
fi

if [[ "$STOP" -eq 1 ]]; then
  stop_port "$FRONTEND_PORT"
  stop_port "$BACKEND_PORT"
  echo "deepLocal local dev servers are stopped."
  exit 0
fi

if port_in_use "$FRONTEND_PORT"; then
  if service_healthy "http://127.0.0.1:$FRONTEND_PORT/"; then
    FRONTEND_ALREADY_RUNNING=1
    echo "Frontend is already running on http://127.0.0.1:$FRONTEND_PORT/."
  else
    echo "Frontend port $FRONTEND_PORT is in use, but the web page is not responding."
    echo "Run this to clean up stale local dev processes and start again:"
    echo "  ./scripts/start-dev.sh --restart"
    exit 1
  fi
fi

if port_in_use "$BACKEND_PORT"; then
  if service_healthy "http://127.0.0.1:$BACKEND_PORT/health"; then
    BACKEND_ALREADY_RUNNING=1
    echo "Backend is already running on http://127.0.0.1:$BACKEND_PORT."
  else
    echo "Backend port $BACKEND_PORT is in use, but the health check is not responding."
    echo "Run this to clean up stale local dev processes and start again:"
    echo "  ./scripts/start-dev.sh --restart"
    exit 1
  fi
fi

if [[ "$BACKEND_ALREADY_RUNNING" -eq 0 ]]; then
  echo "Starting deepLocal backend on http://127.0.0.1:$BACKEND_PORT ..."
  cd "$ROOT_DIR"
  cargo run -p deeplocal -- serve --port "$BACKEND_PORT" &
  BACKEND_PID="$!"
fi

echo "Waiting for backend health check..."
for _ in {1..60}; do
  if service_healthy "http://127.0.0.1:$BACKEND_PORT/health"; then
    break
  fi
  sleep 1
done

if ! service_healthy "http://127.0.0.1:$BACKEND_PORT/health"; then
  echo "Backend did not become ready in time."
  exit 1
fi

if [[ "$FRONTEND_ALREADY_RUNNING" -eq 1 ]]; then
  echo "Open: http://127.0.0.1:$FRONTEND_PORT/"
  if [[ -n "$BACKEND_PID" ]]; then
    echo "Press Ctrl+C to stop the backend started by this script."
    wait "$BACKEND_PID"
  fi
  exit 0
fi

cd "$DESKTOP_DIR"
if [[ ! -d node_modules ]]; then
  echo "Installing frontend dependencies..."
  npm install
fi

echo "Starting deepLocal desktop UI..."
echo "Open: http://127.0.0.1:$FRONTEND_PORT/"
npm run dev -- --host 127.0.0.1 --port "$FRONTEND_PORT"
