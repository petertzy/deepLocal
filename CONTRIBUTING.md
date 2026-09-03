# Contributing To deepLocal

Thanks for helping improve deepLocal. This project is still young, so small,
clear pull requests are especially valuable.

## Start Here

```bash
git clone https://github.com/petertzy/deepLocal.git
cd deepLocal
./scripts/start-dev.sh
```

Open:

```text
http://127.0.0.1:5173/
```

The startup script runs the backend and frontend together. On macOS, it tries to
install `llama.cpp` with Homebrew if `llama-server` is missing.

## Development Requirements

- Rust toolchain with Cargo.
- Node.js and npm.
- `curl` and `lsof`.
- macOS with Homebrew is recommended for the smoothest local model setup.

If you already have `llama-server` installed, keep it available in `PATH`.

## Good First Contributions

Good first issues usually involve:

- UI polish and accessibility.
- Clear error messages.
- Model search and download edge cases.
- Documentation cleanup.
- Small API improvements.
- Focused tests around existing behavior.

Before starting larger work, open or comment on an issue so the direction is
clear.

## Local Checks

Run these before opening a pull request when possible:

```bash
cargo check
cargo test
./scripts/start-dev.sh --build
```

If a check fails for an environment reason, mention that in the pull request.

## Code Style

- Follow the existing Rust and React patterns.
- Prefer small, readable changes over broad rewrites.
- Keep runtime behavior local-first and privacy-conscious.
- Use structured APIs and types instead of ad hoc string parsing when practical.
- Add tests when changing shared behavior, API contracts, or model lifecycle
  logic.

## Pull Requests

Please include:

- What changed.
- Why it changed.
- How you tested it.
- Screenshots or short recordings for UI changes.
- Any follow-up work that should become a separate issue.

Keep pull requests focused on one problem. Separate formatting-only changes from
behavior changes.

## What Not To Commit

Do not commit:

- Hugging Face tokens or other secrets.
- Downloaded model files under `models/`.
- Local SQLite databases.
- `target/`.
- `node_modules/`.
- Frontend build output.
- Machine-specific editor or OS files.

Before submitting secret-related changes, it is reasonable to run:

```bash
rg "hf_" .
```

The expected safe match today is the token placeholder text in the Settings UI.

## Reporting Bugs

Please include:

- Operating system and hardware.
- The model name and file type, if relevant.
- The command you used to start deepLocal.
- The browser console error or backend log, if available.
- Steps to reproduce the problem.

## License

By contributing, you agree that your contribution will be licensed under the MIT
License.
