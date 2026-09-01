use crate::constants::*;
use crate::errors::VaultError;
use crate::events::WithdrawalSettled;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, Transfer};

#[derive(Accounts)]
#[instruction(request_id: u64, amount: u64)]
pub struct Withdraw<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch,
    )]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [VAULT_SOL_SEED],
        bump = config.vault_sol_bump,
    )]
    pub vault_sol: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [USER_POSITION_SEED, position.owner.as_ref()],
        bump = position.bump,
        constraint = position.version == USER_POSITION_VERSION @ VaultError::AccountVersionMismatch,
    )]
    pub position: Account<'info, UserPosition>,

    #[account(
        init,
        payer = payer,
        space = WithdrawalReceipt::LEN,
        seeds = [WITHDRAWAL_RECEIPT_SEED, &request_id.to_le_bytes()],
        bump,
    )]
    pub withdrawal_receipt: Account<'info, WithdrawalReceipt>,

    #[account(
        mut,
        address = position.owner @ VaultError::WithdrawalDestinationMismatch
    )]
    pub destination: UncheckedAccount<'info>,

    #[account(
        constraint = vault_authority.key() == config.vault_authority @ VaultError::UnauthorizedVaultAuthority
    )]
    pub vault_authority: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn withdraw_handler(ctx: Context<Withdraw>, request_id: u64, amount: u64) -> Result<()> {
    crate::logic::validate_amount(amount)?;

    let position = &mut ctx.accounts.position;
    require!(
        amount <= position.balance_lamports,
        VaultError::InsufficientPositionBalance
    );
    let vault_ai = ctx.accounts.vault_sol.to_account_info();
    let rent = Rent::get()?;
    let rent_exempt_min = rent.minimum_balance(vault_ai.data_len());
    let vault_lamports = vault_ai.lamports();
    require!(
        crate::logic::withdrawal_keeps_reserve(vault_lamports, rent_exempt_min, amount)?,
        VaultError::RentReserveBreach
    );

    let vault_bump = [ctx.accounts.config.vault_sol_bump];
    let signer_seeds: &[&[&[u8]]] = &[&[VAULT_SOL_SEED, &vault_bump]];
    system_program::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault_sol.to_account_info(),
                to: ctx.accounts.destination.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )?;

    position.balance_lamports = position
        .balance_lamports
        .checked_sub(amount)
        .ok_or(VaultError::MathOverflow)?;
    position.total_withdrawn_lamports = position
        .total_withdrawn_lamports
        .checked_add(amount)
        .ok_or(VaultError::MathOverflow)?;
    position.withdrawal_count = position
        .withdrawal_count
        .checked_add(1)
        .ok_or(VaultError::MathOverflow)?;

    let clock = Clock::get()?;
    let receipt = &mut ctx.accounts.withdrawal_receipt;
    receipt.version = WITHDRAWAL_RECEIPT_VERSION;
    receipt.bump = ctx.bumps.withdrawal_receipt;
    receipt.owner = position.owner;
    receipt.request_id = request_id;
    receipt.amount = amount;
    receipt.destination = ctx.accounts.destination.key();
    receipt.timestamp = clock.unix_timestamp;

    let config = &mut ctx.accounts.config;
    config.total_withdrawals = config
        .total_withdrawals
        .checked_add(amount)
        .ok_or(VaultError::MathOverflow)?;

    emit!(WithdrawalSettled {
        owner: receipt.owner,
        request_id,
        amount,
        destination: receipt.destination,
        position_balance_after: position.balance_lamports,
        receipt: receipt.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}
