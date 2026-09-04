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
./scripts/uninstall-local.sh
DEEPLOCAL_SKIP_LLAMA_INSTALL=1 ./scripts/start-dev.sh
```

Use `./scripts/uninstall-local.sh --remove-llama` to also remove Homebrew
`llama.cpp` after cleaning local project artifacts.

## Requirements

- macOS is the best-tested development platform.
- Rust toolchain with Cargo.
- Node.js and npm.
- `curl` and `lsof`.
- Homebrew is recommended on macOS for automatic `llama.cpp` installation.

If `llama-server` is already available in `PATH`, deepLocal uses it directly.

## Network Access

deepLocal binds the API to `127.0.0.1` by default, so only local apps on the same
computer can call it.

Advanced users can opt in to LAN access with either a CLI flag:

```bash
cargo run -p deeplocal -- serve --host 0.0.0.0
```

or a config file:

```toml
[server]
host = "0.0.0.0"
port = 14567
enable_cors = true
```

Binding to `0.0.0.0` exposes the API to other devices that can reach your
machine. Those clients may send prompts to loaded models and read local model
responses. Only enable it on trusted networks, and prefer `127.0.0.1` for normal
desktop use. deepLocal prints a warning when public binding is enabled.

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

Python with the OpenAI SDK:

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:14567/v1",
    api_key="not-needed",
)

response = client.chat.completions.create(
    model="your-loaded-model-id",
    messages=[
        {"role": "user", "content": "Explain deepLocal in one sentence."},
    ],
)

print(response.choices[0].message.content)
```

JavaScript or TypeScript with the OpenAI SDK:

```ts
import OpenAI from "openai";

const client = new OpenAI({
  baseURL: "http://127.0.0.1:14567/v1",
  apiKey: "not-needed",
});

const response = await client.chat.completions.create({
  model: "your-loaded-model-id",
  messages: [
    { role: "user", content: "Explain deepLocal in one sentence." },
  ],
});

console.log(response.choices[0]?.message?.content);
```

## Hugging Face Access

Public model downloads work without a token. Gated models require a Hugging Face
token with read access and license acceptance for the exact repository.

You can paste the token in the Settings page or set `HF_TOKEN` /
`HUGGINGFACE_TOKEN` before starting the backend. Tokens are not stored in this
repository.

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
