//! Instruction data for the hand-built CPIs of §4.
//!
//! Discriminators are `sha256("global:<name>")[..8]`, recomputed on 2026-11-06, and the first two
//! match mainnet transaction `56ybV1pr…AfaFW` (slot 461,823,012). Meteora's layouts are from
//! damm-v2 commit `3493cd6`; the tests re-encode that transaction's parameters and compare bytes.

use crate::schedule::{FeeSchedule, BASE_FEE_DATA_LEN};
use crate::FeeError;

pub const INITIALIZE_CUSTOMIZABLE_POOL: [u8; 8] = [0x14, 0xa1, 0xf1, 0x18, 0xbd, 0xdd, 0xb4, 0x02];
pub const PERMANENT_LOCK_POSITION: [u8; 8] = [0xa5, 0xb0, 0x7d, 0x06, 0xe7, 0xab, 0xba, 0xd5];
pub const CLAIM_POSITION_FEE: [u8; 8] = [0xb4, 0x26, 0x9a, 0x11, 0x85, 0x21, 0xa2, 0xd3];

/// `activation_type = 1`: timestamp.
pub const ACTIVATION_TYPE_TIMESTAMP: u8 = 1;
/// `collect_fee_mode = 1`: `CollectFeeMode::OnlyB`.
pub const COLLECT_FEE_MODE_ONLY_B: u8 = 1;

/// Length of `initialize_customizable_pool` data with no dynamic fee and no activation point.
pub const INITIALIZE_POOL_DATA_LEN: usize = 107;

/// `InitializeCustomizablePoolParameters`, with the fields this program always fixes left out:
/// no compounding fee, no dynamic fee, no alpha vault, timestamp activation at the current
/// timestamp, fees in token B only.
pub fn initialize_customizable_pool(
    schedule: &FeeSchedule,
    sqrt_min_price: u128,
    sqrt_max_price: u128,
    liquidity: u128,
    sqrt_price: u128,
) -> [u8; INITIALIZE_POOL_DATA_LEN] {
    let mut out = [0u8; INITIALIZE_POOL_DATA_LEN];
    out[0..8].copy_from_slice(&INITIALIZE_CUSTOMIZABLE_POOL);
    let mut at = 8;
    out[at..at + BASE_FEE_DATA_LEN].copy_from_slice(&schedule.to_borsh_bytes());
    at += BASE_FEE_DATA_LEN;
    // compounding_fee_bps: u16 = 0, padding: u8 = 0, dynamic_fee: None.
    at += 2 + 1 + 1;
    out[at..at + 16].copy_from_slice(&sqrt_min_price.to_le_bytes());
    at += 16;
    out[at..at + 16].copy_from_slice(&sqrt_max_price.to_le_bytes());
    at += 16;
    // has_alpha_vault: false.
    at += 1;
    out[at..at + 16].copy_from_slice(&liquidity.to_le_bytes());
    at += 16;
    out[at..at + 16].copy_from_slice(&sqrt_price.to_le_bytes());
    at += 16;
    out[at] = ACTIVATION_TYPE_TIMESTAMP;
    out[at + 1] = COLLECT_FEE_MODE_ONLY_B;
    // activation_point: None, the last byte, is already 0.
    out
}

/// `permanent_lock_position(permanent_lock_liquidity)`.
pub fn permanent_lock_position(liquidity: u128) -> [u8; 24] {
    let mut out = [0u8; 24];
    out[0..8].copy_from_slice(&PERMANENT_LOCK_POSITION);
    out[8..24].copy_from_slice(&liquidity.to_le_bytes());
    out
}

/// Metaplex limits, enforced there too; checked here so a launch fails before it spends anything.
pub const MAX_NAME_LEN: usize = 32;
pub const MAX_SYMBOL_LEN: usize = 10;
pub const MAX_URI_LEN: usize = 200;
/// Discriminator of `CreateMetadataAccountV3` in mpl-token-metadata's generated client.
pub const CREATE_METADATA_ACCOUNT_V3: u8 = 33;
/// Enough for the longest name, symbol and uri.
pub const CREATE_METADATA_MAX_LEN: usize = 1
    + (4 + MAX_NAME_LEN)
    + (4 + MAX_SYMBOL_LEN)
    + (4 + MAX_URI_LEN)
    + 2
    + CREATORS_LEN
    + 1
    + 1
    + 1
    + 1;
/// `Some(vec![Creator { address, verified: true, share: 100 }])`: 1 + 4 + 32 + 1 + 1.
const CREATORS_LEN: usize = 39;

fn write_string(out: &mut [u8], at: usize, value: &[u8]) -> usize {
    out[at..at + 4].copy_from_slice(&(value.len() as u32).to_le_bytes());
    out[at + 4..at + 4 + value.len()].copy_from_slice(value);
    at + 4 + value.len()
}

/// `CreateMetadataAccountV3` with `is_mutable = false`, zero royalties, one verified creator at 100%
/// and no collection, uses or collection details. Returns the encoded length.
///
/// Metaplex lets `verified` be set here only when the creator IS the update authority and signs
/// (`assert_data_valid`), so `creator` must be the account signing as update authority. The share
/// is Metaplex's split of royalties, and `seller_fee_basis_points` is 0, so it moves no money.
pub fn create_metadata_account_v3(
    out: &mut [u8; CREATE_METADATA_MAX_LEN],
    name: &[u8],
    symbol: &[u8],
    uri: &[u8],
    creator: &[u8; 32],
) -> Result<usize, FeeError> {
    if name.len() > MAX_NAME_LEN || symbol.len() > MAX_SYMBOL_LEN || uri.len() > MAX_URI_LEN {
        return Err(FeeError::MetadataTooLong);
    }
    *out = [0u8; CREATE_METADATA_MAX_LEN];
    out[0] = CREATE_METADATA_ACCOUNT_V3;
    let mut at = write_string(out, 1, name);
    at = write_string(out, at, symbol);
    at = write_string(out, at, uri);
    // seller_fee_basis_points: u16 = 0, already zero.
    at += 2;
    out[at] = 1; // creators: Some
    out[at + 1..at + 5].copy_from_slice(&1u32.to_le_bytes());
    out[at + 5..at + 37].copy_from_slice(creator);
    out[at + 37] = 1; // verified
    out[at + 38] = 100; // share
    at += CREATORS_LEN;
    // collection: None, uses: None, is_mutable: false, collection_details: None. All zero bytes.
    Ok(at + 1 + 1 + 1 + 1)
}
