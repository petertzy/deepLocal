#!/usr/bin/env python3
"""Opt-in integration smoke test for OPUS-MT model management.

Runs a separate deepLocal server with an isolated temporary models directory,
database, and port. Never touches the user's existing model files.

This test downloads real model weights and requires network access. It is not
part of the default unit test or CI suite.
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def api(base, method, path, payload=None):
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        base + path,
        data=data,
        method=method,
        headers={"Content-Type": "application/json", "Origin": "http://127.0.0.1:5173"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            body = response.read()
            return response.status, json.loads(body) if body else None
    except urllib.error.HTTPError as error:
        raise RuntimeError(
            f"{method} {path}: HTTP {error.code}: {error.read().decode('utf-8', 'replace')}"
        ) from error


def unused_port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def await_job(base, job_id, timeout):
    deadline = time.monotonic() + timeout
    last_status = None
    last_percent = -1
    observed_progress = False
    while time.monotonic() < deadline:
        _, jobs = api(base, "GET", "/runtime/downloads")
        job = next((entry for entry in jobs if entry["id"] == job_id), None)
        if job is None:
            raise RuntimeError(f"Download job {job_id} disappeared")
        status = job["status"]
        downloaded = job.get("downloaded_bytes", 0)
        total = job.get("total_bytes") or 0
        percent = round(downloaded / total * 100) if total else None
        if downloaded > 0:
            observed_progress = True
        if status != last_status or (percent is not None and percent >= last_percent + 10):
            print(f"  {status}: {downloaded / 1048576:.1f} MB"
                  + (f" / {total / 1048576:.1f} MB ({percent}%)" if total else ""),
                  flush=True)
            last_status = status
            if percent is not None:
                last_percent = percent
        if status in ("downloaded", "cancelled", "error"):
            return job, observed_progress
        time.sleep(0.6)
    raise TimeoutError(f"Download job {job_id} exceeded {timeout}s")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--direction", choices=("en-zh", "zh-en"), default="en-zh")
    parser.add_argument("--timeout", type=int, default=1800)
    parser.add_argument(
        "--cancel-test", action="store_true",
        help="Also exercise cancellation using the opposite direction (may download more data).",
    )
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    binary = root / "target" / "debug" / "deeplocal"
    if not binary.is_file():
        print("Build the CLI first: cargo build -p deeplocal", file=sys.stderr)
        return 2

    with tempfile.TemporaryDirectory(prefix="deeplocal-opus-integration-") as tmp:
        workspace = Path(tmp)
        models = workspace / "models"
        models.mkdir()
        config = workspace / "config.toml"
        config.write_text(
            '[models]\n'
            f'directory = {json.dumps(str(models))}\n'
            'auto_load_last_model = false\n',
            encoding="utf-8",
        )
        port = unused_port()
        base = f"http://127.0.0.1:{port}"
        log = workspace / "server.log"
        with log.open("w", encoding="utf-8") as output:
            process = subprocess.Popen(
                [str(binary), "--config", str(config), "serve",
                 "--host", "127.0.0.1", "--port", str(port)],
                cwd=workspace,
                stdout=output,
                stderr=subprocess.STDOUT,
                env=os.environ.copy(),
            )
            try:
                for _ in range(100):
                    if process.poll() is not None:
                        raise RuntimeError("Isolated server exited unexpectedly")
                    try:
                        api(base, "GET", "/health")
                        break
                    except (OSError, RuntimeError):
                        time.sleep(0.2)
                else:
                    raise TimeoutError("Isolated server did not start")

                _, catalog = api(base, "GET", "/runtime/translation/models")
                entry = next(m for m in catalog["models"] if m["direction"] == args.direction)
                expected_path = models / "opus-mt" / args.direction
                assert Path(entry["directory"]).resolve() == expected_path.resolve(), (
                    "API is not using the isolated models directory"
                )
                assert not entry["installed"] and not entry["directory_exists"]
                print(f"Isolated models root: {models}")
                print(f"Installing {args.direction}...")
                status, response = api(base, "POST", "/runtime/translation/models/install",
                                       {"direction": args.direction})
                assert status == 202
                job, progressed = await_job(base, response["job_id"], args.timeout)
                if job["status"] != "downloaded":
                    raise RuntimeError(f"Installation failed: {job.get('error') or job['status']}")
                assert progressed, "No downloaded bytes were reported"
                assert expected_path.is_dir()
                assert (expected_path / ".deeplocal-opus-mt.json").is_file()
                _, verification = api(base, "POST", "/runtime/translation/models/verify",
                                      {"direction": args.direction})
                assert verification["verified"], verification
                print("PASS: download, progress, manifest and SHA-256 verification")

                # Corrupt a small file in the isolated copy and verify detection.
                readme = expected_path / "README.md"
                original = readme.read_bytes()
                try:
                    readme.write_bytes(original + b"\nchecksum test\n")
                    try:
                        api(base, "POST", "/runtime/translation/models/verify",
                            {"direction": args.direction})
                    except RuntimeError as error:
                        assert "HTTP 409" in str(error), error
                    else:
                        raise AssertionError("Corrupted file passed verification")
                finally:
                    readme.write_bytes(original)
                _, verification = api(base, "POST", "/runtime/translation/models/verify",
                                      {"direction": args.direction})
                assert verification["verified"]
                print("PASS: corruption detected; original content restored")

                _, deleted = api(base, "POST", "/runtime/translation/models/delete",
                                 {"direction": args.direction})
                assert deleted["deleted"]
                assert not expected_path.exists()
                print("PASS: deletion of isolated model")

                if args.cancel_test:
                    other = "zh-en" if args.direction == "en-zh" else "en-zh"
                    print(f"Testing cancellation of {other}...")
                    _, response = api(base, "POST", "/runtime/translation/models/install",
                                      {"direction": other})
                    job_id = response["job_id"]
                    api(base, "POST", "/runtime/downloads/cancel", {"job_id": job_id})
                    job, _ = await_job(base, job_id, args.timeout)
                    assert job["status"] == "cancelled", job
                    assert not (models / "opus-mt" / other).exists()
                    assert not list((models / "opus-mt").glob(".install-*"))
                    print("PASS: cancellation removed temporary files")

                print("PASS: all requested isolated integration checks")
                return 0
            finally:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
                if process.returncode not in (0, -15):
                    print("Server output:\n" + log.read_text(encoding="utf-8")[-4000:],
                          file=sys.stderr)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (AssertionError, RuntimeError, TimeoutError, OSError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)
