//! The fee split of `docs/SOLANA_LAUNCH_PROGRAM.md` §1.2.
//!
//! The program cannot see volume, only the stock that arrived when it claimed. So the legs are fixed
//! fractions of each claim, chosen so that at the launch rate they equal the EVM's volume legs. With
//! `R` the rate traders pay, the position earns `0.8 × R` of volume, and the platform's 0.50% of
//! volume is `250 / (4R)` of each claim.
//!
//! Mirrors `PeddlesFeeHook._book`: both fixed legs floor, the creator's excess share floors, and
//! holders take the remainder, so the legs sum to the claim exactly at every setting.

use crate::{schedule, FeeError};

/// Lowest rate traders may pay, in basis points. Derived in §1.3: below 125 the two fixed legs do
/// not fit in what Meteora leaves.
pub const MIN_RATE_BPS: u16 = 125;
/// Highest rate traders may pay, in basis points.
pub const MAX_RATE_BPS: u16 = 500;
/// Basis-point denominator.
pub const BPS_DENOMINATOR: u16 = 10_000;
/// Numerator of each fixed leg: `leg = claim × 250 / (4 × R_bps)`.
pub const FIXED_LEG_NUMERATOR: u128 = 250;
/// Numerator of each fixed leg of a fee Meteora took no cut of: `leg = fee × 50 / R_bps`.
pub const UNCUT_LEG_NUMERATOR: u128 = 50;

/// Meteora's cut of every trading fee, in percent (`PROTOCOL_FEE_PERCENT`, `constants.rs`).
pub const METEORA_PROTOCOL_FEE_PERCENT: u64 = 20;

/// One claim, divided.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Split {
    pub platform: u64,
    pub creator_fixed: u64,
    pub creator_excess: u64,
    pub holders: u64,
}

impl Split {
    /// Everything owed to the creator from this claim.
    pub fn creator(&self) -> Result<u64, FeeError> {
        self.creator_fixed
            .checked_add(self.creator_excess)
            .ok_or(FeeError::MathOverflow)
    }
}

/// The launch terms a creator may choose.
pub fn validate_terms(rate_bps: u16, excess_to_creator_bps: u16) -> Result<(), FeeError> {
    if !(MIN_RATE_BPS..=MAX_RATE_BPS).contains(&rate_bps) {
        return Err(FeeError::InvalidRate);
    }
    if excess_to_creator_bps > BPS_DENOMINATOR {
        return Err(FeeError::InvalidExcessShare);
    }
    Ok(())
}

/// Divide one claim. `claimed` must be what ARRIVED in the vault, never what was sent (§1.7).
pub fn split_claim(
    claimed: u64,
    rate_bps: u16,
    excess_to_creator_bps: u16,
) -> Result<Split, FeeError> {
    validate_terms(rate_bps, excess_to_creator_bps)?;

    let den = 4u128 * rate_bps as u128;
    // claimed < 2^64 and the numerator is 250, so this cannot overflow a u128.
    let leg_wide = (claimed as u128)
        .checked_mul(FIXED_LEG_NUMERATOR)
        .ok_or(FeeError::MathOverflow)?
        / den;
    let leg = u64::try_from(leg_wide).map_err(|_| FeeError::MathOverflow)?;

    let fixed = leg.checked_mul(2).ok_or(FeeError::MathOverflow)?;
    // Cannot underflow at R >= 125: leg <= claimed / 2 (§1.3).
    let excess = claimed.checked_sub(fixed).ok_or(FeeError::MathOverflow)?;

    let creator_excess_wide = (excess as u128)
        .checked_mul(excess_to_creator_bps as u128)
        .ok_or(FeeError::MathOverflow)?
        / BPS_DENOMINATOR as u128;
    let creator_excess = u64::try_from(creator_excess_wide).map_err(|_| FeeError::MathOverflow)?;

    // Holders take all dust.
    let holders = excess
        .checked_sub(creator_excess)
        .ok_or(FeeError::MathOverflow)?;

    let sum = (leg as u128) + (leg as u128) + (creator_excess as u128) + (holders as u128);
    if sum != claimed as u128 {
        return Err(FeeError::SplitMismatch);
    }

    Ok(Split {
        platform: leg,
        creator_fixed: leg,
        creator_excess,
        holders,
    })
}

/// The creator's first buy, split into its fee and what buys coins (§2.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FirstBuyFee {
    /// `⌈quote × R / 10,000⌉`, rounded up as Meteora rounds a trading fee.
    pub fee: u64,
    /// What is left to buy coins with.
    pub net: u64,
}

