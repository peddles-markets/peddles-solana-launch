//! The opening fee: 20% decaying to the launch rate over 180 seconds (§2).
//!
//! Option A of §2.2: Meteora's linear time scheduler in 180 one-second periods, ending exactly at
//! the rate. The reduction is rounded UP and the cliff built back from it, so the end point is exact
//! and the start is within 180 × 1e-9 above 20%.

use crate::{split, FeeError};

/// `FEE_DENOMINATOR` in Meteora's `constants.rs`.
pub const FEE_DENOMINATOR: u64 = 1_000_000_000;
/// 20% in Meteora's fee units.
pub const OPENING_FEE_NUMERATOR: u64 = 200_000_000;
/// Decay steps. `NUMBER_OF_PERIOD × PERIOD_FREQUENCY_SECONDS` is the owner's 3 minutes; Meteora
/// enforces no duration, so this is the only place it is fixed (§2.3).
pub const NUMBER_OF_PERIOD: u16 = 180;
/// One second per step.
pub const PERIOD_FREQUENCY_SECONDS: u64 = 1;
/// `BaseFeeMode::FeeTimeSchedulerLinear` (`state/fee.rs`).
pub const BASE_FEE_MODE_LINEAR: u8 = 0;
/// Size of `BaseFeeParameters.data`.
pub const BASE_FEE_DATA_LEN: usize = 27;

/// `R_bps × 100,000`: exact, because rates are whole basis points.
pub fn rate_numerator(rate_bps: u16) -> Result<u64, FeeError> {
    (rate_bps as u64)
        .checked_mul(100_000)
        .ok_or(FeeError::MathOverflow)
}

/// The parameters Meteora is given, in `BorshFeeTimeScheduler` field order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FeeSchedule {
    pub cliff_fee_numerator: u64,
    pub number_of_period: u16,
    pub period_frequency: u64,
    pub reduction_factor: u64,
    pub base_fee_mode: u8,
}

impl FeeSchedule {
    /// Option A: `reduction = ⌈(20% − R) / 180⌉`, `cliff = R + 180 × reduction`.
    pub fn for_rate(rate_bps: u16) -> Result<Self, FeeError> {
        split::validate_terms(rate_bps, 0)?;
        let rate = rate_numerator(rate_bps)?;
        let span = OPENING_FEE_NUMERATOR
            .checked_sub(rate)
            .ok_or(FeeError::MathOverflow)?;
        let reduction_factor = span.div_ceil(NUMBER_OF_PERIOD as u64);
        let cliff_fee_numerator = reduction_factor
            .checked_mul(NUMBER_OF_PERIOD as u64)
            .and_then(|total| total.checked_add(rate))
            .ok_or(FeeError::MathOverflow)?;
        Ok(Self {
            cliff_fee_numerator,
            number_of_period: NUMBER_OF_PERIOD,
            period_frequency: PERIOD_FREQUENCY_SECONDS,
            reduction_factor,
            base_fee_mode: BASE_FEE_MODE_LINEAR,
        })
    }

    /// The Borsh encoding of `BorshFeeTimeScheduler`, the `data` of `BaseFeeParameters`.
    /// The mode byte lands at offset 26, as Meteora's `BASE_FEE_MODE_OFFSET` requires.
    pub fn to_borsh_bytes(&self) -> [u8; BASE_FEE_DATA_LEN] {
        let mut out = [0u8; BASE_FEE_DATA_LEN];
        out[0..8].copy_from_slice(&self.cliff_fee_numerator.to_le_bytes());
        out[8..10].copy_from_slice(&self.number_of_period.to_le_bytes());
        out[10..18].copy_from_slice(&self.period_frequency.to_le_bytes());
        out[18..26].copy_from_slice(&self.reduction_factor.to_le_bytes());
        out[26] = self.base_fee_mode;
        out
    }

    /// A port of `PodAlignedFeeTimeScheduler::get_base_fee_numerator` for the linear mode.
    pub fn fee_numerator_at(
        &self,
        current_point: u64,
        activation_point: u64,
    ) -> Result<u64, FeeError> {
        if self.period_frequency == 0 {
            return Ok(self.cliff_fee_numerator);
        }
        let period = if current_point < activation_point {
            self.number_of_period as u64
        } else {
            let elapsed = current_point - activation_point;
            (elapsed / self.period_frequency).min(self.number_of_period as u64)
        };
        self.reduction_factor
            .checked_mul(period)
            .and_then(|drop| self.cliff_fee_numerator.checked_sub(drop))
            .ok_or(FeeError::MathOverflow)
    }
}
