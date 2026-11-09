//! The money ledger of one launch: what the `FeeVault` holds, and whose it is (§1.7, §3.8).
//!
//! All fields are in ARRIVED units, so `owed_total` is what the vault should hold. A payout of `owed`
//! reaches the recipient net of any transfer fee; that loss is the recipient's and the ledger still
//! comes down by what was sent, because the vault's balance fell by exactly that.
//!
//! A PAYOUT NEVER DEPENDS ON THE WHOLE LEDGER BEING THERE. If the vault holds less than the ledger
//! says (an issuer seized from it, or recovery took holder money) a payout sends what is there and
//! leaves the rest owed. The alternative is a transfer that reverts forever on every leg.
//!
//! RECOVERY (`recover`, owner, 2026-11-06). `holder_funded` only grows and the root's total is the
//! sum of its leaves, so neither can be reduced without breaking the keeper's arithmetic. Recovered
//! holder money is counted on its own, in `holder_recovered`. What that does:
//!
//! - the vault still equals `owed_total`, so `sync` and every later payout stay exact;
//! - a root can only raise the total up to `publishable()`, `funded − recovered`, so money that left
//!   can never be promised. If recovery took published money, roots wait until new money has covered
//!   the gap: new money refills the published entitlements first, which is what they are owed;
//! - a claim pays what is left and keeps the rest owed, so nothing bricks. Claims are first come,
//!   first served over a shortfall.

use crate::split::{split_claim, split_uncut_fee, Split};
use crate::FeeError;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    pub platform_owed: u64,
    pub creator_owed: u64,
    /// Everything ever credited to holders.
    pub holder_funded: u64,
    /// Everything ever paid to holders.
    pub holder_claimed: u64,
    /// Everything ever taken out of the holder bucket by recovery.
    pub holder_recovered: u64,
}

/// Which part of a launch's vault a recovery takes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoverBucket {
    Platform,
    Creator,
    Holders,
    /// What the vault holds above the ledger: sent in directly and not synced.
    Surplus,
}

/// `after − before`. A fall across a claim means something other than the claim moved the vault.
pub fn arrived(before: u64, after: u64) -> Result<u64, FeeError> {
    after.checked_sub(before).ok_or(FeeError::VaultDecreased)
}

/// `cumulative − already_claimed`, refusing nothing-to-pay.
pub fn holder_payout(cumulative_amount: u64, already_claimed: u64) -> Result<u64, FeeError> {
    match cumulative_amount.checked_sub(already_claimed) {
        Some(0) | None => Err(FeeError::NothingToPay),
        Some(amount) => Ok(amount),
    }
}

impl Ledger {
    /// What the vault must hold to pay everybody. u128 so it cannot overflow while being checked.
    pub fn owed_total(&self) -> Result<u128, FeeError> {
        Ok(self.platform_owed as u128 + self.creator_owed as u128 + self.holder_held()? as u128)
    }

    /// The holder bucket: funded, less paid out, less recovered.
    pub fn holder_held(&self) -> Result<u64, FeeError> {
        self.holder_funded
            .checked_sub(self.holder_claimed)
            .and_then(|left| left.checked_sub(self.holder_recovered))
            .ok_or(FeeError::MathOverflow)
    }

    /// The most a root's cumulative total may be.
    pub fn publishable(&self) -> u64 {
        self.holder_funded.saturating_sub(self.holder_recovered)
    }

    /// Book one split.
    pub fn credit(&mut self, split: &Split) -> Result<(), FeeError> {
        self.platform_owed = self
            .platform_owed
            .checked_add(split.platform)
            .ok_or(FeeError::MathOverflow)?;
        self.creator_owed = self
            .creator_owed
            .checked_add(split.creator()?)
            .ok_or(FeeError::MathOverflow)?;
        self.holder_funded = self
            .holder_funded
            .checked_add(split.holders)
            .ok_or(FeeError::MathOverflow)?;
        Ok(())
    }

    /// Split and book the measured delta of one claim.
    pub fn credit_claim(
        &mut self,
        vault_before: u64,
        vault_after: u64,
        rate_bps: u16,
        excess_to_creator_bps: u16,
    ) -> Result<Split, FeeError> {
        let claimed = arrived(vault_before, vault_after)?;
        let split = split_claim(claimed, rate_bps, excess_to_creator_bps)?;
        self.credit(&split)?;
        Ok(split)
    }

