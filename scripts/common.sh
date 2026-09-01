#!/usr/bin/env bash
set -euo pipefail

export CANONICAL_PROGRAM_ID="BaRfuBXneEAf6eFh3e7ECqNax8NyAmWHb3SkMWtSPUZw"
export PROGRAM_NAME="nummus_vault"
export ORCA_WHIRLPOOL_PROGRAM_ID="whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc"
export DEVNET_GENESIS_HASH="EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"

ANCHOR_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ANCHOR_DIR
export DEPLOY_KEYPAIR="${ANCHOR_DIR}/target/deploy/${PROGRAM_NAME}-keypair.json"
export CANONICAL_IDL="${ANCHOR_DIR}/idl/${PROGRAM_NAME}.json"

log()  { printf '\033[1;36m[vault]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[vault][warn]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[vault][fatal]\033[0m %s\n' "$*" >&2; exit 1; }

require_bin() { command -v "$1" >/dev/null 2>&1 || die "required tool not found: $1"; }

require_release_solana_cli() {
  require_bin solana
  local version
  version="$(solana --version 2>/dev/null || true)"
  case "$version" in
    "solana-cli 1.18.26 "*) ;;
    *) die "Solana CLI 1.18.26 required; found '${version:-unknown}'. Put the pinned release first in PATH." ;;
  esac
}

assert_devnet_rpc() {
  local url="${1:-}"
  assert_not_mainnet "$url"
  local genesis
  genesis="$(solana --url "$url" genesis-hash 2>/dev/null || true)"
  [ "$genesis" = "$DEVNET_GENESIS_HASH" ] || \
    die "RPC endpoint did not report the canonical Solana devnet genesis hash"
}

configure_sbf_toolchain() {
  export HOST_CARGO_BIN="${HOST_CARGO_BIN:-$(command -v cargo)}"
  export HOST_RUSTC_BIN="${HOST_RUSTC_BIN:-$(command -v rustc)}"
  export CARGO_REGISTRIES_CRATES_IO_PROTOCOL="${CARGO_REGISTRIES_CRATES_IO_PROTOCOL:-sparse}"
  export PATH="$ANCHOR_DIR/scripts/toolchain-shims:$PATH"
}

assert_not_mainnet() {
  local url="${1:-}"
  case "$url" in
    *mainnet*|*api.mainnet-beta*)
      die "mainnet cluster detected ($url). Mainnet is hard-gated; use scripts/mainnet-deploy.sh which requires the separate owner gate." ;;
  esac
}

assert_keypair_matches_canonical() {
  [ -f "$DEPLOY_KEYPAIR" ] || die "deploy keypair absent: $DEPLOY_KEYPAIR"
  local got
  got="$(solana-keygen pubkey "$DEPLOY_KEYPAIR")"
  [ "$got" = "$CANONICAL_PROGRAM_ID" ] || \
    die "deploy keypair pubkey ($got) != canonical program id ($CANONICAL_PROGRAM_ID)"
}
