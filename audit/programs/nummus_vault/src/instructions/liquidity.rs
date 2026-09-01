
use crate::constants::*;
use crate::errors::VaultError;
use crate::events::LiquidityOperation;
use crate::orca_cpi::*;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
    sysvar,
};
use anchor_lang::system_program::{self, Transfer};


fn check_liquidity_common(config: &Config, slippage_bps: u16) -> Result<()> {
    require!(!config.liquidity_paused, VaultError::LiquidityPaused);
    require!(
        slippage_bps <= config.max_slippage_bps,
        VaultError::SlippageTooHigh
    );
    Ok(())
}

fn canonical_ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), SPL_TOKEN_PROGRAM_ID.as_ref(), mint.as_ref()],
        &SPL_ATA_PROGRAM_ID,
    )
    .0
}

fn prepare_native_sol<'info>(
    config: &Config,
    vault_sol: &AccountInfo<'info>,
    native_mint: &AccountInfo<'info>,
    native_ata: &AccountInfo<'info>,
    token_program: &AccountInfo<'info>,
    system_program_info: &AccountInfo<'info>,
    associated_token_program: &AccountInfo<'info>,
    required_lamports: u64,
) -> Result<()> {
    require_keys_eq!(*native_mint.key, NATIVE_SOL_MINT, VaultError::InvalidNativeSolMint);
    require_keys_eq!(
        *native_ata.key,
        canonical_ata(vault_sol.key, native_mint.key),
        VaultError::InvalidAssociatedTokenAccount
    );

    let vault_bump = [config.vault_sol_bump];
    let vault_seeds: [&[u8]; 2] = [VAULT_SOL_SEED, &vault_bump];
    let signer_seeds: &[&[&[u8]]] = &[&vault_seeds];

    if native_ata.owner != &SPL_TOKEN_PROGRAM_ID || native_ata.data_len() < SPL_TOKEN_ACCOUNT_LEN {
        let create_ata = Instruction {
            program_id: SPL_ATA_PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(*vault_sol.key, true),
                AccountMeta::new(*native_ata.key, false),
                AccountMeta::new_readonly(*vault_sol.key, false),
                AccountMeta::new_readonly(*native_mint.key, false),
                AccountMeta::new_readonly(anchor_lang::system_program::ID, false),
                AccountMeta::new_readonly(SPL_TOKEN_PROGRAM_ID, false),
            ],
            data: vec![1],
        };
        invoke_signed(
            &create_ata,
            &[
                vault_sol.clone(),
                native_ata.clone(),
                vault_sol.clone(),
                native_mint.clone(),
                system_program_info.clone(),
                token_program.clone(),
                associated_token_program.clone(),
            ],
            signer_seeds,
        )?;
    }

    require_spl_token_account(native_ata, vault_sol.key, &NATIVE_SOL_MINT)?;
    let existing_amount = {
        let data = native_ata.try_borrow_data()?;
        let raw: [u8; 8] = data[SPL_TOKEN_AMOUNT_OFFSET..SPL_TOKEN_AMOUNT_OFFSET + 8]
            .try_into()
            .map_err(|_| error!(VaultError::InvalidVaultTokenAccount))?;
        u64::from_le_bytes(raw)
    };
    let top_up = required_lamports.saturating_sub(existing_amount);
    if top_up > 0 {
        system_program::transfer(
            CpiContext::new_with_signer(
                system_program_info.clone(),
                Transfer {
                    from: vault_sol.clone(),
                    to: native_ata.clone(),
                },
                signer_seeds,
            ),
            top_up,
        )?;
    }

    let sync_native = Instruction {
        program_id: SPL_TOKEN_PROGRAM_ID,
        accounts: vec![AccountMeta::new(*native_ata.key, false)],
        data: vec![17],
    };
    invoke_signed(
        &sync_native,
        &[native_ata.clone(), token_program.clone()],
        signer_seeds,
    )?;
    Ok(())
}

