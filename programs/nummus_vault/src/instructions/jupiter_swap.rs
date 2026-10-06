//! Server-directed Jupiter execution, independent of the Orca LP pool.
//! Wire layouts: https://github.com/jup-ag/jupiter-cpi/blob/main/idl.json
//! Supports V1 `route` and `shared_accounts_route` (ExactIn), not token ledgers,
//! exact-out, Ultra orders or arbitrary instructions. Reject unknown layouts.
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};
use crate::{constants::*, errors::VaultError, orca_cpi::require_spl_token_account, state::Config};
use super::liquidity::{canonical_ata, prepare_native_sol, reclaim_native_sol};

pub const JUPITER_PROGRAM: Pubkey = Pubkey::new_from_array([
    4,121,213,91,242,49,192,110,238,116,197,110,206,104,21,7,
    253,177,178,222,163,244,142,81,2,177,205,162,86,188,19,143,
]);
pub const USDC_MINT: Pubkey = Pubkey::new_from_array([
    198,250,122,243,190,219,173,58,61,101,243,106,171,201,116,49,
    177,187,228,194,210,246,224,228,124,166,2,3,69,47,93,97,
]);
const ROUTE: [u8; 8] = [229,23,203,151,122,227,173,42];
const SHARED_ROUTE: [u8; 8] = [193,32,155,51,65,214,156,129];

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct JupiterSwapArgs {
    pub sol_to_usdc: bool,
    pub amount_in: u64,
    pub minimum_amount_out: u64,
    pub swap_data: Vec<u8>,
}

#[derive(Accounts)]
pub struct ExecuteJupiterSwap<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch)]
    pub config: Account<'info, Config>,
    /// CHECK: Canonical system-owned SOL vault; never writable in Jupiter CPI.
    #[account(mut, seeds = [VAULT_SOL_SEED], bump = config.vault_sol_bump,
        owner = anchor_lang::system_program::ID)]
    pub vault_sol: UncheckedAccount<'info>,
    #[account(address = config.vault_authority @ VaultError::UnauthorizedVaultAuthority)]
    pub vault_authority: Signer<'info>,
    /// CHECK: Pinned executable Jupiter aggregator.
    #[account(address = JUPITER_PROGRAM @ VaultError::InvalidJupiterInstruction, executable)]
    pub jupiter_program: UncheckedAccount<'info>,
    /// CHECK: Canonical ATA, initialized and validated before/after CPI.
    #[account(mut, address = canonical_ata(vault_sol.key, &NATIVE_SOL_MINT)
        @ VaultError::InvalidAssociatedTokenAccount)]
    pub native_ata: UncheckedAccount<'info>,
    /// CHECK: Canonical ATA, initialized and validated before/after CPI.
    #[account(mut, address = canonical_ata(vault_sol.key, &USDC_MINT)
        @ VaultError::InvalidAssociatedTokenAccount)]
    pub usdc_ata: UncheckedAccount<'info>,
    /// CHECK: Pinned native mint.
    #[account(address = NATIVE_SOL_MINT @ VaultError::InvalidMint)]
    pub native_mint: UncheckedAccount<'info>,
    /// CHECK: Pinned mainnet USDC mint.
    #[account(address = USDC_MINT @ VaultError::InvalidMint)]
    pub usdc_mint: UncheckedAccount<'info>,
    /// CHECK: Pinned legacy SPL Token program.
    #[account(address = SPL_TOKEN_PROGRAM_ID @ VaultError::InvalidJupiterAccounts)]
    pub token_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    /// CHECK: Pinned ATA program.
    #[account(address = SPL_ATA_PROGRAM_ID @ VaultError::InvalidJupiterAccounts)]
    pub associated_token_program: UncheckedAccount<'info>,
}

fn require_key(accounts: &[AccountInfo], index: usize, key: &Pubkey) -> Result<()> {
    require!(accounts.get(index).map(|a| a.key == key).unwrap_or(false),
        VaultError::InvalidJupiterAccounts);
    Ok(())
}

