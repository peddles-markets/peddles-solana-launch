//! The holder-claim merkle tree (§3.6).
//!
//! Generic over the hash so it needs no dependency: on chain the program passes the SHA-256 syscall;
//! the tests pass a plain SHA-256. The keeper must build the tree with exactly these rules:
//!
//! - leaf = `sha256(0x00 || launch (32) || holder (32) || cumulative_amount (u64 LE))`
//! - node = `sha256(0x01 || min(a, b) || max(a, b))`, compared as big-endian byte strings
//!
//! The prefixes stop a 64-byte inner node being replayed as a leaf. The launch in the leaf stops a
//! proof being replayed on another launch. Sorting each pair means a proof carries no directions.

use crate::FeeError;

const LEAF_PREFIX: [u8; 1] = [0x00];
const NODE_PREFIX: [u8; 1] = [0x01];

/// The longest proof accepted: 2^24 holders, far beyond what fits in a transaction anyway.
pub const MAX_PROOF_LEN: usize = 24;

/// The hash of one holder's entry.
pub fn leaf_hash<H>(
    hashv: &H,
    launch: &[u8; 32],
    holder: &[u8; 32],
    cumulative_amount: u64,
) -> [u8; 32]
where
    H: Fn(&[&[u8]]) -> [u8; 32],
{
    let amount = cumulative_amount.to_le_bytes();
    hashv(&[&LEAF_PREFIX[..], &launch[..], &holder[..], &amount[..]])
}

/// The hash of two children, order-independent.
pub fn node_hash<H>(hashv: &H, a: &[u8; 32], b: &[u8; 32]) -> [u8; 32]
where
    H: Fn(&[&[u8]]) -> [u8; 32],
{
    let (left, right) = if a <= b { (a, b) } else { (b, a) };
    hashv(&[&NODE_PREFIX[..], &left[..], &right[..]])
}

/// Walk `proof` from `leaf` and compare against `root`.
pub fn verify<H>(
    hashv: &H,
    proof: &[[u8; 32]],
    root: &[u8; 32],
    leaf: [u8; 32],
) -> Result<(), FeeError>
where
    H: Fn(&[&[u8]]) -> [u8; 32],
{
    if proof.len() > MAX_PROOF_LEN {
        return Err(FeeError::ProofTooLong);
    }
    let mut computed = leaf;
    for sibling in proof {
        computed = node_hash(hashv, &computed, sibling);
    }
    if &computed == root {
        Ok(())
    } else {
        Err(FeeError::InvalidProof)
    }
}
