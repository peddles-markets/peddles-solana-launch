use anchor_lang::prelude::*;

#[event]
pub struct Launched {
    pub launch: Pubkey,
    pub coin_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub pool: Pubkey,
    pub creator: Pubkey,
    pub rate_bps: u16,
    pub excess_to_creator_bps: u16,
    pub activation_point: i64,
    pub launch_fee_lamports: u64,
    pub bound_to_creator: bool,
    pub first_buy_coins: u64,
    pub first_buy_fee: u64,
}

#[event]
pub struct FeesCranked {
    pub launch: Pubkey,
    pub arrived: u64,
    pub platform: u64,
    pub creator: u64,
    pub holders: u64,
}

#[event]
pub struct PlatformSwept {
    pub launch: Pubkey,
    pub amount: u64,
}

#[event]
pub struct CreatorClaimed {
    pub launch: Pubkey,
    pub payout: Pubkey,
    pub amount: u64,
}

#[event]
pub struct CreatorPayoutChanged {
    pub launch: Pubkey,
    pub payout: Pubkey,
}

#[event]
pub struct RootPublished {
    pub launch: Pubkey,
    pub epoch: u64,
    pub root: [u8; 32],
    pub cumulative_total: u64,
}

#[event]
pub struct PauseSet {
    /// `None` for every launch.
    pub launch: Option<Pubkey>,
    pub paused: bool,
}

#[event]
pub struct HolderClaimed {
    pub launch: Pubkey,
    pub holder: Pubkey,
    pub amount: u64,
    pub cumulative_amount: u64,
}

#[event]
pub struct VaultSynced {
    pub launch: Pubkey,
    pub credited_to_holders: u64,
}

/// `bucket`: 0 platform, 1 creator, 2 holders, 3 surplus.
#[event]
pub struct AdminRecovered {
    pub launch: Pubkey,
    pub bucket: u8,
    pub amount: u64,
    pub destination: Pubkey,
    pub platform_owed: u64,
    pub creator_owed: u64,
    pub holder_recovered: u64,
}

#[event]
pub struct AdminRescued {
    pub pda_owner: Pubkey,
    pub source: Pubkey,
    pub mint: Pubkey,
    pub amount: u64,
    pub destination: Pubkey,
}

#[event]
pub struct AdminRecoveredLamports {
    pub target: Pubkey,
    pub amount: u64,
    pub destination: Pubkey,
}
