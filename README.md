# Nummus Vault

This is Nummus Vault V9 smart contract for Solana.

Main idea is simple. User deposits SOL from own Phantom wallet and funds go directly into program controlled vault. Deposit and position are saved together on-chain, so if browser is closed after signing, deposit is still there and can be found by our reconciler.

Every position is connected to the wallet which made the deposit. Contract keeps principal balance on-chain and withdrawal can only go back to this same wallet, not some random adress. Every withdrawal has unique request ID and permanent receipt, so same request cant be paid twice.

For Orca liquidity, backend can choose the strategy but it cant just move funds anywhere. Contract checks configured SOL/USDC Whirlpool, mints, token accounts, tick arrays, range and slippage limits before signing CPI with vault PDA. Funds and collected fees always need to come back into vault controlled accounts.

Turnkey is used for signing and approvals. Normal approved liquidity operations can run automatic under limited policy. User withdrawal needs automation approval plus manual approval, and configuration or other root actions need stronger approval.

This V9 code is not automatically active on mainnet. Existing production system stays in legacy mode unless contract mode is separately enabled after testing, audit and final approval.

Program ID: `BaRfuBXneEAf6eFh3e7ECqNax8NyAmWHb3SkMWtSPUZw`