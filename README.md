# Peddles Solana launch program

**Status: compiled, not deployed.** On 2026-11-06, in the pinned image, `cargo test -p peddles-fees`
passed 48 of 48 and `anchor build --ignore-keys` built `peddles_launch.so` (606,800 B) and its IDL.
Nothing has run on a validator or on chain. The Solana launchpad stays COMING SOON until it has. The specification is
`docs/SOLANA_LAUNCH_PROGRAM.md`, with `docs/SOLANA_LAUNCH.md` for the decisions.

This is an Anchor workspace on its own. It is not part of the pnpm workspace, and no service builds
from it.

| Path | What it is |
| --- | --- |
| `programs/peddles-launch/` | The program. `meteora.rs` holds the hand-built CPIs; `instructions/` holds one file per group of instructions |
| `crates/peddles-fees/` | ALL the arithmetic: no dependencies, `no_std`, testable without a validator |
| `crates/peddles-fees/tests/fixtures/` | Real bytes read from mainnet on 2026-11-06: the TSLAx and PreStocks ANTHROPIC mints (slot 466,192,573) and the instruction data of `56ybV1pr…AfaFW` |
| `tests/PLAN.md` | What needs a validator or mainnet, and is not written yet |

## Versions

Pinned from release pages and source read on 2026-11-06, not from memory.

