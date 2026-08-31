# deepLocal

deepLocal is an open-source local AI runtime and desktop studio for running,
managing, serving, and benchmarking open models on your own hardware.

## Start The App

Run this from the project root:

```bash
./scripts/start-dev.sh
```

Then open:

```text
http://127.0.0.1:5173/
```

Local API base URL:

```text
http://127.0.0.1:14567/v1
```

Keep the terminal open while using the web UI. Press `Ctrl+C` in that terminal to
stop both the frontend and the backend.

If the servers were started earlier and your terminal prompt has already
returned, stop them with:

```bash
./scripts/start-dev.sh --stop
```

The script does the routine setup for you:

- Checks that `cargo`, `npm`, `curl`, and `lsof` are available.
- Uses a system-installed `llama-server` when it is available in `PATH`.
- Installs `llama.cpp` automatically on macOS with Homebrew when `llama-server`
  is missing.
- Starts the backend runtime on `http://127.0.0.1:14567`.
- Installs frontend dependencies if `apps/desktop/node_modules` is missing.
- Starts the desktop UI on `http://127.0.0.1:5173/`.

If a previous local server got stuck, restart both local dev servers:

```bash
./scripts/start-dev.sh --restart
```

To skip automatic `llama.cpp` installation:

```bash
DEEPLOCAL_SKIP_LLAMA_INSTALL=1 ./scripts/start-dev.sh
```

## If The Web Page Does Not Open

`http://127.0.0.1:5173/` only works while the Vite frontend server is running.
If the browser shows no response, the most common cause is that the server was
stopped, the terminal was closed, the computer restarted, or `node_modules` was
removed during cleanup.

First, run:

```bash
./scripts/start-dev.sh
```

If the script says port `5173` is already in use and the page is responding, the
UI is already running. Open:

```text
http://127.0.0.1:5173/
```

If the script says port `5173` is in use but the web page is not responding, run:

```bash
./scripts/start-dev.sh --restart
```

To check the port manually:

```bash
lsof -nP -iTCP:5173 -sTCP:LISTEN
```

If this prints nothing, nothing is listening on port `5173`, so the frontend
server needs to be started again.

## Manual Startup

Use this only if the one-command script is not suitable.

Start the backend runtime in one terminal:

```bash
cargo run -p deeplocal -- serve --port 14567
```

Start the frontend in another terminal:

```bash
cd apps/desktop
npm install
npm run dev -- --host 127.0.0.1
```

After Vite prints `Local: http://127.0.0.1:5173/`, refresh the browser.

## Local API Access

deepLocal exposes an OpenAI-compatible local API.

Base URL:

```text
http://127.0.0.1:14567/v1
```

Endpoints:

```text
GET  http://127.0.0.1:14567/v1/models
POST http://127.0.0.1:14567/v1/chat/completions
```

The desktop Server page shows these URLs and provides copy buttons. Load a GGUF
model first, then use its model ID in local API calls.

## Model Storage

Downloaded models are stored under:

```text
./models/
```

For example, a Hugging Face model may be saved under a repository-specific
subfolder such as:

```text
./models/google__gemma-3-1b-it-qat-q4_0-gguf/
```

The app registers completed downloads with the local runtime so they can be
loaded from the UI.

## llama.cpp During Development And Packaging

For local development, deepLocal expects `llama-server` to be installed on the
system and available in `PATH`. If `llama-server` is found,
`./scripts/start-dev.sh` automatically sets `DEEPLOCAL_LLAMA_SERVER` to that
system executable before starting the backend.

If `llama-server` is not installed, `./scripts/start-dev.sh` tries to install
`llama.cpp` automatically on macOS with Homebrew. If Homebrew is not available,
the app can still open, but real GGUF chat will not work until llama.cpp is
installed and a GGUF model is downloaded or registered.

This development setup does not decide how the final app is distributed. A
packaged macOS app should bundle `llama-server` and its required `.dylib` files
inside the app bundle, for example:

```text
deepLocal.app/
  Contents/
    Resources/
      bin/
        llama-server
        libllama.dylib
        libggml.dylib
        libggml-metal.dylib
```

With that packaging approach, a fresh Mac does not need a system-wide llama.cpp
installation to run deepLocal.

## Hugging Face Access

The desktop Models page can search Hugging Face GGUF repositories and start
background downloads.

Search results are filtered before they reach the UI. deepLocal excludes
uncensored/NSFW-style model names.

Download progress appears inline on the model file row. While a file is queued,
starting, or downloading, use the same row's `Stop` button to cancel it. A
cancelled download removes the partial local file and can be retried from the
same row.

Some official repositories, including Google Gemma models, are gated on Hugging
Face. To download them:

- Log in to Hugging Face in your browser.
- Accept the license for the exact model repository.
- Create or use a token with read access.
- Paste the token into the desktop Settings page, or set `HF_TOKEN` /
  `HUGGINGFACE_TOKEN` before starting the backend.

deepLocal sends the token only to the local runtime API. The runtime uses it as a
Bearer token when calling Hugging Face.

Example search:

```bash
curl 'http://127.0.0.1:14567/runtime/huggingface/search?query=Gemma%203%201B%20GGUF&limit=2'
```

## Quick Runtime Commands

```bash
cargo run -p deeplocal -- hardware
cargo run -p deeplocal -- serve --port 14567
cargo run -p deeplocal -- --config config/deeplocal.example.toml serve
cargo run -p deeplocal -- models describe-local gemma3 ./models/gemma3.gguf
```

Test the local API:

```bash
curl http://127.0.0.1:14567/health
```

Streaming chat:

```bash
curl http://127.0.0.1:14567/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{
    "model": "your-loaded-model-id",
    "stream": true,
    "messages": [
      { "role": "user", "content": "Could you please introduce yourself in detail? Thank you." }
    ]
  }'
```

## Runtime API

Development endpoints:

```text
GET  /health
GET  /runtime/hardware
GET  /runtime/models
POST /runtime/models
GET  /runtime/models/loaded
POST /runtime/models/load
POST /runtime/models/unload
GET  /runtime/huggingface/search?query=<term>
POST /runtime/huggingface/download
GET  /runtime/downloads
```

OpenAI-compatible endpoints:

```text
GET  /v1/models
POST /v1/chat/completions
```

## Project Scope

This repository starts with a v0.1 foundation:

- Rust workspace with separated core, hardware, storage, runtime, and API crates.
- OpenAI-compatible `POST /v1/chat/completions` endpoint.
- Runtime backend abstraction.
- Mock inference backend for development and API testing.
- Hardware profile endpoint.
- SQLite schema for models, sessions, messages, and benchmarks.
- React desktop UI source skeleton.


## Workspace Layout

```text
apps/
    cli/                Command line entry point
    desktop/            React/Tauri-ready desktop UI

crates/
    core/               Domain types and runtime traits
    hardware/           CPU/RAM/GPU detection
    storage/            SQLite persistence
    runtime/            Runtime manager and backend adapters
    api/                HTTP and OpenAI-compatible API

config/
    deeplocal.example.toml
```

## Design Principles

- Runtime first.
- Local-first state and configuration.
- Backend adapters instead of hard-coded engines.
- OpenAI-compatible API from the beginning.
- Desktop UI as a client over a reusable core.