fn reclaim_native_sol<'info>(
    config: &Config,
    vault_sol: &AccountInfo<'info>,
    native_ata: &AccountInfo<'info>,
    token_program: &AccountInfo<'info>,
) -> Result<()> {
    require_keys_eq!(
        *native_ata.key,
        canonical_ata(vault_sol.key, &NATIVE_SOL_MINT),
        VaultError::InvalidAssociatedTokenAccount
    );
    require_spl_token_account(native_ata, vault_sol.key, &NATIVE_SOL_MINT)?;
    let close = Instruction {
        program_id: SPL_TOKEN_PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*native_ata.key, false),
            AccountMeta::new(*vault_sol.key, false),
            AccountMeta::new_readonly(*vault_sol.key, true),
        ],
        data: vec![9],
    };
    let vault_bump = [config.vault_sol_bump];
    let vault_seeds: [&[u8]; 2] = [VAULT_SOL_SEED, &vault_bump];
    invoke_signed(
        &close,
        &[
            native_ata.clone(),
            vault_sol.clone(),
            vault_sol.clone(),
            token_program.clone(),
        ],
        &[&vault_seeds],
    )?;
    Ok(())
}



#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct OpenPositionArgs {
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub position_bump: u8,
}

#[derive(Accounts)]
pub struct OpenPosition<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch,
    )]
    pub config: Account<'info, Config>,

    #[account(mut, seeds = [VAULT_SOL_SEED], bump = config.vault_sol_bump)]
    pub vault_sol: UncheckedAccount<'info>,

    #[account(constraint = vault_authority.key() == config.vault_authority @ VaultError::UnauthorizedVaultAuthority)]
    pub vault_authority: Signer<'info>,

    #[account(address = ORCA_WHIRLPOOL_PROGRAM_ID @ VaultError::InvalidWhirlpoolProgram)]
    pub whirlpool_program: UncheckedAccount<'info>,

    #[account(mut, address = config.whirlpool @ VaultError::InvalidWhirlpool)]
    pub whirlpool: UncheckedAccount<'info>,

    #[account(mut)]
    pub position: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [POSITION_MINT_SEED, config.position_sequence.to_le_bytes().as_ref()],
        bump,
    )]
    pub position_mint: UncheckedAccount<'info>,

    #[account(mut)]
    pub position_token_account: UncheckedAccount<'info>,

    #[account(address = SPL_TOKEN_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub token_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    #[account(address = sysvar::rent::ID @ VaultError::OrcaAccountConstraintViolation)]
    pub rent: UncheckedAccount<'info>,

    #[account(address = SPL_ATA_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub associated_token_program: UncheckedAccount<'info>,
}

pub fn open_position_handler(ctx: Context<OpenPosition>, args: OpenPositionArgs) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(!config.liquidity_paused, VaultError::LiquidityPaused);

    require!(
        config.position == Pubkey::default(),
        VaultError::PositionAlreadyOpen
    );

    require!(args.tick_lower < args.tick_upper, VaultError::TickRangeOutOfBounds);
    require!(
        args.tick_lower >= config.min_tick && args.tick_upper <= config.max_tick,
        VaultError::TickRangeOutOfBounds
    );

    let _ = read_and_check_whirlpool(
        &ctx.accounts.whirlpool.to_account_info(),
        &config.token_mint_a,
        &config.token_mint_b,
    )?;

    let position_mint_key = ctx.accounts.position_mint.key();

    let expected_position = Pubkey::create_program_address(
        &[
            ORCA_POSITION_SEED,
            position_mint_key.as_ref(),
            &[args.position_bump],
        ],
        &ORCA_WHIRLPOOL_PROGRAM_ID,
    )
    .map_err(|_| error!(VaultError::InvalidPosition))?;
    require_keys_eq!(
        ctx.accounts.position.key(),
        expected_position,
        VaultError::InvalidPosition
    );

    let (expected_ata, _ata_bump) = Pubkey::find_program_address(
        &[
            ctx.accounts.vault_sol.key.as_ref(),
            SPL_TOKEN_PROGRAM_ID.as_ref(),
            position_mint_key.as_ref(),
        ],
        &SPL_ATA_PROGRAM_ID,
    );
    require_keys_eq!(
        ctx.accounts.position_token_account.key(),
        expected_ata,
        VaultError::InvalidPositionTokenAccount
    );

    let mut data = Vec::with_capacity(8 + 1 + 4 + 4);
    data.extend_from_slice(&DISC_OPEN_POSITION);
    data.push(args.position_bump);
    data.extend_from_slice(&args.tick_lower.to_le_bytes());
    data.extend_from_slice(&args.tick_upper.to_le_bytes());

    let accounts = vec![
        OrcaCpiAccount { info: ctx.accounts.vault_sol.to_account_info(), is_signer: true, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.vault_sol.to_account_info(), is_signer: false, is_writable: false },
        OrcaCpiAccount { info: ctx.accounts.position.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.position_mint.to_account_info(), is_signer: true, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.position_token_account.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.whirlpool.to_account_info(), is_signer: false, is_writable: false },
        OrcaCpiAccount { info: ctx.accounts.token_program.to_account_info(), is_signer: false, is_writable: false },
        OrcaCpiAccount { info: ctx.accounts.system_program.to_account_info(), is_signer: false, is_writable: false },
        OrcaCpiAccount { info: ctx.accounts.rent.to_account_info(), is_signer: false, is_writable: false },
        OrcaCpiAccount { info: ctx.accounts.associated_token_program.to_account_info(), is_signer: false, is_writable: false },
    ];

    let vault_bump = [config.vault_sol_bump];
    let mint_bump = [ctx.bumps.position_mint];
    let seq_bytes = config.position_sequence.to_le_bytes();
    let vault_seeds: [&[u8]; 2] = [VAULT_SOL_SEED, &vault_bump];
    let mint_seeds: [&[u8]; 3] = [POSITION_MINT_SEED, seq_bytes.as_ref(), &mint_bump];
    let signer_seeds: &[&[&[u8]]] = &[&vault_seeds, &mint_seeds];
    invoke_orca(
        &ctx.accounts.whirlpool_program.to_account_info(),
        &accounts,
        &data,
        signer_seeds,
    )?;

    let config = &mut ctx.accounts.config;
    config.position = ctx.accounts.position.key();
    config.position_mint = position_mint_key;
    config.position_token_account = ctx.accounts.position_token_account.key();
    config.position_sequence = config
        .position_sequence
        .checked_add(1)
        .ok_or(VaultError::MathOverflow)?;

    emit_op(
        OrcaOpKind::OpenPosition,
        ctx.accounts.whirlpool.key(),
        args.tick_lower,
        args.tick_upper,
    )
}


