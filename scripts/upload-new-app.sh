#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

git status

latest_release_version() {
  local github_tags local_tags
  github_tags="$(gh release list --limit 1000 --json tagName --jq '.[].tagName' 2>/dev/null || true)"
  local_tags="$(git tag --list 'v*')"

  printf '%s\n%s\n' "$github_tags" "$local_tags" |
    awk '
      $0 ~ /^v[0-9]+\.[0-9]+\.[0-9]+$/ {
        version = $0
        sub(/^v/, "", version)
        split(version, parts, ".")
        major = parts[1] + 0
        minor = parts[2] + 0
        patch = parts[3] + 0
        if (!found || major > best_major ||
            (major == best_major && minor > best_minor) ||
            (major == best_major && minor == best_minor && patch > best_patch)) {
          best_major = major
          best_minor = minor
          best_patch = patch
          found = 1
        }
      }
      END {
        if (found) printf "v%d.%d.%d\n", best_major, best_minor, best_patch
      }
    '
}

if [[ -n "${DEEPLOCAL_RELEASE_VERSION:-}" ]]; then
  RELEASE_VERSION="$DEEPLOCAL_RELEASE_VERSION"
else
  LATEST_VERSION="$(latest_release_version)"
  if [[ -z "$LATEST_VERSION" ]]; then
    RELEASE_VERSION="v0.1.0"
  else
    RELEASE_VERSION="$(awk -F. '{ printf "v%d.%d.%d\n", substr($1, 2), $2, $3 + 1 }' <<< "$LATEST_VERSION")"
  fi
fi

if [[ ! "$RELEASE_VERSION" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Invalid release version: $RELEASE_VERSION" >&2
  echo "Use DEEPLOCAL_RELEASE_VERSION=vMAJOR.MINOR.PATCH to override it." >&2
  exit 1
fi

echo "Preparing release $RELEASE_VERSION"

if [[ "${DEEPLOCAL_UPLOAD_DRY_RUN:-}" == "1" ]]; then
  echo "Dry run: no package will be built and no GitHub release will be uploaded."
  exit 0
fi

SIGNING_KEY_FILE="${TAURI_SIGNING_PRIVATE_KEY_FILE:-${HOME}/.tauri/deepLocal.key}"
PUBLIC_KEY_FILE="${TAURI_UPDATER_PUBLIC_KEY_FILE:-${SIGNING_KEY_FILE}.pub}"
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" && -f "$SIGNING_KEY_FILE" ]]; then
  export TAURI_SIGNING_PRIVATE_KEY="$(cat "$SIGNING_KEY_FILE")"
fi
if [[ -z "${TAURI_UPDATER_PUBLIC_KEY:-}" && -f "$PUBLIC_KEY_FILE" ]]; then
  export TAURI_UPDATER_PUBLIC_KEY="$(cat "$PUBLIC_KEY_FILE")"
fi

if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  echo "TAURI_SIGNING_PRIVATE_KEY is required to create signed updater artifacts." >&2
  echo "Generate a key with: npm run tauri signer generate -w $SIGNING_KEY_FILE" >&2
  echo "Or set TAURI_SIGNING_PRIVATE_KEY_FILE to an existing private-key path." >&2
  exit 1
fi
if [[ -z "${TAURI_UPDATER_PUBLIC_KEY:-}" ]]; then
  echo "TAURI_UPDATER_PUBLIC_KEY is required for updater verification." >&2
  echo "Expected public key file: $PUBLIC_KEY_FILE" >&2
  echo "Or set TAURI_UPDATER_PUBLIC_KEY_FILE to an existing .pub path." >&2
  exit 1
fi

TAURI_CONFIG="$ROOT_DIR/apps/desktop/src-tauri/tauri.conf.json"
ORIGINAL_CONFIG="$(mktemp)"
PACKAGING_CONFIG="$(mktemp "${TMPDIR:-/tmp}/deeplocal-release-config.XXXXXX.json")"
cleanup_config() {
  rm -f "$ORIGINAL_CONFIG"
  rm -f "$PACKAGING_CONFIG"
}
trap cleanup_config EXIT
VERSION_NO_V="${RELEASE_VERSION#v}"
cp "$TAURI_CONFIG" "$ORIGINAL_CONFIG"
node - "$TAURI_CONFIG" "$PACKAGING_CONFIG" "$VERSION_NO_V" "$TAURI_UPDATER_PUBLIC_KEY" <<'NODE'
const fs = require('fs');
const [sourcePath, targetPath, version, publicKey] = process.argv.slice(2);
const config = JSON.parse(fs.readFileSync(sourcePath, 'utf8'));
config.version = version;
config.plugins = config.plugins || {};
config.plugins.updater = config.plugins.updater || {};
config.plugins.updater.pubkey = publicKey.trim();
fs.writeFileSync(targetPath, `${JSON.stringify(config, null, 2)}\n`);
NODE
export DEEPLOCAL_TAURI_CONFIG="$PACKAGING_CONFIG"

./scripts/package-macos-app.sh

echo ""
echo "=== SHA256 ==="
shasum -a 256 dist/deepLocal-macos.zip
shasum -a 256 dist/deepLocal-macos.dmg
echo ""

UPDATER_ARCHIVE="$(find target/release/bundle/macos -maxdepth 1 -name '*.app.tar.gz' -print -quit)"
UPDATER_SIGNATURE="${UPDATER_ARCHIVE}.sig"
if [[ -z "$UPDATER_ARCHIVE" || ! -f "$UPDATER_SIGNATURE" ]]; then
  echo "Tauri updater artifacts were not generated." >&2
  exit 1
fi

UPDATER_FILENAME="$(basename "$UPDATER_ARCHIVE")"
UPDATER_SIGNATURE_VALUE="$(cat "$UPDATER_SIGNATURE")"
cat > latest.json <<EOF
{
  "version": "$VERSION_NO_V",
  "notes": "Packaged macOS preview release.",
  "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "platforms": {
    "darwin-aarch64": {
      "signature": "$UPDATER_SIGNATURE_VALUE",
      "url": "https://github.com/petertzy/deepLocal/releases/download/$RELEASE_VERSION/$UPDATER_FILENAME"
    }
  }
}
EOF

gh release create "$RELEASE_VERSION" \
  dist/deepLocal-macos.zip \
  dist/deepLocal-macos.dmg \
  "$UPDATER_ARCHIVE" \
  "$UPDATER_SIGNATURE" \
  latest.json \
  --title "deepLocal $RELEASE_VERSION" \
  --notes "Packaged macOS preview release.

Highlights:

- Updated native Tauri macOS app.
- Updated React and Rust components.
- Includes the latest deepLocal desktop changes.

Notes:

- This preview build is unsigned and not notarized, so macOS Gatekeeper may warn on first launch.
- Recommended download: deepLocal-macos.dmg
- ZIP version: deepLocal-macos.zip

SHA256:
- deepLocal-macos.zip: $(shasum -a 256 dist/deepLocal-macos.zip | awk '{print $1}')
- deepLocal-macos.dmg: $(shasum -a 256 dist/deepLocal-macos.dmg | awk '{print $1}')"

echo ""
echo "=== Release ==="
gh release view "$RELEASE_VERSION"