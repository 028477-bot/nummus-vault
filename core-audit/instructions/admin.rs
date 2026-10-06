
use crate::constants::*;
use crate::errors::VaultError;
use crate::events::{AuthorityRotationAccepted, AuthorityRotationProposed, ConfigUpdated, PauseFlagsUpdated};
use crate::state::*;
use anchor_lang::prelude::*;

pub(crate) fn validate_distinct_authorities(admin: Pubkey, vault_authority: Pubkey) -> Result<()> {
    require!(
        admin != Pubkey::default()
            && vault_authority != Pubkey::default()
            && admin != vault_authority,
        VaultError::UnauthorizedAdmin
    );
    Ok(())
}

fn validate_rotation_consent(
    admin: Pubkey,
    vault_authority: Pubkey,
    consenting_admin: Option<Pubkey>,
    outgoing_authority: Option<Pubkey>,
) -> Result<()> {
    require!(consenting_admin == Some(admin), VaultError::UnauthorizedAdmin);
    require!(
        outgoing_authority == Some(vault_authority),
        VaultError::UnauthorizedVaultAuthority
    );
    Ok(())
}

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
    crate::logic::validate_range(min_tick, max_tick)?;
    crate::logic::validate_native_sol_config(
        &ctx.accounts.config.token_mint_a,
        &ctx.accounts.config.token_mint_b,
        &vault_token_account_a,
        &vault_token_account_b,
    )?;

    let config = &mut ctx.accounts.config;
    emit!(ConfigUpdated {
        config: config.key(),
        actor: ctx.accounts.admin.key(),
        old_min_tick: config.min_tick,
        new_min_tick: min_tick,
        old_max_tick: config.max_tick,
        new_max_tick: max_tick,
        old_max_slippage_bps: config.max_slippage_bps,
        new_max_slippage_bps: max_slippage_bps,
        old_vault_token_account_a: config.vault_token_account_a,
        new_vault_token_account_a: vault_token_account_a,
        old_vault_token_account_b: config.vault_token_account_b,
        new_vault_token_account_b: vault_token_account_b,
    });
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
    // Preserve the existing zero-key cancellation convention.
    if new_authority != Pubkey::default() {
        match parsed {
            AuthorityRole::VaultAuthority => {
                validate_distinct_authorities(config.admin, new_authority)?;
                require!(new_authority != config.vault_authority, VaultError::UnauthorizedPendingAuthority);
            }
            AuthorityRole::Admin => {
                validate_distinct_authorities(new_authority, config.vault_authority)?;
                require!(new_authority != config.admin, VaultError::UnauthorizedPendingAuthority);
            }
        }
    }
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
    // Required for vault-authority acceptance only. Admin rotation retains its
    // existing proposal/acceptance semantics. No key-loss recovery is implied.
    pub outgoing_authority: Option<Signer<'info>>,
    pub current_admin: Option<Signer<'info>>,
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
            validate_rotation_consent(
                config.admin,
                config.vault_authority,
                ctx.accounts.current_admin.as_ref().map(|s| s.key()),
                ctx.accounts.outgoing_authority.as_ref().map(|s| s.key()),
            )?;
            require!(
                config.pending_vault_authority != Pubkey::default(),
                VaultError::NoPendingAuthority
            );
            require_keys_eq!(
                ctx.accounts.new_authority.key(),
                config.pending_vault_authority,
                VaultError::UnauthorizedPendingAuthority
            );
            validate_distinct_authorities(config.admin, config.pending_vault_authority)?;
            require!(
                config.pending_vault_authority != config.vault_authority,
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
            // Recheck at acceptance: the other role may have rotated since proposal.
            validate_distinct_authorities(config.pending_admin, config.vault_authority)?;
            require!(config.pending_admin != config.admin, VaultError::UnauthorizedPendingAuthority);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_roles_must_be_distinct_and_nonzero() {
        let admin = Pubkey::new_unique();
        let vault = Pubkey::new_unique();
        assert!(validate_distinct_authorities(admin, vault).is_ok());
        assert!(validate_distinct_authorities(admin, admin).is_err());
        assert!(validate_distinct_authorities(admin, Pubkey::default()).is_err());
        assert!(validate_distinct_authorities(Pubkey::default(), vault).is_err());
    }

    #[test]
    fn vault_replacement_requires_both_current_authorities() {
        let admin = Pubkey::new_unique();
        let vault = Pubkey::new_unique();
        let attacker = Pubkey::new_unique();
        assert!(validate_rotation_consent(admin, vault, Some(admin), Some(vault)).is_ok());
        assert!(validate_rotation_consent(admin, vault, None, Some(vault)).is_err());
        assert!(validate_rotation_consent(admin, vault, Some(admin), None).is_err());
        assert!(validate_rotation_consent(admin, vault, Some(attacker), Some(vault)).is_err());
        assert!(validate_rotation_consent(admin, vault, Some(admin), Some(attacker)).is_err());
    }
}
