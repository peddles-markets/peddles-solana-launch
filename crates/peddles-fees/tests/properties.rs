//! The unit and property tests of `docs/SOLANA_LAUNCH_PROGRAM.md` §8, without a validator.
//!
//! The crate takes no dependencies, so randomness is a seeded xorshift: every run tests the same
//! numbers and a failure reproduces. `tests/fixtures/*.hex` are bytes read from Solana mainnet on
//! 2026-11-06 (see README.md), never typed by hand.

use peddles_fees::ix_data;
use peddles_fees::ledger::{arrived, holder_payout, Ledger, RecoverBucket};
use peddles_fees::merkle;
use peddles_fees::mint::{inspect_mint, require_launchable, require_transferable};
use peddles_fees::price::{
    self, delta_amount_a_down, delta_amount_a_up, delta_amount_b_up, first_buy_opening,
    liquidity_deposits_exactly, liquidity_for_exact_supply, sqrt_price_after_buy, MAX_SQRT_PRICE,
};
use peddles_fees::roots::RootState;
use peddles_fees::schedule::{
    rate_numerator, FeeSchedule, FEE_DENOMINATOR, NUMBER_OF_PERIOD, OPENING_FEE_NUMERATOR,
    PERIOD_FREQUENCY_SECONDS,
};
use peddles_fees::split::{
    first_buy_fee, meteora_swap_fee, split_claim, split_uncut_fee, validate_terms, MAX_RATE_BPS,
    MIN_RATE_BPS,
};
use peddles_fees::suffix::{
    base58_suffix_value, binding_seed, ends_with_pedl, PEDL_SUFFIX_VALUE,
};
use peddles_fees::transfer_fee::{received, transfer_fee};
use peddles_fees::FeeError;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// Spread across every magnitude, not just near u64::MAX.
    fn any_size(&mut self) -> u64 {
        let shift = self.below(64) as u32;
        self.next() >> shift
    }
}

fn claim_edges() -> Vec<u64> {
    let mut edges = vec![0, 1, 2, 3, 4, 5, u64::MAX, u64::MAX - 1, u64::MAX / 2];
    for bit in 1..64u32 {
        let power = 1u64 << bit;
        edges.extend_from_slice(&[power - 1, power, power + 1]);
    }
    edges
}

const EXCESS_SHARES: [u16; 6] = [0, 1, 2_500, 5_000, 9_999, 10_000];

fn hex_bytes(text: &str) -> Vec<u8> {
    let text = text.trim();
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

// --- §1.2, §1.3, §1.4: the split ---------------------------------------------------------------

#[test]
fn worked_table_1_4() {
    // rate, x, F, P, C, platform, creator excess, holders
    let rows: [(u16, u16, u64, u64, u64, u64, u64, u64); 8] = [
        (125, 0, 1_250_000, 250_000, 1_000_000, 500_000, 0, 0),
        (125, 5_000, 1_250_000, 250_000, 1_000_000, 500_000, 0, 0),
        (200, 0, 2_000_000, 400_000, 1_600_000, 500_000, 0, 600_000),
        (200, 5_000, 2_000_000, 400_000, 1_600_000, 500_000, 300_000, 300_000),
        (300, 0, 3_000_000, 600_000, 2_400_000, 500_000, 0, 1_400_000),
        (300, 5_000, 3_000_000, 600_000, 2_400_000, 500_000, 700_000, 700_000),
        (500, 0, 5_000_000, 1_000_000, 4_000_000, 500_000, 0, 3_000_000),
        (500, 5_000, 5_000_000, 1_000_000, 4_000_000, 500_000, 1_500_000, 1_500_000),
    ];
    for (rate, x, fee, protocol, claim, platform, excess, holders) in rows {
        let swap = meteora_swap_fee(100_000_000, rate_numerator(rate).unwrap()).unwrap();
        assert_eq!((swap.fee, swap.protocol, swap.claimable), (fee, protocol, claim));
        let split = split_claim(claim, rate, x).unwrap();
        assert_eq!(split.platform, platform);
        assert_eq!(split.creator_fixed, platform);
        assert_eq!(split.creator_excess, excess);
        assert_eq!(split.holders, holders);
    }
}

fn check_split(claim: u64, rate: u16, x: u16) {
    let split = split_claim(claim, rate, x).unwrap();

    // The legs always sum to the claim, with no overflow.
    let sum = split.platform as u128
        + split.creator_fixed as u128
        + split.creator_excess as u128
        + split.holders as u128;
    assert_eq!(sum, claim as u128);

    let den = 4 * rate as u128;
    // Exact, as rationals over `den`: the platform's real share is claim × 250 / den.
    // platform <= that, and within one base unit below it.
    let exact_platform = claim as u128 * 250;
    let platform_scaled = split.platform as u128 * den;
    assert!(platform_scaled <= exact_platform);
    assert!(exact_platform - platform_scaled < den);
    assert_eq!(split.platform, split.creator_fixed);

    // Holders' exact share is claim × (den − 500) / den × (10,000 − x) / 10,000.
    // Rounding must never take from holders in the platform's favour: they get at least that.
    let holder_exact = claim as u128 * (den - 500) * (10_000 - x as u128);
    let holder_scaled = split.holders as u128 * den * 10_000;
    assert!(holder_scaled >= holder_exact, "holders short: {claim} {rate} {x}");
    // And never more than the two floors plus the creator-excess floor can add: under 3 units.
    assert!(holder_scaled - holder_exact < 3 * den * 10_000);
}

#[test]
fn split_holds_exhaustively_for_small_claims() {
    for rate in MIN_RATE_BPS..=MAX_RATE_BPS {
        for x in EXCESS_SHARES {
            for claim in 0..4_000u64 {
                check_split(claim, rate, x);
            }
        }
    }
}

#[test]
fn split_holds_at_every_power_of_two_edge() {
    for rate in MIN_RATE_BPS..=MAX_RATE_BPS {
        for x in EXCESS_SHARES {
            for &claim in &claim_edges() {
                check_split(claim, rate, x);
            }
        }
    }
}

#[test]
fn split_holds_for_random_terms() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..500_000 {
        let rate = MIN_RATE_BPS + rng.below((MAX_RATE_BPS - MIN_RATE_BPS + 1) as u64) as u16;
        let x = rng.below(10_001) as u16;
        check_split(rng.any_size(), rate, x);
    }
}

#[test]
fn minimum_is_exactly_125_bps() {
    // At 125 the two fixed legs fit at every claim around rounding, and holders get C mod 2.
    for claim in 0..20_000u64 {
        let split = split_claim(claim, 125, 7_000).unwrap();
        assert_eq!(split.platform, claim / 2);
        assert_eq!(split.creator_excess + split.holders, claim % 2);
    }
    for &claim in &claim_edges() {
        let split = split_claim(claim, 125, 0).unwrap();
        assert_eq!(split.holders, claim % 2);
    }
    // Below it the same formula would not fit. This is why 124 is refused.
    let unfit = |rate: u128| (0..20_000u128).any(|c| 2 * (c * 250 / (4 * rate)) > c);
    assert!(!unfit(125));
    for rate in 120..125 {
        assert!(unfit(rate), "rate {rate} should not fit");
    }
    assert_eq!(validate_terms(124, 0), Err(FeeError::InvalidRate));
    assert_eq!(validate_terms(501, 0), Err(FeeError::InvalidRate));
    assert_eq!(validate_terms(125, 10_001), Err(FeeError::InvalidExcessShare));
    assert_eq!(split_claim(1, 124, 0), Err(FeeError::InvalidRate));
    assert!(validate_terms(125, 10_000).is_ok());
    assert!(validate_terms(500, 0).is_ok());
}

