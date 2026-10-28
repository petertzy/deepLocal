# deepLocal

deepLocal is an open-source, local-first AI workbench for downloading, managing,
chatting with, and serving GGUF models on your own computer.

It pairs a Rust local runtime with a React desktop-style UI. The goal is simple:
make local AI easier to run, inspect, and integrate without sending prompts or
model files to a remote service.

<img width="1432" height="745" alt="Image" src="https://github.com/user-attachments/assets/ebaddb08-39c0-4990-81f0-97d47dafa93b" />

## Features

- Search and download GGUF models from Hugging Face.
- Track download progress inline and cancel active downloads.
- Store downloaded models under `./models/`.
- Load local GGUF models through `llama.cpp`.
- Chat with loaded models in the browser UI.
- Index local text documents and retrieve relevant excerpts during chat.
- Render Markdown responses in chat.
- Expose an OpenAI-compatible local API at `http://127.0.0.1:14567/v1` in development.
- Keep Hugging Face tokens local to your machine.

## Quick Start

### Install the packaged app

Desktop installers are currently published in separate releases by platform:

| Platform | Version | Download |
| --- | --- | --- |
| Windows x64 | v0.1.13 | [Windows installer (.exe)](https://github.com/petertzy/deepLocal/releases/tag/v0.1.13) |
| Linux x64 (Debian/Ubuntu) | v0.1.13 | [DEB package (.deb)](https://github.com/petertzy/deepLocal/releases/tag/v0.1.13) |
| Linux x64 (portable) | v0.1.13 | [AppImage](https://github.com/petertzy/deepLocal/releases/tag/v0.1.13) |
| macOS | v0.1.14 | [macOS release](https://github.com/petertzy/deepLocal/releases/tag/v0.1.14) |

Open the linked release and select the installer for your operating system
under **Assets**. Windows users should choose the `.exe` installer. Debian and
Ubuntu users can install the `.deb` package; users of other Linux distributions
may prefer the `.AppImage` file. The AppImage may require additional FUSE
compatibility libraries on some distributions.

**Release versions currently differ across platforms.** GitHub's
[latest release](https://github.com/petertzy/deepLocal/releases/latest)
points to the newest published release, not necessarily the newest installer
for every operating system. In particular, v0.1.14 is the macOS release,
while the Windows and Linux installers are available under v0.1.13.
The Windows and Linux installers are intended for manual installation and do
not currently provide signed Tauri in-app updates.

Install the latest macOS app:

```bash
curl -fsSL https://raw.githubusercontent.com/petertzy/deepLocal/main/scripts/install-macos.sh | bash
```

The installer downloads the latest GitHub Release, installs `deepLocal.app` to
`/Applications`, removes the macOS quarantine marker when possible, and opens
the app. macOS may ask for administrator permission when replacing or copying
the app into `/Applications`.

### If macOS blocks the downloaded app

Current public macOS builds may not be notarized. Only use this workaround if
you downloaded deepLocal from the official
[deepLocal GitHub Releases](https://github.com/petertzy/deepLocal/releases) page
and trust that build. Move `deepLocal.app` to `/Applications`, then remove its
quarantine attribute in Terminal:

```bash
xattr -dr com.apple.quarantine "/Applications/deepLocal.app"
```

This removes Gatekeeper's download quarantine marker from this app bundle. It
does not notarize or code-sign the app. If you are unsure whether the download
is authentic, do not run this command; verify the release and its published
SHA-256 checksum first.

### Run from source

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

For the full list of helper scripts grouped by purpose, see [Scripts](#scripts).

## Document Chat

Open **Documents** and add local `.txt`, `.md`, `.markdown`, `.rst`, `.csv`,
`.json`, `.yaml`, `.yml`, `.html`, `.htm`, or `.log` files. deepLocal chunks
the readable text, creates deterministic local vectors, and stores both the
chunks and vectors in its local SQLite database. No document content or
embeddings are sent to a remote service.

In **Chat**, the **Documents** control is enabled when the local index contains
files. When enabled, deepLocal retrieves the most relevant excerpts for each
question before sending it to the loaded model. Turn that control off for a
normal model-only chat. Removing an item from **Documents** removes its local
index entries but never deletes the original file.

The local runtime also exposes document endpoints for integrations:

```text
GET  /runtime/documents
POST /runtime/documents
POST /runtime/documents/search
POST /runtime/documents/delete
```

`POST /v1/chat/completions` uses indexed documents by default. Set
`"use_documents": false` in a request to disable retrieval for that completion.

## Packaging A macOS App

Build a shareable Tauri macOS app from the project root:

```bash
./scripts/package-macos-app.sh
```

For local packaging/testing, this command builds an unsigned app when
`TAURI_SIGNING_PRIVATE_KEY` is not set. The local build intentionally skips
updater artifacts. Use `./scripts/upload-new-app.sh` with the signing variables
configured when creating a release that supports in-app updates.

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

### Updating the packaged app

Packaged builds include the Tauri updater. From the **Settings** tab, choose
**Check for updates** to check the latest GitHub Release and download/install a
new signed app bundle without manually deleting the existing app. The updater
uses this endpoint:

```text
https://github.com/petertzy/deepLocal/releases/latest/download/latest.json
```

Updater artifacts must be signed. Before publishing, generate a Tauri signing
key once and keep the private key outside the repository:

```bash
cd apps/desktop
npm run tauri signer generate -w ~/.tauri/deepLocal.key
```

The command writes the private key to:

```text
~/.tauri/deepLocal.key
```

and writes the matching public key to:

```text
~/.tauri/deepLocal.key.pub
```

Do not commit either file. The repository ignores `*.key` and `*.key.pub`.

Configure these environment variables when running the release script:

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/deepLocal.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="..." # if the key has a password
export TAURI_UPDATER_PUBLIC_KEY="$(cat ~/.tauri/deepLocal.key.pub)"
./scripts/upload-new-app.sh
```

Alternatively, the release script automatically reads those two default files,
so after generating the key you can simply run:

```bash
./scripts/upload-new-app.sh
```

Custom locations are supported with `TAURI_SIGNING_PRIVATE_KEY_FILE` and
`TAURI_UPDATER_PUBLIC_KEY_FILE`. The private key can also be supplied directly
through `TAURI_SIGNING_PRIVATE_KEY` (for example, from a CI secret).

The release script uploads the `.app.tar.gz`, `.sig`, and `latest.json` updater
artifacts in addition to the ZIP and DMG. Without the signing key and public key,
the script refuses to publish because an unsigned updater would not be safe.

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
it is intentionally not fixed and is not part of the public API contract. Copy
the actual base URL shown on the app's **Server** page when using the packaged
app. The fixed URL above is for development mode.

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

For incremental output, set `stream` to `true` and keep curl running with
`-N`. Each `data:` event is an OpenAI-compatible completion chunk; a successful
request ends with a finish chunk followed by `data: [DONE]`. Errors that happen
after streaming starts are sent as an SSE `data:` error event, while errors
before the stream starts use the normal HTTP error response.

```bash
curl -N http://127.0.0.1:14567/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{
    "model": "your-loaded-model-id",
    "stream": true,
    "messages": [{ "role": "user", "content": "Explain deepLocal in one sentence." }]
  }'
```

Load a model in the UI first, then use its model ID in API calls. Find and copy
the exact ID on the **Server** page; model IDs are not necessarily the model's
display name.

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

### Troubleshooting Connection Errors

- **Connection refused / failed to fetch:** Make sure deepLocal is running and
  its local API is online. In development, start it with `./scripts/start-dev.sh`.
  In the packaged app, use the base URL displayed on the **Server** page because
  its port is assigned dynamically.
- **The health check does not respond:** For the development server, run
  `curl http://127.0.0.1:14567/health`. If it fails, restart deepLocal and check
  the backend terminal output or the app's **Server** page for its status.
- **The request reaches the API but returns an error:** Check that the model is
  loaded and that the `model` value exactly matches an ID shown on the
  **Server** page. `GET /v1/models` lists the models currently available to API
  clients.
- **A client on another computer cannot connect:** The API binds to
  `127.0.0.1` by default and accepts local connections only. LAN access requires
  explicitly changing the host binding; see [Network Access](#network-access)
  and only enable it on a trusted network.

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

## Scripts

Helper scripts grouped by purpose. See [Project Layout](#project-layout) for where
they live.

### Developing

```bash
./scripts/start-dev.sh --restart
./scripts/start-dev.sh --stop
./scripts/start-dev.sh --build
DEEPLOCAL_SKIP_LLAMA_INSTALL=1 ./scripts/start-dev.sh
```

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

### Packaging and releases

### Desktop release runbook (maintainers)

Windows/Linux packaging and publishing are **two separate, manually triggered**
GitHub Actions workflows. Neither starts the other automatically. The macOS
release is built and uploaded locally from a Mac.

#### Publish Windows and Linux

1. Merge the intended changes into `main`. Open
   [GitHub Actions](https://github.com/petertzy/deepLocal/actions), select
   [Desktop packaging](https://github.com/petertzy/deepLocal/actions/workflows/desktop-packaging.yml),
   choose **Run workflow**, select `main`, and start the run.
2. Wait until **both** `Package ubuntu-22.04` and `Package windows-2022`
   succeed. Open the run and inspect its artifacts. Each platform artifact
   contains a `release-version.txt` manifest; both must contain the same
   `vMAJOR.MINOR.PATCH` version. The Linux build produces a `.deb` and an
   `.AppImage`; Windows produces an NSIS `.exe`.
3. Confirm the proposed version is unused and the build corresponds to the
   commit you intend to release. The packaging workflow calculates the next
   patch version from published semantic-version GitHub Releases and embeds
   that version in both installers. For example, if the newest release is
   `v0.1.14`, the next packaged version is normally `v0.1.15`.
4. Select
   [Publish desktop release](https://github.com/petertzy/deepLocal/actions/workflows/publish-desktop-release.yml)
   and choose **Run workflow** on `main`. No input fields are required.
   The publisher selects the **latest successful manually triggered**
   `Desktop packaging` run, downloads its artifacts, checks that both
   manifests match, validates the three installers, and creates a GitHub
   Release tagged with the packaged version at the packaging run's commit.
5. Open [GitHub Releases](https://github.com/petertzy/deepLocal/releases)
   and verify the new tag and all three installer assets. Download and
   smoke-test the Windows and Linux installers on suitable machines.

**Important safeguards and limitations:**

- Do not run another successful packaging job between the intended build and
  publishing: the publisher has no run-selection input and always picks the
  latest successful packaging run, which may not be the run you intended.
- The publisher refuses to overwrite an existing tag or Release. If another
  release was created after packaging, run **Desktop packaging** again to
  calculate a fresh version; do not rename old installer files or bypass the
  version checks.
- Packaging artifacts expire after 14 days. Rebuild if artifacts have expired
  or if their version is incorrect. Re-running only the publisher cannot
  repair installers built with the wrong embedded version.
- If packaging fails to determine the latest semantic-version Release, it
  stops rather than guessing `v0.1.0`. Inspect the **Determine next release
  version** step and the existing release tags.
- Windows/Linux installer publication does not currently provide signed Tauri
  updater artifacts. The macOS updater uses its own signed release assets.

#### Publish macOS

On a Mac with the existing Tauri signing key and public key configured,
choose one of the following release modes.

**Create a new macOS-only patch release:** from the intended source revision,
run:

```bash
./scripts/upload-new-app.sh
```

The script normally increments the highest semantic version found in GitHub
Releases or local Git tags. Check the selected version before publishing:

```bash
DEEPLOCAL_UPLOAD_DRY_RUN=1 ./scripts/upload-new-app.sh
```

**Add macOS assets to an existing Windows/Linux release:** use the *same
commit* as the release tag, then explicitly select its version:

```bash
git fetch origin --tags
git checkout vX.Y.Z
DEEPLOCAL_RELEASE_VERSION=vX.Y.Z ./scripts/upload-new-app.sh
```

Replace `vX.Y.Z` with the actual existing tag. The script checks that
`HEAD` matches that tag before uploading to an existing Release. It uploads
the macOS ZIP and DMG, signed updater archive and signature, and
`latest.json`. It does **not** replace existing assets; if a macOS asset
with the same name already exists, resolve that conflict before retrying.

**Release coordination:** to ship all platforms under one version, first
publish Windows/Linux, then check out that exact release tag on the Mac and
upload macOS assets with `DEEPLOCAL_RELEASE_VERSION`. A normal macOS upload
without this override creates the next patch release instead. That is why
Windows/Linux `v0.1.13` and macOS `v0.1.14` currently appear as separate
releases. This split is acceptable temporarily, but keep the
[platform download table](#install-the-packaged-app) up to date when versions
diverge.

#### Troubleshooting the release sequence

- **Desktop packaging is missing from Actions:** check that
  `.github/workflows/desktop-packaging.yml` exists on the default branch
  and is valid YAML; malformed workflow files may not appear in the Actions
  sidebar.
- **Publisher reports an existing tag:** the downloaded installers were built
  for an already published version. Fix the version calculation if necessary,
  then run packaging again and publish the new artifacts.
- **Publisher cannot find manifests or installers:** verify both packaging
  jobs succeeded, their artifacts have not expired, and each artifact
  includes `release-version.txt`.
- **macOS upload fails because the commit differs:** check out the release
  tag and rebuild on that exact commit. Do not force-upload binaries built
  from a different revision.

```bash
./scripts/package-macos-app.sh
./scripts/upload-new-app.sh
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

### Installing and cleaning up

```bash
./scripts/install-macos.sh
./scripts/uninstall-local.sh
```

Use `./scripts/uninstall-local.sh --remove-llama` to also remove Homebrew
`llama.cpp` after cleaning local project artifacts.

## Contributing

Contributions are welcome. A good first path is:

1. Read [CONTRIBUTING.md](./CONTRIBUTING.md).
2. Run `./scripts/start-dev.sh`.
3. Pick an open issue with clear acceptance criteria.
4. Keep pull requests small and focused.

Useful local checks:

```bash
./scripts/check.sh
```

Formatting commands:

```bash
# Rust
cargo fmt --all
cargo fmt --all -- --check

# Frontend
cd apps/desktop
npm run format
npm run format:check
```

`./scripts/check.sh` runs the common Rust and frontend formatting, type,
build, and test checks together.

Do not commit downloaded models, tokens, local databases, `target/`,
`node_modules/`, or build output.

## Learning Resources

Want to learn more about the technologies behind local AI inference?

Explore our [Learning Resources](docs/learning-resources.md) for curated references on llama.cpp, GGUF, model quantization, GPU acceleration, and inference optimization.

## License

deepLocal is released under the [MIT License](./LICENSE).
