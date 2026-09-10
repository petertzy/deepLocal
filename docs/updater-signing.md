# Tauri Updater Signing and Release Guide

This document explains how to create, protect, and use the Tauri updater signing
keys for deepLocal. It is intended for maintainers who publish macOS releases
with `scripts/upload-new-app.sh`.

## What the keys are used for

Tauri updater releases use asymmetric signing:

- The **private key** signs the updater archive during the release build.
- The **public key** is embedded in the application and verifies future updates.
- The updater signature is safe to publish in `latest.json` and in the GitHub
  Release assets. It is not the private key.

Never publish or commit the private key.

## Generate a key pair

Run this once on a trusted release machine. Create the directory first:

```bash
cd /Users/Downloads/copilot/deepLocal/apps/desktop
mkdir -p "$HOME/.tauri"
npx tauri signer generate --write-keys "$HOME/.tauri/deepLocal.key"
```

The command creates:

```text
$HOME/.tauri/deepLocal.key       # private key; keep secret
$HOME/.tauri/deepLocal.key.pub   # public key; safe to use in app configuration
```

To protect the private key with a password, use the signer option and follow the
prompt:

```bash
npx tauri signer generate \
  --write-keys "$HOME/.tauri/deepLocal.key" \
  --password
```

The password is not stored in this repository. Keep it in a password manager or
provide it through `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` when publishing.

Verify that both files exist without printing their contents:

```bash
test -s "$HOME/.tauri/deepLocal.key" && echo "Private key exists"
test -s "$HOME/.tauri/deepLocal.key.pub" && echo "Public key exists"
```

Do **not** run `cat "$HOME/.tauri/deepLocal.key"` in a shared terminal, log, or
screen recording.

## Local release configuration

`scripts/upload-new-app.sh` automatically reads these default files:

```text
$HOME/.tauri/deepLocal.key
$HOME/.tauri/deepLocal.key.pub
```

Therefore, after generating the key pair, a release can normally be published
with:

```bash
cd /Users/Downloads/copilot/deepLocal
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="your-key-password" # omit if unset
./scripts/upload-new-app.sh
```

The script also accepts explicit environment variables:

```bash
export TAURI_SIGNING_PRIVATE_KEY_FILE="$HOME/.tauri/deepLocal.key"
export TAURI_UPDATER_PUBLIC_KEY_FILE="$HOME/.tauri/deepLocal.key.pub"
```

Or direct key values, which are useful in CI when loaded from protected secrets:

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.tauri/deepLocal.key")"
export TAURI_UPDATER_PUBLIC_KEY="$(cat "$HOME/.tauri/deepLocal.key.pub")"
```

Never put either direct value in a committed shell script, Markdown file, GitHub
Release note, or public CI log.

## Dry-run and local packaging

Preview the release version without building or uploading anything:

```bash
cd /Users/Downloads/copilot/deepLocal
DEEPLOCAL_UPLOAD_DRY_RUN=1 ./scripts/upload-new-app.sh
```

Build a local app for testing without updater signing artifacts:

```bash
./scripts/package-macos-app.sh
```

If `TAURI_SIGNING_PRIVATE_KEY` is not set, the local packaging script disables
updater artifact generation for that local build. This is expected and avoids
requiring release credentials for ordinary development.

## Public release artifacts

A signed GitHub Release must contain at least:

```text
deepLocal-macos.zip
deepLocal-macos.dmg
deepLocal.app.tar.gz
deepLocal.app.tar.gz.sig
latest.json
```

`latest.json` is served from:

```text
https://github.com/petertzy/deepLocal/releases/latest/download/latest.json
```

The updater signature inside `latest.json` is public by design. It lets installed
applications verify that the downloaded archive was signed by the matching private
key. Publishing the signature does not disclose the private key.

## Verify a release

List the assets of a GitHub Release:

```bash
gh release view v0.1.7 --json assets --jq '.assets[].name'
```

The public metadata should be readable:

```bash
curl -fsSL \
  https://github.com/petertzy/deepLocal/releases/latest/download/latest.json
```

The metadata should contain a newer semantic version and the platform key:

```text
darwin-aarch64
```

Check that the repository history does not contain obvious private-key material:

```bash
git grep -n -I -E \
  'untrusted comment:.*secret key|BEGIN.*PRIVATE KEY|TAURI_SIGNING_PRIVATE_KEY=' \
  $(git rev-list --all) 2>/dev/null || true
```

## Key rotation

If the private key is exposed or lost:

1. Do not reuse the exposed key.
2. Generate a new key pair on a trusted machine.
3. Update the public key used by the application configuration.
4. Build and publish a new application version with the new public key.
5. Keep the old release available only as a manual-download fallback if needed.

The private key is the root of trust for updater artifacts. Losing it prevents
future releases from being accepted by applications that contain its public key;
leaking it allows an attacker to sign a fake update for applications containing
that public key.

## CI usage

For GitHub Actions or another CI provider, store the private key and optional
password as encrypted secrets. Do not write the key to a normal repository file.
Set these environment variables only for the release job:

```text
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD
TAURI_UPDATER_PUBLIC_KEY
```

The public key is not a secret, but keeping it in the release configuration and
reviewing changes to it helps detect accidental key rotation.