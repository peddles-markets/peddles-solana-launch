//! Peddles launch program. Specification: `docs/SOLANA_LAUNCH_PROGRAM.md`.
//!
//! One transaction creates the coin, its Meteora DAMM v2 pool against an approved quote, and a
//! permanently locked position owned by the launch's own PDA. Meteora has no swap hook, so a
//! permissionless crank does the EVM hook's job after the fact: it claims the position's fees in
//! the quote token and splits them. All arithmetic lives in the `peddles-fees` crate.
//!
//! No instruction writes a launch's terms after `launch`, transfers the position NFT, or calls
//! Meteora's `split_position*` or `remove_liquidity`. That holds while the code is what it is:
//! see `constants::UPGRADE_AUTHORITY_PLAN`.

#![allow(deprecated)]

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod meteora;
pub mod state;

use anchor_lang::prelude::*;
pub use instructions::*;

// PLACEHOLDER. Nobody holds a key for this address, and deploying to it is impossible by design.
// The real id is generated at deploy time and replaces this string and the one in Anchor.toml.
// No keypair is created or committed here.
declare_id!("4jTFPcqrwzxP9Ve7xo6dUk4oYWbfbi78qm63tX1JC4tL");

// Read by explorers and `query-security-txt`, from the binary itself.
#[cfg(not(feature = "no-entrypoint"))]
solana_security_txt::security_txt! {
    name: "Peddles",
    project_url: "https://peddles.xyz",
    contacts: "link:https://t.me/PeddlesEntry",
    policy: "https://docs.peddles.xyz",
    source_code: "https://github.com/peddles-markets/peddles-solana-launch"
}

#[program]
pub mod peddles_launch {
    use super::*;

    /// Create `Config`. Only the program's upgrade authority may call it, once.
    pub fn initialize(ctx: Context<Initialize>, params: InitializeParams) -> Result<()> {
        instructions::initialize::initialize_handler(ctx, params)
    }

    /// §3.2. Coin, metadata, pool, locked position, record, and the optional first buy at the
    /// ordinary rate, which is not a swap: the pool opens at the price the buy would reach.
    pub fn launch(ctx: Context<Launch>, params: LaunchParams) -> Result<()> {
        instructions::launch::launch_handler(ctx, params)
    }

    /// §3.3. Permissionless. Claims the position's fees and books the split of what arrived.
    pub fn crank_fees(ctx: Context<CrankFees>) -> Result<()> {
        instructions::crank_fees::crank_fees_handler(ctx)
    }

    /// §3.4. Permissionless. Pays `platform_owed` to the platform wallet's token account.
    pub fn sweep_platform(ctx: Context<SweepPlatform>) -> Result<()> {
        instructions::payouts::sweep_platform_handler(ctx)
    }

    /// §3.5. Permissionless. Pays `creator_owed` to the current payout address's token account.
    pub fn claim_creator(ctx: Context<ClaimCreator>) -> Result<()> {
        instructions::payouts::claim_creator_handler(ctx)
    }

    /// §3.5. Signed by the current payout address. The only mutable term.
    pub fn set_creator_payout(ctx: Context<SetCreatorPayout>, new_payout: Pubkey) -> Result<()> {
        instructions::payouts::set_creator_payout_handler(ctx, new_payout)
    }

    /// §3.6. Signed by the keeper. Effective at once; at most one per launch per 6 hours.
    pub fn publish_root(
        ctx: Context<PublishRoot>,
        epoch: u64,
        root: [u8; 32],
        cumulative_total: u64,
    ) -> Result<()> {
        instructions::holders::publish_root_handler(ctx, epoch, root, cumulative_total)
    }

    /// §3.6. Signed by the guardian: stops or resumes roots and holder claims on every launch.
    pub fn set_global_pause(ctx: Context<SetGlobalPause>, paused: bool) -> Result<()> {
        instructions::holders::set_global_pause_handler(ctx, paused)
    }

    /// §3.6. Signed by the guardian: the same stop for one launch.
    pub fn set_launch_pause(ctx: Context<SetLaunchPause>, paused: bool) -> Result<()> {
        instructions::holders::set_launch_pause_handler(ctx, paused)
    }

