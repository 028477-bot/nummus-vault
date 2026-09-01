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
  warn "==================== SBF BUILD BLOCKER ===================="
  warn "cargo-build-sbf / anchor SBF platform-tools are NOT installed in this"
  warn "environment, and 'anchor idl build' requires 'cargo +nightly' + the"
  warn "nightly-only proc_macro2 Span::source_file() API (no rustup/nightly here)."
  warn "Host unit tests PASSED and the IDL/types were generated FROM SOURCE via"
  warn "gen-idl.mjs. To produce the deployable .so, run this script in an"
  warn "environment with: rustup (nightly) + solana platform-tools"
  warn "(cargo-build-sbf). Then re-run scripts/verify.sh <cluster>."
  warn "=========================================================="
fi

log "build step complete."