#[test]
fn meteora_rounding_favours_the_position() {
    // C = F − ⌊F/5⌋ = ⌈4F/5⌉, so the position earns at least 0.8 × R of volume.
    for fee in 0..10_000u128 {
        assert_eq!(fee - fee * 20 / 100, (4 * fee).div_ceil(5));
    }
    let mut rng = Rng(11);
    for _ in 0..200_000 {
        let amount = rng.any_size();
        let rate = MIN_RATE_BPS + rng.below(376) as u16;
        let swap = meteora_swap_fee(amount, rate_numerator(rate).unwrap()).unwrap();
        // claimable >= amount × 0.8 × R, as rationals.
        assert!(
            swap.claimable as u128 * FEE_DENOMINATOR as u128 * 5
                >= amount as u128 * rate_numerator(rate).unwrap() as u128 * 4
        );
    }
}

// --- §2.2: the schedule ------------------------------------------------------------------------

#[test]
fn schedule_table_2_2() {
    let rows: [(u16, u64, u64, [(u64, u64); 6]); 2] = [
        (
            125,
            200_000_060,
            1_041_667,
            [
                (0, 200_000_060),
                (60, 137_500_040),
                (120, 75_000_020),
                (179, 13_541_667),
                (180, 12_500_000),
                (100_000, 12_500_000),
            ],
        ),
        (
            500,
            200_000_120,
            833_334,
            [
                (0, 200_000_120),
                (60, 150_000_080),
                (120, 100_000_040),
                (179, 50_833_334),
                (180, 50_000_000),
                (100_000, 50_000_000),
            ],
        ),
    ];
    let activation = 1_793_000_000;
    for (rate, cliff, reduction, points) in rows {
        let schedule = FeeSchedule::for_rate(rate).unwrap();
        assert_eq!(schedule.cliff_fee_numerator, cliff);
        assert_eq!(schedule.reduction_factor, reduction);
        for (elapsed, fee) in points {
            assert_eq!(schedule.fee_numerator_at(activation + elapsed, activation), Ok(fee));
        }
        // Before activation Meteora charges the end of the schedule.
        assert_eq!(
            schedule.fee_numerator_at(activation - 1, activation),
            Ok(rate_numerator(rate).unwrap())
        );
    }
}

#[test]
fn schedule_holds_for_every_rate() {
    let activation = 5_000u64;
    for rate in MIN_RATE_BPS..=MAX_RATE_BPS {
        let schedule = FeeSchedule::for_rate(rate).unwrap();
        let end = rate_numerator(rate).unwrap();
        assert_eq!(schedule.number_of_period, NUMBER_OF_PERIOD);
        assert_eq!(schedule.period_frequency, PERIOD_FREQUENCY_SECONDS);
        assert_eq!(schedule.base_fee_mode, 0);
        assert_eq!(
            NUMBER_OF_PERIOD as u64 * PERIOD_FREQUENCY_SECONDS,
            180,
            "the owner's 3 minutes"
        );

        // Starts within 180 × 1e-9 above 20%, ends exactly at the rate, and never rises.
        assert!(schedule.cliff_fee_numerator >= OPENING_FEE_NUMERATOR);
        assert!(schedule.cliff_fee_numerator < OPENING_FEE_NUMERATOR + 180);
        let mut last = u64::MAX;
        for elapsed in 0..=200u64 {
            let fee = schedule.fee_numerator_at(activation + elapsed, activation).unwrap();
            assert!(fee <= last);
            assert!(fee >= end);
            // Below Meteora's 99% maximum and above its 0.01% minimum.
            assert!(fee <= 990_000_000 && fee >= 100_000);
            last = fee;
            if elapsed >= 180 {
                assert_eq!(fee, end);
            }
        }
    }
}

#[test]
fn schedule_encoding_matches_borsh_layout() {
    let bytes = FeeSchedule::for_rate(125).unwrap().to_borsh_bytes();
    assert_eq!(&bytes[0..8], &200_000_060u64.to_le_bytes());
    assert_eq!(&bytes[8..10], &180u16.to_le_bytes());
    assert_eq!(&bytes[10..18], &1u64.to_le_bytes());
    assert_eq!(&bytes[18..26], &1_041_667u64.to_le_bytes());
    assert_eq!(bytes[26], 0, "BASE_FEE_MODE_OFFSET is 26");
}

// --- §2.5: the first buy's fee ----------------------------------------------------------------

#[test]
fn first_buy_pays_the_ordinary_rate() {
    let parts = first_buy_fee(100_000_000, 125).unwrap();
    assert_eq!((parts.fee, parts.net), (1_250_000, 98_750_000));
    assert_eq!(first_buy_fee(0, 125).unwrap().fee, 0);
    assert_eq!(first_buy_fee(1, 125).unwrap().fee, 1, "rounds up");
    assert_eq!(first_buy_fee(1, 124), Err(FeeError::InvalidRate));
    let mut rng = Rng(77);
    for _ in 0..200_000 {
        let rate = MIN_RATE_BPS + rng.below(376) as u16;
        let quote = rng.any_size();
        let parts = first_buy_fee(quote, rate).unwrap();
        assert_eq!(parts.fee as u128 + parts.net as u128, quote as u128);
    }
}

/// Meteora takes no cut of this fee, so each fixed leg is 0.50% of volume, exactly the EVM's legs,
/// where `split_claim` would pay 0.625%. The 20% it would have kept goes to the platform.
#[test]
fn an_uncut_fee_pays_the_venue_share_to_the_platform() {
    let volume = 100_000_000u64;
    for (rate, x, platform, excess, holders) in [
        (125u16, 0u16, 750_000u64, 0u64, 0u64),
        (200, 5_000, 900_000, 300_000, 300_000),
        (500, 0, 1_500_000, 0, 3_000_000),
    ] {
        let fee = first_buy_fee(volume, rate).unwrap().fee;
        let split = split_uncut_fee(fee, rate, x).unwrap();
        assert_eq!(split.platform, platform, "0.50% of volume + 20% of the fee");
        assert_eq!(split.creator_fixed, 500_000, "0.50% of volume");
        assert_eq!(split.creator_excess, excess);
        assert_eq!(split.holders, holders);
    }
    let mut rng = Rng(3);
    for _ in 0..200_000 {
        let rate = MIN_RATE_BPS + rng.below(376) as u16;
        let x = rng.below(10_001) as u16;
        let fee = rng.any_size();
        let split = split_uncut_fee(fee, rate, x).unwrap();
        let sum = split.platform as u128
            + split.creator_fixed as u128
            + split.creator_excess as u128
            + split.holders as u128;
        assert_eq!(sum, fee as u128);
        assert!(split.creator_fixed as u128 * rate as u128 <= fee as u128 * 50);
        assert!(
            split.platform as u128 * rate as u128 * 5 <= fee as u128 * (250 + rate as u128)
        );
    }
}