    /// Split and book the measured delta of the creator's first-buy fee, which Meteora took no cut
    /// of. A transfer-fee quote arrives short, and the split runs on what arrived.
    pub fn credit_first_buy_fee(
        &mut self,
        vault_before: u64,
        vault_after: u64,
        rate_bps: u16,
        excess_to_creator_bps: u16,
    ) -> Result<Split, FeeError> {
        let got = arrived(vault_before, vault_after)?;
        let split = split_uncut_fee(got, rate_bps, excess_to_creator_bps)?;
        self.credit(&split)?;
        Ok(split)
    }

    /// Take the platform leg, up to what the vault holds, and return what to send.
    pub fn take_platform(&mut self, vault_balance: u64) -> Result<u64, FeeError> {
        let amount = self.platform_owed.min(vault_balance);
        if amount == 0 {
            return Err(FeeError::NothingToPay);
        }
        self.platform_owed -= amount;
        Ok(amount)
    }

    /// Take the creator leg, up to what the vault holds, and return what to send.
    pub fn take_creator(&mut self, vault_balance: u64) -> Result<u64, FeeError> {
        let amount = self.creator_owed.min(vault_balance);
        if amount == 0 {
            return Err(FeeError::NothingToPay);
        }
        self.creator_owed -= amount;
        Ok(amount)
    }

    /// Book one holder payout of up to `due` and return what to send. The total ever paid can never
    /// pass the active root's total or leave the holder bucket, so a keeper whose leaves sum to more
    /// than its total cannot reach the other legs. Over a shortfall it pays what is left; the
    /// holder's own `claimed` moves by this amount, so the rest stays claimable.
    pub fn pay_holder(
        &mut self,
        due: u64,
        active_root_total: u64,
        vault_balance: u64,
    ) -> Result<u64, FeeError> {
        let room = active_root_total
            .saturating_sub(self.holder_claimed)
            .min(self.holder_held()?)
            .min(vault_balance);
        let paid = due.min(room);
        if paid == 0 {
            return Err(FeeError::ClaimAboveRootTotal);
        }
        self.holder_claimed = self
            .holder_claimed
            .checked_add(paid)
            .ok_or(FeeError::MathOverflow)?;
        Ok(paid)
    }

    /// Take `amount` out of one bucket. Afterwards the ledger owes exactly `amount` less, or, for
    /// `Surplus`, the same. The vault must hold it: nothing here can write off tokens that are gone.
    pub fn recover(
        &mut self,
        bucket: RecoverBucket,
        amount: u64,
        vault_balance: u64,
    ) -> Result<(), FeeError> {
        if amount == 0 || amount > vault_balance {
            return Err(FeeError::RecoverAboveBucket);
        }
        match bucket {
            RecoverBucket::Platform => {
                self.platform_owed = self
                    .platform_owed
                    .checked_sub(amount)
                    .ok_or(FeeError::RecoverAboveBucket)?;
            }
            RecoverBucket::Creator => {
                self.creator_owed = self
                    .creator_owed
                    .checked_sub(amount)
                    .ok_or(FeeError::RecoverAboveBucket)?;
            }
            RecoverBucket::Holders => {
                if amount > self.holder_held()? {
                    return Err(FeeError::RecoverAboveBucket);
                }
                self.holder_recovered = self
                    .holder_recovered
                    .checked_add(amount)
                    .ok_or(FeeError::MathOverflow)?;
            }
            RecoverBucket::Surplus => {
                let surplus = (vault_balance as u128).saturating_sub(self.owed_total()?);
                if amount as u128 > surplus {
                    return Err(FeeError::RecoverAboveBucket);
                }
            }
        }
        Ok(())
    }

    /// Credit anything sent to the vault directly to holders (`sync_vault`, §3.8). Returns the
    /// amount credited. A balance below the ledger (a seizure by the issuer) credits nothing.
    pub fn sync(&mut self, vault_balance: u64) -> Result<u64, FeeError> {
        let owed = self.owed_total()?;
        let balance = vault_balance as u128;
        if balance <= owed {
            return Ok(0);
        }
        let unaccounted = u64::try_from(balance - owed).map_err(|_| FeeError::MathOverflow)?;
        self.holder_funded = self
            .holder_funded
            .checked_add(unaccounted)
            .ok_or(FeeError::MathOverflow)?;
        Ok(unaccounted)
    }
}
