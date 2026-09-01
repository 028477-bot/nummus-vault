use anchor_lang::prelude::*;

#[event]
pub struct VaultInitialized {
    pub config: Pubkey,
    pub vault_sol: Pubkey,
    pub admin: Pubkey,
    pub vault_authority: Pubkey,
    pub whirlpool: Pubkey,
    pub token_mint_a: Pubkey,
    pub token_mint_b: Pubkey,
}

#[event]
pub struct DepositCommitted {
    pub owner: Pubkey,
    pub deposit_id: u64,
    pub amount: u64,
    pub position_balance_after: u64,
    pub receipt: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct WithdrawalSettled {
    pub owner: Pubkey,
    pub request_id: u64,
    pub amount: u64,
    pub destination: Pubkey,
    pub position_balance_after: u64,
    pub receipt: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct AuthorityRotationProposed {
    pub role: u8,
    pub current: Pubkey,
    pub pending: Pubkey,
}

#[event]
pub struct AuthorityRotationAccepted {
    pub role: u8,
    pub new_authority: Pubkey,
}

#[event]
pub struct PauseFlagsUpdated {
    pub deposits_paused: bool,
    pub liquidity_paused: bool,
}

#[event]
pub struct LiquidityOperation {
    pub kind: u8,
    pub whirlpool: Pubkey,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub timestamp: i64,
}
