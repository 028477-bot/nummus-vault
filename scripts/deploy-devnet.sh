#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
require_release_solana_cli
require_bin solana-keygen

DEVNET_URL="${DEVNET_RPC_URL:-https://api.devnet.solana.com}"
assert_devnet_rpc "$DEVNET_URL"
assert_keypair_matches_canonical

cd "$ANCHOR_DIR"
[ -f "target/deploy/${PROGRAM_NAME}.so" ] || die "missing built program .so — run scripts/build.sh first"
DEVNET_PAYER_KEYPAIR="${DEVNET_PAYER_KEYPAIR:-target/deploy/devnet-payer-keypair.json}"
[ -f "$DEVNET_PAYER_KEYPAIR" ] || die "missing funded devnet payer: $DEVNET_PAYER_KEYPAIR"
DEVNET_BUFFER_KEYPAIR="${DEVNET_BUFFER_KEYPAIR:-target/deploy/devnet-buffer-keypair.json}"

PAYER_BALANCE="$(solana --url "$DEVNET_URL" balance "$(solana-keygen pubkey "$DEVNET_PAYER_KEYPAIR")" --lamports | awk '{print $1}')"
[ "$PAYER_BALANCE" -gt 0 ] || die "devnet payer has no SOL; fund it explicitly before deployment"

if DEVNET_RPC_URL="$DEVNET_URL" bash scripts/verify.sh devnet >/dev/null 2>&1; then
  log "exact ${PROGRAM_NAME} release is already deployed on devnet"
  DEVNET_RPC_URL="$DEVNET_URL" bash scripts/verify.sh devnet
  exit 0
fi

if [ ! -f "$DEVNET_BUFFER_KEYPAIR" ]; then
  log "creating persistent gitignored devnet buffer identity ..."
  solana-keygen new --no-bip39-passphrase --silent --force -o "$DEVNET_BUFFER_KEYPAIR"
fi

BINARY_BYTES="$(stat -c %s "target/deploy/${PROGRAM_NAME}.so")"
BUFFER_ADDRESS="$(solana-keygen pubkey "$DEVNET_BUFFER_KEYPAIR")"

log "writing/resuming the exact prebuilt ${PROGRAM_NAME} release buffer ..."
solana --url "$DEVNET_URL" program write-buffer --use-rpc \
  "target/deploy/${PROGRAM_NAME}.so" \
  --buffer "$DEVNET_BUFFER_KEYPAIR" \
  --buffer-authority "$DEVNET_PAYER_KEYPAIR" \
  --fee-payer "$DEVNET_PAYER_KEYPAIR" \
  --keypair "$DEVNET_PAYER_KEYPAIR" \
  --max-len "$BINARY_BYTES" \
  --max-sign-attempts 20

VAULT_RPC_URL="$DEVNET_URL" node scripts/verify-buffer.mjs \
  "$BUFFER_ADDRESS" "target/deploy/${PROGRAM_NAME}.so"

log "finalizing the verified buffer as the canonical devnet program ..."
solana --url "$DEVNET_URL" program deploy --use-rpc \
  --buffer "$BUFFER_ADDRESS" \
  --max-len "$BINARY_BYTES" \
  --program-id "$DEPLOY_KEYPAIR" \
  --upgrade-authority "$DEVNET_PAYER_KEYPAIR" \
  --keypair "$DEVNET_PAYER_KEYPAIR"

log "devnet deploy complete; verifying deployed bytes and interface identity"
DEVNET_RPC_URL="$DEVNET_URL" bash scripts/verify.sh devnet
warn "Upgrade authority MUST be transferred to the Turnkey 4-of-6 root policy"
warn "before any user funds enter the vault."
