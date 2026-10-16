#!/usr/bin/env bash
# Local build of Tessera on macOS / Linux (Windows: use scripts/build.ps1).
#   ./scripts/build.sh        release bundles
#   ./scripts/build.sh --dev  development mode with hot reload
set -euo pipefail
cd "$(dirname "$0")/.."

for tool in pnpm cargo; do
  command -v "$tool" >/dev/null || { echo "$tool is not installed. See README > Building locally." >&2; exit 1; }
done

pnpm install --frozen-lockfile
if [[ "${1:-}" == "--dev" ]]; then
  exec pnpm tauri dev
fi
# Updater artifacts must be signed. Use the key in ~/.tauri if present; otherwise build without them.
if [[ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" && -f "$HOME/.tauri/tessera.key" ]]; then
  export TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.tauri/tessera.key")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
fi
if [[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]]; then
  pnpm tauri build
else
  echo "No signing key found: building without auto-update artifacts." >&2
  pnpm tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'
fi
echo
echo "Bundles are in src-tauri/target/release/bundle/"
