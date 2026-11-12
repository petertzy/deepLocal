# Experimental OPUS-MT local translation

This is an **opt-in prototype** on PR #126. It adds `POST /v1/translate`
without changing the existing GGUF/llama.cpp chat endpoints. The endpoint
runs a local Python/CTranslate2 helper on the user's machine. Models are
downloaded only after the user explicitly selects Install in the Models tab.
The translation endpoint does not call a cloud translation service.

## Prerequisites

- Python 3 with `ctranslate2` and `sentencepiece` installed.
- A **Marian OPUS-MT model converted to CTranslate2 format** for each translation
  direction. Original Hugging Face PyTorch weights are not directly supported.
- A converted model directory containing `model.bin`, `source.spm`, and
  `target.spm`. The SentencePiece files must match the original model.

Install the worker packages into your chosen Python environment:

```bash
python3 -m pip install ctranslate2 sentencepiece
```

Conversion is a separate **one-time preparation step** and may require
`transformers`, `torch`, and `sentencepiece` on the conversion machine.
For example, after obtaining a compatible Marian checkpoint:

```bash
ct2-transformers-converter --model Helsinki-NLP/opus-mt-en-zh --output_dir ./models/opus-mt/en-zh
```

Ensure `source.spm` and `target.spm` from the **same checkpoint** are
present in that output directory. Converter behavior and filenames can vary
by model and converter version; verify the files before running the API.
For reverse translation, convert a separate Chinese-to-English checkpoint
into `./models/opus-mt/zh-en`. A forward model is not reversible.

If deepLocal uses a custom models directory, place the `opus-mt` folder
inside that configured directory. If the worker packages live in a virtual
environment, point to its interpreter:

```bash
export DEEPLOCAL_OPUS_MT_PYTHON=/absolute/path/to/venv/bin/python
```

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

## Runtime deployment strategy

Model download and runtime installation are intentionally separate:

1. **Current development mode:** run Python with CTranslate2 and SentencePiece
   installed and set `DEEPLOCAL_OPUS_MT_PYTHON` to the interpreter. Use
   **Check runtime** in Models to test these imports. Neither PyTorch nor
   Transformers is required to run preconverted checkpoints.
2. **Production deployment (not implemented):** publish a versioned, signed
   per-platform translation worker/runtime archive as an optional download
   into the application data directory. Verify its pinned SHA-256 before
   activation, keep it outside the app bundle, show progress, and allow
   removal. Do not execute arbitrary installation commands or silently run
   `pip install` on an end user's machine.
3. A production worker should ideally use the CTranslate2 native runtime
   directly (or a tightly controlled standalone Python runtime). Keep model
   binaries and the runtime separately updatable. Validate macOS universal/
   arm64, Windows, and Linux builds independently before enabling a one-click
   runtime installation button.

Until the optional runtime distribution exists, the Models page downloads
and verifies model data but does **not** promise a ready-to-use translation
environment on clean end-user installations. There is no cloud translation
fallback.

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
curl -sS -X POST http://127.0.0.1:8080/v1/translate \
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
