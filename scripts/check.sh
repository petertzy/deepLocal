#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$ROOT_DIR/apps/desktop"

cd "$ROOT_DIR"
echo "Checking Rust formatting..."
cargo fmt --all -- --check

echo "Checking Rust workspace..."
cargo check

echo "Running Rust tests..."
cargo test --workspace

cd "$DESKTOP_DIR"
echo "Checking frontend formatting..."
npm run format:check

echo "Checking frontend types and building..."
npm run build

echo "Running frontend component tests..."
npm test -- --run

echo "All checks passed."