//! All of the Peddles Solana launch arithmetic, and nothing else.
//!
//! No Solana SDK, no Anchor, no allocator: every function here takes integers or bytes and returns
//! integers or bytes, so all of it can be tested without a validator. The on-chain program in
//! `programs/peddles-launch` only moves accounts around these functions.
//!
//! Amounts are raw base units, always. Nothing here knows a UI amount, a multiplier or a price in
//! dollars: a 10x `ScaledUiAmount` display change must never become 10x money.
//!
//! Specification: `docs/SOLANA_LAUNCH_PROGRAM.md`, §1 (fees), §2 (schedule), §3 (ledger and roots),
//! §4 (Meteora encodings). Meteora facts are from damm-v2 commit
//! `3493cd67251ad77b71f8fc9999c0ad90aeed7d68`.

#![no_std]
#![forbid(unsafe_code)]

pub mod error;
pub mod ix_data;
pub mod ledger;
pub mod merkle;
pub mod mint;
pub mod price;
pub mod roots;
pub mod schedule;
pub mod split;
pub mod suffix;
pub mod transfer_fee;

pub use error::FeeError;
