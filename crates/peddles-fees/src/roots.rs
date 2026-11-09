//! The published-root state of one launch (§3.6).
//!
//! NO VETO WINDOW — owner, 2026-11-06: "payout 4 times a day no safety windows". A published root
//! is claimable at once, and a launch may publish at most once every 6 hours.
//!
//! ACCEPTED RISK, stated by the owner's decision, not an oversight: a merkle root's leaves cannot be
//! checked on chain. A stolen keeper key can publish a root that gives one address every unclaimed
//! unit up to the root's total, and claim it straight away. What bounds it: the total can only rise
//! by what has arrived since the last root (`cumulative_total <= holder_funded`), a root must raise
//! the total, at most one root per 6 hours, and however fast the guardian pauses.
//!
//! This check does NOT protect entitlements already published but unclaimed: a later root replaces
//! the old one whole, so those units can be reassigned. Per-root tranches would stop that, and were
//! rejected at four roots a day (one account per root per launch, one claim per root per holder).

use crate::FeeError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RootState {
    pub active_root: [u8; 32],
    pub active_total: u64,
    /// 0 until the first root is published.
    pub active_epoch: u64,
    pub last_published_at: i64,
}

impl RootState {
    /// `publish_root`, effective immediately.
    pub fn publish(
        &mut self,
        epoch: u64,
        root: [u8; 32],
        cumulative_total: u64,
        holder_funded: u64,
        now: i64,
        min_interval_seconds: i64,
    ) -> Result<(), FeeError> {
        if root == [0u8; 32] {
            return Err(FeeError::RootEmpty);
        }
        let next = self
            .active_epoch
            .checked_add(1)
            .ok_or(FeeError::MathOverflow)?;
        if epoch != next {
            return Err(FeeError::RootEpochOutOfOrder);
        }
        if self.active_epoch != 0 {
            let earliest = self
                .last_published_at
                .checked_add(min_interval_seconds)
                .ok_or(FeeError::MathOverflow)?;
            if now < earliest {
                return Err(FeeError::RootTooSoon);
            }
        }
        if cumulative_total < self.active_total {
            return Err(FeeError::RootTotalDecreased);
        }
        // Nothing new arrived, so nothing may be republished: a key cannot rewrite allocations
        // during a quiet period.
        if cumulative_total == self.active_total {
            return Err(FeeError::RootTotalNotRaised);
        }
        // `holder_funded` only grows, so this is exactly "the raise is at most what arrived".
        if cumulative_total > holder_funded {
            return Err(FeeError::RootTotalAboveFunded);
        }
        self.active_root = root;
        self.active_total = cumulative_total;
        self.active_epoch = epoch;
        self.last_published_at = now;
        Ok(())
    }

    /// The root a claim verifies against, and its cumulative total.
    pub fn active_for_claim(&self) -> Result<([u8; 32], u64), FeeError> {
        if self.active_epoch == 0 {
            return Err(FeeError::NoActiveRoot);
        }
        Ok((self.active_root, self.active_total))
    }
}
