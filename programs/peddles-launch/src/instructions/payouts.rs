//! `sweep_platform`, `claim_creator`, `set_creator_payout` (§3.4, §3.5).
//!
//! Separate from the crank so a frozen recipient account can never block the other legs. The
//! destination is always derived, never a parameter. Both payouts are permissionless pushes: a
//! caller can only send the owed amount to its rightful owner. Each pays what the vault holds, up to
//! what is owed, and the rest stays owed, so a short vault cannot block a leg forever. The ledger
//! moves before the transfer, though a failed transfer reverts the whole transaction anyway.

use crate::constants::*;
use crate::errors::{fee_err, LaunchError};
use crate::events::{CreatorClaimed, CreatorPayoutChanged, PlatformSwept};
use crate::state::{Config, LaunchRecord};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

/// Send `amount` out of a launch's `FeeVault`. With a transfer-fee quote the recipient gets it net
/// of that fee; the vault falls by exactly `amount`, which is what the ledger booked.
#[allow(clippy::too_many_arguments)]
pub(crate) fn pay_from_vault<'info>(
    token_program: Pubkey,
    fee_vault: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    to: AccountInfo<'info>,
    launch_record: AccountInfo<'info>,
    coin_mint: Pubkey,
    record_bump: u8,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    let bump = [record_bump];
    let seeds: &[&[u8]] = &[LAUNCH_SEED, coin_mint.as_ref(), &bump];
    let signer: &[&[&[u8]]] = &[seeds];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            token_program,
            TransferChecked {
                from: fee_vault,
                mint,
                to,
                authority: launch_record,
            },
            signer,
        ),
        amount,
        decimals,
    )
}

#[derive(Accounts)]
pub struct SweepPlatform<'info> {
    /// Pays only if the platform's token account does not exist yet.
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        seeds = [LAUNCH_SEED, launch_record.coin_mint.as_ref()],
        bump = launch_record.bump,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,

    #[account(mut, address = launch_record.fee_vault)]
    pub fee_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(address = launch_record.quote_mint)]
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: must be the configured wallet.
    #[account(address = config.platform_wallet)]
    pub platform_wallet: UncheckedAccount<'info>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = quote_mint,
        associated_token::authority = platform_wallet,
        associated_token::token_program = quote_token_program,
    )]
    pub platform_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        address = launch_record.quote_token_program @ LaunchError::WrongTokenProgram,
    )]
    pub quote_token_program: Interface<'info, TokenInterface>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn sweep_platform_handler(ctx: Context<SweepPlatform>) -> Result<()> {
    let launch = ctx.accounts.launch_record.key();
    let record = &mut ctx.accounts.launch_record;
    let mut book = record.ledger();
    let amount = book
        .take_platform(ctx.accounts.fee_vault.amount)
        .map_err(fee_err)?;
    record.set_ledger(book);
    let (coin_mint, bump) = (record.coin_mint, record.bump);

    pay_from_vault(
        ctx.accounts.quote_token_program.key(),
        ctx.accounts.fee_vault.to_account_info(),
        ctx.accounts.quote_mint.to_account_info(),
        ctx.accounts.platform_token_account.to_account_info(),
        ctx.accounts.launch_record.to_account_info(),
        coin_mint,
        bump,
        amount,
        ctx.accounts.quote_mint.decimals,
    )?;

    emit!(PlatformSwept { launch, amount });
    Ok(())
}

#[derive(Accounts)]
pub struct ClaimCreator<'info> {
    /// Pays only if the payout's token account does not exist yet.
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds = [LAUNCH_SEED, launch_record.coin_mint.as_ref()],
        bump = launch_record.bump,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,

    #[account(mut, address = launch_record.fee_vault)]
    pub fee_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(address = launch_record.quote_mint)]
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: must be the launch's current payout address.
    #[account(address = launch_record.creator_payout)]
    pub creator_payout: UncheckedAccount<'info>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = quote_mint,
        associated_token::authority = creator_payout,
        associated_token::token_program = quote_token_program,
    )]
    pub payout_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        address = launch_record.quote_token_program @ LaunchError::WrongTokenProgram,
    )]
    pub quote_token_program: Interface<'info, TokenInterface>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn claim_creator_handler(ctx: Context<ClaimCreator>) -> Result<()> {
    let launch = ctx.accounts.launch_record.key();
    let record = &mut ctx.accounts.launch_record;
    let mut book = record.ledger();
    let amount = book
        .take_creator(ctx.accounts.fee_vault.amount)
        .map_err(fee_err)?;
    record.set_ledger(book);
    let (coin_mint, bump, payout) = (record.coin_mint, record.bump, record.creator_payout);

    pay_from_vault(
        ctx.accounts.quote_token_program.key(),
        ctx.accounts.fee_vault.to_account_info(),
        ctx.accounts.quote_mint.to_account_info(),
        ctx.accounts.payout_token_account.to_account_info(),
        ctx.accounts.launch_record.to_account_info(),
        coin_mint,
        bump,
        amount,
        ctx.accounts.quote_mint.decimals,
    )?;

    emit!(CreatorClaimed {
        launch,
        payout,
        amount,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SetCreatorPayout<'info> {
    pub creator_payout: Signer<'info>,

    #[account(
        mut,
        seeds = [LAUNCH_SEED, launch_record.coin_mint.as_ref()],
        bump = launch_record.bump,
        has_one = creator_payout,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,
}

/// The only term of a launch that changes, as on EVM.
pub fn set_creator_payout_handler(ctx: Context<SetCreatorPayout>, new_payout: Pubkey) -> Result<()> {
    require!(new_payout != Pubkey::default(), LaunchError::ZeroAddress);
    let launch = ctx.accounts.launch_record.key();
    ctx.accounts.launch_record.creator_payout = new_payout;
    emit!(CreatorPayoutChanged {
        launch,
        payout: new_payout,
    });
    Ok(())
}
