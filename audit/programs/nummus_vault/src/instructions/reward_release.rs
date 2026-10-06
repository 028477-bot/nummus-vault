//! Root-authorized recovery of collected third-token rewards.
//! SOL and the configured trading pair cannot leave through this instruction.
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};
use crate::{constants::*, errors::VaultError, orca_cpi::*, state::Config};
use super::liquidity::canonical_ata;

#[derive(Accounts)]
pub struct ReleaseReward<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch)]
    pub config: Account<'info, Config>,
    #[account(seeds = [VAULT_SOL_SEED], bump = config.vault_sol_bump)]
    pub vault_sol: UncheckedAccount<'info>,
    #[account(address = config.admin @ VaultError::UnauthorizedAdmin)]
    pub admin: Signer<'info>,
    #[account(address = config.whirlpool @ VaultError::InvalidWhirlpool)]
    pub whirlpool: UncheckedAccount<'info>,
    pub reward_mint: UncheckedAccount<'info>,
    #[account(mut)]
    pub source: UncheckedAccount<'info>,
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,
    #[account(address = SPL_TOKEN_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub token_program: UncheckedAccount<'info>,
}

#[event]
pub struct RewardReleased {
    pub mint: Pubkey,
    pub destination: Pubkey,
    pub amount: u64,
    pub admin: Pubkey,
}

pub fn release_reward_handler(
    ctx: Context<ReleaseReward>, reward_index: u8, amount: u64,
) -> Result<()> {
    require!(ctx.remaining_accounts.is_empty(), VaultError::OrcaAccountConstraintViolation);
    crate::logic::validate_amount(amount)?;
    let a = ctx.accounts;
    let c = &a.config;
    read_and_check_whirlpool(&a.whirlpool.to_account_info(), &c.token_mint_a, &c.token_mint_b)?;
    let (mint, _) = read_reward(&a.whirlpool.to_account_info(), reward_index)?;
    require!(mint != c.token_mint_a && mint != c.token_mint_b && mint != NATIVE_SOL_MINT,
        VaultError::InvalidMint);
    require_keys_eq!(a.reward_mint.key(), mint, VaultError::InvalidMint);
    require_keys_eq!(*a.reward_mint.owner, SPL_TOKEN_PROGRAM_ID, VaultError::InvalidMint);
    let decimals = {
        let data = a.reward_mint.try_borrow_data()?;
        require!(data.len() == 82 && data[45] == 1, VaultError::InvalidMint);
        data[44]
    };
    require_keys_eq!(a.source.key(), canonical_ata(a.vault_sol.key, &mint),
        VaultError::InvalidAssociatedTokenAccount);
    // Fixed root-controlled destination, never a caller-selected wallet.
    require_keys_eq!(a.destination.key(), canonical_ata(&c.admin, &mint),
        VaultError::WithdrawalDestinationMismatch);
    require_spl_token_account(&a.source.to_account_info(), a.vault_sol.key, &mint)?;
    require_spl_token_account(&a.destination.to_account_info(), &c.admin, &mint)?;
    let mut data = vec![12]; // SPL Token TransferChecked.
    data.extend_from_slice(&amount.to_le_bytes());
    data.push(decimals);
    let instruction = Instruction {
        program_id: SPL_TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(a.source.key(), false),
            AccountMeta::new_readonly(mint, false),
            AccountMeta::new(a.destination.key(), false),
            AccountMeta::new_readonly(a.vault_sol.key(), true),
        ],
        data,
    };
    invoke_signed(&instruction, &[
        a.source.to_account_info(), a.reward_mint.to_account_info(),
        a.destination.to_account_info(), a.vault_sol.to_account_info(),
        a.token_program.to_account_info(),
    ], &[&[VAULT_SOL_SEED, &[c.vault_sol_bump]]])?;
    emit!(RewardReleased { mint, destination: a.destination.key(), amount, admin: c.admin });
    Ok(())
}