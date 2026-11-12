"""Dependency-free tests for the persistent OPUS-MT worker protocol."""
import importlib.util
import pathlib
import tempfile
import types
import unittest
from unittest.mock import patch

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "scripts" / "opus_mt_translate.py"
spec = importlib.util.spec_from_file_location("opus_mt_translate", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class FakeSentencePiece:
    def __init__(self, model_file):
        self.model_file = model_file

    def encode(self, text, out_type):
        return [text]

    def decode(self, tokens):
        return " ".join(tokens)


class FakeTranslator:
    loads = []

    def __init__(self, model_dir, device):
        self.loads.append(model_dir)

    def translate_batch(self, batch, beam_size, max_decoding_length):
        assert batch[0][-1] == "</s>"
        return [types.SimpleNamespace(hypotheses=[["translated"]])]


class WorkerTests(unittest.TestCase):
    def test_validates_text(self):
        self.assertEqual(module.validated_text({"text": "hello"}), "hello")
        for invalid in ({}, {"text": ""}, {"text": " "}, {"text": 1}):
            with self.assertRaises(ValueError):
                module.validated_text(invalid)

    def test_loads_each_direction_once_and_appends_eos(self):
        FakeTranslator.loads.clear()
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            for direction in ("en-zh", "zh-en"):
                folder = root / direction
                folder.mkdir()
                for filename in ("model.bin", "source.spm", "target.spm"):
                    (folder / filename).touch()
            fake_ct2 = types.SimpleNamespace(Translator=FakeTranslator)
            fake_spm = types.SimpleNamespace(SentencePieceProcessor=FakeSentencePiece)
            with patch.dict("sys.modules", {"ctranslate2": fake_ct2, "sentencepiece": fake_spm}):
                worker = module.TranslationWorker(4)
                self.assertEqual(worker.translate(root / "en-zh", "Hello"), "translated")
                self.assertEqual(worker.translate(root / "zh-en", "你好"), "translated")
                self.assertEqual(worker.translate(root / "en-zh", "Again"), "translated")
            self.assertEqual(len(FakeTranslator.loads), 2)


if __name__ == "__main__":
    unittest.main()
