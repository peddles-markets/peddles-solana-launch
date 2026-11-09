//! Holder distribution (§3.6) and `sync_vault` (§3.8).
//!
//! THE TRUST POINT, STATED PLAINLY. A merkle root's leaves cannot be checked on chain: the keeper
//! decides who is owed what, and nothing here can tell a fair allocation from a theft.
//!
//! ACCEPTED RISK, owner, 2026-11-06 ("payout 4 times a day no safety windows"): there is no veto
//! window, so a published root is claimable at once. A stolen keeper key can therefore direct ALL
//! unclaimed holder rewards to itself immediately, including entitlements published earlier and not
//! yet claimed, because each root replaces the last one whole. The only limits are:
//!
//! - the total: it may rise only by what has arrived since the last root, and must rise, so a root
//!   can never reach the platform's or the creator's money, and nothing can be rewritten while
//!   nothing arrives;
//! - the cadence: one root per launch per `ROOT_MIN_INTERVAL_SECONDS`;
//! - the guardian: it can stop roots and claims at once, for one launch or all, but only as fast as
//!   it reacts. On chain the keeper and guardian are always different keys.
//!
//! A fair root published next cannot recover what a bad root already paid.

use super::payouts::pay_from_vault;
use crate::constants::*;
use crate::errors::{fee_err, LaunchError};
use crate::events::{HolderClaimed, PauseSet, RootPublished, VaultSynced};
use crate::state::{Config, HolderClaim, LaunchRecord};
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, SyncNative, TokenAccount, TokenInterface};
use peddles_fees::{ledger, merkle};

fn sha256v(values: &[&[u8]]) -> [u8; 32] {
    solana_sha256_hasher::hashv(values).to_bytes()
}

#[derive(Accounts)]
pub struct PublishRoot<'info> {
    pub keeper: Signer<'info>,

    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = keeper,
        constraint = !config.paused @ LaunchError::Paused,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        seeds = [LAUNCH_SEED, launch_record.coin_mint.as_ref()],
        bump = launch_record.bump,
        constraint = !launch_record.paused @ LaunchError::Paused,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,
}

pub fn publish_root_handler(
    ctx: Context<PublishRoot>,
    epoch: u64,
    root: [u8; 32],
    cumulative_total: u64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let launch = ctx.accounts.launch_record.key();
    let record = &mut ctx.accounts.launch_record;
    let mut roots = record.root_state();
    roots
        .publish(
            epoch,
            root,
            cumulative_total,
            // Money recovery took can never be promised again.
            record.ledger().publishable(),
            now,
            ROOT_MIN_INTERVAL_SECONDS,
        )
        .map_err(fee_err)?;
    record.set_root_state(roots);
    emit!(RootPublished {
        launch,
        epoch,
        root,
        cumulative_total,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SetGlobalPause<'info> {
    pub guardian: Signer<'info>,

    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = guardian)]
    pub config: Box<Account<'info, Config>>,
}

/// The instant stop. It is not a window: nothing waits for it, it only ends what is happening.
/// Trading, cranks and the platform and creator payouts carry on: only holder money is at risk from
/// a bad root.
pub fn set_global_pause_handler(ctx: Context<SetGlobalPause>, paused: bool) -> Result<()> {
    ctx.accounts.config.paused = paused;
    emit!(PauseSet {
        launch: None,
        paused,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SetLaunchPause<'info> {
    pub guardian: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = guardian)]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        seeds = [LAUNCH_SEED, launch_record.coin_mint.as_ref()],
        bump = launch_record.bump,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,
}

