# Live results: the verifiable binary on devnet (2026-11-08)

**One pass of `tests/live.ts` against the binary `scripts/verifiable-build.sh` builds**, deployed
under a throwaway devnet id. It is a rehearsal of the mainnet build path, not the
three-per-instruction proof, which stays `LIVE_RESULTS.md` (an `anchor build` binary).

- Program `GK7WqXoQCUjSnNvWfhUidMu4fm2QFhrS5PF8Vk3MuBA9`, genesis `EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG`.
- Binary: 643,744 B, SBPF v3, sha256 `408aa9a7fce9b0bdd5b15d1031cb1a50267735a4acfe604918e4442331daba95`,
  verify_hash (trailing zeros stripped, as solana-verify hashes) `4a9a5c2f434d88a0627daa6b299fa1d72947eaf67753a862d605b846eaa00e73`.
  Built twice from the same tree, identical both times.
- Deployed at slot 520,190,058, 2026-11-08T23:43:22Z, `3Wpe8JvKpUdixVup9PXCDKyCiCKkQcMRRb1jBB7vEqfLrWGmLPqKd7kpsMK1uMycNrKn5uEw8HVoLrkDQHZcpz9h`.
  Program data `DfaNLmZ5da6bmEYKyVbgBXGobRgAvzCxvnz9yQ9JSgGE`, 643,789 B, 3,271,098,360 lamports
  (= `getMinimumBalanceForRentExemption(643789)`, slot 520,190,601); its bytes equal the built file.
- Run 23:46:09Z to 00:16:26Z: **134 of 134 checks, all 24 instructions**. Each check sends, reads the
  signature back with `getTransaction`, and compares the state read before and after (or, for a
  refusal, the named error and no change).
- **Why this file is the run's log and not its usual report**: the run finished every check, then
  its final write (`writeResults`, and the `second-root.json` it saves) failed with `ENOSPC`, because
  this PC's C: drive reached 0 bytes free at that moment. The per-check signatures were in that
  report. They are on chain: `getSignaturesForAddress` on the program lists 141 transactions from the
  deploy to `4U4dZz9Eg2KYtoEVHKzS53mUYTjoaLEUJPG1SV9sNzvVSAzLr1Xj6o3dLkV5n4Xj6b9vge4enzWzodgY36KYR2Wb`
  (slot 520,198,349): 78 succeeded and 63 failed, and 63 is exactly the number of refusals below.
- Not covered by this pass: the 6-hour second root (its saved state was not written), and the
  launch size and compute figures, which were not re-recorded.

## By instruction

| Instruction | Succeeded | Refused as expected | Failed |
| --- | ---: | ---: | ---: |
| `accept_admin` | 2 | 1 | 0 |
| `admin_recover_lamports` | 2 | 2 | 0 |
| `admin_recover_vault_lamports` | 2 | 2 | 0 |
| `admin_recover` | 7 | 7 | 0 |
| `claim_creator` | 2 | 2 | 0 |
| `claim_holder` | 6 | 4 | 0 |
| `crank_fees` | 4 | 2 | 0 |
| `initialize` | 1 | 4 | 0 |
| `launch` | 5 | 9 | 0 |
| `propose_admin` | 2 | 2 | 0 |
| `publish_root` | 3 | 6 | 0 |
| `rescue_foreign_token` | 3 | 4 | 0 |
| `set_creator_payout` | 2 | 2 | 0 |
| `set_global_pause` | 2 | 1 | 0 |
| `set_guardian` | 2 | 1 | 0 |
| `set_keeper` | 2 | 2 | 0 |
| `set_launch_fee` | 1 | 2 | 0 |
| `set_launch_pause` | 4 | 1 | 0 |
| `set_platform_wallet` | 2 | 2 | 0 |
| `set_quote` | 4 | 3 | 0 |
| `sweep_excess_lamports` | 3 | 2 | 0 |
| `sweep_platform` | 5 | 1 | 0 |
| `sweep_vault_lamports` | 2 | 1 | 0 |
| `sync_vault` | 3 | 0 | 0 |