#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct IncreaseLiquidityArgs {
    pub liquidity_amount: u128,
    pub token_max_a: u64,
    pub token_max_b: u64,
    pub slippage_bps: u16,
}

#[derive(Accounts)]
pub struct ModifyLiquidity<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch,
    )]
    pub config: Account<'info, Config>,

    #[account(mut, seeds = [VAULT_SOL_SEED], bump = config.vault_sol_bump)]
    pub vault_sol: UncheckedAccount<'info>,

    #[account(constraint = vault_authority.key() == config.vault_authority @ VaultError::UnauthorizedVaultAuthority)]
    pub vault_authority: Signer<'info>,

    #[account(address = ORCA_WHIRLPOOL_PROGRAM_ID @ VaultError::InvalidWhirlpoolProgram)]
    pub whirlpool_program: UncheckedAccount<'info>,

    #[account(mut, address = config.whirlpool @ VaultError::InvalidWhirlpool)]
    pub whirlpool: UncheckedAccount<'info>,

    #[account(mut, address = config.position @ VaultError::InvalidPosition)]
    pub position: UncheckedAccount<'info>,

    #[account(address = config.position_token_account @ VaultError::InvalidPositionTokenAccount)]
    pub position_token_account: UncheckedAccount<'info>,

    #[account(mut, address = config.vault_token_account_a @ VaultError::InvalidVaultTokenAccount)]
    pub vault_token_account_a: UncheckedAccount<'info>,

    #[account(mut, address = config.vault_token_account_b @ VaultError::InvalidVaultTokenAccount)]
    pub vault_token_account_b: UncheckedAccount<'info>,

    #[account(mut)]
    pub token_vault_a: UncheckedAccount<'info>,

    #[account(mut)]
    pub token_vault_b: UncheckedAccount<'info>,

    #[account(mut)]
    pub tick_array_lower: UncheckedAccount<'info>,

    #[account(mut)]
    pub tick_array_upper: UncheckedAccount<'info>,

    #[account(address = SPL_TOKEN_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub token_program: UncheckedAccount<'info>,

    #[account(address = config.token_mint_a @ VaultError::InvalidMint)]
    pub token_mint_a: UncheckedAccount<'info>,

    #[account(address = config.token_mint_b @ VaultError::InvalidMint)]
    pub token_mint_b: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,

    #[account(address = SPL_ATA_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub associated_token_program: UncheckedAccount<'info>,
}

