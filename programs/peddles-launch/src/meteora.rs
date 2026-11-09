//! Hand-built CPIs into Meteora DAMM v2 and Metaplex Token Metadata (§4).
//!
//! No dependency on Meteora's crate: account order is the Anchor struct's field order at damm-v2
//! commit `3493cd6`, with `event_authority` and `program` appended by `#[event_cpi]`, and the
//! instruction data comes from `peddles_fees::ix_data`. The program id is a constant, so a wrong
//! account here fails inside Meteora's own constraints rather than reaching a look-alike.
//!
//! Every Meteora PDA passed to `initialize_customizable_pool` is an `init` with seeds on Meteora's
//! side, so Meteora itself proves each one is the right address.

use crate::constants::{METAPLEX_TOKEN_METADATA, METEORA_DAMM_V2};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::invoke_signed;
use peddles_fees::ix_data;
use peddles_fees::schedule::FeeSchedule;

fn readonly(info: &AccountInfo) -> AccountMeta {
    AccountMeta::new_readonly(*info.key, false)
}

fn writable(info: &AccountInfo) -> AccountMeta {
    AccountMeta::new(*info.key, false)
}

fn readonly_signer(info: &AccountInfo) -> AccountMeta {
    AccountMeta::new_readonly(*info.key, true)
}

fn writable_signer(info: &AccountInfo) -> AccountMeta {
    AccountMeta::new(*info.key, true)
}

/// §4.1. `creator` is the `LaunchRecord`, which becomes `pool.creator` and the NFT account's owner.
pub struct InitializeCustomizablePool<'info> {
    pub creator: AccountInfo<'info>,
    pub position_nft_mint: AccountInfo<'info>,
    pub position_nft_account: AccountInfo<'info>,
    pub payer: AccountInfo<'info>,
    pub pool_authority: AccountInfo<'info>,
    pub pool: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub token_a_mint: AccountInfo<'info>,
    pub token_b_mint: AccountInfo<'info>,
    pub token_a_vault: AccountInfo<'info>,
    pub token_b_vault: AccountInfo<'info>,
    pub payer_token_a: AccountInfo<'info>,
    pub payer_token_b: AccountInfo<'info>,
    pub token_a_program: AccountInfo<'info>,
    pub token_b_program: AccountInfo<'info>,
    pub token_2022_program: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub event_authority: AccountInfo<'info>,
    pub program: AccountInfo<'info>,
    /// remaining[0]: any account; the coin is plain SPL Token and needs no badge.
    pub token_a_badge: AccountInfo<'info>,
    /// remaining[1]: `[b"token_badge", b_mint]`.
    pub token_b_badge: AccountInfo<'info>,
}

#[inline(never)]
pub fn initialize_customizable_pool<'info>(
    a: &InitializeCustomizablePool<'info>,
    schedule: &FeeSchedule,
    sqrt_min_price: u128,
    sqrt_max_price: u128,
    liquidity: u128,
    sqrt_price: u128,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let ix = Instruction {
        program_id: METEORA_DAMM_V2,
        accounts: vec![
            readonly(&a.creator),
            writable_signer(&a.position_nft_mint),
            writable(&a.position_nft_account),
            writable_signer(&a.payer),
            readonly(&a.pool_authority),
            writable(&a.pool),
            writable(&a.position),
            readonly(&a.token_a_mint),
            readonly(&a.token_b_mint),
            writable(&a.token_a_vault),
            writable(&a.token_b_vault),
            writable(&a.payer_token_a),
            writable(&a.payer_token_b),
            readonly(&a.token_a_program),
            readonly(&a.token_b_program),
            readonly(&a.token_2022_program),
            readonly(&a.system_program),
            readonly(&a.event_authority),
            readonly(&a.program),
            readonly(&a.token_a_badge),
            readonly(&a.token_b_badge),
        ],
        data: ix_data::initialize_customizable_pool(
            schedule,
            sqrt_min_price,
            sqrt_max_price,
            liquidity,
            sqrt_price,
        )
        .to_vec(),
    };
    invoke_signed(
        &ix,
        &[
            a.creator.clone(),
            a.position_nft_mint.clone(),
            a.position_nft_account.clone(),
            a.payer.clone(),
            a.pool_authority.clone(),
            a.pool.clone(),
            a.position.clone(),
            a.token_a_mint.clone(),
            a.token_b_mint.clone(),
            a.token_a_vault.clone(),
            a.token_b_vault.clone(),
            a.payer_token_a.clone(),
            a.payer_token_b.clone(),
            a.token_a_program.clone(),
            a.token_b_program.clone(),
            a.token_2022_program.clone(),
            a.system_program.clone(),
            a.event_authority.clone(),
            a.program.clone(),
            a.token_a_badge.clone(),
            a.token_b_badge.clone(),
        ],
        signer_seeds,
    )
    .map_err(Into::into)
}

/// §4.2.
pub struct PermanentLockPosition<'info> {
    pub pool: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub position_nft_account: AccountInfo<'info>,
    pub signer: AccountInfo<'info>,
    pub event_authority: AccountInfo<'info>,
    pub program: AccountInfo<'info>,
}

