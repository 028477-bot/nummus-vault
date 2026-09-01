use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("Config account version does not match the program's expected version")]
    ConfigVersionMismatch,
    #[msg("Account version does not match the program's expected version")]
    AccountVersionMismatch,

    #[msg("Signer is not the configured vault authority")]
    UnauthorizedVaultAuthority,
    #[msg("Signer is not the configured admin authority")]
    UnauthorizedAdmin,
    #[msg("Signer is not the pending authority for this rotation")]
    UnauthorizedPendingAuthority,
    #[msg("No pending authority rotation is in progress")]
    NoPendingAuthority,

    #[msg("Deposits are currently paused")]
    DepositsPaused,
    #[msg("Reserved legacy error code")]
    ReservedLegacyError6007,
    #[msg("Liquidity operations are currently paused")]
    LiquidityPaused,

    #[msg("Deposit amount must be greater than zero")]
    ZeroAmount,
    #[msg("Deposit id has already been used for this owner (replay)")]
    DepositReplay,
    #[msg("Destination vault account does not match the canonical SOL vault PDA")]
    InvalidVaultDestination,

    #[msg("Position owner does not match the provided wallet")]
    PositionOwnerMismatch,
    #[msg("Withdrawal destination is not the wallet bound to this position")]
    WithdrawalDestinationMismatch,
    #[msg("Position does not have enough recorded balance for this withdrawal")]
    InsufficientPositionBalance,

    #[msg("Withdrawal request id has already been settled (replay)")]
    WithdrawalReplay,
    #[msg("Withdrawing this amount would breach the vault rent reserve")]
    RentReserveBreach,

    #[msg("Checked arithmetic overflow")]
    MathOverflow,

    #[msg("Provided Orca Whirlpool program does not match the pinned program id")]
    InvalidWhirlpoolProgram,
    #[msg("Provided Whirlpool does not match the configured pool")]
    InvalidWhirlpool,
    #[msg("Provided token mint does not match the configured mint")]
    InvalidMint,
    #[msg("Provided vault token account is not owned/authorized by the vault PDA")]
    InvalidVaultTokenAccount,
    #[msg("Requested tick range is outside the configured/allowed bounds")]
    TickRangeOutOfBounds,
    #[msg("Requested slippage exceeds the configured maximum")]
    SlippageTooHigh,
    #[msg("Orca CPI instruction discriminator is not on the allowlist")]
    DisallowedOrcaInstruction,
    #[msg("Orca CPI account set does not match the required, constrained layout")]
    OrcaAccountConstraintViolation,
    #[msg("Liquidity proceeds must return to a vault-controlled account")]
    NonVaultProceedsDestination,
    #[msg("Provided Orca position does not belong to the configured pool/mint")]
    InvalidPosition,
    #[msg("Provided tick array does not belong to the pool or configured ticks")]
    InvalidTickArray,
    #[msg("A position is already open; close it before opening another")]
    PositionAlreadyOpen,
    #[msg("No position is open for this operation")]
    NoOpenPosition,
    #[msg("Provided position token account is not the pinned vault-owned NFT account")]
    InvalidPositionTokenAccount,
    #[msg("Requested Orca operation kind is not supported")]
    UnsupportedOrcaOperation,
    #[msg("Configured SOL/USDC pool must contain wrapped SOL as exactly one mint")]
    InvalidNativeSolMint,
    #[msg("Configured vault token account must be the canonical ATA for the vault PDA and mint")]
    InvalidAssociatedTokenAccount,
}