fn validate_modify(ctx_accounts: &ModifyLiquidityRefs) -> Result<()> {
    let config = ctx_accounts.config;

    let (vault_a, vault_b) = read_and_check_whirlpool(
        ctx_accounts.whirlpool,
        &config.token_mint_a,
        &config.token_mint_b,
    )?;
    require_keys_eq!(*ctx_accounts.token_vault_a.key, vault_a, VaultError::OrcaAccountConstraintViolation);
    require_keys_eq!(*ctx_accounts.token_vault_b.key, vault_b, VaultError::OrcaAccountConstraintViolation);

    require_spl_token_account(ctx_accounts.vault_token_account_a, ctx_accounts.vault_sol.key, &config.token_mint_a)?;
    require_spl_token_account(ctx_accounts.vault_token_account_b, ctx_accounts.vault_sol.key, &config.token_mint_b)?;

    let (tick_lower, tick_upper) =
        read_and_check_position(ctx_accounts.position, &config.whirlpool, &config.position_mint)?;

    require_spl_token_account(ctx_accounts.position_token_account, ctx_accounts.vault_sol.key, &config.position_mint)?;

    let spacing = read_whirlpool_tick_spacing(ctx_accounts.whirlpool)?;
    require_tick_array(ctx_accounts.tick_array_lower, &config.whirlpool, tick_lower, spacing)?;
    require_tick_array(ctx_accounts.tick_array_upper, &config.whirlpool, tick_upper, spacing)?;
    Ok(())
}

struct ModifyLiquidityRefs<'a, 'info> {
    config: &'a Config,
    vault_sol: &'a AccountInfo<'info>,
    whirlpool: &'a AccountInfo<'info>,
    position: &'a AccountInfo<'info>,
    position_token_account: &'a AccountInfo<'info>,
    vault_token_account_a: &'a AccountInfo<'info>,
    vault_token_account_b: &'a AccountInfo<'info>,
    token_vault_a: &'a AccountInfo<'info>,
    token_vault_b: &'a AccountInfo<'info>,
    tick_array_lower: &'a AccountInfo<'info>,
    tick_array_upper: &'a AccountInfo<'info>,
}

fn modify_refs<'a, 'info>(
    accounts: &'a ModifyLiquidity<'info>,
    infos: &'a ModifyInfos<'info>,
) -> ModifyLiquidityRefs<'a, 'info> {
    ModifyLiquidityRefs {
        config: &accounts.config,
        vault_sol: &infos.vault_sol,
        whirlpool: &infos.whirlpool,
        position: &infos.position,
        position_token_account: &infos.position_token_account,
        vault_token_account_a: &infos.vault_token_account_a,
        vault_token_account_b: &infos.vault_token_account_b,
        token_vault_a: &infos.token_vault_a,
        token_vault_b: &infos.token_vault_b,
        tick_array_lower: &infos.tick_array_lower,
        tick_array_upper: &infos.tick_array_upper,
    }
}

struct ModifyInfos<'info> {
    vault_sol: AccountInfo<'info>,
    whirlpool: AccountInfo<'info>,
    position: AccountInfo<'info>,
    position_token_account: AccountInfo<'info>,
    vault_token_account_a: AccountInfo<'info>,
    vault_token_account_b: AccountInfo<'info>,
    token_vault_a: AccountInfo<'info>,
    token_vault_b: AccountInfo<'info>,
    tick_array_lower: AccountInfo<'info>,
    tick_array_upper: AccountInfo<'info>,
    token_program: AccountInfo<'info>,
}