pub fn set_launch_pause_handler(ctx: Context<SetLaunchPause>, paused: bool) -> Result<()> {
    let launch = ctx.accounts.launch_record.key();
    ctx.accounts.launch_record.paused = paused;
    emit!(PauseSet {
        launch: Some(launch),
        paused,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct ClaimHolder<'info> {
    #[account(mut)]
    pub holder: Signer<'info>,

    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = !config.paused @ LaunchError::Paused,
    )]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        seeds = [LAUNCH_SEED, launch_record.coin_mint.as_ref()],
        bump = launch_record.bump,
        constraint = !launch_record.paused @ LaunchError::Paused,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,

    /// Created on the first claim, at the holder's cost, and never closed: it blocks double claims.
    #[account(
        init_if_needed,
        payer = holder,
        space = 8 + HolderClaim::INIT_SPACE,
        seeds = [CLAIM_SEED, launch_record.key().as_ref(), holder.key().as_ref()],
        bump,
    )]
    pub holder_claim: Box<Account<'info, HolderClaim>>,

    #[account(mut, address = launch_record.fee_vault)]
    pub fee_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(address = launch_record.quote_mint)]
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init_if_needed,
        payer = holder,
        associated_token::mint = quote_mint,
        associated_token::authority = holder,
        associated_token::token_program = quote_token_program,
    )]
    pub holder_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        address = launch_record.quote_token_program @ LaunchError::WrongTokenProgram,
    )]
    pub quote_token_program: Interface<'info, TokenInterface>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn claim_holder_handler(
    ctx: Context<ClaimHolder>,
    cumulative_amount: u64,
    proof: Vec<[u8; 32]>,
) -> Result<()> {
    let launch = ctx.accounts.launch_record.key();
    let holder = ctx.accounts.holder.key();

    let record = &mut ctx.accounts.launch_record;
    let (root, root_total) = record.root_state().active_for_claim().map_err(fee_err)?;

    let leaf = merkle::leaf_hash(
        &sha256v,
        &launch.to_bytes(),
        &holder.to_bytes(),
        cumulative_amount,
    );
    merkle::verify(&sha256v, &proof, &root, leaf).map_err(fee_err)?;

    let due = ledger::holder_payout(cumulative_amount, ctx.accounts.holder_claim.claimed)
        .map_err(fee_err)?;

    // Over a shortfall this pays what is left; the rest stays claimable with this same proof.
    let mut book = record.ledger();
    let amount = book
        .pay_holder(due, root_total, ctx.accounts.fee_vault.amount)
        .map_err(fee_err)?;
    record.set_ledger(book);
    let (coin_mint, bump) = (record.coin_mint, record.bump);

    let claimed = ctx
        .accounts
        .holder_claim
        .claimed
        .checked_add(amount)
        .ok_or(LaunchError::MathOverflow)?;
    ctx.accounts.holder_claim.claimed = claimed;
    ctx.accounts.holder_claim.bump = ctx.bumps.holder_claim;

    pay_from_vault(
        ctx.accounts.quote_token_program.key(),
        ctx.accounts.fee_vault.to_account_info(),
        ctx.accounts.quote_mint.to_account_info(),
        ctx.accounts.holder_token_account.to_account_info(),
        ctx.accounts.launch_record.to_account_info(),
        coin_mint,
        bump,
        amount,
        ctx.accounts.quote_mint.decimals,
    )?;

    emit!(HolderClaimed {
        launch,
        holder,
        amount,
        cumulative_amount,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SyncVault<'info> {
    #[account(
        mut,
        seeds = [LAUNCH_SEED, launch_record.coin_mint.as_ref()],
        bump = launch_record.bump,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,

    #[account(mut, address = launch_record.fee_vault)]
    pub fee_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        address = launch_record.quote_token_program @ LaunchError::WrongTokenProgram,
    )]
    pub quote_token_program: Interface<'info, TokenInterface>,
}

/// §3.8: anything sent in directly becomes holder money. On a wrapped-SOL vault, lamports forced in
/// are wrapped first, which is how they leave (`sweep_vault_lamports` is Token-2022 only).
pub fn sync_vault_handler(ctx: Context<SyncVault>) -> Result<()> {
    if ctx.accounts.launch_record.quote_mint == WSOL_MINT {
        token_interface::sync_native(CpiContext::new(
            ctx.accounts.quote_token_program.key(),
            SyncNative {
                account: ctx.accounts.fee_vault.to_account_info(),
            },
        ))?;
        ctx.accounts.fee_vault.reload()?;
    }

    let balance = ctx.accounts.fee_vault.amount;
    let launch = ctx.accounts.launch_record.key();
    let record = &mut ctx.accounts.launch_record;
    let mut book = record.ledger();
    let credited = book.sync(balance).map_err(fee_err)?;
    record.set_ledger(book);

    emit!(VaultSynced {
        launch,
        credited_to_holders: credited,
    });
    Ok(())
}
