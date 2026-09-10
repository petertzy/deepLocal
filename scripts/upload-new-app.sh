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

./scripts/package-macos-app.sh

echo ""
echo "=== SHA256 ==="
shasum -a 256 dist/deepLocal-macos.zip
shasum -a 256 dist/deepLocal-macos.dmg
echo ""

gh release create "$RELEASE_VERSION" \
  dist/deepLocal-macos.zip \
  dist/deepLocal-macos.dmg \
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