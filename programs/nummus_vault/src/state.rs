
use anchor_lang::prelude::*;

#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthorityRole {
    VaultAuthority = 0,
    Admin = 1,
}

#[account]
pub struct Config {
    pub version: u8,
    pub config_bump: u8,
    pub vault_sol_bump: u8,

    pub admin: Pubkey,
    pub vault_authority: Pubkey,

    pub pending_admin: Pubkey,
    pub pending_vault_authority: Pubkey,

    pub deposits_paused: bool,
    pub legacy_reserved_byte: u8,
    pub liquidity_paused: bool,

    pub whirlpool: Pubkey,
    pub token_mint_a: Pubkey,
    pub token_mint_b: Pubkey,
    pub vault_token_account_a: Pubkey,
    pub vault_token_account_b: Pubkey,

    pub position: Pubkey,
    pub position_mint: Pubkey,
    pub position_token_account: Pubkey,
    pub position_sequence: u64,

    pub min_tick: i32,
    pub max_tick: i32,
    pub max_slippage_bps: u16,

    pub total_deposits: u64,
    pub total_withdrawals: u64,

    pub reserved: [u8; 56],
}

impl Config {
    pub const LEN: usize = 8
        + 1
        + 1
        + 1
        + 32
        + 32
        + 32
        + 32
        + 1 + 1
        + 32
        + 32
        + 32
        + 32
        + 32
        + 32
        + 32
        + 32
        + 8
        + 4 + 4
        + 2
        + 8 + 8
        + 57;
}

#[account]
#[derive(Default)]
pub struct UserPosition {
    pub version: u8,
    pub bump: u8,
    pub owner: Pubkey,
    pub balance_lamports: u64,
    pub deposit_count: u64,
    pub withdrawal_count: u64,
    pub created_at: i64,
    pub total_withdrawn_lamports: u64,
    pub reserved: [u8; 24],
}

impl UserPosition {
    pub const LEN: usize = 8 + 1 + 1 + 32 + 8 + 8 + 8 + 8 + 8 + 24;
}

#[account]
#[derive(Default)]
pub struct DepositReceipt {
    pub version: u8,
    pub bump: u8,
    pub owner: Pubkey,
    pub deposit_id: u64,
    pub amount: u64,
    pub timestamp: i64,
}

impl DepositReceipt {
    pub const LEN: usize = 8 + 1 + 1 + 32 + 8 + 8 + 8;
}

#[account]
#[derive(Default)]
pub struct WithdrawalReceipt {
    pub version: u8,
    pub bump: u8,
    pub owner: Pubkey,
    pub request_id: u64,
    pub amount: u64,
    pub destination: Pubkey,
    pub timestamp: i64,
}

impl WithdrawalReceipt {
    pub const LEN: usize = 8 + 1 + 1 + 32 + 8 + 8 + 32 + 8;
}