/// The first buy pays the ordinary rate `R`, the EVM's `_effectiveTaxBps` rule. It is not a swap, so
/// neither the opening 20% nor Meteora's cut applies.
pub fn first_buy_fee(quote: u64, rate_bps: u16) -> Result<FirstBuyFee, FeeError> {
    validate_terms(rate_bps, 0)?;
    let fee_wide = (quote as u128)
        .checked_mul(rate_bps as u128)
        .ok_or(FeeError::MathOverflow)?
        .div_ceil(BPS_DENOMINATOR as u128);
    let fee = u64::try_from(fee_wide).map_err(|_| FeeError::MathOverflow)?;
    let net = quote.checked_sub(fee).ok_or(FeeError::MathOverflow)?;
    Ok(FirstBuyFee { fee, net })
}

/// Divide a fee that Meteora took no cut of, so each fixed leg is 0.50% of VOLUME, as on EVM:
/// `leg = ⌊fee × 50 / R⌋`. The 20% Meteora would have kept on a swap goes to the platform (owner,
/// 2026-11-06): `platform = leg + ⌊fee × 20 / 100⌋`. The rest is the excess, divided as on a swap.
///
/// Why not `split_claim`: its fractions assume Meteora already kept 20%, so on an uncut fee they
/// would pay each fixed leg 0.625% of volume. Same rounding rules: every share floors, holders
/// take the remainder.
pub fn split_uncut_fee(
    fee: u64,
    rate_bps: u16,
    excess_to_creator_bps: u16,
) -> Result<Split, FeeError> {
    validate_terms(rate_bps, excess_to_creator_bps)?;
    let leg = u64::try_from(
        (fee as u128)
            .checked_mul(UNCUT_LEG_NUMERATOR)
            .ok_or(FeeError::MathOverflow)?
            / rate_bps as u128,
    )
    .map_err(|_| FeeError::MathOverflow)?;
    let venue_share = u64::try_from(
        (fee as u128)
            .checked_mul(METEORA_PROTOCOL_FEE_PERCENT as u128)
            .ok_or(FeeError::MathOverflow)?
            / 100,
    )
    .map_err(|_| FeeError::MathOverflow)?;
    let platform = leg.checked_add(venue_share).ok_or(FeeError::MathOverflow)?;
    // Cannot underflow at R >= 125: 2 × leg <= 80% of the fee and the venue share is 20% of it.
    let excess = fee
        .checked_sub(leg)
        .and_then(|rest| rest.checked_sub(platform))
        .ok_or(FeeError::MathOverflow)?;
    let creator_excess = u64::try_from(
        (excess as u128)
            .checked_mul(excess_to_creator_bps as u128)
            .ok_or(FeeError::MathOverflow)?
            / BPS_DENOMINATOR as u128,
    )
    .map_err(|_| FeeError::MathOverflow)?;
    let holders = excess
        .checked_sub(creator_excess)
        .ok_or(FeeError::MathOverflow)?;
    Ok(Split {
        platform,
        creator_fixed: leg,
        creator_excess,
        holders,
    })
}

/// What Meteora does to one swap's fee. It runs inside Meteora, not here; it is modelled so the
/// tests can reproduce the design's tables end to end (§1.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeteoraSwapFee {
    /// The trading fee `F`.
    pub fee: u64,
    /// Meteora's cut `P`.
    pub protocol: u64,
    /// What the position may claim, `C = F − P`.
    pub claimable: u64,
}

/// `F = ⌈amount × numerator / 1e9⌉`, `P = ⌊F × 20 / 100⌋`, `C = F − P`.
pub fn meteora_swap_fee(amount: u64, fee_numerator: u64) -> Result<MeteoraSwapFee, FeeError> {
    let fee_wide = (amount as u128)
        .checked_mul(fee_numerator as u128)
        .ok_or(FeeError::MathOverflow)?
        .div_ceil(schedule::FEE_DENOMINATOR as u128);
    let fee = u64::try_from(fee_wide).map_err(|_| FeeError::MathOverflow)?;
    let protocol_wide = (fee as u128)
        .checked_mul(METEORA_PROTOCOL_FEE_PERCENT as u128)
        .ok_or(FeeError::MathOverflow)?
        / 100;
    let protocol = u64::try_from(protocol_wide).map_err(|_| FeeError::MathOverflow)?;
    let claimable = fee.checked_sub(protocol).ok_or(FeeError::MathOverflow)?;
    Ok(MeteoraSwapFee {
        fee,
        protocol,
        claimable,
    })
}