#[test]
fn a_first_buy_fee_with_a_transfer_fee_splits_what_arrived() {
    let sent = 1_250_000;
    let got = received(sent, 100, u64::MAX).unwrap();
    assert_eq!(got, 1_237_500);
    let mut ledger = Ledger::default();
    let split = ledger.credit_first_buy_fee(0, got, 125, 0).unwrap();
    assert_eq!(split.platform, 742_500);
    assert_eq!(split.creator_fixed, 495_000);
    assert_eq!(ledger.owed_total(), Ok(got as u128));
}

// --- §1.7: the arrived amount ------------------------------------------------------------------

#[test]
fn transfer_fee_matches_token_2022() {
    assert_eq!(transfer_fee(1, 100, u64::MAX), Ok(1), "ceiling: 1 unit at 1% costs 1");
    assert_eq!(transfer_fee(0, 100, u64::MAX), Ok(0));
    assert_eq!(transfer_fee(1_000, 0, u64::MAX), Ok(0));
    assert_eq!(transfer_fee(1_000_000_000, 100, u64::MAX), Ok(10_000_000));
    assert_eq!(transfer_fee(1_000_000_000, 100, 7), Ok(7), "capped by the maximum");
    assert_eq!(transfer_fee(5, 10_000, u64::MAX), Ok(5), "100% takes all of it");
    assert_eq!(received(1, 100, u64::MAX), Ok(0));
    assert!(transfer_fee(1, 10_001, u64::MAX).is_err());
}

#[test]
fn worked_example_1_7() {
    let bps = 100;
    let input = 1_000_000_000u64;
    assert_eq!(transfer_fee(input, bps, u64::MAX), Ok(10_000_000));
    let in_vault = received(input, bps, u64::MAX).unwrap();
    assert_eq!(in_vault, 990_000_000);

    // R = 1.25%
    let swap = meteora_swap_fee(in_vault, rate_numerator(125).unwrap()).unwrap();
    assert_eq!((swap.fee, swap.protocol, swap.claimable), (12_375_000, 2_475_000, 9_900_000));
    let at_vault = received(swap.claimable, bps, u64::MAX).unwrap();
    assert_eq!(at_vault, 9_801_000);
    let mut ledger = Ledger::default();
    let split = ledger.credit_claim(0, at_vault, 125, 0).unwrap();
    assert_eq!(split.platform, 4_900_500);
    assert_eq!(split.creator_fixed, 4_900_500);
    assert_eq!(split.holders, 0);
    assert_eq!(received(split.platform, bps, u64::MAX), Ok(4_851_495));

    // R = 5%
    let swap = meteora_swap_fee(in_vault, rate_numerator(500).unwrap()).unwrap();
    let at_vault = received(swap.claimable, bps, u64::MAX).unwrap();
    let split = split_claim(at_vault, 500, 0).unwrap();
    assert_eq!(split.platform, 4_900_500);
    assert_eq!(split.holders, 29_403_000);
    assert_eq!(received(split.holders, bps, u64::MAX), Ok(29_108_970));
}

#[test]
fn a_falling_vault_is_refused() {
    assert_eq!(arrived(10, 9), Err(FeeError::VaultDecreased));
    assert_eq!(arrived(9, 10), Ok(1));
    let mut ledger = Ledger::default();
    assert_eq!(ledger.credit_claim(10, 9, 200, 0), Err(FeeError::VaultDecreased));
    assert_eq!(ledger, Ledger::default());
}

/// The ledger equals the vault after every crank and payout, under any transfer fee, including a
/// fee that changes between cranks, and however tiny the cranks are.
#[test]
fn ledger_always_equals_the_vault() {
    for seed in 1..=64u64 {
        let mut rng = Rng(seed.wrapping_mul(0x2545_f491_4f6c_dd1d));
        let rate = MIN_RATE_BPS + rng.below(376) as u16;
        let x = rng.below(10_001) as u16;
        let mut fee_bps = 100u16;
        let mut maximum = u64::MAX;
        let mut ledger = Ledger::default();
        let mut vault = 0u64;

        for _ in 0..2_000 {
            match rng.below(8) {
                0 | 1 => {
                    // A crank, sometimes of a single raw unit.
                    let sent = if rng.below(3) == 0 { rng.below(3) } else { rng.below(1 << 40) };
                    let got = received(sent, fee_bps, maximum).unwrap();
                    let before = vault;
                    vault += got;
                    ledger.credit_claim(before, vault, rate, x).unwrap();
                }
                2 => {
                    // The issuer changes the fee, within Token-2022's limits.
                    fee_bps = rng.below(10_001) as u16;
                    maximum = if rng.below(2) == 0 { u64::MAX } else { rng.below(1 << 20) };
                }
                3 => {
                    if let Ok(amount) = ledger.take_platform(vault) {
                        vault -= amount;
                    }
                }
                4 => {
                    if let Ok(amount) = ledger.take_creator(vault) {
                        vault -= amount;
                    }
                }
                5 => {
                    let unclaimed = ledger.holder_held().unwrap();
                    if unclaimed > 0 {
                        let due = 1 + rng.below(unclaimed);
                        let paid = ledger.pay_holder(due, ledger.holder_funded, vault).unwrap();
                        assert_eq!(paid, due);
                        vault -= paid;
                    }
                }
                6 => {
                    // Admin recovery from any bucket, any amount: refused or exact, never partial.
                    let bucket = [
                        RecoverBucket::Platform,
                        RecoverBucket::Creator,
                        RecoverBucket::Holders,
                        RecoverBucket::Surplus,
                    ][rng.below(4) as usize];
                    let amount = rng.below(1 << 41);
                    let before = ledger;
                    if ledger.recover(bucket, amount, vault).is_ok() {
                        vault -= amount;
                    } else {
                        assert_eq!(ledger, before, "a refused recovery changes nothing");
                    }
                }
                _ => {
                    // Someone sends tokens straight to the vault; `sync_vault` books them.
                    let donated = rng.below(1 << 20);
                    vault += donated;
                    assert_eq!(ledger.sync(vault), Ok(donated));
                }
            }
            assert_eq!(ledger.owed_total(), Ok(vault as u128));
            assert!(ledger.holder_claimed <= ledger.holder_funded);
        }
    }
}

#[test]
fn many_tiny_cranks_stay_exact_and_dust_goes_to_holders() {
    let (rate, x) = (125, 0);
    let mut ledger = Ledger::default();
    let mut vault = 0;
    for _ in 0..10_000 {
        let before = vault;
        vault += 1;
        ledger.credit_claim(before, vault, rate, x).unwrap();
    }
    assert_eq!(ledger.owed_total(), Ok(10_000));
    // Griefing with 1-unit cranks moves it all to holders, nothing to the platform.
    assert_eq!(ledger.platform_owed, 0);
    assert_eq!(ledger.creator_owed, 0);
    assert_eq!(ledger.holder_funded, 10_000);
}

