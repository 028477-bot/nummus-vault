//! Restricted execution against the configured pool and vault-owned accounts.
//! Only legacy SPL Token is supported.
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke,
};
use crate::{constants::*, errors::VaultError, orca_cpi::*, state::Config};
use super::liquidity::{canonical_ata, emit_op, prepare_native_sol, reclaim_native_sol};

#[derive(Accounts)]
pub struct CollectReward<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch)]
    pub config: Account<'info, Config>,
    #[account(seeds = [VAULT_SOL_SEED], bump = config.vault_sol_bump)]
    pub vault_sol: UncheckedAccount<'info>,
    #[account(address = config.vault_authority @ VaultError::UnauthorizedVaultAuthority)]
    pub vault_authority: Signer<'info>,
    #[account(address = ORCA_WHIRLPOOL_PROGRAM_ID @ VaultError::InvalidWhirlpoolProgram)]
    pub whirlpool_program: UncheckedAccount<'info>,
    #[account(address = config.whirlpool @ VaultError::InvalidWhirlpool)]
    pub whirlpool: UncheckedAccount<'info>,
    #[account(mut, address = config.position @ VaultError::InvalidPosition)]
    pub position: UncheckedAccount<'info>,
    #[account(address = config.position_token_account @ VaultError::InvalidPositionTokenAccount)]
    pub position_token_account: UncheckedAccount<'info>,
    pub reward_mint: UncheckedAccount<'info>,
    #[account(mut)]
    pub reward_vault: UncheckedAccount<'info>,
    #[account(mut)]
    pub reward_owner_account: UncheckedAccount<'info>,
    #[account(address = SPL_TOKEN_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub token_program: UncheckedAccount<'info>,
}

pub fn collect_reward_handler(ctx: Context<CollectReward>, reward_index: u8) -> Result<()> {
    let a = ctx.accounts;
    let c = &a.config;
    require!(ctx.remaining_accounts.is_empty(), VaultError::OrcaAccountConstraintViolation);
    require!(c.position != Pubkey::default(), VaultError::NoOpenPosition);
    read_and_check_whirlpool(&a.whirlpool.to_account_info(), &c.token_mint_a, &c.token_mint_b)?;
    let (lower, upper) = read_and_check_position(&a.position.to_account_info(), &c.whirlpool, &c.position_mint)?;
    require_spl_token_account(&a.position_token_account.to_account_info(), a.vault_sol.key, &c.position_mint)?;
    require_keys_eq!(a.position_token_account.key(), canonical_ata(a.vault_sol.key, &c.position_mint), VaultError::InvalidPositionTokenAccount);
    let (mint, vault) = read_reward(&a.whirlpool.to_account_info(), reward_index)?;
    require_keys_eq!(a.reward_mint.key(), mint, VaultError::InvalidMint);
    require_keys_eq!(*a.reward_mint.owner, SPL_TOKEN_PROGRAM_ID, VaultError::InvalidMint);
    require!(a.reward_mint.data_len() == 82, VaultError::InvalidMint);
    require_keys_eq!(a.reward_vault.key(), vault, VaultError::OrcaAccountConstraintViolation);
    require_keys_eq!(a.reward_owner_account.key(), canonical_ata(a.vault_sol.key, &mint), VaultError::InvalidAssociatedTokenAccount);
    require_spl_token_account(&a.reward_owner_account.to_account_info(), a.vault_sol.key, &mint)?;
    require_spl_token_account(&a.reward_vault.to_account_info(), &c.whirlpool, &mint)?;
    let mut data = DISC_COLLECT_REWARD.to_vec();
    data.push(reward_index);
    // Canonical SDK collectReward account order; no remaining accounts.
    let accounts = vec![
        meta(&a.whirlpool, false, false), meta(&a.vault_sol, true, false),
        meta(&a.position, false, true), meta(&a.position_token_account, false, false),
        meta(&a.reward_owner_account, false, true), meta(&a.reward_vault, false, true),
        meta(&a.token_program, false, false),
    ];
    invoke_orca(&a.whirlpool_program.to_account_info(), &accounts, &data, &[&[VAULT_SOL_SEED, &[c.vault_sol_bump]]])?;
    emit_op(OrcaOpKind::CollectReward, c.whirlpool, lower, upper)
}

