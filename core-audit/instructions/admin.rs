
use crate::constants::*;
use crate::errors::VaultError;
use crate::events::{AuthorityRotationAccepted, AuthorityRotationProposed, PauseFlagsUpdated};
use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch,
        has_one = admin @ VaultError::UnauthorizedAdmin,
    )]
    pub config: Account<'info, Config>,
    pub admin: Signer<'info>,
}

pub fn set_pause_flags(
    ctx: Context<AdminOnly>,
    deposits_paused: bool,
    liquidity_paused: bool,
) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.deposits_paused = deposits_paused;
    config.liquidity_paused = liquidity_paused;

    emit!(PauseFlagsUpdated {
        deposits_paused,
        liquidity_paused,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn update_config(
    ctx: Context<AdminOnly>,
    min_tick: i32,
    max_tick: i32,
    max_slippage_bps: u16,
    vault_token_account_a: Pubkey,
    vault_token_account_b: Pubkey,
) -> Result<()> {
    require!(
        max_slippage_bps <= MAX_SLIPPAGE_BPS,
        VaultError::SlippageTooHigh
    );
    require!(min_tick < max_tick, VaultError::TickRangeOutOfBounds);
    require!(
        (max_tick as i64 - min_tick as i64) <= MAX_TICK_RANGE_WIDTH as i64,
        VaultError::TickRangeOutOfBounds
    );
    crate::logic::validate_native_sol_config(
        &ctx.accounts.config.token_mint_a,
        &ctx.accounts.config.token_mint_b,
        &vault_token_account_a,
        &vault_token_account_b,
    )?;

    let config = &mut ctx.accounts.config;
    config.min_tick = min_tick;
    config.max_tick = max_tick;
    config.max_slippage_bps = max_slippage_bps;
    config.vault_token_account_a = vault_token_account_a;
    config.vault_token_account_b = vault_token_account_b;
    Ok(())
}

pub fn propose_authority(ctx: Context<AdminOnly>, role: u8, new_authority: Pubkey) -> Result<()> {
    let config = &mut ctx.accounts.config;
    let parsed = match role {
        0 => AuthorityRole::VaultAuthority,
        1 => AuthorityRole::Admin,
        _ => return err!(VaultError::UnauthorizedAdmin),
    };
    match parsed {
        AuthorityRole::VaultAuthority => config.pending_vault_authority = new_authority,
        AuthorityRole::Admin => config.pending_admin = new_authority,
    }
    emit!(AuthorityRotationProposed {
        role,
        current: match parsed {
            AuthorityRole::VaultAuthority => config.vault_authority,
            AuthorityRole::Admin => config.admin,
        },
        pending: new_authority,
    });
    Ok(())
}

#[derive(Accounts)]
#[instruction(role: u8)]
pub struct AcceptAuthority<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch,
    )]
    pub config: Account<'info, Config>,
    pub new_authority: Signer<'info>,
}

pub fn accept_authority(ctx: Context<AcceptAuthority>, role: u8) -> Result<()> {
    let config = &mut ctx.accounts.config;
    let parsed = match role {
        0 => AuthorityRole::VaultAuthority,
        1 => AuthorityRole::Admin,
        _ => return err!(VaultError::UnauthorizedAdmin),
    };
    match parsed {
        AuthorityRole::VaultAuthority => {
            require!(
                config.pending_vault_authority != Pubkey::default(),
                VaultError::NoPendingAuthority
            );
            require_keys_eq!(
                ctx.accounts.new_authority.key(),
                config.pending_vault_authority,
                VaultError::UnauthorizedPendingAuthority
            );
            config.vault_authority = config.pending_vault_authority;
            config.pending_vault_authority = Pubkey::default();
        }
        AuthorityRole::Admin => {
            require!(
                config.pending_admin != Pubkey::default(),
                VaultError::NoPendingAuthority
            );
            require_keys_eq!(
                ctx.accounts.new_authority.key(),
                config.pending_admin,
                VaultError::UnauthorizedPendingAuthority
            );
            config.admin = config.pending_admin;
            config.pending_admin = Pubkey::default();
        }
    }
    emit!(AuthorityRotationAccepted {
        role,
        new_authority: ctx.accounts.new_authority.key(),
    });
    Ok(())
}
