#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS_DIR="$ROOT_DIR/.tools"
DOWNLOADS_DIR="$TOOLS_DIR/downloads"
NODE_VERSION="24.20.0"
OS="$(uname -s)"
MACHINE="$(uname -m)"

case "$MACHINE" in
  x86_64|amd64) ARCH="x64" ;;
  arm64|aarch64) ARCH="arm64" ;;
  *) echo "Unsupported CPU architecture: $MACHINE"; exit 1 ;;
esac

install_linux_packages() {
  local packages=(curl ca-certificates tar gzip lsof build-essential)
  if command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y "${packages[@]}"
  elif command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y curl ca-certificates tar gzip lsof gcc gcc-c++ make
  elif command -v yum >/dev/null 2>&1; then
    sudo yum install -y curl ca-certificates tar gzip lsof gcc gcc-c++ make
  elif command -v pacman >/dev/null 2>&1; then
    sudo pacman -Syu --needed --noconfirm curl ca-certificates tar gzip lsof base-devel
  elif command -v zypper >/dev/null 2>&1; then
    sudo zypper --non-interactive install curl ca-certificates tar gzip lsof gcc gcc-c++ make
  else
    echo "Unsupported Linux package manager. Install curl, tar, gzip, lsof, GCC and make, then retry."
    exit 1
  fi
}

if [[ "$OS" == "Linux" ]]; then
  missing=0
  for command_name in curl tar gzip lsof cc; do
    command -v "$command_name" >/dev/null 2>&1 || missing=1
  done
  [[ "$missing" -eq 0 ]] || install_linux_packages
elif [[ "$OS" == "Darwin" ]]; then
  if ! xcode-select -p >/dev/null 2>&1; then
    echo "Installing Apple Command Line Tools. Confirm the macOS dialog to continue."
    xcode-select --install || true
    until xcode-select -p >/dev/null 2>&1; do sleep 10; done
  fi
else
  echo "Unsupported operating system: $OS"
  exit 1
fi

mkdir -p "$TOOLS_DIR" "$DOWNLOADS_DIR"

NODE_DIR="$TOOLS_DIR/node"
if [[ -x "$NODE_DIR/bin/node" ]]; then
  export PATH="$NODE_DIR/bin:$PATH"
elif ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then
  [[ "$OS" == "Darwin" ]] && node_platform="darwin" || node_platform="linux"
  node_archive="node-v${NODE_VERSION}-${node_platform}-${ARCH}.tar.gz"
  echo "Downloading Node.js $NODE_VERSION..."
  curl -fL --retry 3 "https://nodejs.org/dist/v${NODE_VERSION}/${node_archive}" -o "$DOWNLOADS_DIR/$node_archive"
  curl -fL --retry 3 "https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt" -o "$DOWNLOADS_DIR/node-SHASUMS256.txt"
  if command -v sha256sum >/dev/null 2>&1; then
    (cd "$DOWNLOADS_DIR" && grep " ${node_archive}$" node-SHASUMS256.txt | sha256sum -c -)
  else
    (cd "$DOWNLOADS_DIR" && grep " ${node_archive}$" node-SHASUMS256.txt | shasum -a 256 -c -)
  fi
  rm -rf "$NODE_DIR" "$DOWNLOADS_DIR/${node_archive%.tar.gz}"
  tar -xzf "$DOWNLOADS_DIR/$node_archive" -C "$DOWNLOADS_DIR"
  mv "$DOWNLOADS_DIR/${node_archive%.tar.gz}" "$NODE_DIR"
  export PATH="$NODE_DIR/bin:$PATH"
fi
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Installing Rust and Cargo for the current user..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --profile minimal
  export PATH="$HOME/.cargo/bin:$PATH"
fi

LLAMA_DIR="$TOOLS_DIR/llama.cpp"
if [[ -x "$LLAMA_DIR/llama-server" ]]; then
  export DEEPLOCAL_LLAMA_SERVER="$LLAMA_DIR/llama-server"
elif command -v llama-server >/dev/null 2>&1; then
  export DEEPLOCAL_LLAMA_SERVER="$(command -v llama-server)"
elif [[ "${DEEPLOCAL_SKIP_LLAMA_INSTALL:-}" != "1" ]]; then
  [[ "$OS" == "Darwin" ]] && llama_platform="macos" || llama_platform="ubuntu"
  asset_url="$(node - "$llama_platform" "$ARCH" <<'NODE'
const [platform, arch] = process.argv.slice(2);
const pattern = new RegExp(`^llama-.+-bin-${platform}-${arch}\\.tar\\.gz$`);
fetch('https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10', {
  headers: { 'User-Agent': 'deepLocal-Unix-Setup' },
}).then(r => {
  if (!r.ok) throw new Error(`GitHub API returned ${r.status}`);
  return r.json();
}).then(releases => {
  const asset = releases.filter(r => !r.draft).flatMap(r => r.assets).find(a => pattern.test(a.name));
  if (!asset) throw new Error(`No recent llama.cpp ${platform}-${arch} package was found`);
  process.stdout.write(asset.browser_download_url);
}).catch(error => { console.error(error.message); process.exit(1); });
NODE
)"
  llama_archive="$DOWNLOADS_DIR/$(basename "$asset_url")"
  echo "Downloading llama.cpp $(basename "$asset_url")..."
  curl -fL --retry 3 "$asset_url" -o "$llama_archive"
  rm -rf "$LLAMA_DIR"
  mkdir -p "$LLAMA_DIR"
  tar -xzf "$llama_archive" -C "$LLAMA_DIR"
  llama_server="$(find "$LLAMA_DIR" -type f -name llama-server -print -quit)"
  [[ -n "$llama_server" ]] || { echo "llama-server was not found in the downloaded package."; exit 1; }
  chmod +x "$llama_server"
  export DEEPLOCAL_LLAMA_SERVER="$llama_server"
fi

echo "Unix dependencies are ready:"
node --version
npm --version
cargo --version
if [[ "${DEEPLOCAL_SKIP_LLAMA_INSTALL:-}" != "1" ]]; then
  echo "${DEEPLOCAL_LLAMA_SERVER:-$(find "$LLAMA_DIR" -type f -name llama-server -print -quit)}"
fi
