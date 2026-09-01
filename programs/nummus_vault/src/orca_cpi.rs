
use crate::constants::*;
use crate::errors::VaultError;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke_signed;

#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum OrcaOpKind {
    OpenPosition = 0,
    IncreaseLiquidity = 1,
    UpdateFeesAndRewards = 2,
    CollectFees = 3,
    DecreaseLiquidity = 4,
    ClosePosition = 5,
}

pub const DISC_OPEN_POSITION: [u8; 8] = [135, 128, 47, 77, 15, 152, 240, 49];
pub const DISC_INCREASE_LIQUIDITY: [u8; 8] = [46, 156, 243, 118, 13, 205, 251, 178];
pub const DISC_UPDATE_FEES_AND_REWARDS: [u8; 8] = [154, 230, 250, 13, 236, 209, 75, 223];
pub const DISC_COLLECT_FEES: [u8; 8] = [164, 152, 207, 99, 30, 186, 19, 182];
pub const DISC_DECREASE_LIQUIDITY: [u8; 8] = [160, 38, 208, 111, 104, 91, 44, 1];
pub const DISC_CLOSE_POSITION: [u8; 8] = [123, 134, 81, 0, 49, 68, 98, 98];

pub const ALLOWED_DISCRIMINATORS: [[u8; 8]; 6] = [
    DISC_OPEN_POSITION,
    DISC_INCREASE_LIQUIDITY,
    DISC_UPDATE_FEES_AND_REWARDS,
    DISC_COLLECT_FEES,
    DISC_DECREASE_LIQUIDITY,
    DISC_CLOSE_POSITION,
];

pub fn is_allowed_discriminator(data: &[u8]) -> bool {
    if data.len() < 8 {
        return false;
    }
    let mut head = [0u8; 8];
    head.copy_from_slice(&data[..8]);
    ALLOWED_DISCRIMINATORS.contains(&head)
}


fn read_pubkey(data: &[u8], offset: usize) -> Result<Pubkey> {
    let end = offset
        .checked_add(32)
        .ok_or(error!(VaultError::OrcaAccountConstraintViolation))?;
    require!(
        data.len() >= end,
        VaultError::OrcaAccountConstraintViolation
    );
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&data[offset..end]);
    Ok(Pubkey::new_from_array(buf))
}

fn read_i32(data: &[u8], offset: usize) -> Result<i32> {
    let end = offset
        .checked_add(4)
        .ok_or(error!(VaultError::OrcaAccountConstraintViolation))?;
    require!(
        data.len() >= end,
        VaultError::OrcaAccountConstraintViolation
    );
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&data[offset..end]);
    Ok(i32::from_le_bytes(buf))
}

pub fn require_spl_token_account(
    acc: &AccountInfo,
    expected_owner: &Pubkey,
    expected_mint: &Pubkey,
) -> Result<()> {
    require_keys_eq!(
        *acc.owner,
        SPL_TOKEN_PROGRAM_ID,
        VaultError::InvalidVaultTokenAccount
    );
    let data = acc.try_borrow_data()?;
    require!(
        data.len() == SPL_TOKEN_ACCOUNT_LEN,
        VaultError::InvalidVaultTokenAccount
    );
    let mint = read_pubkey(&data, SPL_TOKEN_MINT_OFFSET)?;
    let owner = read_pubkey(&data, SPL_TOKEN_OWNER_OFFSET)?;
    require_keys_eq!(mint, *expected_mint, VaultError::InvalidMint);
    require_keys_eq!(owner, *expected_owner, VaultError::InvalidVaultTokenAccount);
    Ok(())
}

pub fn read_and_check_whirlpool(
    whirlpool: &AccountInfo,
    expected_mint_a: &Pubkey,
    expected_mint_b: &Pubkey,
) -> Result<(Pubkey, Pubkey)> {
    require_keys_eq!(
        *whirlpool.owner,
        ORCA_WHIRLPOOL_PROGRAM_ID,
        VaultError::InvalidWhirlpool
    );
    let data = whirlpool.try_borrow_data()?;
    let mint_a = read_pubkey(&data, WHIRLPOOL_TOKEN_MINT_A_OFFSET)?;
    let vault_a = read_pubkey(&data, WHIRLPOOL_TOKEN_VAULT_A_OFFSET)?;
    let mint_b = read_pubkey(&data, WHIRLPOOL_TOKEN_MINT_B_OFFSET)?;
    let vault_b = read_pubkey(&data, WHIRLPOOL_TOKEN_VAULT_B_OFFSET)?;
    require_keys_eq!(mint_a, *expected_mint_a, VaultError::InvalidMint);
    require_keys_eq!(mint_b, *expected_mint_b, VaultError::InvalidMint);
    Ok((vault_a, vault_b))
}

