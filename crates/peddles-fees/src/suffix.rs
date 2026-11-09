//! The base58 ending of a mint address (`PEDL`, owner, 2026-11-05).
//!
//! A base58 address is a big-endian number written in base 58, so its last four characters are that
//! number modulo 58^4. No encoding, no allocation, a few hundred compute units.
//!
//! A coin mint is a PDA in one of two forms, chosen by a launch argument:
//!
//! - unbound `[b"coin", salt]`: the API's instant pool of pre-ground salts. Anyone holding the salt
//!   can use it.
//! - bound `[b"coin", creator, salt]`: a Pre-Launch address. Only that creator can launch at it,
//!   because the program derives it from the signer.
//!
//! The two cannot collide: the seed bytes hashed differ in length by 32. An empty seed adds nothing
//! to the hash, which is what lets one `seeds = [...]` list express both.

/// 58^4.
const SUFFIX_MODULUS: u64 = 11_316_496;
/// `PEDL` as base58 digits: P=22, E=13, D=12, L=19, i.e. 22×58³ + 13×58² + 12×58 + 19.
pub const PEDL_SUFFIX_VALUE: u64 = 4_336_911;

/// The address modulo 58^4.
pub fn base58_suffix_value(address: &[u8; 32]) -> u64 {
    let mut remainder: u64 = 0;
    for byte in address {
        // remainder < 58^4 < 2^24, so this cannot overflow.
        remainder = (remainder * 256 + *byte as u64) % SUFFIX_MODULUS;
    }
    remainder
}

/// True when the address's base58 form ends in `PEDL`.
pub fn ends_with_pedl(address: &[u8; 32]) -> bool {
    base58_suffix_value(address) == PEDL_SUFFIX_VALUE
}

/// The middle seed of the coin mint PDA: the creator's address when bound, nothing when not.
pub fn binding_seed(bound_to_creator: bool, creator: &[u8]) -> &[u8] {
    if bound_to_creator {
        creator
    } else {
        &[]
    }
}