#[test]
fn nothing_owed_refuses() {
    let mut ledger = Ledger::default();
    assert_eq!(ledger.take_platform(100), Err(FeeError::NothingToPay));
    assert_eq!(ledger.take_creator(100), Err(FeeError::NothingToPay));
}

#[test]
fn holder_payouts_are_bounded() {
    let mut ledger = Ledger {
        holder_funded: 100,
        ..Ledger::default()
    };
    // Bounded by the root's total, by the bucket and by the vault, paying what fits.
    assert_eq!(ledger.pay_holder(60, 50, 1_000), Ok(50));
    assert_eq!(ledger.pay_holder(1, 50, 1_000), Err(FeeError::ClaimAboveRootTotal));
    assert_eq!(ledger.pay_holder(500, 500, 1_000), Ok(50));
    assert_eq!(ledger.pay_holder(1, 500, 1_000), Err(FeeError::ClaimAboveRootTotal));
    let mut short = Ledger {
        holder_funded: 100,
        ..Ledger::default()
    };
    assert_eq!(short.pay_holder(80, 100, 30), Ok(30), "the vault is short");
}

/// An issuer seized from the vault. Every leg must still pay out what is there, not revert forever.
#[test]
fn a_short_vault_pays_what_is_there_and_keeps_the_rest_owed() {
    let mut ledger = Ledger {
        platform_owed: 100,
        creator_owed: 100,
        holder_funded: 100,
        ..Ledger::default()
    };
    let mut vault = 120;
    let paid = ledger.take_platform(vault).unwrap();
    assert_eq!(paid, 100);
    vault -= paid;
    let paid = ledger.take_creator(vault).unwrap();
    assert_eq!(paid, 20);
    vault -= paid;
    assert_eq!(ledger.creator_owed, 80, "the rest stays owed");
    assert_eq!(vault, 0);
    assert_eq!(ledger.take_creator(vault), Err(FeeError::NothingToPay));
}

#[test]
fn recovery_comes_off_the_ledger_exactly() {
    let base = Ledger {
        platform_owed: 10,
        creator_owed: 20,
        holder_funded: 100,
        holder_claimed: 30,
        holder_recovered: 0,
    };
    let vault = 100; // 10 + 20 + 70
    let mut l = base;
    l.recover(RecoverBucket::Platform, 4, vault).unwrap();
    assert_eq!(l.platform_owed, 6);
    assert_eq!(l.owed_total(), Ok(96));
    let mut l = base;
    l.recover(RecoverBucket::Creator, 20, vault).unwrap();
    assert_eq!(l.creator_owed, 0);
    let mut l = base;
    l.recover(RecoverBucket::Holders, 70, vault).unwrap();
    assert_eq!(l.holder_recovered, 70);
    assert_eq!(l.holder_held(), Ok(0));
    assert_eq!(l.owed_total(), Ok(30));

    for (bucket, amount) in [
        (RecoverBucket::Platform, 11),
        (RecoverBucket::Creator, 21),
        (RecoverBucket::Holders, 71),
        (RecoverBucket::Surplus, 1),
        (RecoverBucket::Platform, 0),
    ] {
        let mut l = base;
        assert_eq!(l.recover(bucket, amount, vault), Err(FeeError::RecoverAboveBucket));
        assert_eq!(l, base);
    }
    // Sent in directly and never synced: only what is above the ledger.
    let mut l = base;
    assert_eq!(l.recover(RecoverBucket::Surplus, 6, 105), Err(FeeError::RecoverAboveBucket));
    l.recover(RecoverBucket::Surplus, 5, 105).unwrap();
    assert_eq!(l, base, "surplus was never on the ledger");
    // Tokens that are gone cannot be written off by recovery.
    let mut l = base;
    assert_eq!(l.recover(RecoverBucket::Holders, 50, 40), Err(FeeError::RecoverAboveBucket));
}

/// Recovery takes everything unpublished: published claims are untouched and no root can promise it.
#[test]
fn recovering_unpublished_money_leaves_published_claims_whole() {
    let mut ledger = Ledger {
        holder_funded: 100,
        ..Ledger::default()
    };
    let mut roots = RootState::default();
    roots.publish(1, R1, 60, ledger.publishable(), 0, SIX_HOURS).unwrap();
    ledger.recover(RecoverBucket::Holders, 40, 100).unwrap();
    assert_eq!(ledger.publishable(), 60);
    // Every published unit is still there.
    assert_eq!(ledger.pay_holder(60, 60, 60), Ok(60));
    // Nothing new can be promised until money arrives.
    assert_eq!(
        roots.publish(2, R2, 61, ledger.publishable(), SIX_HOURS, SIX_HOURS),
        Err(FeeError::RootTotalAboveFunded)
    );
    // New money publishes as usual.
    ledger.holder_funded += 5;
    roots.publish(2, R2, 65, ledger.publishable(), SIX_HOURS, SIX_HOURS).unwrap();
}

/// Recovery takes published money. Claims keep working over what is left, first come, partially,
/// and new money refills the gap before any new root.
#[test]
fn recovering_published_money_never_bricks_claims() {
    let mut ledger = Ledger {
        holder_funded: 100,
        ..Ledger::default()
    };
    let mut roots = RootState::default();
    roots.publish(1, R1, 100, ledger.publishable(), 0, SIX_HOURS).unwrap();
    ledger.recover(RecoverBucket::Holders, 70, 100).unwrap();
    let mut vault = 30;

    // A holder owed 50 gets the 30 that is left; the leaf still says 50, `claimed` says 30.
    let paid = ledger.pay_holder(50, 100, vault).unwrap();
    assert_eq!(paid, 30);
    vault -= paid;
    assert_eq!(ledger.pay_holder(20, 100, vault), Err(FeeError::ClaimAboveRootTotal));

    // Money arrives: the published root is refilled first, and no root can be raised yet.
    ledger.holder_funded += 20;
    vault += 20;
    assert_eq!(ledger.publishable(), 50);
    assert_eq!(
        roots.publish(2, R2, 101, ledger.publishable(), SIX_HOURS, SIX_HOURS),
        Err(FeeError::RootTotalAboveFunded)
    );
    assert_eq!(ledger.pay_holder(20, 100, vault), Ok(20), "the same proof finishes the claim");
    assert_eq!(ledger.owed_total(), Ok(0));
}

#[test]
fn a_recovery_changes_nothing_a_sync_would_double() {
    let mut ledger = Ledger {
        holder_funded: 100,
        ..Ledger::default()
    };
    ledger.recover(RecoverBucket::Holders, 40, 100).unwrap();
    assert_eq!(ledger.sync(60), Ok(0), "the vault matches the ledger");
    assert_eq!(ledger.sync(61), Ok(1));
}

