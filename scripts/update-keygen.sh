#!/usr/bin/env bash
#
# Generate the Ed25519 key used to sign OpsTerm auto-update manifests.
# The private key stays outside the repository; only the public key is
# written to egui/update-pubkey.txt.
#
set -euo pipefail

cd "$(dirname "$0")/.."

key="${OPSTERM_UPDATE_SIGNING_KEY:-$HOME/.config/opsterm/update-signing.key}"
pubfile="egui/update-pubkey.txt"

if [[ -e "$key" && "${OPSTERM_KEYGEN_FORCE:-0}" != "1" ]]; then
  echo "signing key already exists: $key"
  echo "set OPSTERM_KEYGEN_FORCE=1 to replace it (this invalidates old updates)"
else
  mkdir -p "$(dirname "$key")"
  umask 077
  openssl genpkey -algorithm ED25519 -out "$key"
  chmod 600 "$key"
  echo "created signing key: $key"
fi

public_key="$(openssl pkey -in "$key" -pubout -outform DER | tail -c 32 | base64 -w0)"
printf '%s\n' "$public_key" > "$pubfile"
chmod 644 "$pubfile"

echo "public key: $public_key"
echo "written to: $pubfile"
echo "Back up the private key; without it you cannot sign future updates."
