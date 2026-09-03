# deepLocal

deepLocal is an open-source, local-first AI workbench for downloading, managing,
chatting with, and serving GGUF models on your own computer.

It pairs a Rust local runtime with a React desktop-style UI. The goal is simple:
make local AI easier to run, inspect, and integrate without sending prompts or
model files to a remote service.

<img width="1440" height="621" alt="deepLocal desktop UI" src="https://github.com/user-attachments/assets/dca8f98c-2f08-451a-bf77-9df5db2191d1" />

## Features

- Search and download GGUF models from Hugging Face.
- Track download progress inline and cancel active downloads.
- Store downloaded models under `./models/`.
- Load local GGUF models through `llama.cpp`.
- Chat with loaded models in the browser UI.
- Render Markdown responses in chat.
- Expose an OpenAI-compatible local API at `http://127.0.0.1:14567/v1`.
- Keep Hugging Face tokens local to your machine.

## Quick Start

From the project root:

```bash
./scripts/start-dev.sh
```

Then open:

```text
http://127.0.0.1:5173/
```

The script starts both the backend and frontend. On macOS, it also tries to
install `llama.cpp` with Homebrew if `llama-server` is missing.

Useful commands:

```bash
./scripts/start-dev.sh --restart
./scripts/start-dev.sh --stop
./scripts/start-dev.sh --build
DEEPLOCAL_SKIP_LLAMA_INSTALL=1 ./scripts/start-dev.sh
```

## Requirements

- macOS is the best-tested development platform.
- Rust toolchain with Cargo.
- Node.js and npm.
- `curl` and `lsof`.
- Homebrew is recommended on macOS for automatic `llama.cpp` installation.

If `llama-server` is already available in `PATH`, deepLocal uses it directly.

## Local API

Base URL:

```text
http://127.0.0.1:14567/v1
```

Endpoints:

```text
GET  /v1/models
POST /v1/chat/completions
```

Example:

```bash
curl http://127.0.0.1:14567/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{
    "model": "your-loaded-model-id",
    "messages": [
      { "role": "user", "content": "Explain deepLocal in one sentence." }
    ]
  }'
```

Load a model in the UI first, then use that model ID in API calls.

## Hugging Face Access

Public model downloads work without a token. Gated models require a Hugging Face
token with read access and license acceptance for the exact repository.

You can paste the token in the Settings page or set `HF_TOKEN` /
`HUGGINGFACE_TOKEN` before starting the backend. Tokens are not stored in this
repository.

### 401 Unauthorized

A 401 usually means Hugging Face rejected the token before checking model
access. Common fixes:

- Create a fresh Hugging Face token if the old one was revoked or expired.
- Make sure the token has read permissions.
- Paste only the token value, with no extra spaces or quotes.
- If using an environment variable, restart deepLocal after changing it.

Use the Settings page token check before retrying a download.

### 403 Forbidden On Gated Repositories

A 403 usually means the token is valid, but the account does not have access to
that exact model repository.

For gated repositories such as official Gemma releases:

- Log in to Hugging Face in the browser.
- Open the exact repository you want to download from.
- Accept that repository's license or access terms.
- Use a token from the same account.
- Make sure the token has read access to public gated repositories.

Accepting a related model license is not always enough. Hugging Face can gate
each repository separately, so the account must be approved for the exact repo
shown in the download URL.

## Project Layout

```text
apps/
  cli/          Command-line entry point
  desktop/      React desktop-style UI

crates/
  api/          HTTP routes and OpenAI-compatible endpoints
  core/         Shared domain types and traits
  hardware/     Local hardware detection
  runtime/      Model runtime manager and backend adapters
  storage/      SQLite persistence

config/         Example runtime configuration
scripts/        Development helper scripts
```

## Contributing

Contributions are welcome. A good first path is:

1. Read [CONTRIBUTING.md](./CONTRIBUTING.md).
2. Run `./scripts/start-dev.sh`.
3. Pick an open issue with clear acceptance criteria.
4. Keep pull requests small and focused.

Useful local checks:

```bash
cargo check
cargo test
./scripts/start-dev.sh --build
```

Do not commit downloaded models, tokens, local databases, `target/`,
`node_modules/`, or build output.

## License

deepLocal is released under the [MIT License](./LICENSE).