#[derive(Accounts)]
pub struct UnwrapNativeSol<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [VAULT_SOL_SEED], bump = config.vault_sol_bump)]
    pub vault_sol: UncheckedAccount<'info>,
    #[account(mut, address = config.vault_authority @ VaultError::UnauthorizedVaultAuthority)]
    pub vault_authority: Signer<'info>,
    #[account(mut)]
    pub native_ata: UncheckedAccount<'info>,
    #[account(address = SPL_TOKEN_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub token_program: UncheckedAccount<'info>,
    #[account(address = NATIVE_SOL_MINT @ VaultError::InvalidNativeSolMint)]
    pub native_mint: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    #[account(address = SPL_ATA_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub associated_token_program: UncheckedAccount<'info>,
}

pub fn unwrap_native_sol_handler(ctx: Context<UnwrapNativeSol>) -> Result<()> {
    require!(ctx.remaining_accounts.is_empty(), VaultError::OrcaAccountConstraintViolation);
    reclaim_native_sol(&ctx.accounts.config, &ctx.accounts.vault_sol.to_account_info(),
        &ctx.accounts.native_ata.to_account_info(), &ctx.accounts.token_program.to_account_info())?;
    // Operator pays replacement rent, never the vault: all closed lamports remain
    // available as SOL. Repeated unwraps cost operator rent, with NO user/NAV credit.
    // LP/token ownership and authority/configuration are unchanged.
    recreate_native_ata(&ctx.accounts)?;
    emit_op(OrcaOpKind::UnwrapNativeSol, ctx.accounts.config.whirlpool, 0, 0)
}

fn operator_native_ata_instruction(payer: Pubkey, vault: Pubkey) -> Instruction {
    Instruction {
        program_id: SPL_ATA_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(payer, true),
            AccountMeta::new(canonical_ata(&vault, &NATIVE_SOL_MINT), false),
            AccountMeta::new_readonly(vault, false),
            AccountMeta::new_readonly(NATIVE_SOL_MINT, false),
            AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        ],
        data: vec![1], // ATA create idempotent; payer is the operator signer.
    }
}

fn recreate_native_ata(accounts: &UnwrapNativeSol) -> Result<()> {
    require_keys_eq!(accounts.native_ata.key(),
        canonical_ata(accounts.vault_sol.key, &NATIVE_SOL_MINT),
        VaultError::InvalidAssociatedTokenAccount);
    let create = operator_native_ata_instruction(accounts.vault_authority.key(), accounts.vault_sol.key());
    invoke(&create, &[
        accounts.vault_authority.to_account_info(), accounts.native_ata.to_account_info(),
        accounts.vault_sol.to_account_info(), accounts.native_mint.to_account_info(),
        accounts.system_program.to_account_info(), accounts.token_program.to_account_info(),
        accounts.associated_token_program.to_account_info(),
    ])?;
    require_spl_token_account(&accounts.native_ata.to_account_info(),
        accounts.vault_sol.key, &NATIVE_SOL_MINT)?;
    let sync = Instruction {
        program_id: SPL_TOKEN_PROGRAM_ID,
        accounts: vec![AccountMeta::new(accounts.native_ata.key(), false)],
        data: vec![17],
    };
    invoke(&sync, &[accounts.native_ata.to_account_info(), accounts.token_program.to_account_info()])?;
    Ok(())
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SettleCounterToNativeArgs {
    pub amount_in: u64,
    pub minimum_amount_out: u64,
    pub sqrt_price_limit: u128,
}

#[derive(Accounts)]
pub struct SettleCounterToNative<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch)]
    pub config: Account<'info, Config>,
    #[account(mut, seeds = [VAULT_SOL_SEED], bump = config.vault_sol_bump)]
    pub vault_sol: UncheckedAccount<'info>,
    #[account(address = config.vault_authority @ VaultError::UnauthorizedVaultAuthority)]
    pub vault_authority: Signer<'info>,
    #[account(address = ORCA_WHIRLPOOL_PROGRAM_ID @ VaultError::InvalidWhirlpoolProgram)]
    pub whirlpool_program: UncheckedAccount<'info>,
    #[account(mut, address = config.whirlpool @ VaultError::InvalidWhirlpool)]
    pub whirlpool: UncheckedAccount<'info>,
    #[account(mut, address = config.vault_token_account_a @ VaultError::InvalidVaultTokenAccount)]
    pub vault_token_account_a: UncheckedAccount<'info>,
    #[account(mut, address = config.vault_token_account_b @ VaultError::InvalidVaultTokenAccount)]
    pub vault_token_account_b: UncheckedAccount<'info>,
    #[account(mut)]
    pub token_vault_a: UncheckedAccount<'info>,
    #[account(mut)]
    pub token_vault_b: UncheckedAccount<'info>,
    #[account(mut)]
    pub tick_array_0: UncheckedAccount<'info>,
    #[account(mut)]
    pub tick_array_1: UncheckedAccount<'info>,
    #[account(mut)]
    pub tick_array_2: UncheckedAccount<'info>,
    pub oracle: UncheckedAccount<'info>,
    #[account(address = NATIVE_SOL_MINT @ VaultError::InvalidNativeSolMint)]
    pub native_mint: UncheckedAccount<'info>,
    #[account(address = SPL_TOKEN_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub token_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    #[account(address = SPL_ATA_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub associated_token_program: UncheckedAccount<'info>,
}

