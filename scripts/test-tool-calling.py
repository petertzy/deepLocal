#!/usr/bin/env python3
"""Exercise OpenAI tool calling against a running deepLocal desktop API."""

import argparse
import json
import pathlib
import sys
import urllib.error
import urllib.request


def request_json(url, payload=None):
    data = None if payload is None else json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        url,
        data=data,
        headers={"Content-Type": "application/json"},
        method="GET" if data is None else "POST",
    )
    try:
        return urllib.request.urlopen(request, timeout=120)
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"HTTP {error.code} from {url}: {detail}") from error


def loaded_model(base_url):
    with request_json(base_url + "/models") as response:
        data = json.load(response)
    models = [model for model in data.get("data", []) if model.get("status") == "loaded"]
    if not models:
        raise RuntimeError("No loaded model found in /v1/models")
    return models[0]["id"]


def parse_sse(response):
    pending = []
    for raw in response:
        line = raw.decode("utf-8").rstrip("\r\n")
        if line.startswith("data:"):
            pending.append(line[5:].strip())
        elif not line and pending:
            text = "\n".join(pending)
            pending = []
            if text == "[DONE]":
                return
            yield json.loads(text)
    if pending:
        text = "\n".join(pending)
        if text != "[DONE]":
            yield json.loads(text)


def collect_stream(response):
    calls = {}
    finish_reason = None
    completed = False
    for chunk in parse_sse(response):
        if "error" in chunk:
            raise RuntimeError(f"Stream error: {chunk['error']}")
        for choice in chunk.get("choices", []):
            if choice.get("finish_reason") is not None:
                finish_reason = choice["finish_reason"]
            for part in choice.get("delta", {}).get("tool_calls", []):
                index = part.get("index", 0)
                current = calls.setdefault(index, {
                    "id": "", "type": "function",
                    "function": {"name": "", "arguments": ""},
                })
                current["id"] += part.get("id", "")
                current["function"]["name"] += part.get("function", {}).get("name", "")
                current["function"]["arguments"] += part.get("function", {}).get("arguments", "")
    return finish_reason, [calls[index] for index in sorted(calls)]


def validate_call(finish_reason, calls):
    if finish_reason != "tool_calls":
        raise RuntimeError(f"Expected finish_reason=tool_calls, got {finish_reason!r}")
    if len(calls) != 1:
        raise RuntimeError(f"Expected one read_file call, got {len(calls)}")
    call = calls[0]
    if not call.get("id") or call["function"]["name"] != "read_file":
        raise RuntimeError(f"Invalid tool call identity: {call}")
    try:
        arguments = json.loads(call["function"]["arguments"])
    except json.JSONDecodeError as error:
        raise RuntimeError(f"Tool arguments are not JSON: {call}") from error
    if not isinstance(arguments, dict) or arguments.get("path") not in ("README.md", "./README.md"):
        raise RuntimeError(f"Expected read_file(path='README.md'), got {arguments}")
    return call


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:14567/v1")
    parser.add_argument("--model", help="Override the first loaded /v1/models model")
    options = parser.parse_args()
    base_url = options.base_url.rstrip("/")
    model = options.model or loaded_model(base_url)
    readme = pathlib.Path("README.md")
    if not readme.is_file():
        raise RuntimeError("Run this script from the deepLocal repository root")

    print(f"Model: {model}")
    tools = [{
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read a file by relative path. Use path README.md for this task.",
            "parameters": {
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"],
            },
        },
    }]
    prompt = [{
        "role": "user",
        "content": "Use the read_file tool with path exactly README.md. Do not answer before receiving the tool result.",
    }]
    for streaming in (False, True):
        label = "streaming" if streaming else "non-streaming"
        body = {
            "model": model,
            "messages": prompt,
            "tools": tools,
            "tool_choice": "required",
            "max_tokens": 256,
            "temperature": 0,
            "stream": streaming,
        }
        with request_json(base_url + "/chat/completions", body) as response:
            if streaming:
                finish_reason, calls = collect_stream(response)
            else:
                result = json.load(response)
                if "error" in result:
                    raise RuntimeError(str(result["error"]))
                choice = result["choices"][0]
                finish_reason = choice.get("finish_reason")
                calls = choice["message"].get("tool_calls", [])
        try:
            call = validate_call(finish_reason, calls)
        except RuntimeError as error:
            print(f"FAIL {label}: {error}")
            print(f"Raw calls: {json.dumps(calls, ensure_ascii=False)}")
            return 1
        print(f"PASS {label}: {call['function']['name']} {call['function']['arguments']}")
        if streaming:
            continue

        history = prompt + [
            {"role": "assistant", "content": None, "tool_calls": [call]},
            {
                "role": "tool",
                "tool_call_id": call["id"],
                "content": readme.read_text(encoding="utf-8")[:3000],
            },
        ]
        continuation = {
            "model": model,
            "messages": history,
            "tools": tools,
            "tool_choice": "none",
            "stream": False,
            "max_tokens": 256,
            "temperature": 0,
        }
        with request_json(base_url + "/chat/completions", continuation) as response:
            followup = json.load(response)
        content = followup["choices"][0]["message"].get("content")
        if not isinstance(content, str) or not content.strip():
            print("FAIL follow-up: model did not continue after the tool result")
            print(json.dumps(followup, indent=2, ensure_ascii=False))
            return 1
        print(f"PASS follow-up: {content[:160]!r}")

    print("PASS: native tool-call request, stream, and follow-up are functional")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (RuntimeError, OSError, ValueError, KeyError, IndexError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)