impl<'info> ModifyLiquidity<'info> {
    fn infos(&self) -> ModifyInfos<'info> {
        ModifyInfos {
            vault_sol: self.vault_sol.to_account_info(),
            whirlpool: self.whirlpool.to_account_info(),
            position: self.position.to_account_info(),
            position_token_account: self.position_token_account.to_account_info(),
            vault_token_account_a: self.vault_token_account_a.to_account_info(),
            vault_token_account_b: self.vault_token_account_b.to_account_info(),
            token_vault_a: self.token_vault_a.to_account_info(),
            token_vault_b: self.token_vault_b.to_account_info(),
            tick_array_lower: self.tick_array_lower.to_account_info(),
            tick_array_upper: self.tick_array_upper.to_account_info(),
            token_program: self.token_program.to_account_info(),
        }
    }

    fn modify_cpi_accounts(&self, infos: &ModifyInfos<'info>) -> Vec<OrcaCpiAccount<'info>> {
        vec![
            OrcaCpiAccount { info: infos.whirlpool.clone(), is_signer: false, is_writable: true },
            OrcaCpiAccount { info: infos.token_program.clone(), is_signer: false, is_writable: false },
            OrcaCpiAccount { info: infos.vault_sol.clone(), is_signer: true, is_writable: false },
            OrcaCpiAccount { info: infos.position.clone(), is_signer: false, is_writable: true },
            OrcaCpiAccount { info: infos.position_token_account.clone(), is_signer: false, is_writable: false },
            OrcaCpiAccount { info: infos.vault_token_account_a.clone(), is_signer: false, is_writable: true },
            OrcaCpiAccount { info: infos.vault_token_account_b.clone(), is_signer: false, is_writable: true },
            OrcaCpiAccount { info: infos.token_vault_a.clone(), is_signer: false, is_writable: true },
            OrcaCpiAccount { info: infos.token_vault_b.clone(), is_signer: false, is_writable: true },
            OrcaCpiAccount { info: infos.tick_array_lower.clone(), is_signer: false, is_writable: true },
            OrcaCpiAccount { info: infos.tick_array_upper.clone(), is_signer: false, is_writable: true },
        ]
    }
}

pub fn increase_liquidity_handler(
    ctx: Context<ModifyLiquidity>,
    args: IncreaseLiquidityArgs,
) -> Result<()> {
    check_liquidity_common(&ctx.accounts.config, args.slippage_bps)?;
    require!(
        ctx.accounts.config.position != Pubkey::default(),
        VaultError::NoOpenPosition
    );

    if ctx.accounts.config.token_mint_a == NATIVE_SOL_MINT {
        prepare_native_sol(
            &ctx.accounts.config,
            &ctx.accounts.vault_sol.to_account_info(),
            &ctx.accounts.token_mint_a.to_account_info(),
            &ctx.accounts.vault_token_account_a.to_account_info(),
            &ctx.accounts.token_program.to_account_info(),
            &ctx.accounts.system_program.to_account_info(),
            &ctx.accounts.associated_token_program.to_account_info(),
            args.token_max_a,
        )?;
    } else {
        prepare_native_sol(
            &ctx.accounts.config,
            &ctx.accounts.vault_sol.to_account_info(),
            &ctx.accounts.token_mint_b.to_account_info(),
            &ctx.accounts.vault_token_account_b.to_account_info(),
            &ctx.accounts.token_program.to_account_info(),
            &ctx.accounts.system_program.to_account_info(),
            &ctx.accounts.associated_token_program.to_account_info(),
            args.token_max_b,
        )?;
    }

    let infos = ctx.accounts.infos();
    validate_modify(&modify_refs(&ctx.accounts, &infos))?;

    let mut data = Vec::with_capacity(8 + 16 + 8 + 8);
    data.extend_from_slice(&DISC_INCREASE_LIQUIDITY);
    data.extend_from_slice(&args.liquidity_amount.to_le_bytes());
    data.extend_from_slice(&args.token_max_a.to_le_bytes());
    data.extend_from_slice(&args.token_max_b.to_le_bytes());

    let accounts = ctx.accounts.modify_cpi_accounts(&infos);
    let vault_bump = [ctx.accounts.config.vault_sol_bump];
    let seeds: [&[u8]; 2] = [VAULT_SOL_SEED, &vault_bump];
    let signer_seeds: &[&[&[u8]]] = &[&seeds];
    invoke_orca(
        &ctx.accounts.whirlpool_program.to_account_info(),
        &accounts,
        &data,
        signer_seeds,
    )?;

    let (tl, tu) = read_and_check_position(
        &ctx.accounts.position.to_account_info(),
        &ctx.accounts.config.whirlpool,
        &ctx.accounts.config.position_mint,
    )?;
    emit_op(OrcaOpKind::IncreaseLiquidity, ctx.accounts.whirlpool.key(), tl, tu)
}