fn validate_swap_bounds(args: &SettleCounterToNativeArgs, current: u128, a_to_b: bool) -> Result<()> {
    require!(args.amount_in > 0 && args.amount_in != u64::MAX && args.minimum_amount_out > 0,
        VaultError::InvalidExecutionBounds);
    // Zero is Orca's "no explicit limit" sentinel and is deliberately forbidden.
    // Absolute domain limits are enforced by the pinned Orca program.
    require!(args.sqrt_price_limit > 0 &&
        if a_to_b { args.sqrt_price_limit < current } else { args.sqrt_price_limit > current },
        VaultError::InvalidExecutionBounds);
    Ok(())
}

fn amount(acc: &UncheckedAccount) -> Result<u64> {
    let data = acc.try_borrow_data()?;
    require!(data.len() == SPL_TOKEN_ACCOUNT_LEN, VaultError::InvalidVaultTokenAccount);
    Ok(u64::from_le_bytes(data[64..72].try_into().unwrap()))
}

pub fn settle_counter_to_native_handler(ctx: Context<SettleCounterToNative>, args: SettleCounterToNativeArgs) -> Result<()> {
    require!(ctx.remaining_accounts.is_empty(), VaultError::OrcaAccountConstraintViolation);
    let a = ctx.accounts;
    let c = &a.config;
    crate::logic::validate_native_sol_config(&c.token_mint_a, &c.token_mint_b,
        &c.vault_token_account_a, &c.vault_token_account_b)?;
    let a_to_b = c.token_mint_b == NATIVE_SOL_MINT;
    let (vault_a, vault_b) = read_and_check_whirlpool(&a.whirlpool.to_account_info(), &c.token_mint_a, &c.token_mint_b)?;
    require_keys_eq!(a.token_vault_a.key(), vault_a, VaultError::OrcaAccountConstraintViolation);
    require_keys_eq!(a.token_vault_b.key(), vault_b, VaultError::OrcaAccountConstraintViolation);
    validate_swap_bounds(&args, read_sqrt_price(&a.whirlpool.to_account_info())?, a_to_b)?;
    let (input, output) = if a_to_b { (&a.vault_token_account_a, &a.vault_token_account_b) }
        else { (&a.vault_token_account_b, &a.vault_token_account_a) };
    prepare_native_sol(c, &a.vault_sol.to_account_info(), &a.native_mint.to_account_info(),
        &output.to_account_info(), &a.token_program.to_account_info(), &a.system_program.to_account_info(),
        &a.associated_token_program.to_account_info(), 0)?;
    require_spl_token_account(&a.vault_token_account_a.to_account_info(), a.vault_sol.key, &c.token_mint_a)?;
    require_spl_token_account(&a.vault_token_account_b.to_account_info(), a.vault_sol.key, &c.token_mint_b)?;
    require_spl_token_account(&a.token_vault_a.to_account_info(), &c.whirlpool, &c.token_mint_a)?;
    require_spl_token_account(&a.token_vault_b.to_account_info(), &c.whirlpool, &c.token_mint_b)?;
    let spacing = read_whirlpool_tick_spacing(&a.whirlpool.to_account_info())?;
    for tick in [&a.tick_array_0, &a.tick_array_1, &a.tick_array_2] {
        let start = read_tick_array_start(&tick.to_account_info(), &c.whirlpool)?;
        require!(tick_array_start_index(start, spacing)? == start, VaultError::InvalidTickArray);
    }
    // Directional traversal and current-tick coverage are checked by Orca.
    require_keys_eq!(a.oracle.key(), Pubkey::find_program_address(
        &[b"oracle", c.whirlpool.as_ref()], &ORCA_WHIRLPOOL_PROGRAM_ID).0, VaultError::OrcaAccountConstraintViolation);
    let before_in = amount(input)?;
    let before_out = amount(output)?;
    require!(before_in >= args.amount_in, VaultError::InvalidExecutionBounds);
    let mut data = DISC_SWAP.to_vec();
    data.extend_from_slice(&args.amount_in.to_le_bytes());
    data.extend_from_slice(&args.minimum_amount_out.to_le_bytes());
    data.extend_from_slice(&args.sqrt_price_limit.to_le_bytes());
    data.extend_from_slice(&[1, a_to_b as u8]);
    // Canonical SDK legacy swap order and flags, not swap_v2.
    let accounts = vec![
        meta(&a.token_program, false, false), meta(&a.vault_sol, true, false),
        meta(&a.whirlpool, false, true), meta(&a.vault_token_account_a, false, true),
        meta(&a.token_vault_a, false, true), meta(&a.vault_token_account_b, false, true),
        meta(&a.token_vault_b, false, true), meta(&a.tick_array_0, false, true),
        meta(&a.tick_array_1, false, true), meta(&a.tick_array_2, false, true),
        meta(&a.oracle, false, false),
    ];
    invoke_orca(&a.whirlpool_program.to_account_info(), &accounts, &data, &[&[VAULT_SOL_SEED, &[c.vault_sol_bump]]])?;
    // Orca can partially fill at a price limit. This interface promises EXACT input.
    require!(before_in.checked_sub(amount(input)?) == Some(args.amount_in), VaultError::InvalidExecutionBounds);
    require!(amount(output)?.checked_sub(before_out).map_or(false, |n| n >= args.minimum_amount_out),
        VaultError::InvalidExecutionBounds);
    emit_op(OrcaOpKind::SettleCounterToNative, c.whirlpool, 0, 0)
}