#[inline(never)]
pub fn permanent_lock_position<'info>(
    a: &PermanentLockPosition<'info>,
    liquidity: u128,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let ix = Instruction {
        program_id: METEORA_DAMM_V2,
        accounts: vec![
            writable(&a.pool),
            writable(&a.position),
            readonly(&a.position_nft_account),
            readonly_signer(&a.signer),
            readonly(&a.event_authority),
            readonly(&a.program),
        ],
        data: ix_data::permanent_lock_position(liquidity).to_vec(),
    };
    invoke_signed(
        &ix,
        &[
            a.pool.clone(),
            a.position.clone(),
            a.position_nft_account.clone(),
            a.signer.clone(),
            a.event_authority.clone(),
            a.program.clone(),
        ],
        signer_seeds,
    )
    .map_err(Into::into)
}

/// §4.3. Meteora's `has_one` constraints tie both vaults and both mints to the pool, and the owner
/// path of `assert_authority_with_owner_destinations` lets the fee go to our `FeeVault`.
///
/// `token_a_account` receives nothing under `OnlyB`, but Meteora still wants a writable token account
/// there and checks neither its mint nor its owner. We pass the launch's own position NFT account,
/// so a crank write-locks no one else's account and nobody needs an account per coin. Should a token
/// A fee ever exist, the mint mismatch reverts the claim rather than paying the cranker.
pub struct ClaimPositionFee<'info> {
    pub pool_authority: AccountInfo<'info>,
    pub pool: AccountInfo<'info>,
    pub position: AccountInfo<'info>,
    pub token_a_account: AccountInfo<'info>,
    pub token_b_account: AccountInfo<'info>,
    pub token_a_vault: AccountInfo<'info>,
    pub token_b_vault: AccountInfo<'info>,
    pub token_a_mint: AccountInfo<'info>,
    pub token_b_mint: AccountInfo<'info>,
    pub position_nft_account: AccountInfo<'info>,
    pub signer: AccountInfo<'info>,
    pub token_a_program: AccountInfo<'info>,
    pub token_b_program: AccountInfo<'info>,
    pub event_authority: AccountInfo<'info>,
    pub program: AccountInfo<'info>,
}

#[inline(never)]
pub fn claim_position_fee<'info>(
    a: &ClaimPositionFee<'info>,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let ix = Instruction {
        program_id: METEORA_DAMM_V2,
        accounts: vec![
            readonly(&a.pool_authority),
            readonly(&a.pool),
            writable(&a.position),
            writable(&a.token_a_account),
            writable(&a.token_b_account),
            writable(&a.token_a_vault),
            writable(&a.token_b_vault),
            readonly(&a.token_a_mint),
            readonly(&a.token_b_mint),
            readonly(&a.position_nft_account),
            readonly_signer(&a.signer),
            readonly(&a.token_a_program),
            readonly(&a.token_b_program),
            readonly(&a.event_authority),
            readonly(&a.program),
        ],
        data: ix_data::CLAIM_POSITION_FEE.to_vec(),
    };
    invoke_signed(
        &ix,
        &[
            a.pool_authority.clone(),
            a.pool.clone(),
            a.position.clone(),
            a.token_a_account.clone(),
            a.token_b_account.clone(),
            a.token_a_vault.clone(),
            a.token_b_vault.clone(),
            a.token_a_mint.clone(),
            a.token_b_mint.clone(),
            a.position_nft_account.clone(),
            a.signer.clone(),
            a.token_a_program.clone(),
            a.token_b_program.clone(),
            a.event_authority.clone(),
            a.program.clone(),
        ],
        signer_seeds,
    )
    .map_err(Into::into)
}

/// `CreateMetadataAccountV3`, account order from mpl-token-metadata's generated client (no `rent`).
pub struct CreateMetadataAccount<'info> {
    pub metadata: AccountInfo<'info>,
    pub mint: AccountInfo<'info>,
    pub mint_authority: AccountInfo<'info>,
    pub payer: AccountInfo<'info>,
    pub update_authority: AccountInfo<'info>,
    pub system_program: AccountInfo<'info>,
    pub program: AccountInfo<'info>,
}

#[inline(never)]
pub fn create_metadata_account_v3<'info>(
    a: &CreateMetadataAccount<'info>,
    data: &[u8],
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    let ix = Instruction {
        program_id: METAPLEX_TOKEN_METADATA,
        accounts: vec![
            writable(&a.metadata),
            readonly(&a.mint),
            readonly_signer(&a.mint_authority),
            writable_signer(&a.payer),
            readonly_signer(&a.update_authority),
            readonly(&a.system_program),
        ],
        data: data.to_vec(),
    };
    invoke_signed(
        &ix,
        &[
            a.metadata.clone(),
            a.mint.clone(),
            a.mint_authority.clone(),
            a.payer.clone(),
            a.update_authority.clone(),
            a.system_program.clone(),
            a.program.clone(),
        ],
        signer_seeds,
    )
    .map_err(Into::into)
}