pub fn decrease_liquidity_handler(
    ctx: Context<ModifyLiquidity>,
    args: DecreaseLiquidityArgs,
) -> Result<()> {
    check_liquidity_common(&ctx.accounts.config, args.slippage_bps)?;
    require!(
        ctx.accounts.config.position != Pubkey::default(),
        VaultError::NoOpenPosition
    );

    let infos = ctx.accounts.infos();
    validate_modify(&modify_refs(&ctx.accounts, &infos))?;

    let mut data = Vec::with_capacity(8 + 16 + 8 + 8);
    data.extend_from_slice(&DISC_DECREASE_LIQUIDITY);
    data.extend_from_slice(&args.liquidity_amount.to_le_bytes());
    data.extend_from_slice(&args.token_min_a.to_le_bytes());
    data.extend_from_slice(&args.token_min_b.to_le_bytes());

    let accounts = ctx.accounts.modify_cpi_accounts(&infos);
    let vault_bump = [ctx.accounts.config.vault_sol_bump];
    let seeds: [&[u8]; 2] = [VAULT_SOL_SEED, &vault_bump];
    let signer_seeds: &[&[&[u8]]] = &[&seeds];
    invoke_orca(
        &ctx.accounts.whirlpool_program.to_account_info(),
        &accounts,
        &data,
        signer_seeds,
    )?;

    let (tl, tu) = read_and_check_position(
        &ctx.accounts.position.to_account_info(),
        &ctx.accounts.config.whirlpool,
        &ctx.accounts.config.position_mint,
    )?;
    emit_op(OrcaOpKind::DecreaseLiquidity, ctx.accounts.whirlpool.key(), tl, tu)
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct DecreaseLiquidityArgs {
    pub liquidity_amount: u128,
    pub token_min_a: u64,
    pub token_min_b: u64,
    pub slippage_bps: u16,
}


#[derive(Accounts)]
pub struct UpdateFeesAndRewards<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch,
    )]
    pub config: Account<'info, Config>,

    #[account(seeds = [VAULT_SOL_SEED], bump = config.vault_sol_bump)]
    pub vault_sol: UncheckedAccount<'info>,

    #[account(constraint = vault_authority.key() == config.vault_authority @ VaultError::UnauthorizedVaultAuthority)]
    pub vault_authority: Signer<'info>,

    #[account(address = ORCA_WHIRLPOOL_PROGRAM_ID @ VaultError::InvalidWhirlpoolProgram)]
    pub whirlpool_program: UncheckedAccount<'info>,

    #[account(mut, address = config.whirlpool @ VaultError::InvalidWhirlpool)]
    pub whirlpool: UncheckedAccount<'info>,

    #[account(mut, address = config.position @ VaultError::InvalidPosition)]
    pub position: UncheckedAccount<'info>,

    pub tick_array_lower: UncheckedAccount<'info>,

    pub tick_array_upper: UncheckedAccount<'info>,
}

pub fn update_fees_and_rewards_handler(ctx: Context<UpdateFeesAndRewards>) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(!config.liquidity_paused, VaultError::LiquidityPaused);
    require!(config.position != Pubkey::default(), VaultError::NoOpenPosition);

    let _ = read_and_check_whirlpool(
        &ctx.accounts.whirlpool.to_account_info(),
        &config.token_mint_a,
        &config.token_mint_b,
    )?;
    let (tick_lower, tick_upper) = read_and_check_position(
        &ctx.accounts.position.to_account_info(),
        &config.whirlpool,
        &config.position_mint,
    )?;
    let spacing = read_whirlpool_tick_spacing(&ctx.accounts.whirlpool.to_account_info())?;
    require_tick_array(
        &ctx.accounts.tick_array_lower.to_account_info(),
        &config.whirlpool,
        tick_lower,
        spacing,
    )?;
    require_tick_array(
        &ctx.accounts.tick_array_upper.to_account_info(),
        &config.whirlpool,
        tick_upper,
        spacing,
    )?;

    let data = DISC_UPDATE_FEES_AND_REWARDS.to_vec();
    let accounts = vec![
        OrcaCpiAccount { info: ctx.accounts.whirlpool.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.position.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.tick_array_lower.to_account_info(), is_signer: false, is_writable: false },
        OrcaCpiAccount { info: ctx.accounts.tick_array_upper.to_account_info(), is_signer: false, is_writable: false },
    ];

    let vault_bump = [config.vault_sol_bump];
    let seeds: [&[u8]; 2] = [VAULT_SOL_SEED, &vault_bump];
    let signer_seeds: &[&[&[u8]]] = &[&seeds];
    invoke_orca(
        &ctx.accounts.whirlpool_program.to_account_info(),
        &accounts,
        &data,
        signer_seeds,
    )?;

    emit_op(OrcaOpKind::UpdateFeesAndRewards, ctx.accounts.whirlpool.key(), tick_lower, tick_upper)
}


