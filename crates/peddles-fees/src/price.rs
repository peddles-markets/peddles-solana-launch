//! Meteora's opening amounts, and the creator's first buy (§2.5, §3.2).
//!
//! `initialize_customizable_pool` takes `sqrt_min_price`, `sqrt_price` and `liquidity`, and deposits
//! `get_initial_pool_information` of them (`liquidity_handler/concentrated_liquidity.rs`, damm-v2
//! `3493cd6`):
//!
//! ```text
//! token_a_amount = ⌈L × (√Pmax − √P) / (√Pmax × √P)⌉      get_delta_amount_a_unsigned, Up
//! token_b_amount = ⌈L × (√P − √Pmin) / 2^128⌉             get_delta_amount_b_unsigned, Up
//! ```
//!
//! A launch with a first buy therefore needs no swap. `L` stays what the single-sided launch would
//! have used, so `√P0` deposits exactly the supply; the pool opens at the price the buy would reach,
//! `√P1 = √P0 + ⌊net × 2^128 / L⌋` (`get_next_sqrt_price_from_amount_in_b_rounding_down`); and the
//! coins the pool does not pull stay with the creator. Both amounts round up, into the pool.
//!
//! On chain, `L` is checked with multiplications only (`liquidity_deposits_exactly`), `√P1` needs one
//! division, and the coins are READ from the creator's balance after the CPI. `first_buy_opening`
//! is the full prediction, for clients and tests.

/// `MAX_SQRT_PRICE` in Meteora's `constants.rs`.
pub const MAX_SQRT_PRICE: u128 = 79_226_673_521_066_979_257_578_248_091;

/// A launch's opening price must sit in the band the admin set for that quote. The $3.5K opening
/// market cap is a fact about the quote's dollar price, so it is the admin's to set, not the
/// creator's to pick; see `QuoteConfig` in the program.
pub fn in_band(sqrt_price: u128, min_sqrt_price: u128, max_sqrt_price: u128) -> bool {
    min_sqrt_price != 0 && sqrt_price >= min_sqrt_price && sqrt_price <= max_sqrt_price
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct U256 {
    hi: u128,
    lo: u128,
}

const LOW_64: u128 = u64::MAX as u128;

impl U256 {
    const ZERO: U256 = U256 { hi: 0, lo: 0 };

    fn mul_u128(a: u128, b: u128) -> U256 {
        let (a1, a0) = (a >> 64, a & LOW_64);
        let (b1, b0) = (b >> 64, b & LOW_64);
        let p00 = a0 * b0;
        let p01 = a0 * b1;
        let p10 = a1 * b0;
        let p11 = a1 * b1;
        let middle = (p00 >> 64) + (p01 & LOW_64) + (p10 & LOW_64);
        U256 {
            hi: p11 + (p01 >> 64) + (p10 >> 64) + (middle >> 64),
            lo: (p00 & LOW_64) | (middle << 64),
        }
    }

    fn checked_mul_u128(self, b: u128) -> Option<U256> {
        let low = U256::mul_u128(self.lo, b);
        let high = U256::mul_u128(self.hi, b);
        if high.hi != 0 {
            return None;
        }
        let hi = low.hi.checked_add(high.lo)?;
        Some(U256 { hi, lo: low.lo })
    }

    fn wrapping_sub(self, other: U256) -> U256 {
        let (lo, borrow) = self.lo.overflowing_sub(other.lo);
        let hi = self.hi.wrapping_sub(other.hi).wrapping_sub(borrow as u128);
        U256 { hi, lo }
    }

    fn checked_add_one(self) -> Option<U256> {
        let (lo, carry) = self.lo.overflowing_add(1);
        let hi = self.hi.checked_add(carry as u128)?;
        Some(U256 { hi, lo })
    }

    /// Shift left one bit, returning the bit shifted out of the top.
    fn shl1(self) -> (U256, bool) {
        let carry = (self.hi >> 127) != 0;
        let hi = (self.hi << 1) | (self.lo >> 127);
        (
            U256 {
                hi,
                lo: self.lo << 1,
            },
            carry,
        )
    }

    fn bit(self, index: u32) -> bool {
        if index >= 128 {
            (self.hi >> (index - 128)) & 1 == 1
        } else {
            (self.lo >> index) & 1 == 1
        }
    }

    fn set_bit(&mut self, index: u32) {
        if index >= 128 {
            self.hi |= 1u128 << (index - 128);
        } else {
            self.lo |= 1u128 << index;
        }
    }

    /// Binary long division. Off-chain only: 256 iterations.
    fn div_rem(self, divisor: U256) -> Option<(U256, U256)> {
        if divisor == U256::ZERO {
            return None;
        }
        let mut quotient = U256::ZERO;
        let mut remainder = U256::ZERO;
        for index in (0..256u32).rev() {
            let (shifted, carry) = remainder.shl1();
            remainder = shifted;
            if self.bit(index) {
                remainder.lo |= 1;
            }
            if carry || remainder >= divisor {
                remainder = remainder.wrapping_sub(divisor);
                quotient.set_bit(index);
            }
        }
        Some((quotient, remainder))
    }
}

/// Meteora's token A amount for a range, rounded up: `None` if it does not fit a u64.
pub fn delta_amount_a_up(sqrt_price: u128, sqrt_max_price: u128, liquidity: u128) -> Option<u64> {
    let (quotient, remainder) = delta_a_parts(sqrt_price, sqrt_max_price, liquidity)?;
    let rounded = if remainder != U256::ZERO {
        quotient.checked_add_one()?
    } else {
        quotient
    };
    if rounded.hi != 0 {
        return None;
    }
    u64::try_from(rounded.lo).ok()
}

/// The same, rounded down: what a swap over this range pays out.
pub fn delta_amount_a_down(lower: u128, upper: u128, liquidity: u128) -> Option<u64> {
    let (quotient, _) = delta_a_parts(lower, upper, liquidity)?;
    if quotient.hi != 0 {
        return None;
    }
    u64::try_from(quotient.lo).ok()
}

fn delta_a_parts(lower: u128, upper: u128, liquidity: u128) -> Option<(U256, U256)> {
    if lower == 0 || upper <= lower {
        return None;
    }
    let numerator = U256::mul_u128(liquidity, upper - lower);
    let denominator = U256::mul_u128(lower, upper);
    numerator.div_rem(denominator)
}

/// Meteora's token B amount for a range, rounded up.
pub fn delta_amount_b_up(lower: u128, upper: u128, liquidity: u128) -> Option<u64> {
    if upper < lower {
        return None;
    }
    let product = U256::mul_u128(liquidity, upper - lower);
    let rounded = product.hi.checked_add((product.lo != 0) as u128)?;
    u64::try_from(rounded).ok()
}

/// True when `liquidity` at `sqrt_price` deposits exactly `supply`. `⌈n/d⌉ == S` is
/// `(S − 1) × d < n <= S × d`, so this needs no division. It is what stops a creator passing a thin
/// `liquidity` to take most of the supply for a small first buy.
pub fn liquidity_deposits_exactly(
    sqrt_price: u128,
    sqrt_max_price: u128,
    liquidity: u128,
    supply: u64,
) -> bool {
    if sqrt_price == 0 || sqrt_max_price <= sqrt_price || supply == 0 {
        return false;
    }
    let numerator = U256::mul_u128(liquidity, sqrt_max_price - sqrt_price);
    let denominator = U256::mul_u128(sqrt_price, sqrt_max_price);
    match (
        denominator.checked_mul_u128(supply as u128),
        denominator.checked_mul_u128(supply as u128 - 1),
    ) {
        (Some(high), Some(low)) => numerator <= high && numerator > low,
        _ => false,
    }
}

/// `√P1 = √P0 + ⌊net × 2^128 / L⌋`, staying below the top of the range. Rounds down: the price, and
/// so the coins, never exceed what `net` pays for.
pub fn sqrt_price_after_buy(
    sqrt_price: u128,
    sqrt_max_price: u128,
    liquidity: u128,
    net_quote: u64,
) -> Option<u128> {
    if liquidity == 0 {
        return None;
    }
    if net_quote == 0 {
        return Some(sqrt_price);
    }
    let numerator = U256 {
        hi: net_quote as u128,
        lo: 0,
    };
    let (quotient, _) = numerator.div_rem(U256 {
        hi: 0,
        lo: liquidity,
    })?;
    if quotient.hi != 0 {
        return None;
    }
    let next = sqrt_price.checked_add(quotient.lo)?;
    (next < sqrt_max_price).then_some(next)
}

/// Where a launch opens after the creator's first buy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FirstBuyOpening {
    /// `√P1`, passed to Meteora as `sqrt_price`; `√P0` stays `sqrt_min_price`.
    pub sqrt_price: u128,
    /// Coins Meteora pulls: `token_a_amount`.
    pub pool_coins: u64,
    /// Quote Meteora credits the pool: `token_b_amount`. At most `net`; any difference simply stays
    /// in the creator's wallet, because Meteora pulls this amount and no more.
    pub pool_quote: u64,
    /// `supply − pool_coins`: what the creator keeps.
    pub creator_coins: u64,
}

