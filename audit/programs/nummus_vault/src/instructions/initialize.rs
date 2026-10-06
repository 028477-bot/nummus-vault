
use crate::constants::*;
use crate::errors::VaultError;
use crate::events::VaultInitialized;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::bpf_loader_upgradeable;
use anchor_lang::system_program::{self, Transfer};

fn is_current_upgrade_authority(program_data: &ProgramData, admin: Pubkey) -> bool {
    program_data.upgrade_authority_address == Some(admin)
}

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

    // Account<ProgramData> verifies loader ownership and the ProgramData variant.
    // The loader's canonical PDA binds this authority to this program, not to an
    // arbitrary program controlled by the initializer.
    #[account(
        seeds = [crate::ID.as_ref()],
        bump,
        seeds::program = bpf_loader_upgradeable::ID,
        constraint = is_current_upgrade_authority(&program_data, admin.key())
            @ VaultError::UnauthorizedAdmin,
    )]
    pub program_data: Account<'info, ProgramData>,
}

pub fn initialize_handler(ctx: Context<Initialize>, args: InitializeArgs) -> Result<()> {
    require!(
        args.max_slippage_bps <= MAX_SLIPPAGE_BPS,
        VaultError::SlippageTooHigh
    );
    crate::logic::validate_range(args.min_tick, args.max_tick)?;
    super::admin::validate_distinct_authorities(
        ctx.accounts.admin.key(),
        args.vault_authority,
    )?;
    crate::logic::validate_native_sol_config(
        &args.token_mint_a,
        &args.token_mint_b,
        &args.vault_token_account_a,
        &args.vault_token_account_b,
    )?;

    // `init` funds rent; the admin separately funds the operating reserve.
    // Neither component is credited as a user's deposit.
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.admin.to_account_info(),
                to: ctx.accounts.vault_sol.to_account_info(),
            },
        ),
        VAULT_RENT_RESERVE_LAMPORTS,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializer_must_be_current_upgrade_authority() {
        let admin = Pubkey::new_unique();
        let replacement = Pubkey::new_unique();
        let mut data = ProgramData { slot: 0, upgrade_authority_address: Some(admin) };
        assert!(is_current_upgrade_authority(&data, admin));
        assert!(!is_current_upgrade_authority(&data, replacement));
        data.upgrade_authority_address = Some(replacement);
        assert!(!is_current_upgrade_authority(&data, admin));
        assert!(is_current_upgrade_authority(&data, replacement));
        data.upgrade_authority_address = None;
        assert!(!is_current_upgrade_authority(&data, admin));
        assert!(!is_current_upgrade_authority(&data, replacement));
    }

    #[test]
    fn program_data_account_rejects_wrong_owner_and_wrong_variant() {
        let key = Pubkey::new_unique();
        let wrong_owner = Pubkey::new_unique();
        let mut lamports = 1;
        let mut bytes = [0u8; 45];
        let info = AccountInfo::new(
            &key, false, false, &mut lamports, &mut bytes, &wrong_owner, false, 0,
        );
        assert!(Account::<ProgramData>::try_from(&info).is_err());

        let mut lamports = 1;
        // Loader state tag 0 is Uninitialized, not ProgramData (tag 3).
        let mut bytes = [0u8; 45];
        let loader = bpf_loader_upgradeable::ID;
        let info = AccountInfo::new(
            &key, false, false, &mut lamports, &mut bytes, &loader, false, 0,
        );
        assert!(Account::<ProgramData>::try_from(&info).is_err());
    }
}