#[test]
fn a_seized_vault_credits_nothing_to_sync() {
    let mut ledger = Ledger {
        platform_owed: 10,
        creator_owed: 10,
        holder_funded: 30,
        holder_claimed: 0,
        holder_recovered: 0,
    };
    assert_eq!(ledger.sync(5), Ok(0));
    assert_eq!(ledger.sync(50), Ok(0));
    assert_eq!(ledger.holder_funded, 30);
    assert_eq!(ledger.sync(51), Ok(1));
    assert_eq!(ledger.holder_funded, 31);
}

// --- §3.6: merkle and roots --------------------------------------------------------------------

mod sha256 {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    pub fn hashv(values: &[&[u8]]) -> [u8; 32] {
        let mut message: Vec<u8> = values.concat();
        let bit_length = (message.len() as u64) * 8;
        message.push(0x80);
        while message.len() % 64 != 56 {
            message.push(0);
        }
        message.extend_from_slice(&bit_length.to_be_bytes());

        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];
        for block in message.chunks(64) {
            let mut w = [0u32; 64];
            for (i, word) in block.chunks(4).enumerate() {
                w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }
            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ (!e & g);
                let t1 = hh
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let t2 = s0.wrapping_add(maj);
                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(t1);
                d = c;
                c = b;
                b = a;
                a = t1.wrapping_add(t2);
            }
            for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
                *slot = slot.wrapping_add(value);
            }
        }
        let mut out = [0u8; 32];
        for (i, word) in h.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

fn hash32(text: &str) -> [u8; 32] {
    hex_bytes(text).try_into().unwrap()
}