/// The whole prediction. Every unit of `supply` is in `pool_coins + creator_coins`.
pub fn first_buy_opening(
    sqrt_open: u128,
    sqrt_max_price: u128,
    liquidity: u128,
    supply: u64,
    net_quote: u64,
) -> Option<FirstBuyOpening> {
    if !liquidity_deposits_exactly(sqrt_open, sqrt_max_price, liquidity, supply) {
        return None;
    }
    let sqrt_price = sqrt_price_after_buy(sqrt_open, sqrt_max_price, liquidity, net_quote)?;
    let pool_coins = delta_amount_a_up(sqrt_price, sqrt_max_price, liquidity)?;
    let pool_quote = delta_amount_b_up(sqrt_open, sqrt_price, liquidity)?;
    Some(FirstBuyOpening {
        sqrt_price,
        pool_coins,
        pool_quote,
        creator_coins: supply.checked_sub(pool_coins)?,
    })
}

/// The `liquidity` whose opening amount is exactly `supply`, or `None` if no such value exists.
/// `L = ⌊supply × √Pmax × √P / (√Pmax − √P)⌋`, which lands exactly on `supply` whenever one unit of
/// `L` moves the amount by less than one raw unit.
pub fn liquidity_for_exact_supply(
    sqrt_price: u128,
    sqrt_max_price: u128,
    supply: u64,
) -> Option<u128> {
    if sqrt_price == 0 || sqrt_max_price <= sqrt_price {
        return None;
    }
    let numerator = U256::mul_u128(sqrt_price, sqrt_max_price).checked_mul_u128(supply as u128)?;
    let divisor = U256 {
        hi: 0,
        lo: sqrt_max_price - sqrt_price,
    };
    let (quotient, _) = numerator.div_rem(divisor)?;
    if quotient.hi != 0 {
        return None;
    }
    let liquidity = quotient.lo;
    (delta_amount_a_up(sqrt_price, sqrt_max_price, liquidity) == Some(supply)).then_some(liquidity)
}
