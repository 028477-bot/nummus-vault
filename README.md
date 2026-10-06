# Nummus Vault V10.4

Solana custody contract with restricted Orca SOL/USDC position operations.
The vault owns the liquidity position; an authorized caller requests execution.

## Jupiter swap execution

The authorized server supplies direction, exact input, minimum output and Jupiter
instruction data. `execute_jupiter_swap` executes SOL ↔ USDC with vault-controlled
accounts; it does not choose quotes, routes or trading strategy. Jupiter routing
is independent of the configured Orca liquidity pool. The existing liquidity
pause also blocks this new instruction.

The supported Jupiter instruction encoding is V1 `route` or
`shared_accounts_route`, ExactIn, without platform fees. Unknown versions,
ExactOut and token-ledger routes are rejected. Jupiter API version, Jupiter
instruction version and Solana transaction version are separate concepts.
Live API availability and successful Jupiter CPI execution remain unverified.
This source/binary package does not activate a signing policy or deploy a program.

## Withdrawal execution

The requesting application determines the withdrawal amount outside this
contract. The vault authority authorizes that exact amount for the user's bound
wallet. An approved withdrawal of 11.25 SOL transfers exactly 11.25 SOL: the
contract does not recalculate, reduce or cap it at the recorded deposit balance.
If available SOL cannot cover the full payout while preserving the required
reserve, the transaction fails atomically; it does not make a partial payment.

The contract no longer calculates a remaining user balance. The deprecated
balance field remains only as an unused storage slot for account-layout
compatibility. Successful deposits and withdrawals write zero to it; the
corresponding event field also emits zero. Untouched existing accounts may
contain historical values until their next successful operation under this
version; those values are not withdrawal entitlements. Receipts and cumulative
withdrawal counters continue to record the full amount actually paid.
Authority authorization, wallet binding and request replay protection remain
required. The contract does not independently verify the application's
entitlement calculation or guarantee that aggregate approved payouts are funded.
This execution model does not by itself resolve the audit's backing-shortfall
concern; auditor reassessment is required.

## Execution responsibilities and audit dispositions

- **IVAP / USEL / SPDFE — operator price responsibility:** the server selects
  quotes, their freshness and acceptable trading limits for both liquidity
  directions and counter-token swaps. The contract and Orca enforce the supplied
  token maxima/minima and explicit swap price limit. They do not independently
  assess fair price or certify a percentage slippage guarantee. The retained
  `slippage_bps` declaration and ceiling are compatibility/authorization inputs,
  not an independent market-price check. A compromised authorized execution
  service can choose economically harmful limits. No oracle guarantee is claimed.
- **PFDS — recovery during pause:** allocation pause blocks opening/increasing
  liquidity, not decreasing, fee/reward collection, closure, conversion or
  unwrapping. The server keeps close/collect/settlement paths available. All
  existing authority, account, destination and execution-limit checks still apply.
- **DLBPC — residual cleanup:** both-zero removal minima are accepted only for
  the entire remaining position and only if actual outputs of both tokens are
  zero. Any nonzero output rolls back the transaction. Ordinary removals still
  require the server's limits. Cleanup is independent of updated allocation bands.
- **LRSPC — reserve recovery:** conversion into an existing canonical native ATA
  may proceed below the SOL reserve floor when it spends no SOL. ATA creation
  and SOL top-ups still preserve the floor. If the ATA is absent, the operator
  must fund its creation before recovery; no reserve-spending bypass is provided.
- **TTRRI — third-token exit:** `release_reward` moves only a configured Whirlpool
  reward mint outside the trading pair from the vault's canonical ATA to the
  current root administrator's canonical ATA. SOL and both trading-pair mints
  are forbidden. The administrator signs under root approval; LP automation
  cannot invoke it. The destination ATA must already exist. This releases the
  reward for operator-managed realization; it is not an automatic swap.
- **WRSC — intentional global IDs:** withdrawal receipt seeds stay globally scoped
  and unchanged to preserve existing replay protection. The server is the sole
  issuer: identifiers must be unique across all users of this program, not
  per-user counters. Existing hashed identifiers incorporate the position, wallet
  and signed authorization nonce; hashing is not a mathematical uniqueness
  guarantee. An occupied identifier for a different intent must fail closed and
  be reissued before signing, never treated as another user's successful payout.
  A global unique database index prevents duplicate issuance; collision handling
  compares owner, position, destination, program and exact amount before reporting
  a retry as already processing. A different intent receives an explicit conflict.
  Owner, destination and amount must match the receipt before settlement.
