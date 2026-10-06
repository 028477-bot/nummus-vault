#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "$0")/common.sh"

: "${MAINNET_AUDIT_PASSED:?refusing: set MAINNET_AUDIT_PASSED=yes only after an independent audit passed}"
: "${MAINNET_DEVNET_REHEARSAL_PASSED:?refusing: set MAINNET_DEVNET_REHEARSAL_PASSED=yes only after a full devnet migration rehearsal}"
: "${MAINNET_OWNER_CUTOVER_APPROVED:?refusing: set MAINNET_OWNER_CUTOVER_APPROVED=yes only with an explicit owner cutover decision}"
: "${MAINNET_UPGRADE_AUTHORITY_IS_TURNKEY_4OF6:?refusing: upgrade authority must be the Turnkey 4-of-6 root policy}"

[ "$MAINNET_AUDIT_PASSED" = "yes" ] || die "MAINNET_AUDIT_PASSED must equal 'yes'"
[ "$MAINNET_DEVNET_REHEARSAL_PASSED" = "yes" ] || die "MAINNET_DEVNET_REHEARSAL_PASSED must equal 'yes'"
[ "$MAINNET_OWNER_CUTOVER_APPROVED" = "yes" ] || die "MAINNET_OWNER_CUTOVER_APPROVED must equal 'yes'"
[ "$MAINNET_UPGRADE_AUTHORITY_IS_TURNKEY_4OF6" = "yes" ] || die "upgrade authority gate not satisfied"

warn "ALL mainnet gates are set. This will deploy to mainnet-beta."
warn "This step is intentionally manual and must be executed by an authorized"
warn "operator using the Turnkey-controlled upgrade authority — NOT a local key."
read -r -p "Type the canonical program id to confirm: " CONFIRM
[ "$CONFIRM" = "$CANONICAL_PROGRAM_ID" ] || die "confirmation mismatch; aborting"

die "Manual step: run the deployment through the Turnkey 4-of-6 upgrade-authority flow. This script does not hold or use a private upgrade key by design."
