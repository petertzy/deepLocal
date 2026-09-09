git status

./scripts/package-macos-app.sh

echo ""
echo "=== SHA256 ==="
shasum -a 256 dist/deepLocal-macos.zip
shasum -a 256 dist/deepLocal-macos.dmg
echo ""

gh release create v0.1.2 \
  dist/deepLocal-macos.zip \
  dist/deepLocal-macos.dmg \
  --title "deepLocal v0.1.1" \
  --notes "Second packaged macOS preview release.

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
gh release view v0.1.1