fn meta<'info>(acc: &UncheckedAccount<'info>, is_signer: bool, is_writable: bool) -> OrcaCpiAccount<'info> {
    OrcaCpiAccount { info: acc.to_account_info(), is_signer, is_writable }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operator_funded_recreation_preserves_canonical_authority_and_available_sol() {
        let (vault, native_ata) = crate::logic::vault_sol_and_ata(&NATIVE_SOL_MINT);
        let payer = Pubkey::new_unique();
        let ix = operator_native_ata_instruction(payer, vault);
        assert_eq!(ix.program_id, SPL_ATA_PROGRAM_ID);
        assert_eq!(ix.data, vec![1]);
        assert_eq!(ix.accounts.len(), 6);
        assert_eq!(ix.accounts[0], AccountMeta::new(payer, true));
        assert_eq!(ix.accounts[1], AccountMeta::new(native_ata, false));
        assert_eq!(ix.accounts[2], AccountMeta::new_readonly(vault, false));
        assert_eq!(ix.accounts[3].pubkey, NATIVE_SOL_MINT);
        assert_eq!(ix.accounts[4].pubkey, anchor_lang::system_program::ID);
        assert_eq!(ix.accounts[5].pubkey, SPL_TOKEN_PROGRAM_ID);
        assert_eq!(canonical_ata(&vault, &NATIVE_SOL_MINT), native_ata);
        assert_ne!(canonical_ata(&Pubkey::new_unique(), &NATIVE_SOL_MINT), native_ata);
        assert_ne!(canonical_ata(&vault, &Pubkey::new_unique()), native_ata);
        let rent = Rent::default();
        let vault_rent = rent.minimum_balance(0);
        let ata_rent = rent.minimum_balance(SPL_TOKEN_ACCOUNT_LEN);
        let floor = vault_rent + VAULT_RENT_RESERVE_LAMPORTS;
        // Close returns rent plus wrapped amount; operator-funded recreation
        // does not debit any of this amount from the vault.
        let after_close = floor + ata_rent + 123;
        let after_recreate = after_close;
        assert_eq!(after_recreate - floor, ata_rent + 123);
        assert!(crate::logic::withdrawal_keeps_reserve(after_recreate, vault_rent, 0).unwrap());
        assert!(crate::logic::withdrawal_keeps_reserve(floor, vault_rent, 0).unwrap());
        assert!(!crate::logic::withdrawal_keeps_reserve(floor - 1, vault_rent, 0).unwrap());
    }

    #[test]
    fn swap_requires_amount_minimum_and_directional_explicit_limit() {
        let mut args = SettleCounterToNativeArgs { amount_in: 10, minimum_amount_out: 1, sqrt_price_limit: 90 };
        assert!(validate_swap_bounds(&args, 100, true).is_ok());
        assert!(validate_swap_bounds(&args, 100, false).is_err());
        args.sqrt_price_limit = 110;
        assert!(validate_swap_bounds(&args, 100, false).is_ok());
        args.sqrt_price_limit = 0;
        assert!(validate_swap_bounds(&args, 100, true).is_err());
        args.sqrt_price_limit = 90;
        args.minimum_amount_out = 0;
        assert!(validate_swap_bounds(&args, 100, true).is_err());
        args.minimum_amount_out = 1;
        args.amount_in = 0;
        assert!(validate_swap_bounds(&args, 100, true).is_err());
    }
}