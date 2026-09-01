
use anchor_lang::prelude::*;


pub const CONFIG_SEED: &[u8] = b"config";

pub const VAULT_SOL_SEED: &[u8] = b"vault_sol";

pub const USER_POSITION_SEED: &[u8] = b"user_position";

pub const DEPOSIT_RECEIPT_SEED: &[u8] = b"deposit_receipt";

pub const WITHDRAWAL_RECEIPT_SEED: &[u8] = b"withdrawal_receipt";

pub const POSITION_MINT_SEED: &[u8] = b"position_mint";

pub const ORCA_POSITION_SEED: &[u8] = b"position";


pub const CONFIG_VERSION: u8 = 2;
pub const VAULT_VERSION: u8 = 1;
pub const USER_POSITION_VERSION: u8 = 2;
pub const DEPOSIT_RECEIPT_VERSION: u8 = 1;
pub const WITHDRAWAL_RECEIPT_VERSION: u8 = 1;

pub const VAULT_RENT_RESERVE_LAMPORTS: u64 = 5_000_000;

pub const MAX_SLIPPAGE_BPS: u16 = 5_000;

pub const MAX_TICK_RANGE_WIDTH: i32 = 887_272 * 2;


pub const ORCA_WHIRLPOOL_PROGRAM_ID: Pubkey = Pubkey::new_from_array([
    14, 3, 104, 95, 142, 144, 144, 83, 228, 88, 18, 28, 102, 245, 167, 106, 237, 199, 112, 106,
    161, 28, 130, 248, 170, 149, 42, 143, 43, 120, 121, 169,
]);

pub const SPL_TOKEN_PROGRAM_ID: Pubkey = Pubkey::new_from_array([
    6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172, 28, 180, 133,
    237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
]);

pub const SPL_ATA_PROGRAM_ID: Pubkey = Pubkey::new_from_array([
    140, 151, 37, 143, 78, 36, 137, 241, 187, 61, 16, 41, 20, 142, 13, 131, 11, 90, 19, 153, 218,
    255, 16, 132, 4, 142, 123, 216, 219, 233, 248, 89,
]);

pub const NATIVE_SOL_MINT: Pubkey = Pubkey::new_from_array([
    6, 155, 136, 87, 254, 171, 129, 132, 251, 104, 127, 99, 70, 24, 192, 53, 218, 196, 57,
    220, 26, 235, 59, 85, 152, 160, 240, 0, 0, 0, 0, 1,
]);

pub const SPL_TOKEN_ACCOUNT_LEN: usize = 165;
pub const SPL_TOKEN_MINT_OFFSET: usize = 0;
pub const SPL_TOKEN_OWNER_OFFSET: usize = 32;
pub const SPL_TOKEN_AMOUNT_OFFSET: usize = 64;

pub const WHIRLPOOL_TOKEN_MINT_A_OFFSET: usize = 8 + 101;
pub const WHIRLPOOL_TOKEN_VAULT_A_OFFSET: usize = 8 + 133;
pub const WHIRLPOOL_TOKEN_MINT_B_OFFSET: usize = 8 + 181;
pub const WHIRLPOOL_TOKEN_VAULT_B_OFFSET: usize = 8 + 213;

pub const POSITION_WHIRLPOOL_OFFSET: usize = 8;
pub const POSITION_MINT_OFFSET: usize = 8 + 32;
pub const POSITION_TICK_LOWER_OFFSET: usize = 8 + 32 + 32 + 16;
pub const POSITION_TICK_UPPER_OFFSET: usize = 8 + 32 + 32 + 16 + 4;

pub const TICK_ARRAY_START_TICK_OFFSET: usize = 8;
pub const TICK_ARRAY_WHIRLPOOL_OFFSET: usize = 8 + 4 + 88 * 113;
pub const TICK_ARRAY_SIZE: i32 = 88;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orca_program_id_base58_roundtrip() {
        assert_eq!(
            ORCA_WHIRLPOOL_PROGRAM_ID.to_string(),
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc"
        );
    }

    #[test]
    fn spl_program_ids_base58_roundtrip() {
        assert_eq!(
            SPL_TOKEN_PROGRAM_ID.to_string(),
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        );
        assert_eq!(
            SPL_ATA_PROGRAM_ID.to_string(),
            "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        );
        assert_eq!(
            NATIVE_SOL_MINT.to_string(),
            "So11111111111111111111111111111111111111112"
        );
    }

    #[test]
    fn position_tick_offsets_follow_the_u128_liquidity_field() {
        let mut position = vec![0u8; 96];
        let lower = -12_345i32;
        let upper = 54_321i32;
        position[POSITION_TICK_LOWER_OFFSET..POSITION_TICK_LOWER_OFFSET + 4]
            .copy_from_slice(&lower.to_le_bytes());
        position[POSITION_TICK_UPPER_OFFSET..POSITION_TICK_UPPER_OFFSET + 4]
            .copy_from_slice(&upper.to_le_bytes());
        assert_eq!(POSITION_TICK_LOWER_OFFSET, 88);
        assert_eq!(POSITION_TICK_UPPER_OFFSET, 92);
        assert_eq!(
            i32::from_le_bytes(position[88..92].try_into().unwrap()),
            lower
        );
        assert_eq!(
            i32::from_le_bytes(position[92..96].try_into().unwrap()),
            upper
        );
        assert_ne!(
            i32::from_le_bytes(position[80..84].try_into().unwrap()),
            lower
        );
    }

}
