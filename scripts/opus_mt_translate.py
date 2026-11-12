#!/usr/bin/env python3
"""Local OPUS-MT inference using a converted CTranslate2 Marian model.

Install dependencies: python3 -m pip install ctranslate2 sentencepiece
Convert a Helsinki-NLP Marian model separately with ct2-transformers-converter.
This helper never downloads models or sends text to a network service.
"""
import argparse
import json
import pathlib
import sys


def translate(model_dir: pathlib.Path, text: str, beam_size: int) -> str:
    import ctranslate2
    import sentencepiece as spm

    source = spm.SentencePieceProcessor(model_file=str(model_dir / "source.spm"))
    target = spm.SentencePieceProcessor(model_file=str(model_dir / "target.spm"))
    translator = ctranslate2.Translator(str(model_dir), device="cpu")
    tokens = source.encode(text, out_type=str) + ["</s>"]
    result = translator.translate_batch(
        [tokens], beam_size=beam_size, max_decoding_length=512
    )[0]
    decoded = target.decode(result.hypotheses[0])
    return decoded.strip()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", required=True, type=pathlib.Path)
    parser.add_argument("--beam-size", type=int, default=4)
    args = parser.parse_args()
    if args.beam_size < 1 or args.beam_size > 16:
        parser.error("beam-size must be between 1 and 16")
    try:
        request = json.load(sys.stdin)
        text = request["text"]
        if not isinstance(text, str) or not text.strip():
            raise ValueError("text must be a non-empty string")
        if not (args.model_dir / "model.bin").is_file():
            raise FileNotFoundError("model.bin is missing from the converted model")
        if not (args.model_dir / "source.spm").is_file():
            raise FileNotFoundError("source.spm is missing from the converted model")
        if not (args.model_dir / "target.spm").is_file():
            raise FileNotFoundError("target.spm is missing from the converted model")
        output = translate(args.model_dir, text, args.beam_size)
        print(json.dumps({"translation": output}, ensure_ascii=False))
        return 0
    except Exception as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
