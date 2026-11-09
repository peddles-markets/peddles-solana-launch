//! Admin recovery (§3.9). Owner, 2026-11-06: "admin can always recover funds. funds can never get
//! stuck." No timelock, by the owner's decision.
//!
//! Signed by `Config.admin`, the Squads multisig, and nothing else. No check here reads a pause, a
//! keeper, a guardian, a creator or a holder, so recovery works whatever state those are in. It pays
//! a destination named in the call, and the ledger comes down by exactly what left, so later payouts
//! and claims stay exact (`peddles_fees::ledger`).
//!
//! What this cannot reach, stated in §3.9: the permanently locked liquidity in Meteora, which is the
//! product, and a quote token its issuer has frozen, paused, hooked or seized. Those refuse here
//! rather than reverting somewhere deep, and a refused recovery changes nothing.

use super::payouts::pay_from_vault;
use crate::constants::*;
use crate::errors::{fee_err, LaunchError};
use crate::events::{AdminRecovered, AdminRecoveredLamports};
use crate::state::{Config, LaunchRecord};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    self, Mint, TokenAccount, TokenInterface, WithdrawExcessLamports,
};
use peddles_fees::ledger::RecoverBucket;
use peddles_fees::mint as mint_checks;

/// Which part of the vault a recovery takes from.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Platform,
    Creator,
    Holders,
    /// What the vault holds above the ledger.
    Surplus,
}

impl From<Bucket> for RecoverBucket {
    fn from(bucket: Bucket) -> Self {
        match bucket {
            Bucket::Platform => RecoverBucket::Platform,
            Bucket::Creator => RecoverBucket::Creator,
            Bucket::Holders => RecoverBucket::Holders,
            Bucket::Surplus => RecoverBucket::Surplus,
        }
    }
}

#[derive(Accounts)]
pub struct AdminRecover<'info> {
    pub admin: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin)]
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

    /// Named in the call: any token account of the quote mint.
    #[account(mut, token::mint = quote_mint, token::token_program = quote_token_program)]
    pub destination: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(address = launch_record.quote_token_program @ LaunchError::WrongTokenProgram)]
    pub quote_token_program: Interface<'info, TokenInterface>,
}

pub fn admin_recover_handler(ctx: Context<AdminRecover>, bucket: Bucket, amount: u64) -> Result<()> {
    require_keys_neq!(
        ctx.accounts.fee_vault.key(),
        ctx.accounts.destination.key(),
        LaunchError::RecoverToSource
    );
    // Named refusals for the issuer's powers. Each would revert in the transfer anyway; this says why.
    require!(
        !ctx.accounts.fee_vault.is_frozen() && !ctx.accounts.destination.is_frozen(),
        LaunchError::AccountFrozen
    );
    {
        let mint_info = ctx.accounts.quote_mint.to_account_info();
        let mint_data = mint_info.try_borrow_data()?;
        mint_checks::require_transferable(&mint_data).map_err(fee_err)?;
    }

    let before = ctx.accounts.fee_vault.amount;
    let launch = ctx.accounts.launch_record.key();
    let record = &mut ctx.accounts.launch_record;
    let mut book = record.ledger();
    book.recover(bucket.into(), amount, before).map_err(fee_err)?;
    record.set_ledger(book);
    let (coin_mint, bump) = (record.coin_mint, record.bump);

    pay_from_vault(
        ctx.accounts.quote_token_program.key(),
        ctx.accounts.fee_vault.to_account_info(),
        ctx.accounts.quote_mint.to_account_info(),
        ctx.accounts.destination.to_account_info(),
        ctx.accounts.launch_record.to_account_info(),
        coin_mint,
        bump,
        amount,
        ctx.accounts.quote_mint.decimals,
    )?;

    ctx.accounts.fee_vault.reload()?;
    require!(
        before.checked_sub(ctx.accounts.fee_vault.amount) == Some(amount),
        LaunchError::RecoverMismatch
    );

    emit!(AdminRecovered {
        launch,
        bucket: bucket as u8,
        amount,
        destination: ctx.accounts.destination.key(),
        platform_owed: book.platform_owed,
        creator_owed: book.creator_owed,
        holder_recovered: book.holder_recovered,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct AdminRecoverLamports<'info> {
    pub admin: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin)]
    pub config: Box<Account<'info, Config>>,

    /// CHECK: any account this program owns; only lamports above rent move.
    #[account(mut)]
    pub target: UncheckedAccount<'info>,

    /// CHECK: named in the call.
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,
}

pub fn admin_recover_lamports_handler(ctx: Context<AdminRecoverLamports>) -> Result<()> {
    let target = ctx.accounts.target.to_account_info();
    require!(target.owner == &crate::ID, LaunchError::NotProgramOwned);
    require_keys_neq!(target.key(), ctx.accounts.destination.key(), LaunchError::RecoverToSource);

    let keep = Rent::get()?.minimum_balance(target.data_len());
    let excess = target.lamports().saturating_sub(keep);
    require!(excess > 0, LaunchError::NothingToSweep);

    target.sub_lamports(excess)?;
    ctx.accounts.destination.to_account_info().add_lamports(excess)?;

    emit!(AdminRecoveredLamports {
        target: target.key(),
        amount: excess,
        destination: ctx.accounts.destination.key(),
    });
    Ok(())
}

#[derive(Accounts)]
pub struct AdminRecoverVaultLamports<'info> {
    pub admin: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin)]
    pub config: Box<Account<'info, Config>>,

    #[account(
        seeds = [LAUNCH_SEED, launch_record.coin_mint.as_ref()],
        bump = launch_record.bump,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,

    /// CHECK: must be this launch's vault; the token program does the rent arithmetic.
    #[account(mut, address = launch_record.fee_vault)]
    pub fee_vault: UncheckedAccount<'info>,

    /// CHECK: named in the call.
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,

    #[account(address = launch_record.quote_token_program @ LaunchError::WrongTokenProgram)]
    pub quote_token_program: Interface<'info, TokenInterface>,
}

/// A `FeeVault` is owned by the token program, so `admin_recover_lamports` cannot reach it. Only
/// Token-2022 has this instruction; on a wrapped-SOL vault `sync_vault` wraps stray lamports, and
/// `admin_recover` then takes them as tokens.
pub fn admin_recover_vault_lamports_handler(ctx: Context<AdminRecoverVaultLamports>) -> Result<()> {
    require!(
        ctx.accounts.quote_token_program.key() == anchor_spl::token_2022::ID,
        LaunchError::VaultLamportsViaSync
    );
    let vault = ctx.accounts.fee_vault.to_account_info();
    let destination = ctx.accounts.destination.to_account_info();
    let before = vault.lamports();

    let coin_key = ctx.accounts.launch_record.coin_mint;
    let bump = [ctx.accounts.launch_record.bump];
    let seeds: &[&[u8]] = &[LAUNCH_SEED, coin_key.as_ref(), &bump];
    let signer: &[&[&[u8]]] = &[seeds];

    token_interface::withdraw_excess_lamports(CpiContext::new_with_signer(
        ctx.accounts.quote_token_program.key(),
        WithdrawExcessLamports {
            source: vault.clone(),
            destination,
            authority: ctx.accounts.launch_record.to_account_info(),
        },
        signer,
    ))?;

    emit!(AdminRecoveredLamports {
        target: vault.key(),
        amount: before.saturating_sub(vault.lamports()),
        destination: ctx.accounts.destination.key(),
    });
    Ok(())
}
