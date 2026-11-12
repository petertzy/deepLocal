# Experimental OPUS-MT local translation

This is an **opt-in prototype** on PR #126. It adds `POST /v1/translate`
without changing the existing GGUF/llama.cpp chat endpoints. The endpoint
runs a local Python/CTranslate2 helper on the user's machine. Models are
downloaded only after the user explicitly selects Install in the Models tab.
The translation endpoint does not call a cloud translation service.

## Scope: personal source-checkout workflow

This experimental branch is intended for a single developer running deepLocal
from a local source checkout. It is **not** intended to be merged into main or
distributed as a packaged desktop application. There is no requirement to
publish standalone Python runtimes, sign installers, or support clean end-user
machines. All inference remains local; the browser extension can use the
loopback API without loading a GGUF chat model.

### One-command startup (macOS/Linux)

The source checkout requires an installed Python 3 interpreter, Rust/Cargo,
and the existing frontend development prerequisites. The launcher manages
only the optional OPUS-MT Python packages; it does not install Python itself.

From the repository root:

```bash
bash scripts/start-opus.sh
```

On first run, the script creates an isolated virtual environment at
`.tools/opus-runtime`, installs CTranslate2 4.9.0 and SentencePiece using
pip, verifies both imports, and starts the Rust API and React development UI.
This one-time setup needs internet access. Later starts reuse the environment
without reinstalling dependencies or requiring network access for translation.
The script also skips automatic llama.cpp installation unless explicitly
overridden, because OPUS-MT does not require a GGUF chat model.

Open `http://127.0.0.1:5173/`. The API is at
`http://127.0.0.1:14567/v1`. The script never downloads or deletes model
weights; existing manually installed `models/opus-mt/en-zh` and
`models/opus-mt/zh-en` directories remain usable.

### Runtime command reference

Run each command from the deepLocal repository root. Each example includes
the full script invocation and can be copied independently.

| Action | Complete command | Notes |
| --- | --- | --- |
| Start normally | `bash scripts/start-opus.sh` | Creates missing dependencies on first run |
| Install or verify dependencies only | `bash scripts/start-opus.sh --setup` | Does not start the servers |
| Restart both development servers | `bash scripts/start-opus.sh --restart` | Applies the selected Python interpreter |
| Stop both development servers | `bash scripts/start-opus.sh --stop` | Preserves installed dependencies and models |
| Remove the managed Python runtime | `bash scripts/start-opus.sh --remove-runtime` | Stops servers; deletes only the marked `.tools/opus-runtime` |
| Show all available options | `bash scripts/start-opus.sh --help` | Does not modify files |

The removal command preserves model weights, `.venv-opus`,
`.venv-opus-convert`, other Python environments, and GGUF models.
The next normal launch recreates the managed runtime and installs its
dependencies again.

If `DEEPLOCAL_OPUS_MT_PYTHON` is explicitly set, the launcher uses that
interpreter without installing packages into it. To use an alternate virtual
environment directory, set `DEEPLOCAL_OPUS_MT_VENV`. Custom directories
are never removed by `--remove-runtime`.

### Removing old, manually created virtual environments

The original `.venv-opus` and `.venv-opus-convert` environments are not
needed by the new launcher. **First verify** that
`bash scripts/start-opus.sh --setup` succeeds and that a translation works
using the new runtime. Then stop the servers and remove those legacy
directories manually if they are no longer needed:

```bash
bash scripts/start-opus.sh --stop
rm -rf -- .venv-opus .venv-opus-convert
```

Run the removal command only from the repository root after confirming the
directory names. It does not remove `models/opus-mt` or the managed runtime.
The main branch ignores these virtual environments and does not depend on
them. If an existing `DEEPLOCAL_OPUS_MT_PYTHON` override points into an old
environment, unset it before using the managed runtime.

### Models and manual setup

Each direction needs a compatible CTranslate2 Marian model with
`model.bin`, `source.spm`, and `target.spm`. The existing manually
converted models remain supported and appear as **Installed (manual files)**.
Do not delete them just to test the downloader.

A separate one-time conversion workflow may require Transformers and PyTorch,
but those packages are **not** required for translation inference.
For a manual conversion example:

```bash
ct2-transformers-converter --model Helsinki-NLP/opus-mt-en-zh --output_dir ./models/opus-mt/en-zh
```

The tokenizer files must come from the same checkpoint; the reverse direction
requires a different checkpoint. Prefer reusing already working models.

### Browser extension

Use the experimental `translation-model-opus` pull request in
`petertzy/sovereign-bilingual-reader`. Configure the provider as
`deepLocal (OPUS-MT)`, the API base URL as
`http://127.0.0.1:14567/v1`, and a supported target language
(English or Simplified Chinese). Translation is local, but OPUS-MT may
produce awkward or incorrect translations of long sentences, names, and
dates. Check important text against the source.

## Isolated installer integration tests

The default installer smoke test uses a tiny local HTTP server that emulates
the Hugging Face model API. It does **not** download real model weights,
contact Hugging Face, modify existing installed models, or require a Python
translation runtime. It launches a separate deepLocal API with an isolated
temporary models directory, database, and port.

```bash
cargo build -p deeplocal
python3 scripts/test-opus-mt-install.py
```

