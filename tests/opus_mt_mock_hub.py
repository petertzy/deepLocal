"""Small deterministic Hugging Face-compatible HTTP fixture for installer tests."""

import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import threading
import time
from urllib.parse import urlsplit

FILES = {
    name: (f"fixture:{name}:".encode() * 64)
    for name in ("model.bin", "source.spm", "target.spm",
                 "config.json", "shared_vocabulary.json", "README.md")
}
REVISION = "a" * 40


class FixtureServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, address):
        super().__init__(address, FixtureHandler)
        self.slow = False
        self.corrupt = False
        self.missing_checksum = False
        self.first_chunk = threading.Event()


class FixtureHandler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def respond(self, status, body, content_type="application/octet-stream"):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = urlsplit(self.path).path
        if path in ("/api/models/Sams200/opus-mt-en-zh",
                    "/api/models/Sams200/opus-mt-zh-en"):
            self.respond(200, json.dumps({"sha": REVISION}).encode(), "application/json")
            return
        if path.endswith("/tree/" + REVISION) and path.startswith("/api/models/Sams200/"):
            entries = []
            for name, data in FILES.items():
                sha = hashlib.sha256(data).hexdigest()
                lfs = {} if name == "model.bin" and self.server.missing_checksum else {"oid": sha}
                entries.append({"path": name, "size": len(data), "lfs": lfs})
            self.respond(200, json.dumps(entries).encode(), "application/json")
            return
        prefix = "/resolve/" + REVISION + "/"
        if prefix in path and path.startswith("/Sams200/"):
            name = path.split(prefix, 1)[1]
            if name not in FILES:
                self.respond(404, b"Not found")
                return
            data = FILES[name]
            if self.server.corrupt and name == "model.bin":
                data = b"X" + data[1:]
            if self.server.slow and name == "model.bin":
                self.send_response(200)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data[:64])
                self.wfile.flush()
                self.server.first_chunk.set()
                time.sleep(2.0)
                try:
                    self.wfile.write(data[64:])
                except (BrokenPipeError, ConnectionResetError):
                    pass
            else:
                self.respond(200, data)
            return
        self.respond(404, b"Not found")