#[derive(Accounts)]
pub struct CollectFees<'info> {
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch,
    )]
    pub config: Account<'info, Config>,

    #[account(seeds = [VAULT_SOL_SEED], bump = config.vault_sol_bump)]
    pub vault_sol: UncheckedAccount<'info>,

    #[account(constraint = vault_authority.key() == config.vault_authority @ VaultError::UnauthorizedVaultAuthority)]
    pub vault_authority: Signer<'info>,

    #[account(address = ORCA_WHIRLPOOL_PROGRAM_ID @ VaultError::InvalidWhirlpoolProgram)]
    pub whirlpool_program: UncheckedAccount<'info>,

    #[account(address = config.whirlpool @ VaultError::InvalidWhirlpool)]
    pub whirlpool: UncheckedAccount<'info>,

    #[account(mut, address = config.position @ VaultError::InvalidPosition)]
    pub position: UncheckedAccount<'info>,

    #[account(address = config.position_token_account @ VaultError::InvalidPositionTokenAccount)]
    pub position_token_account: UncheckedAccount<'info>,

    #[account(mut, address = config.vault_token_account_a @ VaultError::InvalidVaultTokenAccount)]
    pub vault_token_account_a: UncheckedAccount<'info>,

    #[account(mut, address = config.vault_token_account_b @ VaultError::InvalidVaultTokenAccount)]
    pub vault_token_account_b: UncheckedAccount<'info>,

    #[account(mut)]
    pub token_vault_a: UncheckedAccount<'info>,

    #[account(mut)]
    pub token_vault_b: UncheckedAccount<'info>,

    #[account(address = SPL_TOKEN_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub token_program: UncheckedAccount<'info>,
}

pub fn collect_fees_handler(ctx: Context<CollectFees>) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(!config.liquidity_paused, VaultError::LiquidityPaused);
    require!(config.position != Pubkey::default(), VaultError::NoOpenPosition);

    let (vault_a, vault_b) = read_and_check_whirlpool(
        &ctx.accounts.whirlpool.to_account_info(),
        &config.token_mint_a,
        &config.token_mint_b,
    )?;
    require_keys_eq!(ctx.accounts.token_vault_a.key(), vault_a, VaultError::OrcaAccountConstraintViolation);
    require_keys_eq!(ctx.accounts.token_vault_b.key(), vault_b, VaultError::OrcaAccountConstraintViolation);

    require_spl_token_account(&ctx.accounts.vault_token_account_a.to_account_info(), ctx.accounts.vault_sol.key, &config.token_mint_a)?;
    require_spl_token_account(&ctx.accounts.vault_token_account_b.to_account_info(), ctx.accounts.vault_sol.key, &config.token_mint_b)?;

    let (tick_lower, tick_upper) = read_and_check_position(
        &ctx.accounts.position.to_account_info(),
        &config.whirlpool,
        &config.position_mint,
    )?;
    require_spl_token_account(&ctx.accounts.position_token_account.to_account_info(), ctx.accounts.vault_sol.key, &config.position_mint)?;

    let data = DISC_COLLECT_FEES.to_vec();
    let accounts = vec![
        OrcaCpiAccount { info: ctx.accounts.whirlpool.to_account_info(), is_signer: false, is_writable: false },
        OrcaCpiAccount { info: ctx.accounts.vault_sol.to_account_info(), is_signer: true, is_writable: false },
        OrcaCpiAccount { info: ctx.accounts.position.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.position_token_account.to_account_info(), is_signer: false, is_writable: false },
        OrcaCpiAccount { info: ctx.accounts.vault_token_account_a.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.token_vault_a.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.vault_token_account_b.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.token_vault_b.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.token_program.to_account_info(), is_signer: false, is_writable: false },
    ];

    let vault_bump = [config.vault_sol_bump];
    let seeds: [&[u8]; 2] = [VAULT_SOL_SEED, &vault_bump];
    let signer_seeds: &[&[&[u8]]] = &[&seeds];
    invoke_orca(
        &ctx.accounts.whirlpool_program.to_account_info(),
        &accounts,
        &data,
        signer_seeds,
    )?;

    emit_op(OrcaOpKind::CollectFees, ctx.accounts.whirlpool.key(), tick_lower, tick_upper)
}


