# Experimental OPUS-MT local translation

This is an **opt-in prototype** on PR #126. It adds `POST /v1/translate`
without changing the existing GGUF/llama.cpp chat endpoints. The endpoint
runs an external Python/CTranslate2 helper on the local machine. It does not
download models or use a cloud translation service.

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
**not** fall back to another provider. Each request starts a fresh worker and
reloads the model, so this prototype is **not optimized for latency**.
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

The existing browser extension has **not** been modified to call this endpoint.