fn validate_route(
    accounts: &[AccountInfo], data: &[u8], vault: &Pubkey,
    source: &Pubkey, destination: &Pubkey, input_mint: &Pubkey, output_mint: &Pubkey,
    amount_in: u64,
) -> Result<()> {
    require!(data.len() >= 31, VaultError::InvalidJupiterInstruction);
    let event_authority = Pubkey::find_program_address(&[b"__event_authority"], &JUPITER_PROGRAM).0;
    require_key(accounts, 0, &SPL_TOKEN_PROGRAM_ID)?;
    if data[..8] == ROUTE {
        require!(accounts.len() >= 9, VaultError::InvalidJupiterAccounts);
        require_key(accounts, 1, vault)?;
        require_key(accounts, 2, source)?;
        require_key(accounts, 3, destination)?;
        // Optional destination must be absent or the same vault destination.
        require!(accounts[4].key == &JUPITER_PROGRAM || accounts[4].key == destination,
            VaultError::InvalidJupiterAccounts);
        require_key(accounts, 5, output_mint)?;
        require_key(accounts, 6, &JUPITER_PROGRAM)?; // No platform-fee account.
        require_key(accounts, 7, &event_authority)?;
        require_key(accounts, 8, &JUPITER_PROGRAM)?;
    } else if data[..8] == SHARED_ROUTE {
        require!(accounts.len() >= 13 && data.len() >= 32, VaultError::InvalidJupiterAccounts);
        let authority = Pubkey::find_program_address(&[b"authority", &[data[8]]], &JUPITER_PROGRAM).0;
        require_key(accounts, 1, &authority)?;
        require_key(accounts, 2, vault)?;
        require_key(accounts, 3, source)?;
        require_key(accounts, 6, destination)?;
        require_key(accounts, 7, input_mint)?;
        require_key(accounts, 8, output_mint)?;
        require_key(accounts, 9, &JUPITER_PROGRAM)?; // No platform fee.
        require_key(accounts, 11, &event_authority)?;
        require_key(accounts, 12, &JUPITER_PROGRAM)?;
        // Jupiter validates its intermediate accounts and optional Token-2022 program.
    } else {
        return err!(VaultError::InvalidJupiterInstruction);
    }
    // Both supported layouts end with inAmount/u64, quotedOut/u64,
    // slippageBps/u16, platformFeeBps/u8. Jupiter decodes the variable route plan.
    let tail = &data[data.len() - 19..];
    require!(u64::from_le_bytes(tail[..8].try_into().unwrap()) == amount_in && tail[18] == 0,
        VaultError::InvalidExecutionBounds);
    Ok(())
}

fn token_snapshot(account: &AccountInfo, owner: &Pubkey, mint: &Pubkey) -> Result<Vec<u8>> {
    require_spl_token_account(account, owner, mint)?;
    let data = account.try_borrow_data()?;
    require!(data[108] == 1 && data[72..76] == [0; 4] && data[129..133] == [0; 4],
        VaultError::InvalidJupiterAccounts); // Initialized; no delegate/close authority.
    Ok(data.to_vec())
}

fn token_amount(data: &[u8]) -> u64 {
    u64::from_le_bytes(data[64..72].try_into().unwrap())
}

fn unchanged_authorities(before: &[u8], after: &[u8]) -> Result<()> {
    require!(before[..64] == after[..64] && before[72..] == after[72..],
        VaultError::InvalidJupiterAccounts);
    Ok(())
}