pub fn read_and_check_position(
    position: &AccountInfo,
    expected_whirlpool: &Pubkey,
    expected_mint: &Pubkey,
) -> Result<(i32, i32)> {
    require_keys_eq!(
        *position.owner,
        ORCA_WHIRLPOOL_PROGRAM_ID,
        VaultError::InvalidPosition
    );
    let data = position.try_borrow_data()?;
    let pool = read_pubkey(&data, POSITION_WHIRLPOOL_OFFSET)?;
    let mint = read_pubkey(&data, POSITION_MINT_OFFSET)?;
    require_keys_eq!(pool, *expected_whirlpool, VaultError::InvalidPosition);
    require_keys_eq!(mint, *expected_mint, VaultError::InvalidPosition);
    let tick_lower = read_i32(&data, POSITION_TICK_LOWER_OFFSET)?;
    let tick_upper = read_i32(&data, POSITION_TICK_UPPER_OFFSET)?;
    Ok((tick_lower, tick_upper))
}

pub fn require_tick_array(
    tick_array: &AccountInfo,
    expected_whirlpool: &Pubkey,
    covered_tick: i32,
    tick_spacing: u16,
) -> Result<()> {
    require_keys_eq!(
        *tick_array.owner,
        ORCA_WHIRLPOOL_PROGRAM_ID,
        VaultError::InvalidTickArray
    );
    let data = tick_array.try_borrow_data()?;
    let pool = read_pubkey(&data, TICK_ARRAY_WHIRLPOOL_OFFSET)?;
    require_keys_eq!(pool, *expected_whirlpool, VaultError::InvalidTickArray);
    let start = read_i32(&data, TICK_ARRAY_START_TICK_OFFSET)?;
    let expected_start = tick_array_start_index(covered_tick, tick_spacing)?;
    require!(start == expected_start, VaultError::InvalidTickArray);
    Ok(())
}

pub fn tick_array_start_index(tick: i32, tick_spacing: u16) -> Result<i32> {
    require!(tick_spacing > 0, VaultError::InvalidTickArray);
    let ticks_in_array = TICK_ARRAY_SIZE
        .checked_mul(tick_spacing as i32)
        .ok_or(error!(VaultError::InvalidTickArray))?;
    let mut real = tick / ticks_in_array;
    if tick % ticks_in_array != 0 && tick < 0 {
        real -= 1;
    }
    real.checked_mul(ticks_in_array)
        .ok_or(error!(VaultError::InvalidTickArray))
}

pub fn read_whirlpool_tick_spacing(whirlpool: &AccountInfo) -> Result<u16> {
    require_keys_eq!(
        *whirlpool.owner,
        ORCA_WHIRLPOOL_PROGRAM_ID,
        VaultError::InvalidWhirlpool
    );
    let data = whirlpool.try_borrow_data()?;
    let off = 8 + 32 + 1;
    let end = off + 2;
    require!(data.len() >= end, VaultError::InvalidWhirlpool);
    Ok(u16::from_le_bytes([data[off], data[off + 1]]))
}

pub struct OrcaCpiAccount<'info> {
    pub info: AccountInfo<'info>,
    pub is_signer: bool,
    pub is_writable: bool,
}

