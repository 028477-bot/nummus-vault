import * as anchor from "anchor30";
import { Program } from "anchor30";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import { assert } from "chai";
import IDL from "../idl/nummus_vault.json";

const PROGRAM_ID = new PublicKey(
  "BaRfuBXneEAf6eFh3e7ECqNax8NyAmWHb3SkMWtSPUZw"
);
const ORCA = new PublicKey("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");

const enc = (s: string) => Buffer.from(s, "utf8");
const u64le = (n: number | bigint) => {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
};

describe("nummus_vault", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = new Program(IDL as any, provider);

  const admin = (provider.wallet as anchor.Wallet).payer;
  const vaultAuthority = Keypair.generate();
  const attacker = Keypair.generate();

  const [config] = PublicKey.findProgramAddressSync(
    [enc("config")],
    PROGRAM_ID
  );
  const [vaultSol] = PublicKey.findProgramAddressSync(
    [enc("vault_sol")],
    PROGRAM_ID
  );
  const SPL_TOKEN = new PublicKey(
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
  );
  const SPL_ATA = new PublicKey(
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
  );
  const NATIVE_SOL_MINT = new PublicKey(
    "So11111111111111111111111111111111111111112"
  );
  const TEST_USDC_MINT = Keypair.generate().publicKey;
  const vaultAta = (mint: PublicKey) =>
    PublicKey.findProgramAddressSync(
      [vaultSol.toBuffer(), SPL_TOKEN.toBuffer(), mint.toBuffer()],
      SPL_ATA
    )[0];

  const positionPda = (owner: PublicKey) =>
    PublicKey.findProgramAddressSync(
      [enc("user_position"), owner.toBuffer()],
      PROGRAM_ID
    )[0];
  const depositReceiptPda = (owner: PublicKey, id: number) =>
    PublicKey.findProgramAddressSync(
      [enc("deposit_receipt"), owner.toBuffer(), u64le(id)],
      PROGRAM_ID
    )[0];
  const withdrawalReceiptPda = (id: number) =>
    PublicKey.findProgramAddressSync(
      [enc("withdrawal_receipt"), u64le(id)],
      PROGRAM_ID
    )[0];
  const positionMintPda = (sequence: number | bigint) =>
    PublicKey.findProgramAddressSync(
      [enc("position_mint"), u64le(sequence)],
      PROGRAM_ID
    )[0];
  const orcaPositionPda = (mint: PublicKey) =>
    PublicKey.findProgramAddressSync([enc("position"), mint.toBuffer()], ORCA)[0];

  const user = Keypair.generate();

  before(async () => {
    for (const kp of [vaultAuthority, attacker, user]) {
      const sig = await provider.connection.requestAirdrop(
        kp.publicKey,
        5 * LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(sig);
    }

    await program.methods
      .initialize({
        whirlpool: Keypair.generate().publicKey,
        tokenMintA: NATIVE_SOL_MINT,
        tokenMintB: TEST_USDC_MINT,
        vaultTokenAccountA: vaultAta(NATIVE_SOL_MINT),
        vaultTokenAccountB: vaultAta(TEST_USDC_MINT),
        minTick: -443636,
        maxTick: 443636,
        maxSlippageBps: 300,
        vaultAuthority: vaultAuthority.publicKey,
      })
      .accounts({
        config,
        vaultSol,
        admin: admin.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();
  });

  it("commits an atomic deposit with position + receipt", async () => {
    const amount = 1 * LAMPORTS_PER_SOL;
    await program.methods
      .deposit(new anchor.BN(1), new anchor.BN(amount))
      .accounts({
        config,
        vaultSol,
        position: positionPda(user.publicKey),
        depositReceipt: depositReceiptPda(user.publicKey, 1),
        depositor: user.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([user])
      .rpc();

    const pos: any = await program.account.userPosition.fetch(
      positionPda(user.publicKey)
    );
    assert.equal(pos.owner.toBase58(), user.publicKey.toBase58());
    assert.equal(pos.balanceLamports.toString(), amount.toString());
  });

  it("rejects a duplicate deposit id (replay)", async () => {
    try {
      await program.methods
        .deposit(new anchor.BN(1), new anchor.BN(1))
        .accounts({
          config,
          vaultSol,
          position: positionPda(user.publicKey),
          depositReceipt: depositReceiptPda(user.publicKey, 1),
          depositor: user.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([user])
        .rpc();
      assert.fail("expected duplicate deposit id to fail");
    } catch (e: any) {
      assert.match(String(e), /already in use|custom program error/i);
    }
  });

  it("rejects a zero-amount deposit", async () => {
    try {
      await program.methods
        .deposit(new anchor.BN(99), new anchor.BN(0))
        .accounts({
          config,
          vaultSol,
          position: positionPda(user.publicKey),
          depositReceipt: depositReceiptPda(user.publicKey, 99),
          depositor: user.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([user])
        .rpc();
      assert.fail("expected zero amount to fail");
    } catch (e: any) {
      assert.match(String(e), /ZeroAmount/);
    }
  });

  it("rejects a withdrawal signed by a non-authority", async () => {
    try {
      await program.methods
        .withdraw(new anchor.BN(1000), new anchor.BN(1000))
        .accounts({
          config,
          vaultSol,
          position: positionPda(user.publicKey),
          withdrawalReceipt: withdrawalReceiptPda(1000),
          destination: user.publicKey,
          vaultAuthority: attacker.publicKey,
          payer: attacker.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([attacker])
        .rpc();
      assert.fail("expected unauthorized withdrawal to fail");
    } catch (e: any) {
      assert.match(String(e), /UnauthorizedVaultAuthority|constraint/i);
    }
  });

  it("rejects a withdrawal to a wallet not bound to the position", async () => {
    try {
      await program.methods
        .withdraw(new anchor.BN(1001), new anchor.BN(1000))
        .accounts({
          config,
          vaultSol,
          position: positionPda(user.publicKey),
          withdrawalReceipt: withdrawalReceiptPda(1001),
          destination: attacker.publicKey,
          vaultAuthority: vaultAuthority.publicKey,
          payer: vaultAuthority.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected destination substitution to fail");
    } catch (e: any) {
      assert.match(String(e), /WithdrawalDestinationMismatch|address|constraint/i);
    }
  });

  it("rejects a withdrawal greater than deposited principal", async () => {
    const pos: any = await program.account.userPosition.fetch(
      positionPda(user.publicKey)
    );
    try {
      await program.methods
        .withdraw(
          new anchor.BN(1002),
          new anchor.BN(pos.balanceLamports.toString()).addn(1)
        )
        .accounts({
          config,
          vaultSol,
          position: positionPda(user.publicKey),
          withdrawalReceipt: withdrawalReceiptPda(1002),
          destination: user.publicKey,
          vaultAuthority: vaultAuthority.publicKey,
          payer: vaultAuthority.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected principal-overdraw withdrawal to fail");
    } catch (e: any) {
      assert.match(String(e), /InsufficientPositionBalance/);
    }
  });

  it("pays a valid withdrawal to the bound wallet exactly once", async () => {
    const amount = 0.1 * LAMPORTS_PER_SOL;

    const destBefore = await provider.connection.getBalance(user.publicKey);
    const vaultBefore = await provider.connection.getBalance(vaultSol);
    const posBefore: any = await program.account.userPosition.fetch(
      positionPda(user.publicKey)
    );

    await program.methods
      .withdraw(new anchor.BN(2000), new anchor.BN(amount))
      .accounts({
        config,
        vaultSol,
        position: positionPda(user.publicKey),
        withdrawalReceipt: withdrawalReceiptPda(2000),
        destination: user.publicKey,
        vaultAuthority: vaultAuthority.publicKey,
        payer: vaultAuthority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([vaultAuthority])
      .rpc();

    const destAfter = await provider.connection.getBalance(user.publicKey);
    const vaultAfter = await provider.connection.getBalance(vaultSol);
    const posAfter: any = await program.account.userPosition.fetch(
      positionPda(user.publicKey)
    );

    assert.equal(
      destAfter - destBefore,
      amount,
      "destination wallet must gain exactly the withdrawn amount"
    );
    assert.equal(
      vaultBefore - vaultAfter,
      amount,
      "vault PDA must lose exactly the withdrawn amount (receipt rent is paid by payer)"
    );
    assert.equal(
      BigInt(posBefore.balanceLamports.toString()) -
        BigInt(posAfter.balanceLamports.toString()),
      BigInt(amount),
      "position ledger must decrease by exactly the withdrawn amount"
    );
    assert.equal(
      posAfter.withdrawalCount.toString(),
      (Number(posBefore.withdrawalCount.toString()) + 1).toString(),
      "withdrawal_count must increment by one"
    );

    try {
      await program.methods
        .withdraw(new anchor.BN(2000), new anchor.BN(1))
        .accounts({
          config,
          vaultSol,
          position: positionPda(user.publicKey),
          withdrawalReceipt: withdrawalReceiptPda(2000),
          destination: user.publicKey,
          vaultAuthority: vaultAuthority.publicKey,
          payer: vaultAuthority.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected withdrawal replay to fail");
    } catch (e: any) {
      assert.match(String(e), /already in use|custom program error/i);
    }
  });

  it("rejects a withdrawal that would breach the vault rent reserve (before any transfer)", async () => {
    const pos: any = await program.account.userPosition.fetch(
      positionPda(user.publicKey)
    );
    const drainAmount = new anchor.BN(pos.balanceLamports.toString());

    const destBefore = await provider.connection.getBalance(user.publicKey);
    const vaultBefore = await provider.connection.getBalance(vaultSol);

    try {
      await program.methods
        .withdraw(new anchor.BN(2001), drainAmount)
        .accounts({
          config,
          vaultSol,
          position: positionPda(user.publicKey),
          withdrawalReceipt: withdrawalReceiptPda(2001),
          destination: user.publicKey,
          vaultAuthority: vaultAuthority.publicKey,
          payer: vaultAuthority.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected rent-reserve breach to fail");
    } catch (e: any) {
      assert.match(String(e), /RentReserveBreach/);
    }

    const destAfter = await provider.connection.getBalance(user.publicKey);
    const vaultAfter = await provider.connection.getBalance(vaultSol);
    assert.equal(destAfter, destBefore, "destination must be unchanged on a rejected withdrawal");
    assert.equal(vaultAfter, vaultBefore, "vault must be unchanged on a rejected withdrawal");
  });

  it("pauses deposits without pausing a valid withdrawal", async () => {
    await program.methods
      .setPauseFlags(true, false)
      .accounts({ config, admin: admin.publicKey })
      .rpc();
    try {
      await program.methods
        .deposit(new anchor.BN(5), new anchor.BN(1000))
        .accounts({
          config,
          vaultSol,
          position: positionPda(user.publicKey),
          depositReceipt: depositReceiptPda(user.publicKey, 5),
          depositor: user.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([user])
        .rpc();
      assert.fail("expected paused deposit to fail");
    } catch (e: any) {
      assert.match(String(e), /DepositsPaused/);
    }

    const destinationBefore = await provider.connection.getBalance(user.publicKey);
    await program.methods
      .withdraw(new anchor.BN(2002), new anchor.BN(1))
      .accounts({
        config,
        vaultSol,
        position: positionPda(user.publicKey),
        withdrawalReceipt: withdrawalReceiptPda(2002),
        destination: user.publicKey,
        vaultAuthority: vaultAuthority.publicKey,
        payer: vaultAuthority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([vaultAuthority])
      .rpc();
    const destinationAfter = await provider.connection.getBalance(user.publicKey);
    assert.equal(
      destinationAfter - destinationBefore,
      1,
      "deposit pause must not prevent or alter a valid withdrawal",
    );

    await program.methods
      .setPauseFlags(false, false)
      .accounts({ config, admin: admin.publicKey })
      .rpc();
  });

  const RENT_SYSVAR = new PublicKey(
    "SysvarRent111111111111111111111111111111111"
  );

  it("rejects open_position with the wrong Orca program id", async () => {
    const cfg = await program.account.config.fetch(config);
    const positionMint = positionMintPda(cfg.positionSequence.toString());
    try {
      await program.methods
        .openPosition({ tickLower: -100, tickUpper: 100, positionBump: 254 })
        .accounts({
          config,
          vaultSol,
          vaultAuthority: vaultAuthority.publicKey,
          whirlpoolProgram: SystemProgram.programId,
          whirlpool: cfg.whirlpool,
          position: orcaPositionPda(positionMint),
          positionMint,
          positionTokenAccount: Keypair.generate().publicKey,
          tokenProgram: SPL_TOKEN,
          systemProgram: SystemProgram.programId,
          rent: RENT_SYSVAR,
          associatedTokenProgram: SPL_ATA,
        })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected wrong program id to fail");
    } catch (e: any) {
      assert.match(String(e), /InvalidWhirlpoolProgram|address|constraint/i);
    }
  });

  it("rejects open_position with an out-of-range tick", async () => {
    const cfg = await program.account.config.fetch(config);
    const positionMint = positionMintPda(cfg.positionSequence.toString());
    try {
      await program.methods
        .openPosition({ tickLower: -999999, tickUpper: 100, positionBump: 254 })
        .accounts({
          config,
          vaultSol,
          vaultAuthority: vaultAuthority.publicKey,
          whirlpoolProgram: ORCA,
          whirlpool: cfg.whirlpool,
          position: orcaPositionPda(positionMint),
          positionMint,
          positionTokenAccount: Keypair.generate().publicKey,
          tokenProgram: SPL_TOKEN,
          systemProgram: SystemProgram.programId,
          rent: RENT_SYSVAR,
          associatedTokenProgram: SPL_ATA,
        })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected out-of-range tick to fail");
    } catch (e: any) {
      assert.match(
        String(e),
        /TickRangeOutOfBounds|InvalidWhirlpool|PositionAlreadyOpen/
      );
    }
  });

  it("open_position needs NO external mint keypair (mint is a program PDA)", async () => {
    const cfg = await program.account.config.fetch(config);
    assert.equal(cfg.positionSequence.toString(), "0");

    const mint = positionMintPda(cfg.positionSequence.toString());

    try {
      await program.methods
        .openPosition({ tickLower: -100, tickUpper: 100, positionBump: 254 })
        .accounts({
          config,
          vaultSol,
          vaultAuthority: vaultAuthority.publicKey,
          whirlpoolProgram: ORCA,
          whirlpool: cfg.whirlpool,
          position: Keypair.generate().publicKey,
          positionMint: mint,
          positionTokenAccount: Keypair.generate().publicKey,
          tokenProgram: SPL_TOKEN,
          systemProgram: SystemProgram.programId,
          rent: RENT_SYSVAR,
          associatedTokenProgram: SPL_ATA,
        })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected substituted position / no-mint-signer path to fail");
    } catch (e: any) {
      assert.match(
        String(e),
        /InvalidPosition|InvalidWhirlpool|InvalidMint|ConstraintSeeds|constraint/i
      );
    }
  });

  it("rejects increase_liquidity with over-slippage / no open position", async () => {
    const cfg = await program.account.config.fetch(config);
    try {
      await program.methods
        .increaseLiquidity({
          liquidityAmount: new anchor.BN(1),
          tokenMaxA: new anchor.BN(1),
          tokenMaxB: new anchor.BN(1),
          slippageBps: 9999,
        })
        .accounts({
          config,
          vaultSol,
          vaultAuthority: vaultAuthority.publicKey,
          whirlpoolProgram: ORCA,
          whirlpool: cfg.whirlpool,
          position: cfg.position,
          positionTokenAccount: cfg.positionTokenAccount,
          vaultTokenAccountA: cfg.vaultTokenAccountA,
          vaultTokenAccountB: cfg.vaultTokenAccountB,
          tokenVaultA: Keypair.generate().publicKey,
          tokenVaultB: Keypair.generate().publicKey,
          tickArrayLower: Keypair.generate().publicKey,
          tickArrayUpper: Keypair.generate().publicKey,
          tokenProgram: SPL_TOKEN,
          tokenMintA: cfg.tokenMintA,
          tokenMintB: cfg.tokenMintB,
          systemProgram: SystemProgram.programId,
          associatedTokenProgram: SPL_ATA,
        })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected over-slippage / no-position to fail");
    } catch (e: any) {
      assert.match(
        String(e),
        /SlippageTooHigh|NoOpenPosition|InvalidWhirlpool|InvalidPosition|constraint/
      );
    }
  });

  it("rejects collect_fees when no position is open", async () => {
    const cfg = await program.account.config.fetch(config);
    try {
      await program.methods
        .collectFees()
        .accounts({
          config,
          vaultSol,
          vaultAuthority: vaultAuthority.publicKey,
          whirlpoolProgram: ORCA,
          whirlpool: cfg.whirlpool,
          position: cfg.position,
          positionTokenAccount: cfg.positionTokenAccount,
          vaultTokenAccountA: cfg.vaultTokenAccountA,
          vaultTokenAccountB: cfg.vaultTokenAccountB,
          tokenVaultA: Keypair.generate().publicKey,
          tokenVaultB: Keypair.generate().publicKey,
          tokenProgram: SPL_TOKEN,
        })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected no-open-position collect_fees to fail");
    } catch (e: any) {
      assert.match(
        String(e),
        /NoOpenPosition|InvalidPosition|InvalidWhirlpool|constraint/
      );
    }
  });

  it("rejects an LP call signed by a non-authority", async () => {
    const cfg = await program.account.config.fetch(config);
    try {
      await program.methods
        .collectFees()
        .accounts({
          config,
          vaultSol,
          vaultAuthority: attacker.publicKey,
          whirlpoolProgram: ORCA,
          whirlpool: cfg.whirlpool,
          position: cfg.position,
          positionTokenAccount: cfg.positionTokenAccount,
          vaultTokenAccountA: cfg.vaultTokenAccountA,
          vaultTokenAccountB: cfg.vaultTokenAccountB,
          tokenVaultA: Keypair.generate().publicKey,
          tokenVaultB: Keypair.generate().publicKey,
          tokenProgram: SPL_TOKEN,
        })
        .signers([attacker])
        .rpc();
      assert.fail("expected unauthorized LP signer to fail");
    } catch (e: any) {
      assert.match(
        String(e),
        /UnauthorizedVaultAuthority|constraint/i
      );
    }
  });
});
