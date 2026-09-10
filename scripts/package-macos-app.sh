#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESKTOP_DIR="$ROOT_DIR/apps/desktop"
DIST_DIR="$ROOT_DIR/dist"
LLAMA_RUNTIME_DIR="$ROOT_DIR/apps/desktop/src-tauri/resources/llama-runtime"
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS app packaging must run on macOS." >&2
  exit 1
fi

command -v cargo >/dev/null 2>&1 || {
  echo "cargo is required. Run ./scripts/start-dev.sh once to install prerequisites." >&2
  exit 1
}

command -v npm >/dev/null 2>&1 || {
  echo "npm is required. Install Node.js or run ./scripts/start-dev.sh once." >&2
  exit 1
}

prepare_llama_runtime() {
  local source_binary="${DEEPLOCAL_LLAMA_SERVER:-${LLAMA_SERVER:-}}"
  local source_root=""
  local archive=""
  local architecture

  case "$(uname -m)" in
    arm64|aarch64) architecture="arm64" ;;
    x86_64|amd64) architecture="x64" ;;
    *)
      echo "Unsupported macOS CPU architecture: $(uname -m)" >&2
      exit 1
      ;;
  esac

  if [[ -z "$source_binary" ]]; then
    source_binary="$(find "$ROOT_DIR/.tools/llama.cpp" -type f -name llama-server -perm -111 -print -quit 2>/dev/null || true)"
  fi

  if [[ -z "$source_binary" ]]; then
    command -v curl >/dev/null 2>&1 || {
      echo "curl is required to download the bundled llama.cpp runtime." >&2
      exit 1
    }
    command -v tar >/dev/null 2>&1 || {
      echo "tar is required to unpack the bundled llama.cpp runtime." >&2
      exit 1
    }
    command -v node >/dev/null 2>&1 || {
      echo "node is required to locate the official llama.cpp macOS archive." >&2
      exit 1
    }

    local asset_url
    asset_url="$(node - "$architecture" <<'NODE'