pub fn invoke_orca<'info>(
    whirlpool_program: &AccountInfo<'info>,
    accounts: &[OrcaCpiAccount<'info>],
    ix_data: &[u8],
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    require_keys_eq!(
        *whirlpool_program.key,
        ORCA_WHIRLPOOL_PROGRAM_ID,
        VaultError::InvalidWhirlpoolProgram
    );

    require!(
        is_allowed_discriminator(ix_data),
        VaultError::DisallowedOrcaInstruction
    );

    let metas: Vec<AccountMeta> = accounts
        .iter()
        .map(|a| {
            if a.is_writable {
                AccountMeta::new(*a.info.key, a.is_signer)
            } else {
                AccountMeta::new_readonly(*a.info.key, a.is_signer)
            }
        })
        .collect();

    let mut infos: Vec<AccountInfo<'info>> =
        accounts.iter().map(|a| a.info.clone()).collect();
    infos.push(whirlpool_program.clone());

    let ix = Instruction {
        program_id: *whirlpool_program.key,
        accounts: metas,
        data: ix_data.to_vec(),
    };

    invoke_signed(&ix, &infos, signer_seeds).map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    mod sha256 {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
            0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
            0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
            0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
            0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
            0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
            0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
            0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];

        pub fn digest(msg: &[u8]) -> [u8; 32] {
            let mut h: [u32; 8] = [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f,
                0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ];
            let ml = (msg.len() as u64) * 8;
            let mut data = msg.to_vec();
            data.push(0x80);
            while data.len() % 64 != 56 {
                data.push(0);
            }
            data.extend_from_slice(&ml.to_be_bytes());

            for chunk in data.chunks(64) {
                let mut w = [0u32; 64];
                for i in 0..16 {
                    w[i] = u32::from_be_bytes([
                        chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3],
                    ]);
                }
                for i in 16..64 {
                    let s0 = w[i - 15].rotate_right(7)
                        ^ w[i - 15].rotate_right(18)
                        ^ (w[i - 15] >> 3);
                    let s1 = w[i - 2].rotate_right(17)
                        ^ w[i - 2].rotate_right(19)
                        ^ (w[i - 2] >> 10);
                    w[i] = w[i - 16]
                        .wrapping_add(s0)
                        .wrapping_add(w[i - 7])
                        .wrapping_add(s1);
                }
                let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
                    (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
                for i in 0..64 {
                    let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                    let ch = (e & f) ^ ((!e) & g);
                    let t1 = hh
                        .wrapping_add(s1)
                        .wrapping_add(ch)
                        .wrapping_add(K[i])
                        .wrapping_add(w[i]);
                    let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                    let maj = (a & b) ^ (a & c) ^ (b & c);
                    let t2 = s0.wrapping_add(maj);
                    hh = g;
                    g = f;
                    f = e;
                    e = d.wrapping_add(t1);
                    d = c;
                    c = b;
                    b = a;
                    a = t1.wrapping_add(t2);
                }
                h[0] = h[0].wrapping_add(a);
                h[1] = h[1].wrapping_add(b);
                h[2] = h[2].wrapping_add(c);
                h[3] = h[3].wrapping_add(d);
                h[4] = h[4].wrapping_add(e);
                h[5] = h[5].wrapping_add(f);
                h[6] = h[6].wrapping_add(g);
                h[7] = h[7].wrapping_add(hh);
            }
            let mut out = [0u8; 32];
            for i in 0..8 {
                out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
            }
            out
        }
    }

    fn anchor_disc(name: &str) -> [u8; 8] {
        let full = sha256::digest(format!("global:{name}").as_bytes());
        let mut d = [0u8; 8];
        d.copy_from_slice(&full[..8]);
        d
    }

    #[test]
    fn sha256_known_answer() {
        assert_eq!(
            sha256::digest(b""),
            [
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8,
                0x99, 0x6f, 0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c,
                0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55,
            ]
        );
    }

    #[test]
    fn discriminators_match_anchor_namespace() {
        assert_eq!(anchor_disc("open_position"), DISC_OPEN_POSITION);
        assert_eq!(anchor_disc("increase_liquidity"), DISC_INCREASE_LIQUIDITY);
        assert_eq!(
            anchor_disc("update_fees_and_rewards"),
            DISC_UPDATE_FEES_AND_REWARDS
        );
        assert_eq!(anchor_disc("collect_fees"), DISC_COLLECT_FEES);
        assert_eq!(anchor_disc("decrease_liquidity"), DISC_DECREASE_LIQUIDITY);
        assert_eq!(anchor_disc("close_position"), DISC_CLOSE_POSITION);
    }

    #[test]
    fn tick_array_start_index_matches_orca_floor_division() {
        assert_eq!(tick_array_start_index(0, 64).unwrap(), 0);
        assert_eq!(tick_array_start_index(5631, 64).unwrap(), 0);
        assert_eq!(tick_array_start_index(5632, 64).unwrap(), 5632);
        assert_eq!(tick_array_start_index(-1, 64).unwrap(), -5632);
        assert_eq!(tick_array_start_index(-5632, 64).unwrap(), -5632);
        assert_eq!(tick_array_start_index(-5633, 64).unwrap(), -11264);
        assert!(tick_array_start_index(0, 0).is_err());
    }

    #[test]
    fn allowlist_rejects_unknown_and_short() {
        assert!(!is_allowed_discriminator(&[]));
        assert!(!is_allowed_discriminator(&[1, 2, 3]));
        assert!(!is_allowed_discriminator(&[9, 9, 9, 9, 9, 9, 9, 9]));
        assert!(is_allowed_discriminator(&DISC_OPEN_POSITION));
        let mut d = DISC_INCREASE_LIQUIDITY.to_vec();
        d.extend_from_slice(&[0xAA; 16]);
        assert!(is_allowed_discriminator(&d));
    }
}
