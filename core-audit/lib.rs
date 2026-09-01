use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod logic;
pub mod orca_cpi;
pub mod state;

use instructions::*;

declare_id!("BaRfuBXneEAf6eFh3e7ECqNax8NyAmWHb3SkMWtSPUZw");

#[cfg(not(feature = "no-entrypoint"))]
solana_security_txt::security_txt! {
    name: "Nummus Vault",
    project_url: "https://quant88.com",
    contacts: "email:security@quant88.com",
    policy: "https://quant88.com/security",
    preferred_languages: "en",
    source_code: "https://github.com/028477-bot/nummus-vault"
}

#[program]
pub mod nummus_vault {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, args: InitializeArgs) -> Result<()> {
        instructions::initialize::initialize_handler(ctx, args)
    }

    pub fn deposit(ctx: Context<Deposit>, deposit_id: u64, amount: u64) -> Result<()> {
        instructions::deposit::deposit_handler(ctx, deposit_id, amount)
    }

    pub fn withdraw(ctx: Context<Withdraw>, request_id: u64, amount: u64) -> Result<()> {
        instructions::withdraw::withdraw_handler(ctx, request_id, amount)
    }

    pub fn set_pause_flags(
        ctx: Context<AdminOnly>,
        deposits_paused: bool,
        liquidity_paused: bool,
    ) -> Result<()> {
        instructions::admin::set_pause_flags(ctx, deposits_paused, liquidity_paused)
    }

    pub fn update_config(
        ctx: Context<AdminOnly>,
        min_tick: i32,
        max_tick: i32,
        max_slippage_bps: u16,
        vault_token_account_a: Pubkey,
        vault_token_account_b: Pubkey,
    ) -> Result<()> {
        instructions::admin::update_config(
            ctx,
            min_tick,
            max_tick,
            max_slippage_bps,
            vault_token_account_a,
            vault_token_account_b,
        )
    }

    pub fn propose_authority(
        ctx: Context<AdminOnly>,
        role: u8,
        new_authority: Pubkey,
    ) -> Result<()> {
        instructions::admin::propose_authority(ctx, role, new_authority)
    }

    pub fn accept_authority(ctx: Context<AcceptAuthority>, role: u8) -> Result<()> {
        instructions::admin::accept_authority(ctx, role)
    }

    pub fn open_position(ctx: Context<OpenPosition>, args: OpenPositionArgs) -> Result<()> {
        instructions::liquidity::open_position_handler(ctx, args)
    }

    pub fn increase_liquidity(
        ctx: Context<ModifyLiquidity>,
        args: IncreaseLiquidityArgs,
    ) -> Result<()> {
        instructions::liquidity::increase_liquidity_handler(ctx, args)
    }

    pub fn decrease_liquidity(
        ctx: Context<ModifyLiquidity>,
        args: DecreaseLiquidityArgs,
    ) -> Result<()> {
        instructions::liquidity::decrease_liquidity_handler(ctx, args)
    }

    pub fn update_fees_and_rewards(ctx: Context<UpdateFeesAndRewards>) -> Result<()> {
        instructions::liquidity::update_fees_and_rewards_handler(ctx)
    }

    pub fn collect_fees(ctx: Context<CollectFees>) -> Result<()> {
        instructions::liquidity::collect_fees_handler(ctx)
    }

    pub fn close_position(ctx: Context<ClosePosition>) -> Result<()> {
        instructions::liquidity::close_position_handler(ctx)
    }
}
