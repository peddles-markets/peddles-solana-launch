//! Fixed addresses, seeds and the decisions that are still the owner's.
//!
//! Every address here was read, not recalled: the Meteora and Metaplex ids from their source and
//! from mainnet (`docs/SOLANA_LAUNCH_PROGRAM.md` §4, §10), the wrapped-SOL mint from mainnet on
//! 2026-11-06 (owner: Token program, 82 bytes).

use anchor_lang::prelude::*;

/// Meteora DAMM v2 (`cp-amm`). The only pool program this program will ever call.
pub const METEORA_DAMM_V2: Pubkey = pubkey!("cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG");
/// Meteora's `pool_authority` PDA (`const_pda.rs`).
pub const METEORA_POOL_AUTHORITY: Pubkey = pubkey!("HLnpSz9h2S4hiLQ43rnSD9XkcUThA7B8hQMKmDaiTLcC");
/// Meteora's `__event_authority` PDA, appended by `#[event_cpi]`.
pub const METEORA_EVENT_AUTHORITY: Pubkey = pubkey!("3rmHSu74h1ZcmAisVcWerTCiRDQbUrBKmcwptYGjHfet");
/// Metaplex Token Metadata.
pub const METAPLEX_TOKEN_METADATA: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");
/// Wrapped SOL. The only quote allowed on the plain SPL Token program.
pub const WSOL_MINT: Pubkey = pubkey!("So11111111111111111111111111111111111111112");

pub const CONFIG_SEED: &[u8] = b"config";
pub const QUOTE_SEED: &[u8] = b"quote";
pub const LAUNCH_SEED: &[u8] = b"launch";
pub const FEE_VAULT_SEED: &[u8] = b"fee_vault";
pub const CLAIM_SEED: &[u8] = b"claim";
pub const COIN_SEED: &[u8] = b"coin";
/// The position NFT mint is our PDA, so a launch needs no second throwaway key (§4.4).
pub const POSITION_NFT_SEED: &[u8] = b"position_nft";
/// One address for every launch: the Metaplex update authority and verified creator. Only this
/// program can sign for it, so it is what proves a coin came from here (`verifyPeddlesMint`).
pub const CREATOR_SEED: &[u8] = b"creator";
/// Metaplex's metadata PDA prefix.
pub const METADATA_SEED: &[u8] = b"metadata";

/// OWNER DECISION, §9. 6 matches the live launched coin `3E9yJ…CH1K`; the design leaves 6 or 9 open.
/// Changing it changes `COIN_SUPPLY_RAW` with it, and nothing else.
pub const COIN_DECIMALS: u8 = 6;
/// 1,000,000,000 whole coins, in raw units: 1e9 × 10^COIN_DECIMALS.
pub const COIN_SUPPLY_RAW: u64 = 1_000_000_000 * 10u64.pow(COIN_DECIMALS as u32);

/// The immutable launch-fee cap, the EVM's `maxLaunchFee`: raising it needs a new program.
/// 0.25 SOL, about 11x the $2 fee at the $89.13 SOL read on 2026-11-05 (0.02244 SOL).
/// THE VALUE IS A PROPOSAL: the design names the cap but not the number. Owner to confirm.
pub const MAX_LAUNCH_FEE_LAMPORTS: u64 = 250_000_000;

/// Roots are published at most this often per launch: four payouts a day. Owner, 2026-11-06:
/// "payout 4 times a day no safety windows". A constant, so no key can raise the cadence.
pub const ROOT_MIN_INTERVAL_SECONDS: i64 = 21_600;

/// The largest first buy, as a share of the supply. 10,000 is NO LIMIT, the EVM's rule: its dev buy
/// is uncapped (`PeddlesFeeHook._effectiveTaxBps`), and the design sets none. Lowering this is the
/// only change a cap needs. A proposal is in README.md; the owner has not chosen one.
pub const MAX_FIRST_BUY_BPS_OF_SUPPLY: u64 = 10_000;

/// STILL UNDECIDED BY THE OWNER, §0 item 3 and §5.4: who holds this program's upgrade authority.
///
/// Nothing on chain reads this; it exists so the decision has one name and shows up in review.
/// The recommendation carried here is the design's (`SOLANA_LAUNCH.md` §5.3: the Squads multisig
/// takes it). While it holds, "fixed at launch" is only as strong as that multisig, and the launch
/// form's wording must say so. Before the first mainnet launch, set this, and transfer the
/// authority to match, with `solana program set-upgrade-authority`.
pub const UPGRADE_AUTHORITY_PLAN: UpgradeAuthorityPlan =
    UpgradeAuthorityPlan::SquadsMultisigUntilAudit;

pub enum UpgradeAuthorityPlan {
    /// Held by the Squads multisig; may be made immutable after an audit.
    SquadsMultisigUntilAudit,
    /// `--final`: no further upgrades, ever.
    ImmutableAfterAudit,
}

/// STILL UNDECIDED BY THE OWNER, §0 item 4 and §3.6: what happens to holder rewards nobody claims.
///
/// `OwedForever` is what the code does today: a cumulative root keeps every entitlement claimable
/// with no expiry, until the admin recovers it (`admin_recover`, owner, 2026-11-06). `RecycleAfterWindow` is not built: it would
/// need per-epoch roots instead of one cumulative root, which changes the leaf format, so it must
/// be decided before the first root is published on mainnet.
pub const UNCLAIMED_HOLDER_REWARDS: UnclaimedHolderRewards = UnclaimedHolderRewards::OwedForever;

pub enum UnclaimedHolderRewards {
    OwedForever,
    RecycleAfterWindow,
}
