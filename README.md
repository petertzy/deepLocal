# deepLocal

deepLocal is an open-source, local-first AI workbench for downloading, managing,
chatting with, and serving GGUF models on your own computer.

It pairs a Rust local runtime with a React desktop-style UI. The goal is simple:
make local AI easier to run, inspect, and integrate without sending prompts or
model files to a remote service.

<img width="1436" height="625" alt="Image" src="https://github.com/user-attachments/assets/07ee40fd-71bb-46da-8005-76d4d0c07ce7" />

## Features

- Search and download GGUF models from Hugging Face.
- Track download progress inline and cancel active downloads.
- Store downloaded models under `./models/`.
- Load local GGUF models through `llama.cpp`.
- Chat with loaded models in the browser UI.
- Render Markdown responses in chat.
- Expose an OpenAI-compatible local API at `http://127.0.0.1:14567/v1` in development.
- Keep Hugging Face tokens local to your machine.

## Quick Start

Install the latest macOS app:

```bash
curl -fsSL https://raw.githubusercontent.com/petertzy/deepLocal/main/scripts/install-macos.sh | bash
```

The installer downloads the latest GitHub Release, installs `deepLocal.app` to
`/Applications`, removes the macOS quarantine marker when possible, and opens
the app. macOS may ask for administrator permission when replacing or copying
the app into `/Applications`.

From the project root on macOS or Linux:

```bash
./scripts/start-dev.sh
```

The first macOS/Linux launch also performs a one-time setup. Node.js and
llama.cpp are installed under `.tools/`, Rust is installed for the current
user with rustup, and missing Linux compiler utilities are installed through
the detected system package manager. macOS may request confirmation for Apple
Command Line Tools; Linux may request the user's `sudo` password.

To install prerequisites without starting the app:

```bash
bash ./scripts/setup-unix.sh
```

On Windows PowerShell (recommended; works even when `.ps1` files are blocked):

```powershell
.\scripts\start-dev.cmd
```

The first Windows launch automatically sets up the required development tools:

- Node.js LTS is downloaded into the project's `.tools/node/` directory.
- Rust and Cargo are installed for the current user with the official `rustup`
  installer.
- Microsoft C++ Build Tools are installed if missing. Windows may show one
  administrator confirmation, and this larger installation can take several
  minutes.
- The newest official llama.cpp Windows CPU package for the system architecture is downloaded into
  `.tools/llama.cpp/`, including `llama-server.exe` for local GGUF chat.

To install the prerequisites without starting the app, run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\setup-windows.ps1
```

Then open:

```text
http://127.0.0.1:5173/
```

The script starts both the backend and frontend and installs missing local
development dependencies on its first run.

Useful commands:

```bash
./scripts/start-dev.sh --restart
./scripts/start-dev.sh --stop
./scripts/start-dev.sh --build
./scripts/install-macos.sh
./scripts/package-macos-app.sh
./scripts/upload-new-app.sh
./scripts/uninstall-local.sh
DEEPLOCAL_SKIP_LLAMA_INSTALL=1 ./scripts/start-dev.sh
```

`upload-new-app.sh` automatically increments the patch version from the latest
GitHub release or local Git tag. To explicitly choose a version, set
`DEEPLOCAL_RELEASE_VERSION`, for example:

```bash
DEEPLOCAL_RELEASE_VERSION=v0.1.3 ./scripts/upload-new-app.sh
```

To preview the automatically selected version without building or uploading:

```bash
DEEPLOCAL_UPLOAD_DRY_RUN=1 ./scripts/upload-new-app.sh
```

Windows commands:

```powershell
.\scripts\start-dev.cmd -Restart
.\scripts\start-dev.cmd -Stop
.\scripts\start-dev.cmd -Build
```

You can also run the PowerShell script directly when local script execution is
enabled:

```powershell
.\scripts\start-dev.ps1
```

Use `./scripts/uninstall-local.sh --remove-llama` to also remove Homebrew
`llama.cpp` after cleaning local project artifacts.

## Packaging A macOS App

Build a shareable Tauri macOS app from the project root:

```bash
./scripts/package-macos-app.sh
```

The script creates:

```text
dist/deepLocal.app
dist/deepLocal-macos.zip
```

When macOS allows disk image creation in the current environment, the script also
creates `dist/deepLocal-macos.dmg`.

The packaged app is a native Tauri shell around the existing React UI. It starts
the local Rust API inside the app process instead of opening a browser window.
Release builds bundle the llama.cpp runtime at
`deepLocal.app/Contents/Resources/llama-runtime/`, including `llama-server` and
its macOS `.dylib` dependencies. The packaged app prefers this bundled runtime,
so a clean Mac does not need Homebrew or a separately installed `llama-server`.
Packaged builds bind the API to `127.0.0.1` on an operating-system-assigned
ephemeral port, while development servers continue to use `14567`. The app
passes the assigned port to the UI at startup, so it does not reserve or reuse a
fixed packaged-app port.
When `LLAMA_SERVER` or `DEEPLOCAL_LLAMA_SERVER` is set, deepLocal uses that
binary. On macOS, the app also checks the common Homebrew locations
`/opt/homebrew/bin/llama-server` and `/usr/local/bin/llama-server`.

At runtime, the app stores user data under:

```text
~/Library/Application Support/deepLocal
```

That folder contains the SQLite database, logs, and downloaded models. The app
still binds only to `127.0.0.1` by default. Release builds are unsigned unless
you configure Apple Developer signing and notarization, so macOS Gatekeeper may
warn on first launch.

## Requirements

- Windows 10/11, macOS, or Linux. Use the platform-specific launcher above.
- Rust toolchain with Cargo (automatically installed on Windows).
- Node.js and npm (automatically installed project-locally on Windows).
- Microsoft C++ Build Tools with the Desktop C++ workload on Windows
  (automatically installed when missing).
- llama.cpp with `llama-server` (automatically installed project-locally on
  Windows).
- On Linux, a supported package manager: apt, dnf, yum, pacman, or zypper.
- On macOS, Apple Command Line Tools (the setup script prompts when missing).

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

Development base URL:

```text
http://127.0.0.1:14567/v1
```

Packaged app base URL: the app assigns an available `127.0.0.1` port at startup;
it is intentionally not fixed and is not part of the public API contract.

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

Use **Settings > Check Token** to validate a token and view its account. In
**Models** search results, click **Check access** next to a model to check that
exact repository and file. It uses your configured token, or checks anonymous
access when no token is set. Each search row also links directly to its
Hugging Face repository for reviewing files, licenses, or requesting access. No
repository name or filename needs to be entered manually.

The desktop UI remembers the last open page, search results, sorting, form
drafts, diagnostics, chat drafts, and scroll positions across navigation,
refreshes, and app restarts. Changing the token clears previous access results
so they can be checked again with the new credentials.

Recommended token setup:

- Use a fine-grained Hugging Face access token.
- Grant read access only.
- Add access to the exact gated repositories you want to download.
- Do not grant write access for deepLocal downloads.

## Search Filters

Hugging Face GGUF search uses a safe default blocked-keyword policy to hide
models whose repository or file names match configured terms. You can inspect
the active policy and add custom blocked keywords from the Settings page.

Advanced users can customize the startup policy in a config file:

```toml
[search_filters]
blocked_keywords = ["nsfw", "uncensored", "custom-term"]
```

Keep the safe defaults unless you intentionally want to change what appears in
model search results.

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
