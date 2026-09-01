
use crate::constants::*;
use crate::errors::VaultError;
use crate::events::DepositCommitted;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, Transfer};

#[derive(Accounts)]
#[instruction(deposit_id: u64, amount: u64)]
pub struct Deposit<'info> {
    #[account(
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
        init_if_needed,
        payer = depositor,
        space = UserPosition::LEN,
        seeds = [USER_POSITION_SEED, depositor.key().as_ref()],
        bump,
    )]
    pub position: Account<'info, UserPosition>,

    #[account(
        init,
        payer = depositor,
        space = DepositReceipt::LEN,
        seeds = [DEPOSIT_RECEIPT_SEED, depositor.key().as_ref(), &deposit_id.to_le_bytes()],
        bump,
    )]
    pub deposit_receipt: Account<'info, DepositReceipt>,

    #[account(mut)]
    pub depositor: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn deposit_handler(ctx: Context<Deposit>, deposit_id: u64, amount: u64) -> Result<()> {
    require!(
        !ctx.accounts.config.deposits_paused,
        VaultError::DepositsPaused
    );
    require!(amount > 0, VaultError::ZeroAmount);

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.depositor.to_account_info(),
                to: ctx.accounts.vault_sol.to_account_info(),
            },
        ),
        amount,
    )?;

    let clock = Clock::get()?;

    let position = &mut ctx.accounts.position;
    if position.owner == Pubkey::default() {
        position.version = USER_POSITION_VERSION;
        position.bump = ctx.bumps.position;
        position.owner = ctx.accounts.depositor.key();
        position.balance_lamports = 0;
        position.deposit_count = 0;
        position.withdrawal_count = 0;
        position.created_at = clock.unix_timestamp;
        position.total_withdrawn_lamports = 0;
        position.reserved = [0u8; 24];
    } else {
        require_keys_eq!(
            position.owner,
            ctx.accounts.depositor.key(),
            VaultError::PositionOwnerMismatch
        );
        require!(
            position.version == USER_POSITION_VERSION,
            VaultError::AccountVersionMismatch
        );
    }

    position.balance_lamports = position
        .balance_lamports
        .checked_add(amount)
        .ok_or(VaultError::MathOverflow)?;
    position.deposit_count = position
        .deposit_count
        .checked_add(1)
        .ok_or(VaultError::MathOverflow)?;

    let receipt = &mut ctx.accounts.deposit_receipt;
    receipt.version = DEPOSIT_RECEIPT_VERSION;
    receipt.bump = ctx.bumps.deposit_receipt;
    receipt.owner = ctx.accounts.depositor.key();
    receipt.deposit_id = deposit_id;
    receipt.amount = amount;
    receipt.timestamp = clock.unix_timestamp;

    let config = &mut ctx.accounts.config;
    config.total_deposits = config
        .total_deposits
        .checked_add(amount)
        .ok_or(VaultError::MathOverflow)?;

    emit!(DepositCommitted {
        owner: receipt.owner,
        deposit_id,
        amount,
        position_balance_after: position.balance_lamports,
        receipt: receipt.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}