| Tool | Pin | Read from |
| --- | --- | --- |
| Anchor | **1.2.2** | `solana-foundation/anchor` latest 1.x release (2026-10-28); crates.io has `anchor-lang` and `anchor-spl` 1.2.2 |
| Solana CLI (Agave) | **4.1.2** | Anchor's 1.2.2 release notes: "The recommended Solana version is 4.1.2, with platform-tools v1.57 and SBPF architecture v3"; `docker/Makefile` sets `SOLANA_CLI=v4.1.2` |
| platform-tools | **v1.57** | `DEFAULT_TOOLS_VERSION` in Anchor's `cli/src/lib.rs`; its release notes say Rust 1.95.0, LLVM 22 |
| Rust (host, `cargo test`) | **1.95.0** | The same Rust as platform-tools v1.57, and what damm-v2 pins in `rust-toolchain.toml` |
| SBPF | **v3** (Anchor's default) | Feature `5cC3foj77CWun58pC51ebHFUWavHWKarWyR5UUik7dnC` read active on mainnet since slot 428,976,000 (read at slot 466,187,308) |
| Docker image | `quay.io/ottersec/anchor@sha256:890d130d703ae9fef27e14bd1e32c70368feb43287b6250da3f08c30e8ea3a0d` | Quay tag `v1.2.2` (multi-arch, published 2026-10-28), built from Anchor's `docker/build/Dockerfile`. Docker Hub's `solanafoundation/anchor` stops at v1.0.2 |

They are mutually compatible by Anchor's own statement above. Damm-v2 at `3493cd6` is one patch
behind on the same line: Anchor 1.2.1, Solana 4.1.2, Rust 1.95.0. Newer releases exist and are
deliberately not used: Agave 4.4.1 (mainnet RPC reported 4.4.0), platform-tools v1.58, Rust 1.99.1.

Two choices worth knowing:

- **Cohort.** Anchor 1.2.2 builds against either the `solana-v3` or the `solana-v4` crate set. This
  workspace stays on `solana-v3`, Anchor's default. Damm-v2 uses `solana-v4`; the two talk over CPI
  without caring.
- **SBPF.** Meteora's deployed program is SBPF v0 (ELF `e_flags` 0, read at slot 466,187,237). A
  program built v3 can call it; CPI does not depend on the caller's version. To match it anyway,
  set `ANCHOR_BUILD_SBF_ARCH=v0`.

`anchor_lang::solana_program` has no hash module in 1.2.2, so `solana-sha256-hasher` 3.1.0 is a
direct dependency for the SHA-256 syscall. There is no `Cargo.lock` yet; the first build makes one,
and it should be committed.

## Build, in Docker

Nothing is installed on the host. From the repo root, in Git Bash:

```bash
export MSYS_NO_PATHCONV=1
IMAGE=quay.io/ottersec/anchor@sha256:890d130d703ae9fef27e14bd1e32c70368feb43287b6250da3f08c30e8ea3a0d

# The arithmetic: no validator, seconds to run once compiled. rustup fetches 1.95.0 on first use.
docker run --rm -v "$(pwd -W)/solana:/workdir" -w /workdir "$IMAGE" \
  bash -lc 'cargo test -p peddles-fees'

# The program, without the IDL first: it is the smaller failure surface.
docker run --rm -v "$(pwd -W)/solana:/workdir" -w /workdir "$IMAGE" \
  bash -lc 'anchor build --ignore-keys --no-idl'

# Then with the IDL.
docker run --rm -v "$(pwd -W)/solana:/workdir" -w /workdir "$IMAGE" \
  bash -lc 'anchor build --ignore-keys'
```

Watch the SBF build's output for `Stack offset of ... exceeded max offset of 4096`. `Launch` has 27
accounts, so its `try_accounts` is the likeliest place. Every large account is already boxed.

**`anchor build` writes a throwaway `target/deploy/peddles_launch-keypair.json`.** It is inside the
ignored `target/`, it means nothing while the id is a placeholder, and it must not be used to deploy.

## Live proof

`tests/live.ts` drives every instruction from the built IDL, against real Meteora. It refuses mainnet
by genesis hash before every send, and keeps its keys in `--keys`, outside the repo, never printed.

```bash
node tests/live.ts --cluster devnet --rpc https://api.devnet.solana.com --program <id> \
  --idl target/idl/peddles_launch.json --keys <scratch dir> --out tests/LIVE_RESULTS.md
```

- **Devnet is the only public cluster with Meteora.** `testnet` has no account at its address (read
  2026-11-06). Devnet's binary is not mainnet's, and neither is exactly commit `3493cd6`. Devnet's RPC
  faucet is closed, so the payer has to be funded by hand: about 5 SOL.
- **`--cluster local`** runs on a local validator with Meteora, Metaplex and Token-2022 cloned from
  mainnet, and the results say it is not a public-cluster proof. Agave 4.1.2's validator asserts
  `io_uring`, which Docker's default seccomp profile blocks, so the local run uses Agave 3.0.14's
  validator, which does not. That version rejects an SBPF v3 program, so the local build sets
  `ANCHOR_BUILD_SBF_ARCH=v0`. A devnet run uses the default v3 build.
- **A second root on one launch needs 6 real hours**: the script publishes one root on each of several
  launches, and refuses a second inside the window.

## Verified builds

- `security.txt` is in the binary (`solana-security-txt` 1.1.3).
- `Cargo.toml` names Solana 4.1.2 for `solana-verify`, and `Cargo.lock` is in the workspace.
- **The pinned image does not qualify on its own.** `quay.io/ottersec/anchor` builds this repo, but an
  explorer's verified badge compares against `solana-verify build`, which uses
  `solanafoundation/solana-verifiable-build:4.1.2`. That tag exists (read 2026-11-07). The binary
  deployed must be the one that build produces, not `anchor build`'s.
- **The source must be public.** `solana-verify verify-from-repo` clones a public repo at a commit, and
  `peddles-markets/peddles-launchpad` is private, so the `source_code` link in `security.txt` does not
  open for an outsider today. It needs the repo opened, or `solana/` published as its own public repo,
  plus the upgrade authority uploading the build data.
- `scripts/verifiable-build.sh` ran on 2026-11-08 with a throwaway devnet id: 643,744 B, the same
  bytes on two builds, deployed to devnet and passed `tests/live.ts` 134 of 134
  (`tests/LIVE_RESULTS_VERIFIABLE.md`; `docs/SOLANA_LAUNCH.md` 13.6). `solana-verify` itself was not run.
  Compare the chain with the script's `verify_hash`, never its plain `sha256`.

## Scripts (mainnet runbook: `docs/SOLANA_LAUNCH.md` §13)

| Script | What it does | Holds a key |
| --- | --- | --- |
| `scripts/verifiable-build.sh` | The deployable binary, built as `solana-verify build --library-name peddles_launch --arch v3` builds it (image pinned by digest) | no |
| `scripts/export-public.mjs <out>` | Copies only what `verify-from-repo` needs into a standalone tree for a public repo, with a manifest; refuses the placeholder id, a mismatched `source_code`, or anything that looks like a key. Pushes nothing | no |
| `scripts/quote-bands.ts` | Reads every offered quote on mainnet (mint, multiplier, badge) and Jupiter's price, and writes the `set_quote` bands for a $3,500 opening | no |
| `scripts/admin.ts` | `initialize`, `set_quote`, fees, roles, pauses, `admin_recover`: simulates by default, sends with `--send` and a key file (mainnet also needs `PEDDLES_SOLANA_ALLOW_MAINNET=true`), and prints Squads instructions | the owner's, at run time |

`quote-bands.ts` and `admin.ts` import from `apps/solana-keeper`, so they run in this monorepo only and
are not exported.

## Program id

`declare_id!` and `Anchor.toml` carry the placeholder `PEDDLESaunchProgramP1aceho1derDoNotDep1oy11`.
It is a valid 32-byte address that nobody holds a key for. No keypair is created or committed here.
At deploy time a real key is generated offline and its address replaces both strings.

## Decisions still open

Each has one name in `programs/peddles-launch/src/constants.rs`, carrying the design's
recommendation. They need the owner.

| Constant | Carries | Question |
| --- | --- | --- |
| `UPGRADE_AUTHORITY_PLAN` | Squads multisig until an audit | Kept by Squads, or immutable after audit (§5.4). While upgradeable, "fixed at launch" is only as strong as that multisig |
| `UNCLAIMED_HOLDER_REWARDS` | Owed forever | Or recycled after a window, which needs per-epoch roots and so a different leaf format. Decide before the first mainnet root |
| `COIN_DECIMALS` | 6 | 6 or 9 (§9) |
| `MAX_LAUNCH_FEE_LAMPORTS` | 0.25 SOL | The design names a cap, not a number. This one is a proposal |
| `MAX_FIRST_BUY_BPS_OF_SUPPLY` | 10,000, no limit | Owner, 2026-11-06: no size limit, as on EVM |

`ROOT_MIN_INTERVAL_SECONDS` is decided: 6 hours, owner, 2026-11-06. There is no veto delay.
Time-weighting (§3.6) lives in `apps/solana-keeper`, not here; the owner accepted it on 2026-11-06.

## Where the source relies on something unverified

From the design's "could not establish" items, and what building it added. **Proven live on devnet
since this table was written (`tests/LIVE_RESULTS.md`, against devnet's Meteora):** PDA mints, Metaplex
with six accounts, the empty seed, `sqrt_price > sqrt_min_price` and later swaps, the NFT account in
`token_a_account`, the stack, the U256 division (launch 236–268k CU, 885 bytes with the table), and a
keeper root and claim. Still open from it: Metaplex's protocol fee amount, clock drift, Meteora's
written position, issuer-confirmed mints, and everything on mainnet's own Meteora.

| Assumption | Where it is relied on | Design |
| --- | --- | --- |
| A PDA-signed `position_nft_mint` works: Meteora's `init, signer` accepts it through CPI | `launch.rs`, `POSITION_NFT_SEED` | §4.4 "expected, not yet tested" |
| A PDA coin mint is accepted by Metaplex | `launch.rs`, `COIN_SEED` | §3.2 "Test that…" |
| The system program and Anchor accept an EMPTY seed in `[b"coin", "", salt]`, so an unbound mint's address is the same as `[b"coin", salt]` | `launch.rs`, `coin_binding_seed` | new |
| The IDL builder tolerates a function call inside `seeds` | `launch.rs`. If not, the account's `pda` is left out of the IDL or a `#[cfg]` seed helper is needed; `--no-idl` is unaffected | new |
| Meteora accepts `sqrt_price` above `sqrt_min_price` on a customizable pool, with a real `token_b_amount` | `launch.rs`. Read in source (`validate_initial_sqrt_price`) and live pools show `sqrt_price != sqrt_min_price`, but never yet driven by this program | new |
| Meteora's later swaps behave as if the first buy had been a swap: same `L`, same curve | the first buy's economics. Read from source only | new |
| The position NFT account works as Meteora's `token_a_account`: Anchor's duplicate check only rejects two `mut` fields, and Meteora has no mint check on that slot | `crank_fees.rs`, `meteora.rs`. The deployed Meteora binary may differ from commit `3493cd6` | new |
| A 256-step U256 division is affordable on SBF, once per launch with a first buy | `price::sqrt_price_after_buy`. Removing `swap2` and the claim frees 63–93k CU by the design's estimate; the division is unmeasured | new |
| `CreateMetadataAccountV3` works with six accounts and no `rent`, as the generated client implies | `meteora.rs` | not read in Metaplex's processor |
| Metaplex's protocol fee on metadata creation: its amount, and how it is collected | `launch.rs` pays nothing explicitly; the creator's lamports would cover it | §7 UNVERIFIED |
| Transaction size: fits only as v0 with a lookup table | the client, not this source. `Launch` has 27 accounts, 29 in the transaction with our program and the compute-budget program, in line with the design's 29–30 | §6 ESTIMATE |
| Compute: was 240–340k CU with a swap; lower now, not re-measured | the client's compute budget | §6 ESTIMATE |
| Stack: `Launch::try_accounts` fits in 4 KB | `launch.rs` | new |
| Token-2022's `WithdrawExcessLamports` accepts the account owner as authority | `rescue.rs`, `sweep_vault_lamports` | new: read from the interface, processor not read |
| `Clock::unix_timestamp` drift against wall-clock time | the 180 s schedule; `activation_point` | §5.5 UNVERIFIED |
| Meteora keeps today's constants and account layouts | everything in `meteora.rs` and `peddles_fees::ix_data`. Meteora is upgradeable: re-read before a mainnet deploy | §5.4 |
| Meteora's written position on `update_pool_fees` and `set_pool_status` | the launch form's wording; nothing here can stop either | §5.1, §9 |
| Issuer-confirmed mints, and each badge staying in place | `QuoteConfig`, set by the admin per mint | `SOLANA_LAUNCH.md` §4, §5 |
| Who controls Meteora's upgrade authority | the same trust as the line above | §5.4 |
| Farming rewards an operator might add to a pool would accrue to the locked position; this program has no `claim_reward` path | not ours, but not exitable either | new |

## Merkle tree, for the keeper

Rules in `crates/peddles-fees/src/merkle.rs`, with fixed test vectors in `merkle_known_answers`:

```
leaf = sha256(0x00 || launch (32) || holder (32) || cumulative_amount (u64 LE))
node = sha256(0x01 || min(a, b) || max(a, b))
```

## Differences from the design

- **The creator's first buy is not a swap** (owner, 2026-11-06; supersedes the rebate). `L` is fixed by
  the supply at the opening price `√P0`. The pool opens at `√P1 = √P0 + ⌊net × 2^128 / L⌋`, Meteora
  pulls both sides, and the coins it does not pull stay with the creator.
- **The first-buy fee splits as 0.50% + 0.50% of VOLUME**, `split_uncut_fee`, not by `split_claim`'s
  fractions, which assume Meteora already took 20% and would pay each fixed leg 0.625% here. The
  20% Meteora would have kept on a swap goes to the platform (owner, 2026-11-06), so at 1.25% the
  platform takes 0.75% of the buy and the creator 0.50%. Anything above that follows `x`.
- **Exact supply is proved before the CPI and read after it.** `liquidity_deposits_exactly` proves
  `L` deposits exactly the supply at `√P0`, with multiplications only. After the CPI the creator's
  coin balance is what the pool did not take: 0 with no first buy.
- **`QuoteConfig` carries an opening-price band**, checked against `√P0`, never `√P1`. §3.2 takes
  `sqrt_price` from the creator, which would let a launch open at any market cap. The $3.5K rule
  needs the band; the admin sets it per quote.
- **Two coin mint forms:** unbound `[b"coin", salt]` and bound `[b"coin", creator, salt]`, chosen by
  `bound_to_creator`.
- **No veto window** (owner, 2026-11-06). A root is claimable at once, at most one per launch per
  6 hours. The guardian has an instant pause, for one launch or all, and on chain the keeper and
  guardian must differ. `veto_root` and all pending-root state are gone.
- **A root must raise the total.** Added: nothing can be rewritten while nothing arrives.
- **`crank_fees` takes no coin account.** The launch's own NFT account fills Meteora's unused slot.
- **`LaunchRecord` is 455 bytes, not ~400.** About 0.00028 SOL more per launch than §7's estimate.
- **`sweep_vault_lamports` is new.** A `FeeVault` belongs to the token program, so
  `sweep_excess_lamports` cannot reach it. Quotes on plain SPL Token are therefore limited to wSOL.
- **`rescue_foreign_token` takes a `RescueOwner`** and pays a token account named in the call, so it
  covers every PDA kind, and refuses the `FeeVault` and the position NFT by address and by mint.
- **Admin recovery is new** (owner, 2026-11-06): `admin_recover`, `admin_recover_lamports`,
  `admin_recover_vault_lamports`. `holder_recovered` keeps the ledger exact, a claim pays partially
  over a shortfall, and every payout pays what the vault holds rather than reverting. Proof table and
  what recovery cannot reach: `docs/SOLANA_LAUNCH_PROGRAM.md` §3.9.
- **A verified creator is new** (owner, 2026-11-06): the PDA `[b"creator"]` is the Metaplex update
  authority and verified creator on every launch. `Launch` has 28 accounts; it goes in the lookup table.
- **`set_platform_wallet` is new**: a lost wallet would otherwise keep receiving platform legs.

## Holder roots: what is and is not protected

A merkle root's leaves cannot be checked on chain. **Accepted by the owner, 2026-11-06: with no veto,
a stolen keeper key can immediately direct all unclaimed holder rewards to itself**, including ones
published earlier and not yet claimed. Only three things limit it: the total can rise only by what
arrived since the last root and must rise, one root per 6 hours, and how fast the guardian pauses.
It cannot reach the platform's or creator's legs. `a_bad_root_can_take_everything_unclaimed_but_no_more`
states this as a test.

Per-root tranches were assessed: they would limit a bad root to new money, but at four roots a day
they cost about 0.005 SOL a day per launch in new accounts and one claim per root per holder.
Keeping an old root valid alongside the new one does not help on its own: the vault can still be
drained through the new root first.

## Pre-Launch grind

Expect about 22.6 million attempts: 58^4 for the suffix, doubled because the program derives the
canonical bump, and half of the bump-255 hashes that hit the suffix land on the curve. Each attempt
is one SHA-256 of 98 bytes for the bound form. Measured on this PC, plain JavaScript on one thread,
under load, 2026-11-06: 192,000 a second, so about **2 minutes on one thread and 30 seconds on four
workers on average, with 1 run in 20 past 90 seconds at four workers**. The time is geometric, so
there is no upper bound. WASM would be faster; a phone slower. Neither was measured.
