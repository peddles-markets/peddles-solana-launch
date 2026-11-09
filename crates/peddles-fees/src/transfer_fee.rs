//! A Token-2022 transfer fee, as charged by `TransferFee::calculate_fee`
//! (`interface/src/extension/transfer_fee/mod.rs`, token-2022 commit `f1a6c88`):
//! `min(⌈amount × bps / 10,000⌉, maximum_fee)`.
//!
//! The program never computes this to move money: it measures what arrived. It is here so the
//! ledger can be tested against the behaviour of a fee-charging mint (§1.7, §8).

use crate::FeeError;

/// `MAX_FEE_BASIS_POINTS`.
pub const MAX_TRANSFER_FEE_BPS: u16 = 10_000;

/// The fee withheld from a transfer of `amount`.
pub fn transfer_fee(amount: u64, fee_bps: u16, maximum_fee: u64) -> Result<u64, FeeError> {
    if fee_bps > MAX_TRANSFER_FEE_BPS {
        return Err(FeeError::MathOverflow);
    }
    if fee_bps == 0 || amount == 0 {
        return Ok(0);
    }
    let fee = (amount as u128 * fee_bps as u128).div_ceil(10_000);
    Ok(u64::try_from(fee)
        .map_err(|_| FeeError::MathOverflow)?
        .min(maximum_fee))
}

/// What a recipient is credited.
pub fn received(amount: u64, fee_bps: u16, maximum_fee: u64) -> Result<u64, FeeError> {
    amount
        .checked_sub(transfer_fee(amount, fee_bps, maximum_fee)?)
        .ok_or(FeeError::MathOverflow)
}