    /// §3.6. Signed by the holder.
    pub fn claim_holder(
        ctx: Context<ClaimHolder>,
        cumulative_amount: u64,
        proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        instructions::holders::claim_holder_handler(ctx, cumulative_amount, proof)
    }

    /// §3.8. Permissionless. Credits anything sent to the vault directly to holders.
    pub fn sync_vault(ctx: Context<SyncVault>) -> Result<()> {
        instructions::holders::sync_vault_handler(ctx)
    }

    /// §3.7.
    pub fn set_launch_fee(ctx: Context<AdminOnly>, lamports: u64) -> Result<()> {
        instructions::admin::set_launch_fee_handler(ctx, lamports)
    }

    /// §3.7.
    pub fn set_keeper(ctx: Context<AdminOnly>, keeper: Pubkey) -> Result<()> {
        instructions::admin::set_keeper_handler(ctx, keeper)
    }

    /// §3.7.
    pub fn set_guardian(ctx: Context<AdminOnly>, guardian: Pubkey) -> Result<()> {
        instructions::admin::set_guardian_handler(ctx, guardian)
    }

    /// §3.7. Where future platform legs go.
    pub fn set_platform_wallet(ctx: Context<AdminOnly>, wallet: Pubkey) -> Result<()> {
        instructions::admin::set_platform_wallet_handler(ctx, wallet)
    }

    /// §3.7, step one of the two-step transfer.
    pub fn propose_admin(ctx: Context<AdminOnly>, new_admin: Pubkey) -> Result<()> {
        instructions::admin::propose_admin_handler(ctx, new_admin)
    }

    /// §3.7, step two: signed by the proposed admin.
    pub fn accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
        instructions::admin::accept_admin_handler(ctx)
    }

    /// §3.7. Approve or disable a quote. Future launches only.
    pub fn set_quote(ctx: Context<SetQuote>, params: SetQuoteParams) -> Result<()> {
        instructions::admin::set_quote_handler(ctx, params)
    }

    /// §3.7. Stray tokens held by one of our PDAs, to a token account named in the call. Refuses the
    /// fee vault, which `admin_recover` handles, and the position NFT, which is never touched.
    pub fn rescue_foreign_token(
        ctx: Context<RescueForeignToken>,
        owner: RescueOwner,
    ) -> Result<()> {
        instructions::rescue::rescue_foreign_token_handler(ctx, owner)
    }

    /// §3.9. Signed by the admin. Any amount of a launch's vault, by bucket, to a token account named
    /// in the call, in every state, paused included. The ledger comes down by what left.
    pub fn admin_recover(
        ctx: Context<AdminRecover>,
        bucket: Bucket,
        amount: u64,
    ) -> Result<()> {
        instructions::recover::admin_recover_handler(ctx, bucket, amount)
    }

    /// §3.9. Signed by the admin. Lamports above rent on an account this program owns, to an address
    /// named in the call.
    pub fn admin_recover_lamports(ctx: Context<AdminRecoverLamports>) -> Result<()> {
        instructions::recover::admin_recover_lamports_handler(ctx)
    }

    /// §3.9. Signed by the admin. Lamports above rent on a Token-2022 `FeeVault`, to an address named
    /// in the call.
    pub fn admin_recover_vault_lamports(ctx: Context<AdminRecoverVaultLamports>) -> Result<()> {
        instructions::recover::admin_recover_vault_lamports_handler(ctx)
    }

    /// §3.7. Permissionless. Lamports above rent on an account this program owns.
    pub fn sweep_excess_lamports(ctx: Context<SweepExcessLamports>) -> Result<()> {
        instructions::rescue::sweep_excess_lamports_handler(ctx)
    }

    /// §3.8. Permissionless. Lamports above rent on a Token-2022 `FeeVault`.
    pub fn sweep_vault_lamports(ctx: Context<SweepVaultLamports>) -> Result<()> {
        instructions::rescue::sweep_vault_lamports_handler(ctx)
    }
}
