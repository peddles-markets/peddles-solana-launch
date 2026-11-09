//! `crank_fees` (§3.3). Anyone may run it, and it can only move fees into our own vault.

use crate::constants::*;
use crate::errors::{fee_err, LaunchError};
use crate::events::FeesCranked;
use crate::meteora;
use crate::state::LaunchRecord;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct CrankFees<'info> {
    pub cranker: Signer<'info>,

    #[account(
        mut,
        seeds = [LAUNCH_SEED, launch_record.coin_mint.as_ref()],
        bump = launch_record.bump,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,

    #[account(mut, address = launch_record.fee_vault)]
    pub fee_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(address = launch_record.coin_mint)]
    pub coin_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(address = launch_record.quote_mint)]
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: must be this launch's pool.
    #[account(address = launch_record.pool)]
    pub pool: UncheckedAccount<'info>,

    /// CHECK: must be this launch's position.
    #[account(mut, address = launch_record.position)]
    pub position: UncheckedAccount<'info>,

    /// CHECK: must be this launch's NFT account. Writable because it also fills Meteora's unused
    /// `token_a_account` slot; see `meteora::ClaimPositionFee`.
    #[account(mut, address = launch_record.position_nft_account)]
    pub position_nft_account: UncheckedAccount<'info>,

    /// CHECK: Meteora's `has_one = token_a_vault` ties it to the pool above.
    #[account(mut)]
    pub token_a_vault: UncheckedAccount<'info>,

    /// CHECK: Meteora's `has_one = token_b_vault` ties it to the pool above.
    #[account(mut)]
    pub token_b_vault: UncheckedAccount<'info>,

    /// CHECK: fixed address.
    #[account(address = METEORA_POOL_AUTHORITY)]
    pub pool_authority: UncheckedAccount<'info>,

    /// CHECK: fixed address.
    #[account(address = METEORA_EVENT_AUTHORITY)]
    pub meteora_event_authority: UncheckedAccount<'info>,

    /// CHECK: fixed address.
    #[account(address = METEORA_DAMM_V2)]
    pub meteora_program: UncheckedAccount<'info>,

    pub coin_token_program: Interface<'info, TokenInterface>,

    #[account(
        address = launch_record.quote_token_program @ LaunchError::WrongTokenProgram,
    )]
    pub quote_token_program: Interface<'info, TokenInterface>,
}

pub fn crank_fees_handler(ctx: Context<CrankFees>) -> Result<()> {
    let coin_key = ctx.accounts.launch_record.coin_mint;
    let bump = [ctx.accounts.launch_record.bump];
    let seeds: &[&[u8]] = &[LAUNCH_SEED, coin_key.as_ref(), &bump];
    let signer: &[&[&[u8]]] = &[seeds];

    let vault_before = ctx.accounts.fee_vault.amount;

    meteora::claim_position_fee(
        &meteora::ClaimPositionFee {
            pool_authority: ctx.accounts.pool_authority.to_account_info(),
            pool: ctx.accounts.pool.to_account_info(),
            position: ctx.accounts.position.to_account_info(),
            token_a_account: ctx.accounts.position_nft_account.to_account_info(),
            token_b_account: ctx.accounts.fee_vault.to_account_info(),
            token_a_vault: ctx.accounts.token_a_vault.to_account_info(),
            token_b_vault: ctx.accounts.token_b_vault.to_account_info(),
            token_a_mint: ctx.accounts.coin_mint.to_account_info(),
            token_b_mint: ctx.accounts.quote_mint.to_account_info(),
            position_nft_account: ctx.accounts.position_nft_account.to_account_info(),
            signer: ctx.accounts.launch_record.to_account_info(),
            token_a_program: ctx.accounts.coin_token_program.to_account_info(),
            token_b_program: ctx.accounts.quote_token_program.to_account_info(),
            event_authority: ctx.accounts.meteora_event_authority.to_account_info(),
            program: ctx.accounts.meteora_program.to_account_info(),
        },
        signer,
    )?;

    ctx.accounts.fee_vault.reload()?;
    let vault_after = ctx.accounts.fee_vault.amount;

    let launch = ctx.accounts.launch_record.key();
    let record = &mut ctx.accounts.launch_record;
    let mut book = record.ledger();
    let split = book
        .credit_claim(
            vault_before,
            vault_after,
            record.rate_bps,
            record.excess_to_creator_bps,
        )
        .map_err(fee_err)?;
    record.set_ledger(book);

    emit!(FeesCranked {
        launch,
        arrived: vault_after.saturating_sub(vault_before),
        platform: split.platform,
        creator: split.creator().map_err(fee_err)?,
        holders: split.holders,
    });
    Ok(())
}
