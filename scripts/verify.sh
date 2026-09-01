#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
CLUSTER="${1:-localnet}"
case "$CLUSTER" in
  localnet) URL="${ANCHOR_PROVIDER_URL:-http://127.0.0.1:18999}" ;;
  devnet)   URL="${DEVNET_RPC_URL:-https://api.devnet.solana.com}" ;;
  *) die "verify.sh accepts only localnet|devnet (mainnet is hard-gated)" ;;
esac
if [ "$CLUSTER" = "devnet" ]; then
  assert_devnet_rpc "$URL"
else
  assert_not_mainnet "$URL"
fi
cd "$ANCHOR_DIR"

fail=0
require_release_solana_cli
require_bin solana-keygen

DECLARED="$(grep -oP 'declare_id!\("\K[^"]+' programs/${PROGRAM_NAME}/src/lib.rs)"
if [ "$DECLARED" = "$CANONICAL_PROGRAM_ID" ]; then
  log "OK  declare_id! == canonical ($CANONICAL_PROGRAM_ID)"
else
  warn "FAIL declare_id! ($DECLARED) != canonical ($CANONICAL_PROGRAM_ID)"; fail=1
fi

if grep -q "$CANONICAL_PROGRAM_ID" Anchor.toml; then
  log "OK  Anchor.toml references canonical program id"
else
  warn "FAIL Anchor.toml missing canonical program id"; fail=1
fi

IDL_ADDR="$(node -e "console.log(require('./idl/${PROGRAM_NAME}.json').address)" 2>/dev/null || true)"
if [ "$IDL_ADDR" = "$CANONICAL_PROGRAM_ID" ]; then
  log "OK  IDL address == canonical"
else
  warn "FAIL IDL address ($IDL_ADDR) != canonical"; fail=1
fi

if command -v cargo >/dev/null 2>&1; then
  if node scripts/gen-idl.mjs --check >/dev/null 2>&1; then
    log "OK  committed IDL == source-generated IDL"
  else
    warn "FAIL committed IDL drifted from source — run: node scripts/gen-idl.mjs"; fail=1
  fi
else
  warn "note: cargo not available; skipped source-vs-IDL regeneration check"
fi

if [ ! -f "$DEPLOY_KEYPAIR" ]; then
  warn "FAIL deploy keypair absent — cannot prove deployment identity"; fail=1
else
  KP="$(solana-keygen pubkey "$DEPLOY_KEYPAIR")"
  if [ "$KP" = "$CANONICAL_PROGRAM_ID" ]; then log "OK  deploy keypair == canonical"; else warn "FAIL deploy keypair ($KP) != canonical"; fail=1; fi
fi

if [ -f "target/deploy/${PROGRAM_NAME}.so" ]; then
  HASH="$(sha256sum "target/deploy/${PROGRAM_NAME}.so" | awk '{print $1}')"
  log "built binary sha256: $HASH"
  if solana --url "$URL" account "$CANONICAL_PROGRAM_ID" --output json >/dev/null 2>&1; then
    log "on-chain program account exists on $CLUSTER"
    rm -f /tmp/${PROGRAM_NAME}_onchain.so
    solana --url "$URL" program dump "$CANONICAL_PROGRAM_ID" /tmp/${PROGRAM_NAME}_onchain.so >/dev/null 2>&1 || {
      warn "FAIL could not dump deployed program bytes"; fail=1;
    }
    if [ -f /tmp/${PROGRAM_NAME}_onchain.so ]; then
      ONCHAIN_ALLOCATION="$(sha256sum /tmp/${PROGRAM_NAME}_onchain.so | awk '{print $1}')"
      log "on-chain allocation sha256: $ONCHAIN_ALLOCATION"
      if node - "target/deploy/${PROGRAM_NAME}.so" /tmp/${PROGRAM_NAME}_onchain.so <<'NODE'
const fs = require("node:fs");
const [localPath, onChainPath] = process.argv.slice(2);
const local = fs.readFileSync(localPath);
const onChain = fs.readFileSync(onChainPath);
const prefixMatches =
  onChain.length >= local.length &&
  onChain.subarray(0, local.length).equals(local);
const zeroPadding = onChain.subarray(local.length).every((byte) => byte === 0);
if (!prefixMatches || !zeroPadding) process.exit(1);
console.log(
  `[vault] deployed release bytes=${local.length}; zero padding=${onChain.length - local.length}`,
);
NODE
      then
        log "OK  local binary == deployed release byte-for-byte (zero-padded allocation allowed)"
      else
        warn "FAIL local binary differs from deployed program"; fail=1
      fi
    fi
  else
    warn "FAIL canonical program is not deployed/reachable on $CLUSTER"; fail=1
  fi
else
  warn "FAIL program .so absent — no deployable release exists"; fail=1
fi

[ "$fail" -eq 0 ] && { log "VERIFY PASSED (identifiers consistent)"; exit 0; } || die "VERIFY FAILED"
