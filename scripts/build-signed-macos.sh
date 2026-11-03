#!/bin/bash
# Tauri owns certificate import, nested signing, notarization and app stapling.
set -euo pipefail
set +x
umask 077
cd "$(dirname "$0")/.."
[[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]] || {
  echo 'Signed builds require macOS arm64.' >&2; exit 1;
}
for name in APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_SIGNING_IDENTITY APPLE_API_ISSUER APPLE_API_KEY APPLE_API_PRIVATE_KEY; do
  [[ -n "${!name:-}" ]] || { echo "Missing required credential: $name" >&2; exit 1; }
done
[[ "$APPLE_SIGNING_IDENTITY" =~ ^Developer\ ID\ Application:\ .+\ \([A-Z0-9]{10}\)$ ]] || {
  echo 'APPLE_SIGNING_IDENTITY must be the full Developer ID Application name including Team ID.' >&2; exit 1;
}
credential_dir=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/zapbeam-signing.XXXXXX")
trap 'rm -rf "$credential_dir"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
export APPLE_API_KEY_PATH="$credential_dir/AuthKey.p8"
printf '%s\n' "$APPLE_API_PRIVATE_KEY" > "$APPLE_API_KEY_PATH"
unset APPLE_API_PRIVATE_KEY
# Prevent alternative local credentials from overriding the requested API key.
unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
# Refuse stale output rather than verifying or uploading an older installer.
if [[ -d src-tauri/target/release/bundle ]]; then
  echo 'Remove stale src-tauri/target/release/bundle before a signed build.' >&2
  exit 1
fi
npm run build:desktop -- --bundles dmg -- --locked
python3 scripts/verify-packages.py --require-signing
