#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
require_bin solana-keygen

mkdir -p "$(dirname "$DEPLOY_KEYPAIR")"
if [ -f "$DEPLOY_KEYPAIR" ]; then
  warn "keypair already exists at $DEPLOY_KEYPAIR (pubkey $(solana-keygen pubkey "$DEPLOY_KEYPAIR"))"
  warn "refusing to overwrite; delete it explicitly if you really mean to."
  exit 0
fi
solana-keygen new --no-bip39-passphrase -o "$DEPLOY_KEYPAIR"
log "generated deploy keypair: $(solana-keygen pubkey "$DEPLOY_KEYPAIR")"
warn "This pubkey must be reconciled with CANONICAL_PROGRAM_ID before deploying."
