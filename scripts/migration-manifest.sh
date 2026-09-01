#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"
cd "$ANCHOR_DIR"

CLUSTER="${1:-devnet}"
[ "$CLUSTER" = "devnet" ] || die "migration rehearsal runs on devnet only"

OUT_DIR="${ANCHOR_DIR}/migration"
mkdir -p "$OUT_DIR"
TS="$(date -u +%Y%m%dT%H%M%SZ)"
MANIFEST="${OUT_DIR}/manifest-${TS}.json"

IDL_HASH="$(sha256sum "$CANONICAL_IDL" | awk '{print $1}')"
BIN_HASH="none"
[ -f "target/deploy/${PROGRAM_NAME}.so" ] && BIN_HASH="$(sha256sum "target/deploy/${PROGRAM_NAME}.so" | awk '{print $1}')"

cat > "$MANIFEST" <<JSON
{
  "kind": "migration-manifest-rehearsal",
  "generatedAtUtc": "${TS}",
  "cluster": "${CLUSTER}",
  "programId": "${CANONICAL_PROGRAM_ID}",
  "idlSha256": "${IDL_HASH}",
  "binarySha256": "${BIN_HASH}",
  "orcaWhirlpoolProgramId": "${ORCA_WHIRLPOOL_PROGRAM_ID}",
  "movesFunds": false,
  "notes": "Rehearsal only. No live funds moved. Populate 'positions' from a read-only snapshot of legacy positions/confirmed deposits/queues before a real migration; preserve original ids + idempotency keys.",
  "positions": []
}
JSON

log "wrote rehearsal manifest: $MANIFEST"
log "idl sha256:    $IDL_HASH"
log "binary sha256: $BIN_HASH"
warn "This is a REHEARSAL. It does not move funds and is not a mainnet gate."
