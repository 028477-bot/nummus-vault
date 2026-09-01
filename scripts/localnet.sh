#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
require_release_solana_cli
require_bin solana-test-validator
require_bin solana
require_bin solana-keygen
require_bin node
cd "$ANCHOR_DIR"

assert_keypair_matches_canonical
[ -f "target/deploy/${PROGRAM_NAME}.so" ] || die "missing SBF binary; run scripts/build.sh first"

RPC_PORT="${VAULT_LOCALNET_RPC_PORT:-18999}"
FAUCET_PORT="${VAULT_LOCALNET_FAUCET_PORT:-19999}"
DYNAMIC_PORTS="${VAULT_LOCALNET_DYNAMIC_PORTS:-20000-20100}"
URL="http://127.0.0.1:${RPC_PORT}"
LEDGER="target/test-ledger"
PAYER="target/deploy/local-payer-keypair.json"

solana-keygen new --no-bip39-passphrase --silent --force -o "$PAYER"

VALIDATOR_ARGS=(
  --reset
  --ledger "$LEDGER"
  --rpc-port "$RPC_PORT"
  --faucet-port "$FAUCET_PORT"
  --dynamic-port-range "$DYNAMIC_PORTS"
)
if [ "${VAULT_LOCALNET_CLONE_ORCA:-1}" = "1" ]; then
  VALIDATOR_ARGS+=(--clone "$ORCA_WHIRLPOOL_PROGRAM_ID" --url https://api.mainnet-beta.solana.com)
  log "cloning the canonical Orca Whirlpool program into localnet"
fi

solana-test-validator "${VALIDATOR_ARGS[@]}" >target/local-validator.stdout 2>&1 &
VALIDATOR_PID=$!
trap 'kill "$VALIDATOR_PID" 2>/dev/null || true; wait "$VALIDATOR_PID" 2>/dev/null || true' EXIT

for _ in $(seq 1 240); do
  solana --url "$URL" cluster-version >/dev/null 2>&1 && break
  sleep 1
done
solana --url "$URL" cluster-version >/dev/null 2>&1 || die "local validator did not become ready"

solana --url "$URL" airdrop 100 "$(solana-keygen pubkey "$PAYER")" >/dev/null
solana --url "$URL" program deploy --use-rpc \
  "target/deploy/${PROGRAM_NAME}.so" \
  --program-id "$DEPLOY_KEYPAIR" \
  --upgrade-authority "$PAYER" \
  --keypair "$PAYER"

TEST_RUNNER="${ANCHOR_DIR}/node_modules/.bin/ts-mocha"
[ -x "$TEST_RUNNER" ] || TEST_RUNNER="${ANCHOR_DIR}/../node_modules/.bin/ts-mocha"
[ -x "$TEST_RUNNER" ] || die "ts-mocha is not installed"

ANCHOR_PROVIDER_URL="$URL" ANCHOR_WALLET="${ANCHOR_DIR}/${PAYER}" \
  "$TEST_RUNNER" -p ./tsconfig.json -t 1000000 tests/**/*.ts
ANCHOR_PROVIDER_URL="$URL" bash scripts/verify.sh localnet
