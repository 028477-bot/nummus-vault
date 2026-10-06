import * as anchor from "anchor30";
import { Program } from "anchor30";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  LAMPORTS_PER_SOL,
  Transaction,
  Connection,
} from "@solana/web3.js";
import {
  ACCOUNT_SIZE,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  NATIVE_MINT,
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  createSyncNativeInstruction,
  getAccount,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { assert } from "chai";
import IDL from "../idl/nummus_vault.json";

const PROGRAM_ID = new PublicKey(
  "BaRfuBXneEAf6eFh3e7ECqNax8NyAmWHb3SkMWtSPUZw"
);
const ORCA = new PublicKey("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
const UPGRADEABLE_LOADER = new PublicKey(
  "BPFLoaderUpgradeab1e11111111111111111111111"
);
const VAULT_OPERATING_RESERVE = 5_000_000;

const enc = (s: string) => Buffer.from(s, "utf8");
const u64le = (n: number | bigint) => {
  const b = Buffer.alloc(8);
  b.writeBigUInt64LE(BigInt(n));
  return b;
};
const sleep = (milliseconds: number) =>
  new Promise((resolve) => setTimeout(resolve, milliseconds));

describe("nummus_vault", () => {
  // Anchor 0.30 defaults to processed. Keep writes and all subsequent account
  // reads at the same confirmed commitment; otherwise a just-confirmed test can
  // accidentally compare two different banks.
  const environmentProvider = anchor.AnchorProvider.env();
  const provider = new anchor.AnchorProvider(
    new Connection(environmentProvider.connection.rpcEndpoint, "confirmed"),
    environmentProvider.wallet,
    { commitment: "confirmed", preflightCommitment: "confirmed" }
  );
  anchor.setProvider(provider);
  const program: any = new Program(IDL as any, provider);

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
  const [programData] = PublicKey.findProgramAddressSync(
    [PROGRAM_ID.toBuffer()],
    UPGRADEABLE_LOADER
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

  const awaitConfirmedTokenAmount = async (
    address: PublicKey,
    expectedAmount: bigint
  ) => {
    let lastError: unknown;
    for (let attempt = 0; attempt < 50; attempt += 1) {
      try {
        const account = await getAccount(
          provider.connection,
          address,
          "confirmed",
          TOKEN_PROGRAM_ID
        );
        if (account.amount === expectedAmount) return account;
      } catch (error) {
        lastError = error;
      }
      await sleep(100);
    }
    assert.fail(
      `token account ${address.toBase58()} did not reach amount ${expectedAmount.toString()} at confirmed commitment${
        lastError ? `: ${String(lastError)}` : ""
      }`
    );
  };

  const awaitConfirmedTransaction = async (signature: string) => {
    for (let attempt = 0; attempt < 50; attempt += 1) {
      const transaction = await provider.connection.getTransaction(signature, {
        commitment: "confirmed",
        maxSupportedTransactionVersion: 0,
      });
      if (transaction) return transaction;
      await sleep(100);
    }
    assert.fail(
      `transaction ${signature} was not visible at confirmed commitment`
    );
  };

  before(async () => {
    assert.match(
      provider.connection.rpcEndpoint,
      /^https?:\/\/(?:127\.0\.0\.1|localhost|\[::1\])(?::|\/|$)/,
      "integration tests refuse to run against a non-local RPC endpoint"
    );
    for (const kp of [vaultAuthority, attacker, user]) {
      const sig = await provider.connection.requestAirdrop(
        kp.publicKey,
        5 * LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(sig);
    }
  });

  it("rejects an initializer that is not the program upgrade authority", async () => {
    try {
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
          admin: attacker.publicKey,
          systemProgram: SystemProgram.programId,
          programData,
        })
        .signers([attacker])
        .rpc();
      assert.fail("expected unauthorized singleton initialization to fail");
    } catch (e: any) {
      assert.match(String(e), /UnauthorizedAdmin|constraint/i);
    }

    assert.isNull(
      await provider.connection.getAccountInfo(config),
      "a rejected initializer must not consume the singleton config PDA"
    );
  });

  it("initializes only through the canonical loader ProgramData authority", async () => {
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
        programData,
      })
      .rpc();

    const rent = await provider.connection.getMinimumBalanceForRentExemption(0);
    const cfg: any = await program.account.config.fetch(config);
    assert.equal(cfg.totalDeposits.toString(), "0");
    assert.equal(cfg.totalWithdrawals.toString(), "0");
    assert.equal(
      await provider.connection.getBalance(vaultSol),
      rent + VAULT_OPERATING_RESERVE,
      "initial reserve is operating capital, not user principal"
    );
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
    assert.equal(pos.balanceLamports.toString(), "0");
  });

  it("persists deposit totals without a remaining user balance", async () => {
    const additions = [20_000, 30_000];
    for (let i = 0; i < additions.length; i += 1) {
      const depositId = i + 2;
      await program.methods
        .deposit(new anchor.BN(depositId), new anchor.BN(additions[i]))
        .accounts({
          config,
          vaultSol,
          position: positionPda(user.publicKey),
          depositReceipt: depositReceiptPda(user.publicKey, depositId),
          depositor: user.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([user])
        .rpc();
    }

    const cfg: any = await program.account.config.fetch(config);
    const pos: any = await program.account.userPosition.fetch(
      positionPda(user.publicKey)
    );
    const expected = LAMPORTS_PER_SOL + additions[0] + additions[1];
    assert.equal(cfg.totalDeposits.toString(), expected.toString());
    assert.equal(pos.balanceLamports.toString(), "0");
    assert.equal(pos.depositCount.toString(), "3");
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

  it("rejects an approved payout that would spend the vault reserve", async () => {
    const vaultBalance = await provider.connection.getBalance(vaultSol);
    try {
      await program.methods
        .withdraw(
          new anchor.BN(1002),
          new anchor.BN(vaultBalance)
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
      assert.fail("expected reserve-spending withdrawal to fail");
    } catch (e: any) {
      assert.match(String(e), /RentReserveBreach/);
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
      posAfter.balanceLamports.toString(),
      "0",
      "deprecated balance slot must remain zero after payout"
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

  it("allows the last user to withdraw all principal while preserving the funded reserve", async () => {
    // This fixture has one depositor; authorize the remaining deposited amount
    // using its lifetime flow totals, never the deprecated user balance slot.
    const cfgBefore: any = await program.account.config.fetch(config);
    const amount = cfgBefore.totalDeposits.sub(cfgBefore.totalWithdrawals);
    const destinationBefore = await provider.connection.getBalance(user.publicKey);

    await program.methods
      .withdraw(new anchor.BN(2003), amount)
      .accounts({
        config,
        vaultSol,
        position: positionPda(user.publicKey),
        withdrawalReceipt: withdrawalReceiptPda(2003),
        destination: user.publicKey,
        vaultAuthority: vaultAuthority.publicKey,
        payer: vaultAuthority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([vaultAuthority])
      .rpc();

    const destinationAfter = await provider.connection.getBalance(user.publicKey);
    const posAfter: any = await program.account.userPosition.fetch(
      positionPda(user.publicKey)
    );
    const cfg: any = await program.account.config.fetch(config);
    const rent = await provider.connection.getMinimumBalanceForRentExemption(0);

    assert.equal(
      destinationAfter - destinationBefore,
      Number(amount.toString()),
      "the final principal withdrawal must be paid in full"
    );
    assert.equal(posAfter.balanceLamports.toString(), "0");
    assert.equal(
      await provider.connection.getBalance(vaultSol),
      rent + VAULT_OPERATING_RESERVE,
      "rent and operating reserve must remain after all user principal exits"
    );
    assert.equal(
      cfg.totalWithdrawals.toString(),
      cfg.totalDeposits.toString(),
      "lifetime totals remain persisted after the live position reaches zero"
    );
  });

  it("never spends the funded operating reserve on an approved payout", async () => {
    const vaultBefore = await provider.connection.getBalance(vaultSol, "confirmed");
    const destinationBefore = await provider.connection.getBalance(
      user.publicKey,
      "confirmed"
    );
    const configBefore: any = await program.account.config.fetch(config);

    try {
      await program.methods
        .withdraw(new anchor.BN(2004), new anchor.BN(1))
        .accounts({
          config,
          vaultSol,
          position: positionPda(user.publicKey),
          withdrawalReceipt: withdrawalReceiptPda(2004),
          destination: user.publicKey,
          vaultAuthority: vaultAuthority.publicKey,
          payer: vaultAuthority.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected an attempt to spend the reserve to fail");
    } catch (e: any) {
      assert.match(String(e), /RentReserveBreach/);
    }

    const configAfter: any = await program.account.config.fetch(config);
    assert.equal(
      await provider.connection.getBalance(vaultSol, "confirmed"),
      vaultBefore
    );
    assert.equal(
      await provider.connection.getBalance(user.publicKey, "confirmed"),
      destinationBefore
    );
    assert.equal(
      configAfter.totalWithdrawals.toString(),
      configBefore.totalWithdrawals.toString()
    );
    assert.isNull(
      await provider.connection.getAccountInfo(
        withdrawalReceiptPda(2004),
        "confirmed"
      ),
      "a rejected reserve-spend attempt must not leave a receipt"
    );
  });

  it("unwraps donated native SOL into the vault and atomically recreates its canonical ATA", async () => {
    const donation = 100_000;
    const nativeAta = getAssociatedTokenAddressSync(
      NATIVE_MINT,
      vaultSol,
      true,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );
    assert.equal(
      nativeAta.toBase58(),
      vaultAta(NATIVE_SOL_MINT).toBase58(),
      "fixture must use the vault's canonical native ATA"
    );

    const wrongAta = getAssociatedTokenAddressSync(
      NATIVE_MINT,
      attacker.publicKey,
      false,
      TOKEN_PROGRAM_ID,
      ASSOCIATED_TOKEN_PROGRAM_ID
    );
    await provider.sendAndConfirm(
      new Transaction()
        .add(
          createAssociatedTokenAccountIdempotentInstruction(
            admin.publicKey,
            nativeAta,
            vaultSol,
            NATIVE_MINT,
            TOKEN_PROGRAM_ID,
            ASSOCIATED_TOKEN_PROGRAM_ID
          )
        )
        .add(
          SystemProgram.transfer({
            fromPubkey: admin.publicKey,
            toPubkey: nativeAta,
            lamports: donation,
          })
        )
        .add(createSyncNativeInstruction(nativeAta, TOKEN_PROGRAM_ID))
        .add(
          createAssociatedTokenAccountIdempotentInstruction(
            admin.publicKey,
            wrongAta,
            attacker.publicKey,
            NATIVE_MINT,
            TOKEN_PROGRAM_ID,
            ASSOCIATED_TOKEN_PROGRAM_ID
          )
        ),
      []
    );

    const wrappedBefore = await awaitConfirmedTokenAmount(
      nativeAta,
      BigInt(donation)
    );
    const replacementAtaRent =
      await provider.connection.getMinimumBalanceForRentExemption(ACCOUNT_SIZE);
    assert.equal(wrappedBefore.amount.toString(), donation.toString());
    assert.equal(
      await provider.connection.getBalance(nativeAta, "confirmed"),
      replacementAtaRent + donation,
      "fixture balance is exactly native ATA rent plus the explicit donation"
    );

    const unwrapAccounts = {
      config,
      vaultSol,
      vaultAuthority: vaultAuthority.publicKey,
      nativeAta,
      tokenProgram: TOKEN_PROGRAM_ID,
      nativeMint: NATIVE_MINT,
      systemProgram: SystemProgram.programId,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
    };

    try {
      await program.methods
        .unwrapNativeSol()
        .accounts({
          ...unwrapAccounts,
          vaultAuthority: attacker.publicKey,
        })
        .signers([attacker])
        .rpc();
      assert.fail("expected an unauthorized unwrap signer to fail");
    } catch (e: any) {
      assert.match(String(e), /UnauthorizedVaultAuthority|constraint/i);
    }

    try {
      await program.methods
        .unwrapNativeSol()
        .accounts({ ...unwrapAccounts, nativeAta: wrongAta })
        .signers([vaultAuthority])
        .rpc();
      assert.fail("expected a non-canonical native ATA to fail");
    } catch (e: any) {
      assert.match(String(e), /InvalidAssociatedTokenAccount|constraint/i);
    }

    const configBefore: any = await program.account.config.fetch(
      config,
      "confirmed"
    );
    const vaultBefore = await provider.connection.getBalance(
      vaultSol,
      "confirmed"
    );
    const oldAtaLamports = await provider.connection.getBalance(
      nativeAta,
      "confirmed"
    );
    const authorityBefore = await provider.connection.getBalance(
      vaultAuthority.publicKey,
      "confirmed"
    );
    await program.methods
      .unwrapNativeSol()
      .accounts(unwrapAccounts)
      .signers([vaultAuthority])
      .rpc();

    const vaultAfter = await provider.connection.getBalance(
      vaultSol,
      "confirmed"
    );
    const authorityAfter = await provider.connection.getBalance(
      vaultAuthority.publicKey,
      "confirmed"
    );
    const wrappedAfter = await awaitConfirmedTokenAmount(nativeAta, 0n);
    const configAfter: any = await program.account.config.fetch(
      config,
      "confirmed"
    );

    assert.equal(
      vaultAfter - vaultBefore,
      oldAtaLamports,
      "the vault recovers the old ATA's wrapped amount and rent in full"
    );
    assert.equal(
      authorityBefore - authorityAfter,
      replacementAtaRent,
      "the operator authority, not user principal, pays replacement ATA rent"
    );
    assert.equal(wrappedAfter.amount.toString(), "0");
    assert.isTrue(wrappedAfter.isNative, "recreated account remains a native SOL account");
    assert.equal(wrappedAfter.owner.toBase58(), vaultSol.toBase58());
    assert.equal(wrappedAfter.mint.toBase58(), NATIVE_MINT.toBase58());
    assert.equal(
      configAfter.totalDeposits.toString(),
      configBefore.totalDeposits.toString(),
      "donated LP fixture funds are not user principal"
    );
    assert.equal(
      configAfter.totalWithdrawals.toString(),
      configBefore.totalWithdrawals.toString()
    );
    for (const field of [
      "position",
      "positionMint",
      "positionTokenAccount",
      "positionSequence",
    ]) {
      assert.equal(
        configAfter[field].toString(),
        configBefore[field].toString(),
        `unwrap must not mutate config.${field}`
      );
    }
  });

  it("rejects configuration values outside the canonical bounds", async () => {
    const cfgBefore: any = await program.account.config.fetch(config);

    try {
      await program.methods
        .updateConfig(
          cfgBefore.minTick,
          cfgBefore.maxTick,
          5_001,
          cfgBefore.vaultTokenAccountA,
          cfgBefore.vaultTokenAccountB
        )
        .accounts({ config, admin: admin.publicKey })
        .rpc();
      assert.fail("expected excessive configuration slippage to fail");
    } catch (e: any) {
      assert.match(String(e), /SlippageTooHigh/);
    }

    try {
      await program.methods
        .updateConfig(
          101,
          100,
          cfgBefore.maxSlippageBps,
          cfgBefore.vaultTokenAccountA,
          cfgBefore.vaultTokenAccountB
        )
        .accounts({ config, admin: admin.publicKey })
        .rpc();
      assert.fail("expected an inverted tick range to fail");
    } catch (e: any) {
      assert.match(String(e), /InvalidTickRange|TickRangeOutOfBounds/);
    }

    const cfgAfter: any = await program.account.config.fetch(config);
    assert.equal(cfgAfter.minTick, cfgBefore.minTick);
    assert.equal(cfgAfter.maxTick, cfgBefore.maxTick);
    assert.equal(
      cfgAfter.maxSlippageBps,
      cfgBefore.maxSlippageBps,
      "rejected updates must not partially mutate configuration"
    );
  });

  it("emits the old and new values for a valid configuration update", async () => {
    const before: any = await program.account.config.fetch(config);
    const newMinTick = -1_000;
    const newMaxTick = 1_000;
    const newMaxSlippageBps = 250;

    const signature = await program.methods
      .updateConfig(
        newMinTick,
        newMaxTick,
        newMaxSlippageBps,
        before.vaultTokenAccountA,
        before.vaultTokenAccountB
      )
      .accounts({ config, admin: admin.publicKey })
      .rpc();
    const transaction = await awaitConfirmedTransaction(signature);

    const parser = new anchor.EventParser(program.programId, program.coder);
    const events = [
      ...parser.parseLogs(transaction.meta?.logMessages ?? []),
    ];
    // Program converts source IDL names to camelCase in Anchor 0.30.
    const update: any = events.find((event) => event.name === "configUpdated");
    assert.isDefined(update, "ConfigUpdated must be emitted");
    assert.equal(update.data.config.toBase58(), config.toBase58());
    assert.equal(update.data.actor.toBase58(), admin.publicKey.toBase58());
    assert.equal(update.data.oldMinTick, before.minTick);
    assert.equal(update.data.newMinTick, newMinTick);
    assert.equal(update.data.oldMaxTick, before.maxTick);
    assert.equal(update.data.newMaxTick, newMaxTick);
    assert.equal(update.data.oldMaxSlippageBps, before.maxSlippageBps);
    assert.equal(update.data.newMaxSlippageBps, newMaxSlippageBps);
    assert.equal(
      update.data.oldVaultTokenAccountA.toBase58(),
      before.vaultTokenAccountA.toBase58()
    );
    assert.equal(
      update.data.newVaultTokenAccountA.toBase58(),
      before.vaultTokenAccountA.toBase58()
    );
    assert.equal(
      update.data.oldVaultTokenAccountB.toBase58(),
      before.vaultTokenAccountB.toBase58()
    );
    assert.equal(
      update.data.newVaultTokenAccountB.toBase58(),
      before.vaultTokenAccountB.toBase58()
    );

    const after: any = await program.account.config.fetch(config);
    assert.equal(after.minTick, newMinTick);
    assert.equal(after.maxTick, newMaxTick);
    assert.equal(after.maxSlippageBps, newMaxSlippageBps);
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

  it("requires new, outgoing, and admin consent for vault-authority rotation", async () => {
    const replacement = Keypair.generate();
    const airdrop = await provider.connection.requestAirdrop(
      replacement.publicKey,
      LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(airdrop);

    await program.methods
      .proposeAuthority(0, replacement.publicKey)
      .accounts({ config, admin: admin.publicKey })
      .rpc();

    try {
      await program.methods
        .acceptAuthority(0)
        .accounts({
          config,
          newAuthority: replacement.publicKey,
          outgoingAuthority: vaultAuthority.publicKey,
          currentAdmin: null,
        })
        .signers([replacement, vaultAuthority])
        .rpc();
      assert.fail("expected missing current-admin consent to fail");
    } catch (e: any) {
      assert.match(String(e), /UnauthorizedAdmin/);
    }

    try {
      await program.methods
        .acceptAuthority(0)
        .accounts({
          config,
          newAuthority: replacement.publicKey,
          outgoingAuthority: null,
          currentAdmin: admin.publicKey,
        })
        .signers([replacement])
        .rpc();
      assert.fail("expected missing outgoing-authority consent to fail");
    } catch (e: any) {
      assert.match(String(e), /UnauthorizedVaultAuthority/);
    }

    const stillPending: any = await program.account.config.fetch(config);
    assert.equal(
      stillPending.vaultAuthority.toBase58(),
      vaultAuthority.publicKey.toBase58(),
      "failed consent attempts must not rotate authority"
    );
    assert.equal(
      stillPending.pendingVaultAuthority.toBase58(),
      replacement.publicKey.toBase58()
    );

    await program.methods
      .acceptAuthority(0)
      .accounts({
        config,
        newAuthority: replacement.publicKey,
        outgoingAuthority: vaultAuthority.publicKey,
        currentAdmin: admin.publicKey,
      })
      .signers([replacement, vaultAuthority])
      .rpc();

    const rotated: any = await program.account.config.fetch(config);
    assert.equal(
      rotated.vaultAuthority.toBase58(),
      replacement.publicKey.toBase58()
    );
    assert.equal(
      rotated.pendingVaultAuthority.toBase58(),
      PublicKey.default.toBase58()
    );
  });
});
