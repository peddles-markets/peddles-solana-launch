# Test plan

Status: the program compiles and the fees crate's tests pass. `tests/live.ts` is the runnable live
proof: every instruction, at least three scenarios, state read before and after, signatures read back
from the chain. Results: `LIVE_RESULTS.md` (devnet) and `LIVE_RESULTS_LOCAL.md` (labelled local).
Besides the main run, `--phase second-root` publishes a second root on one launch at least 6 hours
later, `--phase top-up` adds further successes on the saved launches and restores every role, and
`--phase initialize-instance --label <text> --roles <admin,keeper,guardian> --fee <lamports>`
initializes a throwaway deployment.

The fees crate's tests live in `crates/peddles-fees/tests/properties.rs`. This file is the plan for
what needs a validator or mainnet. It follows `docs/SOLANA_LAUNCH_PROGRAM.md` §8 and
`docs/SOLANA_LAUNCH.md` §7.

## 1. Unit and property: written

`cargo test -p peddles-fees`. No validator, no network.

| §8 property | Test |
| --- | --- |
| Legs sum to the claim, none negative, u128 intermediates | `split_holds_exhaustively_for_small_claims`, `split_holds_at_every_power_of_two_edge`, `split_holds_for_random_terms` |
| The 1.25% minimum around rounding edges | `minimum_is_exactly_125_bps`, `meteora_rounding_favours_the_position` |
| Rounding never favours the platform over holders by more than 1 unit | `check_split`, run by all three split tests |
| `R = 125`: holders get `C mod 2` | `minimum_is_exactly_125_bps` |
| Scheduler at 0/60/120/179/180 s, every rate, against a port of Meteora's function | `schedule_table_2_2`, `schedule_holds_for_every_rate` |
| `liquidity` → `token_a_amount == supply`, both decimals | `liquidity_deposits_exactly_the_supply_for_every_decimals_choice` |
| Transfer-fee quote: ledger equals the vault after every crank and payout, fee changes between cranks | `ledger_always_equals_the_vault` |
| Crank griefing: sums stay exact, dust only to holders | `many_tiny_cranks_stay_exact_and_dust_goes_to_holders` |
| Merkle: double claim, forged leaf, other holder, other launch, inner node as leaf | `merkle_verifies_and_refuses_forgeries`, `merkle_double_claim_pays_nothing` |
| Roots: above funded, decreasing or unchanged total, stale epoch, the 6-hour cadence | `roots_refuse_bad_publishes`, `four_roots_a_day_is_the_most_a_launch_can_publish` |
| The accepted risk, as a test: a bad root takes all unclaimed, and nothing else | `a_bad_root_can_take_everything_unclaimed_but_no_more` |
| First buy: pool + creator is exactly the supply, Meteora pulls no more than was paid, within 1 unit of a swap | `every_unit_of_the_supply_is_accounted_for`, `first_buy_worked_example` |
| First buy: a thin `L` is refused, and the multiplication check agrees with Meteora's division | `liquidity_must_be_exact` |
| First-buy fee: 0.50% + 0.50% of volume, Meteora's would-be 20% to the platform, on what arrived | `an_uncut_fee_pays_the_venue_share_to_the_platform`, `a_first_buy_fee_with_a_transfer_fee_splits_what_arrived` |
| Both coin seed forms | `both_coin_seed_forms` |
| Refused mints (paused, default frozen, hook set), on real mint bytes | `a_paused_mint_is_refused`, `a_default_frozen_mint_is_refused`, `a_mint_with_a_hook_is_refused` |
| Encodings equal real mainnet instruction bytes | `initialize_pool_data_matches_the_measured_transaction`, `permanent_lock_data_matches_the_measured_transaction` |

**A fixture is not a live check.** The mint and instruction fixtures prove the parser and encoders
against bytes the chain really produced. They prove nothing about the program deployed against them.

## 2. Adversarial: to write, on a local validator

Clone Meteora, Metaplex, Token-2022 and the stock mints from mainnet:

```
solana-test-validator --reset \
  --clone-upgradeable-program cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG \
  --clone-upgradeable-program metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s \
  --clone XsDoVfqeBukxuZHWhdvWHBhgEHjGNst4MLodqsJHzoB \
  --clone Pren1FvFX6J3E4kXhJuCiAD5aDmGEb7qJRncwA8Lkhw \
  --url https://api.mainnet-beta.solana.com
```

The creator also needs a raw unit of a stock it cannot mint, so a test either clones a funded token
account with an overridden owner, or uses a local Token-2022 mint with the same extensions. The
badge accounts are cloned the same way (§4.5).

**`launch`**
- Wrong quote mint, disabled `QuoteConfig`, no `QuoteConfig`: refused.
- A mint with no badge, a mint with a hook set, a paused mint, default-frozen: refused.
- `rate_bps` 124 and 501, `excess_to_creator_bps` 10,001: refused.
- A salt whose PDA does not end in `PEDL`: `CoinSuffix`.
- `sqrt_price` outside the quote's band: `OpeningPriceOutOfBand`. A first buy large enough to take
  `√P1` past the band's top still launches, because the band reads `√P0`.
