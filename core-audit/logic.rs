
use crate::constants::{
    MAX_SLIPPAGE_BPS, MAX_TICK_RANGE_WIDTH, NATIVE_SOL_MINT, SPL_ATA_PROGRAM_ID,
    SPL_TOKEN_PROGRAM_ID, VAULT_RENT_RESERVE_LAMPORTS, VAULT_SOL_SEED,
};
use crate::errors::VaultError;
use anchor_lang::prelude::*;

#[inline]
pub fn checked_add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b).ok_or(error!(VaultError::MathOverflow))
}

#[inline]
pub fn checked_sub(a: u64, b: u64) -> Result<u64> {
    a.checked_sub(b).ok_or(error!(VaultError::MathOverflow))
}

#[inline]
pub fn validate_amount(amount: u64) -> Result<()> {
    require!(amount > 0, VaultError::ZeroAmount);
    Ok(())
}

pub fn validate_range(min_tick: i32, max_tick: i32) -> Result<()> {
    require!(min_tick < max_tick, VaultError::TickRangeOutOfBounds);
    require!(
        (max_tick as i64 - min_tick as i64) <= MAX_TICK_RANGE_WIDTH as i64,
        VaultError::TickRangeOutOfBounds
    );
    Ok(())
}

pub fn validate_requested_range(
    tick_lower: i32,
    tick_upper: i32,
    cfg_min: i32,
    cfg_max: i32,
) -> Result<()> {
    require!(tick_lower < tick_upper, VaultError::TickRangeOutOfBounds);
    require!(
        tick_lower >= cfg_min && tick_upper <= cfg_max,
        VaultError::TickRangeOutOfBounds
    );
    Ok(())
}

pub fn validate_slippage_ceiling(slippage_bps: u16) -> Result<()> {
    require!(slippage_bps <= MAX_SLIPPAGE_BPS, VaultError::SlippageTooHigh);
    Ok(())
}

pub fn validate_slippage_against_config(requested_bps: u16, configured_max_bps: u16) -> Result<()> {
    require!(
        requested_bps <= configured_max_bps,
        VaultError::SlippageTooHigh
    );
    Ok(())
}

pub fn withdrawal_keeps_reserve(
    vault_lamports: u64,
    rent_exempt_min: u64,
    amount: u64,
) -> Result<bool> {
    let floor = checked_add(rent_exempt_min, VAULT_RENT_RESERVE_LAMPORTS)?;
    let remaining = checked_sub(vault_lamports, amount)?;
    Ok(remaining >= floor)
}

pub fn vault_sol_and_ata(mint: &Pubkey) -> (Pubkey, Pubkey) {
    let (vault_sol, _) = Pubkey::find_program_address(&[VAULT_SOL_SEED], &crate::ID);
    let (ata, _) = Pubkey::find_program_address(
        &[vault_sol.as_ref(), SPL_TOKEN_PROGRAM_ID.as_ref(), mint.as_ref()],
        &SPL_ATA_PROGRAM_ID,
    );
    (vault_sol, ata)
}

pub fn validate_native_sol_config(
    mint_a: &Pubkey,
    mint_b: &Pubkey,
    token_account_a: &Pubkey,
    token_account_b: &Pubkey,
) -> Result<()> {
    require!(
        (*mint_a == NATIVE_SOL_MINT) ^ (*mint_b == NATIVE_SOL_MINT),
        VaultError::InvalidNativeSolMint
    );
    let (_, expected_a) = vault_sol_and_ata(mint_a);
    let (_, expected_b) = vault_sol_and_ata(mint_b);
    require_keys_eq!(
        *token_account_a,
        expected_a,
        VaultError::InvalidAssociatedTokenAccount
    );
    require_keys_eq!(
        *token_account_b,
        expected_b,
        VaultError::InvalidAssociatedTokenAccount
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_add_sub_overflow() {
        assert!(checked_add(u64::MAX, 1).is_err());
        assert_eq!(checked_add(10, 5).unwrap(), 15);
        assert!(checked_sub(0, 1).is_err());
        assert_eq!(checked_sub(10, 4).unwrap(), 6);
    }

    #[test]
    fn zero_amount_rejected() {
        assert!(validate_amount(0).is_err());
        assert!(validate_amount(1).is_ok());
    }

    #[test]
    fn range_bounds() {
        assert!(validate_range(-100, 100).is_ok());
        assert!(validate_range(100, 100).is_err());
        assert!(validate_range(100, -100).is_err());
        assert!(validate_range(i32::MIN, i32::MAX).is_err());
    }

    #[test]
    fn requested_range_inside_config() {
        assert!(validate_requested_range(-50, 50, -100, 100).is_ok());
        assert!(validate_requested_range(-150, 50, -100, 100).is_err());
        assert!(validate_requested_range(-50, 150, -100, 100).is_err());
        assert!(validate_requested_range(50, -50, -100, 100).is_err());
    }

    #[test]
    fn slippage_ceiling_and_config() {
        assert!(validate_slippage_ceiling(MAX_SLIPPAGE_BPS).is_ok());
        assert!(validate_slippage_ceiling(MAX_SLIPPAGE_BPS + 1).is_err());
        assert!(validate_slippage_against_config(100, 200).is_ok());
        assert!(validate_slippage_against_config(300, 200).is_err());
        assert!(validate_slippage_against_config(200, 200).is_ok());
    }

    #[test]
    fn rent_reserve_math() {
        let rent_min = 890_880u64;
        let floor = rent_min + VAULT_RENT_RESERVE_LAMPORTS;
        let vault = floor + 1_000_000;
        assert!(withdrawal_keeps_reserve(vault, rent_min, 1_000_000).unwrap());
        assert!(!withdrawal_keeps_reserve(vault, rent_min, 1_000_001).unwrap());
        assert!(withdrawal_keeps_reserve(vault, rent_min, vault + 1).is_err());
    }

    #[test]
    fn native_sol_config_requires_one_native_mint_and_canonical_atas() {
        let usdc = Pubkey::new_unique();
        let (_, native_ata) = vault_sol_and_ata(&NATIVE_SOL_MINT);
        let (_, usdc_ata) = vault_sol_and_ata(&usdc);
        assert!(validate_native_sol_config(
            &NATIVE_SOL_MINT,
            &usdc,
            &native_ata,
            &usdc_ata
        )
        .is_ok());
        assert!(validate_native_sol_config(&usdc, &usdc, &usdc_ata, &usdc_ata).is_err());
        assert!(validate_native_sol_config(
            &NATIVE_SOL_MINT,
            &usdc,
            &Pubkey::new_unique(),
            &usdc_ata
        )
        .is_err());
    }
}
