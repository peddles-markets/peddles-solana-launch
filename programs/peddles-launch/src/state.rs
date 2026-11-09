//! The accounts of §3.1.

use anchor_lang::prelude::*;
use peddles_fees::ledger::Ledger;
use peddles_fees::roots::RootState;

/// `[b"config"]`. The one set of roles and the launch fee.
#[account]
#[derive(InitSpace)]
pub struct Config {
    /// The Squads multisig.
    pub admin: Pubkey,
    /// Two-step transfer: `Pubkey::default()` when nothing is proposed.
    pub pending_admin: Pubkey,
    /// Publishes holder roots.
    pub keeper: Pubkey,
    /// Can stop publishing and claims at once. Never the same key as `keeper`.
    pub guardian: Pubkey,
    /// The admin can move it: `set_platform_wallet`.
    pub platform_wallet: Pubkey,
    pub launch_fee_lamports: u64,
    /// The guardian's stop for every launch: no roots, no holder claims.
    pub paused: bool,
    pub bump: u8,
}

/// `[b"quote", stock_mint]`. One approved quote. Affects future launches only.
#[account]
#[derive(InitSpace)]
pub struct QuoteConfig {
    pub mint: Pubkey,
    pub token_program: Pubkey,
    pub decimals: u8,
    /// Meteora's `[b"token_badge", mint]`, or `Pubkey::default()` for a mint that needs none.
    pub badge: Pubkey,
    pub enabled: bool,
    /// The band a launch's opening `sqrt_price` must sit in. This is what holds the $3.5K opening
    /// market cap on chain; the admin retunes it as the quote's dollar price moves. Addition to §3.1.
    pub min_sqrt_price: u128,
    pub max_sqrt_price: u128,
    pub bump: u8,
}

/// `[b"launch", coin_mint]`. The launch's terms, its ledger and its root state.
/// Owns the position NFT and is the `FeeVault`'s authority.
#[account]
#[derive(InitSpace)]
pub struct LaunchRecord {
    // Write-once terms. No instruction writes these after `launch`.
    pub coin_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub quote_token_program: Pubkey,
    pub pool: Pubkey,
    pub position: Pubkey,
    pub position_nft_mint: Pubkey,
    pub position_nft_account: Pubkey,
    pub fee_vault: Pubkey,
    /// Creator of record.
    pub creator: Pubkey,
    pub rate_bps: u16,
    pub excess_to_creator_bps: u16,
    pub activation_point: i64,
    pub cliff_fee_numerator: u64,
    pub reduction_factor: u64,
    pub bump: u8,
    pub fee_vault_bump: u8,

    // The only term that changes.
    pub creator_payout: Pubkey,

    // The ledger, in arrived units.
    pub platform_owed: u64,
    pub creator_owed: u64,
    pub holder_funded: u64,
    pub holder_claimed: u64,
    pub holder_recovered: u64,

    // Root state.
    pub active_root: [u8; 32],
    pub active_total: u64,
    pub active_epoch: u64,
    pub last_root_at: i64,
    /// The guardian's stop for this launch.
    pub paused: bool,
}

impl LaunchRecord {
    pub fn ledger(&self) -> Ledger {
        Ledger {
            platform_owed: self.platform_owed,
            creator_owed: self.creator_owed,
            holder_funded: self.holder_funded,
            holder_claimed: self.holder_claimed,
            holder_recovered: self.holder_recovered,
        }
    }

    pub fn set_ledger(&mut self, ledger: Ledger) {
        self.platform_owed = ledger.platform_owed;
        self.creator_owed = ledger.creator_owed;
        self.holder_funded = ledger.holder_funded;
        self.holder_claimed = ledger.holder_claimed;
        self.holder_recovered = ledger.holder_recovered;
    }

    pub fn root_state(&self) -> RootState {
        RootState {
            active_root: self.active_root,
            active_total: self.active_total,
            active_epoch: self.active_epoch,
            last_published_at: self.last_root_at,
        }
    }

    pub fn set_root_state(&mut self, roots: RootState) {
        self.active_root = roots.active_root;
        self.active_total = roots.active_total;
        self.active_epoch = roots.active_epoch;
        self.last_root_at = roots.last_published_at;
    }
}

/// `[b"claim", launch, holder]`. Must persist: it is what blocks a double claim.
#[account]
#[derive(InitSpace)]
pub struct HolderClaim {
    /// Cumulative, in the units sent from the vault. Below the holder's leaf after a partial payment.
    pub claimed: u64,
    pub bump: u8,
}
