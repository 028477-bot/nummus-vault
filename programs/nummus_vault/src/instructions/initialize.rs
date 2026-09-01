
use crate::constants::*;
use crate::errors::VaultError;
use crate::events::VaultInitialized;
use crate::state::*;
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct InitializeArgs {
    pub whirlpool: Pubkey,
    pub token_mint_a: Pubkey,
    pub token_mint_b: Pubkey,
    pub vault_token_account_a: Pubkey,
    pub vault_token_account_b: Pubkey,
    pub min_tick: i32,
    pub max_tick: i32,
    pub max_slippage_bps: u16,
    pub vault_authority: Pubkey,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = admin,
        space = Config::LEN,
        seeds = [CONFIG_SEED],
        bump
    )]
    pub config: Account<'info, Config>,

    #[account(
        init,
        payer = admin,
        space = 0,
        seeds = [VAULT_SOL_SEED],
        bump,
        owner = system_program.key()
    )]
    pub vault_sol: UncheckedAccount<'info>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_handler(ctx: Context<Initialize>, args: InitializeArgs) -> Result<()> {
    require!(
        args.max_slippage_bps <= MAX_SLIPPAGE_BPS,
        VaultError::SlippageTooHigh
    );
    require!(args.min_tick < args.max_tick, VaultError::TickRangeOutOfBounds);
    require!(
        (args.max_tick as i64 - args.min_tick as i64) <= MAX_TICK_RANGE_WIDTH as i64,
        VaultError::TickRangeOutOfBounds
    );
    crate::logic::validate_native_sol_config(
        &args.token_mint_a,
        &args.token_mint_b,
        &args.vault_token_account_a,
        &args.vault_token_account_b,
    )?;

    let config = &mut ctx.accounts.config;
    config.version = CONFIG_VERSION;
    config.config_bump = ctx.bumps.config;
    config.vault_sol_bump = ctx.bumps.vault_sol;
    config.admin = ctx.accounts.admin.key();
    config.vault_authority = args.vault_authority;
    config.pending_admin = Pubkey::default();
    config.pending_vault_authority = Pubkey::default();
    config.deposits_paused = false;
    config.legacy_reserved_byte = 0;
    config.liquidity_paused = false;
    config.whirlpool = args.whirlpool;
    config.token_mint_a = args.token_mint_a;
    config.token_mint_b = args.token_mint_b;
    config.vault_token_account_a = args.vault_token_account_a;
    config.vault_token_account_b = args.vault_token_account_b;
    config.position = Pubkey::default();
    config.position_mint = Pubkey::default();
    config.position_token_account = Pubkey::default();
    config.position_sequence = 0;
    config.min_tick = args.min_tick;
    config.max_tick = args.max_tick;
    config.max_slippage_bps = args.max_slippage_bps;
    config.total_deposits = 0;
    config.total_withdrawals = 0;
    config.reserved = [0u8; 56];

    emit!(VaultInitialized {
        config: config.key(),
        vault_sol: ctx.accounts.vault_sol.key(),
        admin: config.admin,
        vault_authority: config.vault_authority,
        whirlpool: config.whirlpool,
        token_mint_a: config.token_mint_a,
        token_mint_b: config.token_mint_b,
    });

    Ok(())
}