## The checks, in order, as the run printed them

```
ok   initialize: a stranger, not the upgrade authority -> NotUpgradeAuthority (12324 CU)
ok   initialize: keeper and guardian the same key -> KeeperIsGuardian (13099 CU)
ok   initialize: launch fee above the cap -> LaunchFeeAboveCap (12590 CU)
ok   initialize: the upgrade authority creates Config -> success (11522 CU)
ok   initialize: a second initialize -> {"InstructionError":[1,{"Custom":0}]} (8753 CU)
ok   set_launch_fee: lowered to a test fee, read back -> success (4155 CU)
ok   set_launch_fee: above the compiled cap -> LaunchFeeAboveCap (5260 CU)
ok   set_launch_fee: a stranger -> ConstraintHasOne (5384 CU)
ok   set_keeper: rotated to a new key -> success (4190 CU)
ok   set_keeper: keeper gone: the admin replaces it again -> success (4190 CU)
ok   set_keeper: the guardian key -> KeeperIsGuardian (5805 CU)
ok   set_keeper: the zero address -> ZeroAddress (5289 CU)
ok   set_guardian: rotated to a new key -> success (4188 CU)
ok   set_guardian: guardian gone: the admin replaces it again -> success (4188 CU)
ok   set_guardian: the keeper key -> KeeperIsGuardian (5803 CU)
ok   set_platform_wallet: the wallet is lost: moved to a new one -> success (4181 CU)
ok   set_platform_wallet: moved back -> success (4181 CU)
ok   set_platform_wallet: a stranger -> ConstraintHasOne (5403 CU)
ok   set_platform_wallet: the zero address -> ZeroAddress (5285 CU)
ok   propose_admin: a stranger -> ConstraintHasOne (5403 CU)
ok   propose_admin: the zero address -> ZeroAddress (5285 CU)
ok   propose_admin: proposes a new admin -> success (4181 CU)
ok   accept_admin: someone other than the proposed admin -> NotPendingAdmin (5392 CU)
ok   accept_admin: the proposed admin takes over -> success (4153 CU)
ok   propose_admin: hands it back -> success (4181 CU)
ok   accept_admin: the first admin returns -> success (4153 CU)
ok   set_quote: a mock stock on Token-2022, disabled first -> success (11313 CU)
ok   set_quote: a mock PreStocks with a 1% transfer fee -> success (12908 CU)
ok   set_quote: wrapped SOL, the one SPL quote -> success (14298 CU)
ok   set_quote: a pausable mint -> success (11408 CU)
ok   set_quote: a plain SPL mint that is not wSOL -> UnsupportedQuoteProgram (13898 CU)
ok   set_quote: an empty price band -> InvalidPriceBand (9723 CU)
ok   set_quote: a stranger -> ConstraintHasOne (9893 CU)
ok   launch: a quote not yet enabled -> QuoteDisabled (57647 CU)
ok   launch: a salt whose mint does not end PEDL -> CoinSuffix (78101 CU)
ok   launch: a rate of 1.24% -> InvalidRate (72974 CU)
ok   launch: an opening price outside the band -> OpeningPriceOutOfBand (73667 CU)
ok   launch: a thin liquidity, the attack on the first buy -> LiquidityNotExact (74755 CU)
ok   launch: a first buy asking for more coins than it buys -> FirstBuyBelowMinimum (252034 CU)
ok   launch: a paused quote mint -> MintPaused (80998 CU)
ok   launch: unbound salt, no first buy: one-sided, 2% -> success (238463 CU)
ok   launch: bound salt, first buy, 5%, half the excess to the creator -> success (283706 CU)
ok   launch: the same bound salt from another creator derives another mint -> CoinSuffix (70609 CU)
ok   launch: a transfer-fee quote, first buy, 3% -> success (265459 CU)
ok   launch: a wrapped-SOL quote, first buy, 1.25% -> success (258774 CU)
ok   launch: a fifth launch, to be recovered from -> success (221962 CU)
ok   launch: the same salt twice -> {"InstructionError":[1,{"Custom":0}]} (15695 CU)
ok   crank_fees: stock pool, 2%: a stranger cranks -> success (36733 CU)
ok   crank_fees: stock pool, 5%, half the excess to the creator -> success (36732 CU)
ok   crank_fees: transfer-fee quote: splits what arrived, not what was sent -> success (38420 CU)
ok   crank_fees: another launch's vault -> ConstraintAddress (11452 CU)
ok   sweep_platform: a stranger pushes, creating the platform account -> success (46911 CU)
ok   sweep_platform: first-buy fee and trading fees together -> success (21492 CU)
ok   sweep_platform: a transfer-fee quote -> success (68722 CU)
ok   sweep_platform: nothing owed -> NothingToPay (17162 CU)
ok   set_creator_payout: a stranger -> ConstraintHasOne (5856 CU)
ok   set_creator_payout: the zero address -> ZeroAddress (5672 CU)
ok   set_creator_payout: moved to a new wallet -> success (5706 CU)
ok   set_creator_payout: bound launch: creator moves its payout to a wallet that is then lost -> success (5706 CU)
ok   claim_creator: pays the NEW payout address -> success (40233 CU)
ok   claim_creator: fixed leg plus excess share -> success (17814 CU)
ok   claim_creator: nothing owed -> NothingToPay (13435 CU)
ok   claim_creator: the old payout address -> ConstraintAddress (14021 CU)
ok   sync_vault: tokens sent straight to the vault become holder money -> success (6939 CU)
ok   sync_vault: lamports forced into a wSOL vault are wrapped and credited -> success (8829 CU)
ok   sync_vault: nothing unaccounted: a no-op -> success (6929 CU)
ok   publish_root: a stranger signs as keeper -> ConstraintHasOne (6528 CU)
ok   publish_root: a total above what arrived -> RootTotalAboveFunded (7700 CU)
ok   publish_root: an epoch out of order -> RootEpochOutOfOrder (7680 CU)
ok   publish_root: a real keeper root on launch 1, claimable at once -> success (8229 CU)
ok   publish_root: a real keeper root on launch 2 -> success (8229 CU)
ok   publish_root: a real keeper root on launch 3, a transfer-fee quote -> success (8229 CU)
ok   publish_root: a second root inside 6 hours -> RootTooSoon (7680 CU)
ok   claim_holder: a forged amount -> InvalidProof (27664 CU)
ok   claim_holder: a real proof on launch 1 -> success (32341 CU)
ok   claim_holder: the same proof twice -> NothingToPay (25108 CU)
ok   claim_holder: a real proof on launch 2 -> success (27841 CU)
ok   claim_holder: transfer-fee quote: paid net of the issuer's fee, ledger takes the gross -> success (34162 CU)
ok   set_global_pause: the keeper cannot pause -> ConstraintHasOne (5423 CU)
ok   set_global_pause: the guardian stops everything -> success (4345 CU)
ok   claim_holder: during a global pause -> Paused (31764 CU)
ok   publish_root: during a global pause -> Paused (6042 CU)
ok   crank_fees: a pause does not stop a crank -> success (36733 CU)
ok   sweep_platform: a pause does not stop the platform leg -> success (21491 CU)
ok   set_global_pause: the guardian lifts it -> success (4344 CU)
ok   set_launch_pause: a stranger -> ConstraintHasOne (6500 CU)
ok   set_launch_pause: the guardian stops one launch -> success (7850 CU)
ok   claim_holder: on a paused launch -> Paused (33331 CU)
ok   claim_holder: another launch is unaffected -> success (34045 CU)
ok   admin_recover: surplus sent in directly, launch PAUSED -> success (15821 CU)
ok   admin_recover: more surplus than exists -> RecoverAboveBucket (11044 CU)
ok   admin_recover: a stranger -> ConstraintHasOne (9806 CU)
ok   admin_recover: the guardian -> ConstraintHasOne (9806 CU)
ok   admin_recover: paying the vault itself -> ConstraintDuplicateMutableAccount (7652 CU)
ok   admin_recover: a destination of another mint -> ConstraintTokenMint (11443 CU)
ok   admin_recover: part of the platform leg -> success (15781 CU)
ok   admin_recover: the creator leg: wallet lost -> success (15781 CU)
ok   admin_recover: more than the bucket -> RecoverAboveBucket (10998 CU)
ok   admin_recover: unpublished holder money: published claims untouched -> success (15796 CU)
ok   publish_root: money recovery took cannot be promised -> RootTooSoon (7680 CU)
ok   admin_recover: published holder money, leaving a shortfall -> success (15796 CU)
ok   claim_holder: over a shortfall: pays what is left, keeps the rest owed -> success (27843 CU)
ok   claim_holder: new money refills it: the same proof finishes the claim -> success (25288 CU)
ok   sweep_platform: after recovery, the rest of the leg still pays -> success (21492 CU)
ok   admin_recover: creator wallet lost, transfer-fee quote: ledger takes the gross -> success (17480 CU)
ok   admin_recover: a vault frozen by the issuer -> AccountFrozen (11344 CU)
ok   crank_fees: a frozen vault reverts and loses nothing -> {"InstructionError":[1,{"Custom":17}]} (29956 CU)
ok   admin_recover: after the thaw, every holder unit of a launch with no keeper -> success (15796 CU)
ok   set_launch_pause: lifted, read back -> success (7849 CU)
ok   set_launch_pause: a second launch stopped -> success (7850 CU)
ok   set_launch_pause: the second launch lifted -> success (7849 CU)
ok   rescue_foreign_token: stray tokens in an ATA of a LaunchRecord -> success (15387 CU)
ok   rescue_foreign_token: a stranger -> ConstraintHasOne (8427 CU)
ok   rescue_foreign_token: stray tokens in an ATA of Config -> success (14387 CU)
ok   rescue_foreign_token: stray tokens owned by a FeeVault -> success (13147 CU)
ok   rescue_foreign_token: the position NFT, the locked liquidity -> ProtectedAccount (12577 CU)
ok   rescue_foreign_token: the fee vault -> ProtectedAccount (12482 CU)
ok   rescue_foreign_token: seeds that do not match the account -> RescueSeedsMismatch (12158 CU)
ok   sweep_excess_lamports: forced into a LaunchRecord -> success (4056 CU)
ok   sweep_excess_lamports: forced into Config -> success (3838 CU)
ok   sweep_excess_lamports: forced into a QuoteConfig -> success (4056 CU)
ok   sweep_excess_lamports: nothing above rent -> NothingToSweep (5475 CU)
ok   sweep_excess_lamports: an account that is not ours -> NotProgramOwned (4983 CU)
ok   sweep_vault_lamports: forced into a Token-2022 vault -> success (9647 CU)
ok   sweep_vault_lamports: a transfer-fee vault -> success (9709 CU)
ok   sweep_vault_lamports: a wSOL vault wraps through sync_vault instead -> VaultLamportsViaSync (8003 CU)
ok   admin_recover_lamports: to an address named in the call -> success (4759 CU)
ok   admin_recover_lamports: from Config -> success (4541 CU)
ok   admin_recover_lamports: a stranger -> ConstraintHasOne (5013 CU)
ok   admin_recover_lamports: an account that is not ours -> NotProgramOwned (5324 CU)
ok   admin_recover_vault_lamports: to an address named in the call -> success (10413 CU)
ok   admin_recover_vault_lamports: after a launch was emptied -> success (10413 CU)
ok   admin_recover_vault_lamports: a stranger -> ConstraintHasOne (6666 CU)
ok   admin_recover_vault_lamports: a wSOL vault -> VaultLamportsViaSync (8335 CU)
```