This test exercises installation and progress, SHA-256 receipt verification,
detection of a locally corrupted file, deletion, rejection of a corrupt
remote checksum, rejection of missing weight checksum metadata, and
cancellation with staging-directory cleanup. All test files are small
fixtures, **not usable translation models**. The test source override is
accepted only by debug Rust builds, and only for a loopback HTTP address;
release builds always use the fixed Hugging Face URL.

A full network download remains optional:

```bash
python3 scripts/test-opus-mt-install.py --real-download --direction en-zh
```

Add `--cancel-test` to exercise cancellation against the real network
download as well. Real downloads can transfer hundreds of megabytes. The
temporary workspace is removed at the end of either test.

The CLI honors the `models.directory` setting from `--config` when
constructing the API router; this is required for test isolation and
user-configurable model storage.

## Model manager (experimental)

Open the **Models** tab and use **OPUS-MT translation models** to install
English-to-Chinese and/or Chinese-to-English models separately. These are
preconverted CTranslate2 checkpoints from
`Sams200/opus-mt-en-zh` and `Sams200/opus-mt-zh-en` on Hugging Face,
derived from Helsinki-NLP's OPUS-MT checkpoints (CC BY 4.0). Review each
model's license and attribution before redistribution.

The installer resolves the repository revision, fetches the model files into
an isolated temporary directory, checks their sizes, verifies available
Hugging Face LFS SHA-256 values (required for `model.bin`), and records
SHA-256 hashes of **all** installed files. It moves the directory into
`<models-root>/opus-mt/<direction>` only after every file passes the checks.
An interrupted or failed download does not become an installed model.
Install progress and cancellation use the existing download jobs list.

**Verify files** recomputes and compares hashes to the local installation
receipt. A manually converted legacy model can still run, but does not have
a receipt and is labelled as manually installed. **Delete** removes only the
selected translation direction after closing the translation worker.
The GGUF model catalogue and load/unload operations remain separate.

The desktop app uses the configured app-data models root, not its application
bundle. Translation model weights are never bundled in release artifacts.
The small Python worker is embedded as text in the Rust API binary so a
packaged app does not require a source checkout.

## Runtime selection

The API uses `DEEPLOCAL_OPUS_MT_PYTHON` when set, then an existing
`<models-root>/.translation-runtime` interpreter, and finally `python3`.
For this personal source-checkout workflow, the recommended and tested
option is `.venv-opus/bin/python`, selected automatically by
`scripts/start-opus.sh`. No standalone runtime distribution or packaged
app installer is planned for this branch.

### Model manager API

- `GET /runtime/translation/models` lists directions, installation state,
  receipt availability, local paths, and runtime configuration.
- `POST /runtime/translation/models/install` with
  `{"direction":"en-zh"}` queues a background download and returns a
  `job_id`. `GET /runtime/downloads` reports progress; the existing
  `POST /runtime/downloads/cancel` cancels it.
- `POST /runtime/translation/models/verify` checks all installed files
  against the receipt.
- `POST /runtime/translation/models/delete` deletes one selected direction.
- `GET /runtime/translation/runtime/check` tests Python dependencies.

The model installer needs network access only while downloading; inference
is fully local after the files and runtime are installed. The public model
repository is resolved at installation time and individual file downloads
are pinned to that resolved revision.

## Request

```bash
curl -sS -X POST http://127.0.0.1:14567/v1/translate \
  -H 'Content-Type: application/json' \
  -d '{"text":"Hello, world!","source_language":"en","target_language":"zh"}'
```

Replace the host and port with the actual deepLocal API address. A successful
response has `translation`, `source_language`, `target_language`,
`model`, and `provider` fields.

Locale variants such as `en-US` and `zh-CN` map to the base language.
Script-specific Chinese variants are not distinguished by this prototype.

The endpoint returns 404 when the requested direction is not installed,
400 for invalid input, and 502/503 for worker or dependency errors. It does
**not** fall back to another provider. The API starts one persistent Python
worker on the first translation request. The worker lazily loads and caches
each translation direction on first use, then reuses the model for subsequent
requests. Requests are serialized through a shared mutex; a failed or timed-out
request resets the worker so the next request starts a fresh process.
The first request in each direction still pays model-loading cost.
The model runs on CPU. It has not been benchmarked or validated for
translation quality across language pairs.

## Testing

```bash
cargo test -p deeplocal-api
python3 -m py_compile scripts/opus_mt_translate.py
```

These checks do not establish end-to-end inference. For that, install and
convert a real model, start deepLocal, and call the endpoint with representative
English and Chinese sentences.

The separate Sovereign Bilingual Reader extension integrates this API in its
experimental PR #15; that integration is not merged.

## Persistent worker protocol

The API launches the embedded Python worker source with `python -u -c` and
exchanges newline-delimited JSON over standard input/output. Example input:

```json
{"direction":"en-zh","text":"Hello, how are you today?"}
```

Each line returns either `{"translation":"..."}` or `{"error":"..."}`.
Only `en-zh` and `zh-en` directions are accepted. Models remain on disk and
are loaded only when that direction is first requested. The original
`--model-dir` one-shot CLI is still supported for manual checks.

To validate the persistent protocol manually:

```bash
printf '%s\n' \
  '{"direction":"en-zh","text":"Hello, how are you today?"}' \
  '{"direction":"zh-en","text":"今天天气很好。"}' \
  '{"direction":"en-zh","text":"The weather is beautiful today."}' | \
  python3 scripts/opus_mt_translate.py --models-root ./models
```