#[derive(Accounts)]
pub struct ClosePosition<'info> {
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.config_bump,
        constraint = config.version == CONFIG_VERSION @ VaultError::ConfigVersionMismatch,
    )]
    pub config: Account<'info, Config>,

    #[account(mut, seeds = [VAULT_SOL_SEED], bump = config.vault_sol_bump)]
    pub vault_sol: UncheckedAccount<'info>,

    #[account(constraint = vault_authority.key() == config.vault_authority @ VaultError::UnauthorizedVaultAuthority)]
    pub vault_authority: Signer<'info>,

    #[account(address = ORCA_WHIRLPOOL_PROGRAM_ID @ VaultError::InvalidWhirlpoolProgram)]
    pub whirlpool_program: UncheckedAccount<'info>,

    #[account(mut, address = config.position @ VaultError::InvalidPosition)]
    pub position: UncheckedAccount<'info>,

    #[account(mut, address = config.position_mint @ VaultError::InvalidPosition)]
    pub position_mint: UncheckedAccount<'info>,

    #[account(mut, address = config.position_token_account @ VaultError::InvalidPositionTokenAccount)]
    pub position_token_account: UncheckedAccount<'info>,

    #[account(address = SPL_TOKEN_PROGRAM_ID @ VaultError::OrcaAccountConstraintViolation)]
    pub token_program: UncheckedAccount<'info>,

    #[account(mut, address = config.vault_token_account_a @ VaultError::InvalidVaultTokenAccount)]
    pub vault_token_account_a: UncheckedAccount<'info>,

    #[account(mut, address = config.vault_token_account_b @ VaultError::InvalidVaultTokenAccount)]
    pub vault_token_account_b: UncheckedAccount<'info>,
}

pub fn close_position_handler(ctx: Context<ClosePosition>) -> Result<()> {
    let config = &ctx.accounts.config;
    require!(!config.liquidity_paused, VaultError::LiquidityPaused);
    require!(config.position != Pubkey::default(), VaultError::NoOpenPosition);

    let (tick_lower, tick_upper) = read_and_check_position(
        &ctx.accounts.position.to_account_info(),
        &config.whirlpool,
        &config.position_mint,
    )?;
    require_spl_token_account(
        &ctx.accounts.position_token_account.to_account_info(),
        ctx.accounts.vault_sol.key,
        &config.position_mint,
    )?;

    let data = DISC_CLOSE_POSITION.to_vec();
    let accounts = vec![
        OrcaCpiAccount { info: ctx.accounts.vault_sol.to_account_info(), is_signer: true, is_writable: false },
        OrcaCpiAccount { info: ctx.accounts.vault_sol.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.position.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.position_mint.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.position_token_account.to_account_info(), is_signer: false, is_writable: true },
        OrcaCpiAccount { info: ctx.accounts.token_program.to_account_info(), is_signer: false, is_writable: false },
    ];

    let vault_bump = [config.vault_sol_bump];
    let seeds: [&[u8]; 2] = [VAULT_SOL_SEED, &vault_bump];
    let signer_seeds: &[&[&[u8]]] = &[&seeds];
    let whirlpool_key = config.whirlpool;
    invoke_orca(
        &ctx.accounts.whirlpool_program.to_account_info(),
        &accounts,
        &data,
        signer_seeds,
    )?;

    if config.token_mint_a == NATIVE_SOL_MINT {
        reclaim_native_sol(
            config,
            &ctx.accounts.vault_sol.to_account_info(),
            &ctx.accounts.vault_token_account_a.to_account_info(),
            &ctx.accounts.token_program.to_account_info(),
        )?;
    } else {
        reclaim_native_sol(
            config,
            &ctx.accounts.vault_sol.to_account_info(),
            &ctx.accounts.vault_token_account_b.to_account_info(),
            &ctx.accounts.token_program.to_account_info(),
        )?;
    }

    let config = &mut ctx.accounts.config;
    config.position = Pubkey::default();
    config.position_mint = Pubkey::default();
    config.position_token_account = Pubkey::default();

    emit_op(OrcaOpKind::ClosePosition, whirlpool_key, tick_lower, tick_upper)
}


fn emit_op(kind: OrcaOpKind, whirlpool: Pubkey, tick_lower: i32, tick_upper: i32) -> Result<()> {
    let clock = Clock::get()?;
    emit!(LiquidityOperation {
        kind: kind as u8,
        whirlpool,
        tick_lower,
        tick_upper,
        timestamp: clock.unix_timestamp,
    });
    Ok(())
}
