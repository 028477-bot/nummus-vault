#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
require_bin cargo

cd "$ANCHOR_DIR"

log "cargo unit tests (host) ..."
cargo test --locked

log "generating source-derived IDL + TS types ..."
node scripts/gen-idl.mjs

log "anchor build (BPF .so) ..."
if command -v anchor >/dev/null 2>&1 && command -v cargo-build-sbf >/dev/null 2>&1; then
  configure_sbf_toolchain
  anchor build --no-idl
  if [ -f "target/idl/${PROGRAM_NAME}.json" ]; then
    if ! diff -q "target/idl/${PROGRAM_NAME}.json" "$CANONICAL_IDL" >/dev/null 2>&1; then
      warn "anchor-generated IDL differs from source-derived canonical IDL — investigate before release."
    else
      log "anchor-generated IDL matches source-derived canonical IDL."
    fi
  fi
else
  die "SBF build did not run: install the pinned Solana/Anchor tools and put cargo-build-sbf in PATH. Any existing target/deploy binary is NOT evidence of this build."
fi

log "build step complete."