pub fn execute_handler<'info>(
    ctx: Context<'_, '_, '_, 'info, ExecuteJupiterSwap<'info>>, args: JupiterSwapArgs,
) -> Result<()> {
    let a = ctx.accounts;
    let c = &a.config;
    require!(!c.liquidity_paused, VaultError::LiquidityPaused);
    require!(args.amount_in > 0 && args.amount_in != u64::MAX && args.minimum_amount_out > 0,
        VaultError::InvalidExecutionBounds);
    let (source, destination, input_mint, output_mint) = if args.sol_to_usdc {
        (&a.native_ata, &a.usdc_ata, NATIVE_SOL_MINT, USDC_MINT)
    } else {
        (&a.usdc_ata, &a.native_ata, USDC_MINT, NATIVE_SOL_MINT)
    };
    validate_route(ctx.remaining_accounts, &args.swap_data, a.vault_sol.key,
        source.key, destination.key, &input_mint, &output_mint, args.amount_in)?;

    let bump = [c.vault_sol_bump];
    let seeds: &[&[u8]] = &[VAULT_SOL_SEED, &bump];
    // Create the canonical USDC ATA if needed, entirely within this instruction.
    let create_usdc = Instruction {
        program_id: SPL_ATA_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(a.vault_sol.key(), true), AccountMeta::new(a.usdc_ata.key(), false),
            AccountMeta::new_readonly(a.vault_sol.key(), false),
            AccountMeta::new_readonly(USDC_MINT, false),
            AccountMeta::new_readonly(a.system_program.key(), false),
            AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
        ],
        data: vec![1],
    };
    invoke_signed(&create_usdc, &[
        a.vault_sol.to_account_info(), a.usdc_ata.to_account_info(),
        a.usdc_mint.to_account_info(), a.system_program.to_account_info(),
        a.token_program.to_account_info(), a.associated_token_program.to_account_info(),
    ], &[seeds])?;
    require!(crate::logic::withdrawal_keeps_reserve(
        a.vault_sol.lamports(), Rent::get()?.minimum_balance(a.vault_sol.data_len()), 0
    )?, VaultError::RentReserveBreach);
    prepare_native_sol(c, &a.vault_sol.to_account_info(), &a.native_mint.to_account_info(),
        &a.native_ata.to_account_info(), &a.token_program.to_account_info(),
        &a.system_program.to_account_info(), &a.associated_token_program.to_account_info(),
        if args.sol_to_usdc { args.amount_in } else { 0 })?;

    let before_in = token_snapshot(source, a.vault_sol.key, &input_mint)?;
    let before_out = token_snapshot(destination, a.vault_sol.key, &output_mint)?;
    require!(token_amount(&before_in) >= args.amount_in, VaultError::InvalidExecutionBounds);
    let native_lamports = a.vault_sol.lamports();
    let mut metas = Vec::with_capacity(ctx.remaining_accounts.len());
    for account in ctx.remaining_accounts {
        // No external signer or vault program state is exposed to the router.
        require!(!account.is_signer && account.owner != &crate::ID
            && account.key != a.vault_authority.key, VaultError::InvalidJupiterAccounts);
        if account.key != source.key && account.key != destination.key {
            // No other token account owned by the vault, including Token-2022
            // accounts with the common SPL base layout, may enter the route.
            let data = account.try_borrow_data()?;
            require!(!(data.len() >= SPL_TOKEN_ACCOUNT_LEN
                && data[32..64] == a.vault_sol.key.to_bytes()),
                VaultError::InvalidJupiterAccounts);
        }
        let is_vault = account.key == a.vault_sol.key;
        // The PDA can authorize token spending, but Jupiter cannot debit native
        // SOL or use it as a payer. Apply this to EVERY duplicate occurrence.
        metas.push(AccountMeta {
            pubkey: *account.key,
            is_signer: is_vault,
            is_writable: !is_vault && account.is_writable,
        });
    }
    let mut infos = ctx.remaining_accounts.to_vec();
    infos.push(a.jupiter_program.to_account_info());
    invoke_signed(&Instruction {
        program_id: JUPITER_PROGRAM, accounts: metas, data: args.swap_data,
    }, &infos, &[seeds])?;

    let after_in = token_snapshot(source, a.vault_sol.key, &input_mint)?;
    let after_out = token_snapshot(destination, a.vault_sol.key, &output_mint)?;
    unchanged_authorities(&before_in, &after_in)?;
    unchanged_authorities(&before_out, &after_out)?;
    require!(a.vault_sol.lamports() == native_lamports, VaultError::InvalidJupiterAccounts);
    require!(token_amount(&before_in).checked_sub(token_amount(&after_in)) == Some(args.amount_in),
        VaultError::InvalidExecutionBounds);
    require!(token_amount(&after_out).checked_sub(token_amount(&before_out))
        .map_or(false, |n| n >= args.minimum_amount_out), VaultError::InvalidExecutionBounds);
    // Return SOL output (and any WSOL remainder/rent) to the vault, never an operator.
    reclaim_native_sol(c, &a.vault_sol.to_account_info(),
        &a.native_ata.to_account_info(), &a.token_program.to_account_info())
}
