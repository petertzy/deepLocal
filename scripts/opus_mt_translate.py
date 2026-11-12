#!/usr/bin/env python3
"""Offline OPUS-MT translation using converted CTranslate2 Marian models.

--serve uses newline-delimited JSON on stdin/stdout and caches both directions.
--model-dir retains the original single-request CLI for manual testing.
"""
import argparse
import json
import pathlib
import sys


class TranslationWorker:
    def __init__(self, beam_size: int):
        self.beam_size = beam_size
        self.models = {}

    def translate(self, model_dir: pathlib.Path, text: str) -> str:
        key = str(model_dir.resolve())
        if key not in self.models:
            import ctranslate2
            import sentencepiece as spm

            for filename in ("model.bin", "source.spm", "target.spm"):
                if not (model_dir / filename).is_file():
                    raise FileNotFoundError(f"{filename} is missing from {model_dir}")
            source = spm.SentencePieceProcessor(model_file=str(model_dir / "source.spm"))
            target = spm.SentencePieceProcessor(model_file=str(model_dir / "target.spm"))
            translator = ctranslate2.Translator(str(model_dir), device="cpu")
            self.models[key] = (source, target, translator)

        source, target, translator = self.models[key]
        tokens = source.encode(text, out_type=str) + ["</s>"]
        result = translator.translate_batch(
            [tokens], beam_size=self.beam_size, max_decoding_length=512
        )[0]
        return target.decode(result.hypotheses[0]).strip()


def validated_text(request: object) -> str:
    if not isinstance(request, dict) or not isinstance(request.get("text"), str):
        raise ValueError("text must be a non-empty string")
    text = request["text"]
    if not text.strip() or len(text.encode("utf-8")) > 16384:
        raise ValueError("text must contain 1 to 16384 bytes")
    return text


def main() -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--model-dir", type=pathlib.Path)
    group.add_argument("--models-root", type=pathlib.Path)
    parser.add_argument("--beam-size", type=int, default=4)
    args = parser.parse_args()
    if not 1 <= args.beam_size <= 16:
        parser.error("beam-size must be between 1 and 16")
    worker = TranslationWorker(args.beam_size)
    if args.model_dir is not None:
        try:
            text = validated_text(json.load(sys.stdin))
            translation = worker.translate(args.model_dir, text)
            print(json.dumps({"translation": translation}, ensure_ascii=False))
            return 0
        except Exception as error:
            print(str(error), file=sys.stderr)
            return 1

    root = args.models_root.resolve()
    for line in sys.stdin:
        try:
            request = json.loads(line)
            text = validated_text(request)
            direction = request.get("direction")
            if direction not in ("en-zh", "zh-en"):
                raise ValueError("unsupported translation direction")
            model_dir = (root / "opus-mt" / direction).resolve()
            if not model_dir.is_relative_to(root):
                raise ValueError("model directory escapes models root")
            response = {"translation": worker.translate(model_dir, text)}
        except Exception as error:
            response = {"error": str(error)}
        print(json.dumps(response, ensure_ascii=False), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