#[test]
fn sha256_known_answer() {
    assert_eq!(
        sha256::hashv(&[&b"ab"[..], &b"c"[..]]),
        hash32("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
}

const LAUNCH: [u8; 32] = [1; 32];
const HOLDER_A: [u8; 32] = [2; 32];
const HOLDER_B: [u8; 32] = [3; 32];

/// Fixed vectors, computed independently with Node's crypto module, so the keeper has a target.
#[test]
fn merkle_known_answers() {
    let hash = sha256::hashv;
    let a = merkle::leaf_hash(&hash, &LAUNCH, &HOLDER_A, 1_000);
    let b = merkle::leaf_hash(&hash, &LAUNCH, &HOLDER_B, 2_500);
    assert_eq!(a, hash32("e449b96cab4c5972d2296b5a2099d4d446bf3ae7dd591bfd8662e8613eff6f2c"));
    assert_eq!(b, hash32("2569e8264a4a53f3efe1c63b31d1b75e88a2eb1e019896f1963bec972d223965"));
    assert_eq!(
        merkle::node_hash(&hash, &a, &b),
        hash32("2a25b19eebcc49666b8805d47d03ebd951020891e20b7dc2d124b50139856a56")
    );
    assert_eq!(merkle::node_hash(&hash, &a, &b), merkle::node_hash(&hash, &b, &a));
}

/// Four leaves, and the proof for each. Returns (root, leaves, proofs).
fn tree(
    entries: &[([u8; 32], [u8; 32], u64); 4],
) -> ([u8; 32], [[u8; 32]; 4], [Vec<[u8; 32]>; 4]) {
    let hash = sha256::hashv;
    let leaves = (*entries).map(|(launch, holder, amount)| merkle::leaf_hash(&hash, &launch, &holder, amount));
    let n01 = merkle::node_hash(&hash, &leaves[0], &leaves[1]);
    let n23 = merkle::node_hash(&hash, &leaves[2], &leaves[3]);
    let root = merkle::node_hash(&hash, &n01, &n23);
    let proofs = [
        vec![leaves[1], n23],
        vec![leaves[0], n23],
        vec![leaves[3], n01],
        vec![leaves[2], n01],
    ];
    (root, leaves, proofs)
}

#[test]
fn merkle_verifies_and_refuses_forgeries() {
    let hash = sha256::hashv;
    let entries = [
        (LAUNCH, HOLDER_A, 1_000),
        (LAUNCH, HOLDER_B, 2_500),
        (LAUNCH, [4; 32], 40),
        (LAUNCH, [5; 32], 5),
    ];
    let (root, leaves, proofs) = tree(&entries);
    for i in 0..4 {
        assert_eq!(merkle::verify(&hash, &proofs[i], &root, leaves[i]), Ok(()));
    }

    // A forged amount.
    let forged = merkle::leaf_hash(&hash, &LAUNCH, &HOLDER_A, 1_001);
    assert_eq!(merkle::verify(&hash, &proofs[0], &root, forged), Err(FeeError::InvalidProof));
    // Claiming for another holder with their proof.
    let other = merkle::leaf_hash(&hash, &LAUNCH, &HOLDER_B, 1_000);
    assert_eq!(merkle::verify(&hash, &proofs[0], &root, other), Err(FeeError::InvalidProof));
    // A proof from another launch.
    let other_launch = merkle::leaf_hash(&hash, &[9; 32], &HOLDER_A, 1_000);
    assert_eq!(merkle::verify(&hash, &proofs[0], &root, other_launch), Err(FeeError::InvalidProof));
    // An inner node presented as a leaf: the domain prefixes stop the 64-byte replay.
    let inner = merkle::node_hash(&hash, &leaves[0], &leaves[1]);
    let short_proof = vec![proofs[0][1]];
    assert_eq!(merkle::verify(&hash, &short_proof, &root, inner), Ok(()));
    let mut preimage = Vec::new();
    preimage.extend_from_slice(&leaves[0]);
    preimage.extend_from_slice(&leaves[1]);
    let inner_as_leaf = sha256::hashv(&[&[0x00u8][..], &preimage[..]]);
    assert_ne!(inner_as_leaf, inner);
    // An empty proof, and a tampered sibling.
    assert_eq!(merkle::verify(&hash, &[], &root, leaves[0]), Err(FeeError::InvalidProof));
    let mut tampered = proofs[0].clone();
    tampered[0][0] ^= 1;
    assert_eq!(merkle::verify(&hash, &tampered, &root, leaves[0]), Err(FeeError::InvalidProof));
    // A proof that cannot fit.
    let long = vec![[0u8; 32]; merkle::MAX_PROOF_LEN + 1];
    assert_eq!(merkle::verify(&hash, &long, &root, leaves[0]), Err(FeeError::ProofTooLong));
}

#[test]
fn merkle_double_claim_pays_nothing() {
    // The same proof twice: the cumulative entitlement is unchanged, so the delta is zero.
    let cumulative = 1_000;
    let first = holder_payout(cumulative, 0).unwrap();
    assert_eq!(first, 1_000);
    assert_eq!(holder_payout(cumulative, first), Err(FeeError::NothingToPay));
    // A later root pays only the difference.
    assert_eq!(holder_payout(1_500, 1_000), Ok(500));
    // A lowered entitlement cannot underflow into a huge payout.
    assert_eq!(holder_payout(900, 1_000), Err(FeeError::NothingToPay));
}

const SIX_HOURS: i64 = 21_600;
const R1: [u8; 32] = [0xaa; 32];
const R2: [u8; 32] = [0xbb; 32];

#[test]
fn a_root_is_claimable_at_once() {
    let mut roots = RootState::default();
    assert_eq!(roots.active_for_claim(), Err(FeeError::NoActiveRoot));
    roots.publish(1, R1, 100, 100, 1_000, SIX_HOURS).unwrap();
    assert_eq!(roots.active_for_claim(), Ok((R1, 100)));
    assert_eq!(roots.active_epoch, 1);
}

#[test]
fn roots_refuse_bad_publishes() {
    let mut roots = RootState::default();
    assert_eq!(roots.publish(2, R1, 1, 1, 0, SIX_HOURS), Err(FeeError::RootEpochOutOfOrder));
    assert_eq!(roots.publish(0, R1, 1, 1, 0, SIX_HOURS), Err(FeeError::RootEpochOutOfOrder));
    assert_eq!(roots.publish(1, [0; 32], 1, 1, 0, SIX_HOURS), Err(FeeError::RootEmpty));
    // A raise above what has arrived.
    assert_eq!(roots.publish(1, R1, 101, 100, 0, SIX_HOURS), Err(FeeError::RootTotalAboveFunded));
    assert_eq!(roots.publish(1, R1, 0, 100, 0, SIX_HOURS), Err(FeeError::RootTotalNotRaised));
    assert_eq!(roots, RootState::default());

    roots.publish(1, R1, 100, 500, 0, SIX_HOURS).unwrap();
    // The 6-hour cadence: four roots a day at most.
    assert_eq!(roots.publish(2, R2, 200, 500, SIX_HOURS - 1, SIX_HOURS), Err(FeeError::RootTooSoon));
    // A decreasing total, and the same total: a key cannot rewrite allocations on no new money.
    assert_eq!(roots.publish(2, R2, 99, 500, SIX_HOURS, SIX_HOURS), Err(FeeError::RootTotalDecreased));
    assert_eq!(roots.publish(2, R2, 100, 500, SIX_HOURS, SIX_HOURS), Err(FeeError::RootTotalNotRaised));
    assert_eq!(roots.publish(2, R2, 501, 500, SIX_HOURS, SIX_HOURS), Err(FeeError::RootTotalAboveFunded));
    assert_eq!(roots.active_for_claim(), Ok((R1, 100)), "a refused root changes nothing");

    roots.publish(2, R2, 500, 500, SIX_HOURS, SIX_HOURS).unwrap();
    assert_eq!(roots.active_for_claim(), Ok((R2, 500)));
}

#[test]
fn four_roots_a_day_is_the_most_a_launch_can_publish() {
    let mut roots = RootState::default();
    let mut published = 0;
    for hour in 0..24i64 {
        let now = hour * 3_600;
        let total = (published + 1) * 10;
        if roots.publish(published as u64 + 1, R1, total, 1_000, now, SIX_HOURS).is_ok() {
            published += 1;
        }
    }
    assert_eq!(published, 4);
}

/// This documents the accepted risk, owner, 2026-11-06. A root replaces the last one whole, so a
/// bad root CAN reassign entitlements already published and not yet claimed, up to its total, and
/// the ledger bound is all that stops it reaching the other legs. If this test ever fails, the
/// wording in `holders.rs` and §3.6 is out of date.
#[test]
fn a_bad_root_can_take_everything_unclaimed_but_no_more() {
    let hash = sha256::hashv;
    let thief = [7u8; 32];
    let mut ledger = Ledger {
        platform_owed: 400,
        creator_owed: 400,
        holder_funded: 1_000,
        holder_claimed: 200,
        holder_recovered: 0,
    };
    let mut roots = RootState::default();
    roots.publish(1, R1, 900, ledger.holder_funded, 0, SIX_HOURS).unwrap();

    // The bad root: one leaf, the whole total. Its epoch is claimable the moment it is published.
    let leaf = merkle::leaf_hash(&hash, &LAUNCH, &thief, 800);
    roots.publish(2, leaf, 1_000, ledger.holder_funded, SIX_HOURS, SIX_HOURS).unwrap();
    let (root, total) = roots.active_for_claim().unwrap();
    assert_eq!(merkle::verify(&hash, &[], &root, leaf), Ok(()));
    let vault = ledger.owed_total().unwrap() as u64;
    let due = holder_payout(800, 0).unwrap();
    assert_eq!(ledger.pay_holder(due, total, vault), Ok(800));

    assert_eq!(ledger.holder_claimed, 1_000);
    // Every holder unit is gone. The platform and creator legs are untouched and unreachable.
    assert_eq!(ledger.pay_holder(1, total, vault), Err(FeeError::ClaimAboveRootTotal));
    assert_eq!((ledger.platform_owed, ledger.creator_owed), (400, 400));
}

// --- The mint address --------------------------------------------------------------------------

#[test]
fn pedl_suffix() {
    // Built so its base58 form is 9cfBkPsoQ2NPHYPi7b69bcQG8FKfNc33k2UfRxiPPEDL (checked in Node
    // with a real base58 encoder).
    let address = hash32("80000000000000000000000000000000000000000000000000000000001596cf");
    assert!(ends_with_pedl(&address));
    assert_eq!(base58_suffix_value(&address), PEDL_SUFFIX_VALUE);
    let mut other = address;
    other[31] ^= 1;
    assert!(!ends_with_pedl(&other));
    assert_eq!(PEDL_SUFFIX_VALUE, 22 * 58u64.pow(3) + 13 * 58u64.pow(2) + 12 * 58 + 19);
}

#[test]
fn both_coin_seed_forms() {
    let creator = [9u8; 32];
    assert_eq!(binding_seed(true, &creator), &creator[..]);
    assert!(binding_seed(false, &creator).is_empty());
    // An empty seed changes nothing about what is hashed, so one seed list expresses both, and the
    // two forms differ in length by 32 bytes, so they can never derive the same address.
    let hash = |bound: bool| {
        sha256::hashv(&[&b"coin"[..], binding_seed(bound, &creator), &[1u8; 8][..]])
    };
    assert_eq!(hash(false), sha256::hashv(&[&b"coin"[..], &[1u8; 8][..]]));
    assert_ne!(hash(true), hash(false));
}

// --- §3.2 step 1: mints, on real bytes ---------------------------------------------------------

fn tslax() -> Vec<u8> {
    hex_bytes(include_str!("fixtures/tslax_mint.hex"))
}

fn anthropic() -> Vec<u8> {
    hex_bytes(include_str!("fixtures/prestocks_anthropic_mint.hex"))
}

fn tlv_value_offset(data: &[u8], wanted: u16) -> usize {
    let mut offset = 166;
    loop {
        let ext = u16::from_le_bytes([data[offset], data[offset + 1]]);
        let length = u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;
        assert_ne!(ext, 0, "extension {wanted} missing");
        if ext == wanted {
            return offset + 4;
        }
        offset += 4 + length;
    }
}

#[test]
fn real_stock_mints_pass() {
    for data in [tslax(), anthropic()] {
        let facts = inspect_mint(&data).unwrap();
        assert!(!facts.paused);
        assert_eq!(facts.default_account_state, Some(1));
        assert!(!facts.transfer_hook_program_set);
        assert!(!facts.non_transferable);
        assert_eq!(require_launchable(&data), Ok(()));
    }
}

#[test]
fn a_plain_spl_mint_has_no_extensions() {
    assert_eq!(require_launchable(&[0u8; 82]), Ok(()));
}

#[test]
fn a_paused_mint_is_refused() {
    let mut data = tslax();
    let at = tlv_value_offset(&data, 26);
    data[at + 32] = 1;
    assert_eq!(require_launchable(&data), Err(FeeError::MintPaused));
}

#[test]
fn a_default_frozen_mint_is_refused() {
    let mut data = anthropic();
    let at = tlv_value_offset(&data, 6);
    data[at] = 2;
    assert_eq!(require_launchable(&data), Err(FeeError::MintDefaultNotInitialized));
}

#[test]
fn recovery_refuses_a_paused_or_hooked_mint_by_name_and_allows_default_frozen() {
    assert_eq!(require_transferable(&tslax()), Ok(()));
    let mut paused = tslax();
    let at = tlv_value_offset(&paused, 26);
    paused[at + 32] = 1;
    assert_eq!(require_transferable(&paused), Err(FeeError::MintPaused));
    let mut hooked = tslax();
    let at = tlv_value_offset(&hooked, 14);
    hooked[at + 40] = 7;
    assert_eq!(require_transferable(&hooked), Err(FeeError::MintHookSet));
    let mut frozen = anthropic();
    let at = tlv_value_offset(&frozen, 6);
    frozen[at] = 2;
    assert_eq!(require_transferable(&frozen), Ok(()), "new accounts start frozen; existing ones move");
}

#[test]
fn a_mint_with_a_hook_is_refused() {
    let mut data = tslax();
    let at = tlv_value_offset(&data, 14);
    data[at + 40] = 7;
    assert_eq!(require_launchable(&data), Err(FeeError::MintHookSet));
}

#[test]
fn a_malformed_mint_is_refused() {
    assert_eq!(inspect_mint(&[0u8; 100]), Err(FeeError::MalformedMint));
    let mut data = tslax();
    data[165] = 2; // an Account, not a Mint
    assert_eq!(inspect_mint(&data), Err(FeeError::MalformedMint));
    let mut data = tslax();
    let at = tlv_value_offset(&data, 14);
    data[at - 2] = 0xff; // a length running past the end
    data[at - 1] = 0xff;
    assert_eq!(inspect_mint(&data), Err(FeeError::MalformedMint));
}

// --- §4: encodings, against bytes from mainnet -------------------------------------------------

/// `56ybV1pr…AfaFW`, the NVDAx pool's creation: decode its fields, re-encode, compare every byte.
#[test]
fn initialize_pool_data_matches_the_measured_transaction() {
    let measured = hex_bytes(include_str!("fixtures/init_pool_ix_nvdax.hex"));
    assert_eq!(measured.len(), ix_data::INITIALIZE_POOL_DATA_LEN);

    let read_u128 = |at: usize| u128::from_le_bytes(measured[at..at + 16].try_into().unwrap());
    let read_u64 = |at: usize| u64::from_le_bytes(measured[at..at + 8].try_into().unwrap());
    let schedule = FeeSchedule {
        cliff_fee_numerator: read_u64(8),
        number_of_period: u16::from_le_bytes([measured[16], measured[17]]),
        period_frequency: read_u64(18),
        reduction_factor: read_u64(26),
        base_fee_mode: measured[34],
    };
    assert_eq!(schedule.cliff_fee_numerator, 20_000_000, "the pool's 2% base fee");
    let (min, max) = (read_u128(39), read_u128(55));
    assert_eq!(min, 4_295_048_016, "MIN_SQRT_PRICE");
    assert_eq!(max, MAX_SQRT_PRICE);
    let (liquidity, sqrt_price) = (read_u128(72), read_u128(88));

    let encoded = ix_data::initialize_customizable_pool(&schedule, min, max, liquidity, sqrt_price);
    assert_eq!(encoded.to_vec(), measured);
}

#[test]
fn permanent_lock_data_matches_the_measured_transaction() {
    let measured = hex_bytes(include_str!("fixtures/permanent_lock_ix_nvdax.hex"));
    let liquidity = u128::from_le_bytes(measured[8..24].try_into().unwrap());
    assert_eq!(ix_data::permanent_lock_position(liquidity).to_vec(), measured);
}

#[test]
fn metadata_encoding() {
    let mut out = [0xffu8; ix_data::CREATE_METADATA_MAX_LEN];
    let creator = [7u8; 32];
    let length =
        ix_data::create_metadata_account_v3(&mut out, b"Coin", b"CN", b"u", &creator).unwrap();
    let mut expected = vec![33u8];
    expected.extend_from_slice(&4u32.to_le_bytes());
    expected.extend_from_slice(b"Coin");
    expected.extend_from_slice(&2u32.to_le_bytes());
    expected.extend_from_slice(b"CN");
    expected.extend_from_slice(&1u32.to_le_bytes());
    expected.extend_from_slice(b"u");
    expected.extend_from_slice(&[0, 0]); // seller_fee_basis_points: no royalty
    expected.push(1); // creators: Some
    expected.extend_from_slice(&1u32.to_le_bytes());
    expected.extend_from_slice(&creator);
    expected.push(1); // verified
    expected.push(100); // share
    expected.extend_from_slice(&[0, 0]); // collection, uses: None
    expected.push(0); // is_mutable: false
    expected.push(0); // collection_details: None
    assert_eq!(&out[..length], expected.as_slice());

    let long_name = [b'a'; ix_data::MAX_NAME_LEN + 1];
    assert_eq!(
        ix_data::create_metadata_account_v3(&mut out, &long_name, b"", b"", &creator),
        Err(FeeError::MetadataTooLong)
    );
    // The longest of everything fits the buffer.
    let (name, symbol, uri) = ([b'a'; 32], [b'b'; 10], [b'c'; 200]);
    assert_eq!(
        ix_data::create_metadata_account_v3(&mut out, &name, &symbol, &uri, &creator),
        Ok(ix_data::CREATE_METADATA_MAX_LEN)
    );
}

// --- §3.2 step 4: the exact supply --------------------------------------------------------------

#[test]
fn liquidity_deposits_exactly_the_supply_for_every_decimals_choice() {
    // Prices across the range a launch could open at, in Q64.64.
    let prices: [u128; 6] = [
        4_295_048_016, // MIN_SQRT_PRICE
        1_000_000_000_000,
        17_000_000_000_000_000,
        123_456_789_012_345_678,
        1 << 62,
        1 << 64,
    ];
    let mut rng = Rng(5);
    for decimals in [6u32, 9] {
        let supply = 1_000_000_000u64 * 10u64.pow(decimals);
        for price in prices {
            let liquidity = liquidity_for_exact_supply(price, MAX_SQRT_PRICE, supply)
                .unwrap_or_else(|| panic!("no liquidity for {price} at {decimals} decimals"));
            assert_eq!(delta_amount_a_up(price, MAX_SQRT_PRICE, liquidity), Some(supply));
        }
        for _ in 0..500 {
            let price = 4_295_048_016 + (rng.next() as u128 % (1u128 << 60));
            let liquidity = liquidity_for_exact_supply(price, MAX_SQRT_PRICE, supply).unwrap();
            assert_eq!(delta_amount_a_up(price, MAX_SQRT_PRICE, liquidity), Some(supply));
        }
    }
}

/// The worked example of §2.5, values computed independently with BigInt in Node: a 6-decimal coin,
/// 1,000,000,000 supply, TSLAx quote, $3.5K open at about $400 a share, a 1.0 TSLAx first buy at 1.25%.
#[test]
fn first_buy_worked_example() {
    let supply = 1_000_000_000_000_000u64;
    let sqrt_open = 17_255_349_056_330_942u128;
    let liquidity = 17_255_349_056_334_700_167_014_000_818_203u128;
    assert!(liquidity_deposits_exactly(sqrt_open, MAX_SQRT_PRICE, liquidity, supply));

    let parts = first_buy_fee(100_000_000, 125).unwrap();
    let opening =
        first_buy_opening(sqrt_open, MAX_SQRT_PRICE, liquidity, supply, parts.net).unwrap();
    assert_eq!(opening.sqrt_price, 19_202_738_449_830_724);
    assert_eq!(opening.pool_coins, 898_587_933_247_752);
    assert_eq!(opening.pool_quote, 98_750_000);
    assert_eq!(opening.creator_coins, 101_412_066_752_248);
    assert_eq!(
        opening.creator_coins,
        delta_amount_a_down(sqrt_open, opening.sqrt_price, liquidity).unwrap(),
        "here it equals what a fee-free swap would have paid"
    );
}

#[test]
fn no_first_buy_stays_one_sided() {
    let supply = 1_000_000_000_000_000u64;
    let sqrt_open = 17_000_000_000_000_000u128;
    let liquidity = liquidity_for_exact_supply(sqrt_open, MAX_SQRT_PRICE, supply).unwrap();
    let opening = first_buy_opening(sqrt_open, MAX_SQRT_PRICE, liquidity, supply, 0).unwrap();
    assert_eq!(opening.sqrt_price, sqrt_open);
    assert_eq!(opening.pool_coins, supply);
    assert_eq!(opening.pool_quote, 0);
    assert_eq!(opening.creator_coins, 0);
}

#[test]
fn every_unit_of_the_supply_is_accounted_for() {
    let mut rng = Rng(101);
    for round in 0..8_000 {
        let supply = 1_000_000_000u64 * 10u64.pow(if round % 2 == 0 { 6 } else { 9 });
        let sqrt_open = 4_295_048_016 + (rng.next() as u128 % (1u128 << 60));
        let liquidity = liquidity_for_exact_supply(sqrt_open, MAX_SQRT_PRICE, supply).unwrap();
        let net = 1 + rng.below(1 << 40);
        let Some(opening) = first_buy_opening(sqrt_open, MAX_SQRT_PRICE, liquidity, supply, net)
        else {
            continue;
        };
        // Pool + creator is exactly the supply.
        assert_eq!(opening.pool_coins as u128 + opening.creator_coins as u128, supply as u128);
        // Meteora never pulls more than was paid for.
        assert!(opening.pool_quote <= net);
        assert!(opening.sqrt_price >= sqrt_open);
        // Within one unit of a fee-free swap over the same range: both sides round into the pool.
        let swap = delta_amount_a_down(sqrt_open, opening.sqrt_price, liquidity).unwrap();
        assert!(opening.creator_coins <= swap + 1 && opening.creator_coins + 1 >= swap);
        // The pool's coins equal what Meteora's own formula deposits at the opening price.
        assert_eq!(
            delta_amount_a_up(opening.sqrt_price, MAX_SQRT_PRICE, liquidity),
            Some(opening.pool_coins)
        );
    }
}

/// The attack the check exists for: a thin `L` makes a small buy move the price far enough to keep
/// nearly the whole supply.
#[test]
fn liquidity_must_be_exact() {
    let supply = 1_000_000_000_000_000u64;
    let sqrt_open = 17_255_349_056_330_942u128;
    let exact = liquidity_for_exact_supply(sqrt_open, MAX_SQRT_PRICE, supply).unwrap();
    assert!(liquidity_deposits_exactly(sqrt_open, MAX_SQRT_PRICE, exact, supply));

    let thin = exact / 1_000;
    assert!(!liquidity_deposits_exactly(sqrt_open, MAX_SQRT_PRICE, thin, supply));
    assert_eq!(first_buy_opening(sqrt_open, MAX_SQRT_PRICE, thin, supply, 98_750_000), None);
    assert!(!liquidity_deposits_exactly(sqrt_open, MAX_SQRT_PRICE, exact * 2, supply));

    // The multiplication check agrees with Meteora's division around the boundary.
    let mut rng = Rng(31);
    for _ in 0..5_000 {
        let price = 4_295_048_016 + (rng.next() as u128 % (1u128 << 60));
        let liquidity = liquidity_for_exact_supply(price, MAX_SQRT_PRICE, supply).unwrap();
        let step = rng.below(1 << 30) as u128;
        for candidate in [liquidity, liquidity + step, liquidity - step] {
            assert_eq!(
                liquidity_deposits_exactly(price, MAX_SQRT_PRICE, candidate, supply),
                delta_amount_a_up(price, MAX_SQRT_PRICE, candidate) == Some(supply)
            );
        }
    }
}

#[test]
fn a_first_buy_cannot_pass_the_top_of_the_range() {
    assert_eq!(sqrt_price_after_buy(10, 20, 1, u64::MAX), None);
    assert_eq!(sqrt_price_after_buy(10, 20, 0, 5), None);
    assert_eq!(sqrt_price_after_buy(10, 20, 1, 0), Some(10));
}

#[test]
fn delta_b_rounds_up_like_meteora() {
    let q128 = 1u128 << 127;
    assert_eq!(delta_amount_b_up(0, 2, q128), Some(1));
    assert_eq!(delta_amount_b_up(0, 1, q128), Some(1), "half a unit rounds up");
    assert_eq!(delta_amount_b_up(5, 5, q128), Some(0));
    assert_eq!(delta_amount_b_up(6, 5, q128), None);
}

#[test]
fn delta_a_matches_plain_arithmetic_on_small_numbers() {
    let mut rng = Rng(21);
    for _ in 0..50_000 {
        let lower = 1 + rng.below(1 << 20) as u128;
        let upper = lower + 1 + rng.below(1 << 20) as u128;
        let liquidity = rng.below(1 << 40) as u128;
        let expected = (liquidity * (upper - lower)).div_ceil(upper * lower);
        assert_eq!(delta_amount_a_up(lower, upper, liquidity), Some(expected as u64));
    }
    assert_eq!(delta_amount_a_up(0, 1, 1), None);
    assert_eq!(delta_amount_a_up(5, 5, 1), None);
}

#[test]
fn opening_price_band() {
    assert!(price::in_band(10, 5, 20));
    assert!(price::in_band(5, 5, 20));
    assert!(price::in_band(20, 5, 20));
    assert!(!price::in_band(4, 5, 20));
    assert!(!price::in_band(21, 5, 20));
    assert!(!price::in_band(0, 0, 0), "an unset band refuses everything");
}