- **ARNR / PCR — explicit custody dependency:** this is an operator-controlled
  vault, not independent depositor redemption. Normal authority replacement
  requires outgoing-authority consent. There is no on-chain lost-key bypass.
  Turnkey signer availability and its recovery arrangements remain operational
  requirements. If those fail, recovery requires a separately reviewed program
  upgrade authorized by the distinct 4-of-6 root-controlled upgrade authority.
  Root control and recovery procedures must be verified before live funds enter;
  this source package does not prove they are configured.
- **RRPL — permanent replay evidence:** receipts deliberately remain open.
  Closing one would permit reuse of its identifier; hashed and out-of-order IDs
  cannot safely use a high-water mark. Each depositor funds their deposit receipt,
  and the withdrawal payer funds the withdrawal receipt. Rent is permanently
  locked (approximately 0.00135 / 0.00157 SOL respectively at the audit's rent
  parameters; actual rent depends on cluster parameters). It is not deducted
  from the approved withdrawal payout.
- **TRWI — range responsibility:** the full legal Orca tick span is supported
  intentionally. The server chooses strategy width within administrator-configured
  endpoints; those endpoints are enforced on-chain. The absolute width constant
  is a protocol-domain check, not an additional narrow-strategy guarantee.
- **SCLSC — exact payout, not a solvency guarantee:** the server-approved amount
  is paid unchanged or the transaction fails atomically. Nominal user-balance
  accounting and the deposit-balance payout ceiling are removed. The program
  provides authorized custody and execution, not entitlement calculation.
  Ensuring sufficient backing and allocating losses across approved payouts
  remain the operator's responsibility. Removing the ledger does not itself
  prevent withdrawal ordering from shifting losses to later users. This is a
  custody-model response for auditor reassessment, not a claim that the
  underlying risk is eliminated. No on-chain loss allocation, payout haircut
  or independent solvency guarantee is added.

These are code remediations and explicit design disclosures for auditor
reassessment, not a declaration that every finding is resolved. Root policy
templates are not activated by this package.

## Verification status

Prepared for independent audit review. This is not an audit clearance, deployment
attestation or authorization to deploy. No live-cluster verification is claimed
for this candidate. Successful end-to-end Orca CPI execution and live signing
policy enforcement remain unverified. Execution bounds are supplied by the
authorized caller; the declared slippage percentage is not independently derived
from an on-chain reference quote.

Program address: `BaRfuBXneEAf6eFh3e7ECqNax8NyAmWHb3SkMWtSPUZw`.

## Contents

- `audit/programs/nummus_vault/`: complete contract source.
- `audit/idl/nummus_vault.json`: source-generated interface.
- `audit/bin/nummus_vault.so`: locally compiled SBF binary.
- Cargo workspace configuration and locked dependency versions.
- `audit/tools/check-idl.mjs`: source/interface consistency check.
- `SHA256SUMS`: integrity manifest for all included files except itself.

The existing repository folders are preserved. Contract source in
`programs/nummus_vault/src/`, `audit/programs/nummus_vault/src/` and
`core-audit/` is identical. Both published IDL copies match. Existing signing
policy templates are retained, not activated or certified for the new operations.

## Local build and interface check

Use Anchor CLI 0.30.0, Solana CLI 1.18.26 (SBF platform-tools v1.41),
host Rust/Cargo 1.77.2 and Node.js 20 or later. These must be installed first.
The first build may download dependencies. No wallet key is included or needed
for compilation. Do not run deployment commands to reproduce this package.

```sh
sha256sum -c SHA256SUMS
cd audit
cargo test --locked
node tools/check-idl.mjs --check
cargo build-sbf --tools-version v1.41 --manifest-path programs/nummus_vault/Cargo.toml -- --locked
cmp bin/nummus_vault.so target/deploy/nummus_vault.so
```

The binary comparison is a check, not a guarantee of byte-identical output on
an unverified host/toolchain. Report any mismatch; do not replace the reference
binary or interface silently. On a read-only SDK installation, first copy the
SBF SDK to a writable directory and set SBF_SDK_PATH to that directory.
Existing deployment tooling is retained but is not authorization to deploy.