- `liquidity` that deposits one unit more or less than the supply, or a thin `L`: `LiquidityNotExact`.
- The same salt twice: the mint's `init` fails. A front-runner copying an unbound salt wins that
  mint only.
- Bound form: another signer passing the same salt derives a different address, which fails
  `CoinSuffix` or simply is not that mint. An unbound call with a bound salt derives another address.
- An empty seed: the unbound address equals `[b"coin", salt]` as the API's pool derives it.
- Pool squat: creating a `cpool` for the coin before or within the launch is impossible (the mint
  does not exist). A config pool afterwards is possible and the record ignores it.
- A wrong Meteora or Metaplex program id, event authority, pool authority or token program.
- A first buy with `first_buy_min_coins` too high: the whole launch reverts.
- A first buy too small to move the price: `FirstBuyTooSmall`. One past the range: `FirstBuyTooLarge`.
- The creator's coin balance equals `first_buy_opening`'s prediction to the unit; a sell straight
  after returns the pool to `√P0`, within rounding; Meteora's reserves can pay every seller.
- A first buy on PreStocks: the pool receives `pool_quote`, the creator pays the transfer fee on top,
  the fee leg splits what arrived, the ledger equals the vault.
- After the launch: the mint's mint and freeze authorities are null, supply is exactly
  `COIN_SUPPLY_RAW`, the creator's coin balance is only the first buy, `pool.creator` and the NFT
  account's owner are the record, `permanent_locked_liquidity` is the whole position, metadata is
  immutable.

**`crank_fees`**
- Another launch's pool, position, vault or NFT account: refused by `address`.
- No coin account is a parameter: the NFT account fills Meteora's slot. Prove it on the real Meteora
  binary, that no user account is write-locked, and that token A fees stay 0.
- A vault that does not belong to the record.
- A claim with a frozen vault: reverts, nothing is lost, retry after thaw.

**Payouts**
- `sweep_platform` to anything but the derived account: there is no such parameter; passing a
  different account fails Anchor's `associated_token` check.
- `claim_creator` after `set_creator_payout`: pays the new address.
- `set_creator_payout` signed by the old payout, or by the creator after handing it over: refused.
- A frozen platform ATA blocks `sweep_platform` only: `claim_creator` and `claim_holder` still work.

**Holders**
- Every merkle case of §1 again, through the instruction, plus a `HolderClaim` of another holder.
- A root claimable in the slot it was published; a second root inside 6 hours refused.
- `set_global_pause` and `set_launch_pause`: roots and holder claims refused, cranks and the platform
  and creator payouts still work, unpausing restores them. Signed by anyone but the guardian: refused.
- `initialize`, `set_keeper` and `set_guardian` with keeper == guardian: refused.

**Admin and terms**
- For every instruction × every `LaunchRecord` field: only `creator_payout`, the ledger fields and the
  root fields ever change after `launch`. A snapshot-and-diff harness, not a list of single tests.
- `set_launch_fee` above `MAX_LAUNCH_FEE_LAMPORTS`: refused.
- `initialize` from anyone but the upgrade authority; `initialize` twice.
- `accept_admin` by anyone but the proposed admin.
- `set_quote` for a plain SPL Token mint other than wSOL: refused.

**No stuck funds, §3.8, one exit per row, shown**
- `rescue_foreign_token` on the position NFT account: `ProtectedAccount`. On the `FeeVault`:
  `ProtectedAccount`. With a mismatched `RescueOwner`: `RescueSeedsMismatch`. From a non-admin: refused.
- A stray mint in an ATA of each PDA kind reaches the platform wallet.
- Lamports forced into `Config`, `QuoteConfig`, `LaunchRecord` and `HolderClaim`:
  `sweep_excess_lamports` moves only the excess, and the account stays rent-exempt.
- Lamports forced into a Token-2022 `FeeVault`: `sweep_vault_lamports`. Into a wSOL vault: `sync_vault`.
- `sweep_excess_lamports` on an account this program does not own: `NotProgramOwned`.
- Tokens sent straight to the `FeeVault`: `sync_vault` credits holders, and a following `crank_fees`
  is unaffected.

**Reentrancy:** a malicious token program or hook passed in place of the real ones is refused by the
`Interface` and `address` checks; our coin has no hook.

## 3. Live: mainnet, small amounts

Devnet has no stock mints. Each step records the signature, slot and the state read back, never only
"it did not revert". Do not take a hash from anywhere but the chain.

1. Launch with each candidate stock as token B. Read back `collect_fee_mode = 1`, `activation_type = 1`,
   the scheduler bytes, `pool.creator`, the NFT account's owner, `permanent_locked_liquidity`, the
   mint's authorities and supply, and the transaction's bytes and compute units.
2. Swaps at measured seconds after activation: the fee charged against §2.2 on buys and sells;
   Jupiter's quote shows the decayed fee; the split measured including Meteora's 20%. Record the swap
   timestamps, because clock drift was not established (§5.5).
3. `crank_fees` signed by our PDA, fees arriving in the stock, then `sweep_platform`, `claim_creator`
   and one `publish_root` → `claim_holder`: the exit for every bucket, shown on chain.
4. Jupiter's indexing delay for a new pool, visibility on the terminals, pricing under the scaled-UI
   multiplier.