const arch = process.argv[2];
const pattern = new RegExp(`^llama-.+-bin-macos-${arch}\\.tar\\.gz$`);
fetch('https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=10', {
  headers: { 'User-Agent': 'deepLocal-macOS-Packager' },
}).then(response => {
  if (!response.ok) throw new Error(`GitHub API returned ${response.status}`);
  return response.json();
}).then(releases => {
  const asset = releases
    .filter(release => !release.draft)
    .flatMap(release => release.assets)
    .find(asset => pattern.test(asset.name));
  if (!asset) throw new Error(`No llama.cpp macOS ${arch} package was found`);
  process.stdout.write(asset.browser_download_url);
}).catch(error => {
  console.error(error.message);
  process.exit(1);
});
NODE
)"
    archive="$ROOT_DIR/.tools/llama.cpp-macos-${architecture}.tar.gz"
    mkdir -p "$ROOT_DIR/.tools"
    echo "Downloading official llama.cpp macOS runtime..."
    curl -fL --retry 3 "$asset_url" -o "$archive"
    source_root="$(mktemp -d)"
    tar -xzf "$archive" -C "$source_root"
    source_binary="$(find "$source_root" -type f -name llama-server -print -quit)"
  fi

  if [[ ! -x "$source_binary" ]]; then
    echo "Could not find an executable llama-server for the macOS app bundle." >&2
    exit 1
  fi

  if [[ -z "$source_root" ]]; then
    source_root="$(cd "$(dirname "$source_binary")" && pwd)"
  fi

  rm -rf "$LLAMA_RUNTIME_DIR"
  mkdir -p "$LLAMA_RUNTIME_DIR"
  cp "$source_binary" "$LLAMA_RUNTIME_DIR/llama-server"
  chmod +x "$LLAMA_RUNTIME_DIR/llama-server"

  command -v install_name_tool >/dev/null 2>&1 || {
    echo "install_name_tool is required to make bundled llama.cpp libraries relocatable." >&2
    exit 1
  }

  command -v otool >/dev/null 2>&1 || {
    echo "otool is required to inspect bundled llama.cpp dependencies." >&2
    exit 1
  }

  # Copy the binary's non-system dylibs, then recursively copy their non-system
  # dependencies. System frameworks and libSystem are provided by macOS and must
  # not be bundled.
  find_runtime_library() {
    local requested_name="$1"
    local requested_stem="${requested_name%.dylib*}"
    local exact_match versioned_match

    exact_match="$(find "$source_root" -type f -name "$requested_name" -print -quit)"
    [[ -n "$exact_match" ]] && {
      printf '%s\n' "$exact_match"
      return 0
    }

    exact_match="$(find "$source_root" -type l -name "$requested_name" -print -quit)"
    [[ -n "$exact_match" ]] && {
      printf '%s\n' "$exact_match"
      return 0
    }

    # macOS install names often omit trailing dylib version components, for
    # example @rpath/libllama-common.0.dylib while the archive contains
    # libllama-common.0.0.0.dylib.
    versioned_match="$(find "$source_root" \( -type f -o -type l \) -name "${requested_stem}*.dylib*" -print | sort | head -n 1)"
    [[ -n "$versioned_match" ]] && printf '%s\n' "$versioned_match"
  }

  copy_runtime_dependencies() {
    local binary="$1"
    local dependency dependency_name dependency_source destination

    while IFS= read -r dependency; do
      dependency="${dependency#[$'\t ']}"
      dependency="${dependency%% \(*}"
      [[ -n "$dependency" ]] || continue
      case "$dependency" in
        /System/Library/*|/usr/lib/*|/System/iOSSupport/*|@executable_path/*|@loader_path/*)
          continue
          ;;
      esac

      dependency_name="$(basename "$dependency")"
      [[ "$dependency_name" == *.dylib* ]] || continue
      if [[ -f "$LLAMA_RUNTIME_DIR/$dependency_name" ]]; then
        continue
      fi

      dependency_source=""
      if [[ "$dependency" == @rpath/* || "$dependency" == @loader_path/* ]]; then
        dependency_source="$(find_runtime_library "$dependency_name")"
      elif [[ -f "$dependency" ]]; then
        dependency_source="$dependency"
      else
        dependency_source="$(find_runtime_library "$dependency_name")"
      fi
      if [[ -z "$dependency_source" || ! -f "$dependency_source" ]]; then
        echo "Could not locate required llama.cpp dependency: $dependency" >&2
        exit 1
      fi

      destination="$LLAMA_RUNTIME_DIR/$dependency_name"
      cp "$dependency_source" "$destination"
      copy_runtime_dependencies "$dependency_source"
    done < <(otool -L "$binary" | tail -n +2)
  }

  copy_runtime_dependencies "$source_binary"

  # Make every bundled reference relative to the runtime directory. This also
  # handles binaries that were built with absolute Homebrew library paths.
  rewrite_runtime_dependencies() {
    local binary="$1"
    local dependency dependency_name
    while IFS= read -r dependency; do
      dependency="${dependency#[$'\t ']}"
      dependency="${dependency%% \(*}"
      [[ -n "$dependency" ]] || continue
      case "$dependency" in
        /System/Library/*|/usr/lib/*|/System/iOSSupport/*|@executable_path/*|@loader_path/*)
          continue
          ;;
      esac
      dependency_name="$(basename "$dependency")"
      [[ -f "$LLAMA_RUNTIME_DIR/$dependency_name" ]] || continue
      install_name_tool -change "$dependency" "@loader_path/$dependency_name" "$binary"
    done < <(otool -L "$binary" | tail -n +2)
  }

  rewrite_runtime_dependencies "$LLAMA_RUNTIME_DIR/llama-server"
  while IFS= read -r dylib; do
    rewrite_runtime_dependencies "$dylib"
  done < <(find "$LLAMA_RUNTIME_DIR" -type f -name '*.dylib*')

  install_name_tool -add_rpath '@loader_path' "$LLAMA_RUNTIME_DIR/llama-server" 2>/dev/null || true
  while IFS= read -r dylib; do
    install_name_tool -add_rpath '@loader_path' "$dylib" 2>/dev/null || true
  done < <(find "$LLAMA_RUNTIME_DIR" -type f \( -name '*.dylib' -o -name '*.dylib.*' \))

  echo "Bundled llama.cpp runtime: $LLAMA_RUNTIME_DIR"
  find "$LLAMA_RUNTIME_DIR" -maxdepth 1 -type f -print | sort
}

prepare_llama_runtime

echo "Building Tauri app..."
(
  cd "$DESKTOP_DIR"
  if [[ ! -d node_modules ]]; then
    npm ci
  fi
  npm run tauri:build
)

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

APP_PATH="$(find "$ROOT_DIR/target/release/bundle/macos" -maxdepth 1 -name 'deepLocal.app' -print -quit)"
DMG_PATH="$(find "$ROOT_DIR/target/release/bundle/dmg" -maxdepth 1 -name '*.dmg' -print -quit 2>/dev/null || true)"

if [[ -n "$APP_PATH" ]]; then
  cp -R "$APP_PATH" "$DIST_DIR/"
fi

if [[ -d "$DIST_DIR/deepLocal.app" ]]; then
  BUNDLED_LLAMA_DIR="$DIST_DIR/deepLocal.app/Contents/Resources/llama-runtime"
  if [[ ! -x "$BUNDLED_LLAMA_DIR/llama-server" ]]; then
    echo "Packaged app is missing executable llama-server at $BUNDLED_LLAMA_DIR/llama-server" >&2
    exit 1
  fi
  echo "Verified bundled llama.cpp runtime in the app bundle."
  if ! hdiutil create \
    -volname "deepLocal" \
    -srcfolder "$DIST_DIR/deepLocal.app" \
    -ov \
    -format UDZO \
    "$DIST_DIR/deepLocal-macos.dmg" >/dev/null; then
    echo "DMG creation failed; continuing with the app bundle and zip archive."
  fi
elif [[ -n "$DMG_PATH" ]]; then
  cp "$DMG_PATH" "$DIST_DIR/deepLocal-macos.dmg"
fi

if [[ -d "$DIST_DIR/deepLocal.app" ]]; then
  (
    cd "$DIST_DIR"
    ditto -c -k --sequesterRsrc --keepParent "deepLocal.app" "deepLocal-macos.zip"
  )
fi

echo "Packaged app artifacts:"
find "$DIST_DIR" -maxdepth 1 \( -name 'deepLocal.app' -o -name 'deepLocal-macos.*' \) -print
