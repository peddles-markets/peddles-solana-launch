//! Admin (§3.7). Signer: `Config.admin`, the Squads multisig.
//!
//! Nothing here can touch a launched coin's terms. `set_quote` changes `QuoteConfig` only, and a
//! launch copied everything it needs out of that into its own record.

use crate::constants::*;
use crate::errors::LaunchError;
use crate::state::{Config, QuoteConfig};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    pub admin: Signer<'info>,

    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump, has_one = admin)]
    pub config: Account<'info, Config>,
}

/// Bounded by the compiled-in cap, the immutable cap the EVM has.
pub fn set_launch_fee_handler(ctx: Context<AdminOnly>, lamports: u64) -> Result<()> {
    require!(
        lamports <= MAX_LAUNCH_FEE_LAMPORTS,
        LaunchError::LaunchFeeAboveCap
    );
    ctx.accounts.config.launch_fee_lamports = lamports;
    Ok(())
}

pub fn set_keeper_handler(ctx: Context<AdminOnly>, keeper: Pubkey) -> Result<()> {
    require!(keeper != Pubkey::default(), LaunchError::ZeroAddress);
    require_keys_neq!(keeper, ctx.accounts.config.guardian, LaunchError::KeeperIsGuardian);
    ctx.accounts.config.keeper = keeper;
    Ok(())
}

pub fn set_guardian_handler(ctx: Context<AdminOnly>, guardian: Pubkey) -> Result<()> {
    require!(guardian != Pubkey::default(), LaunchError::ZeroAddress);
    require_keys_neq!(guardian, ctx.accounts.config.keeper, LaunchError::KeeperIsGuardian);
    ctx.accounts.config.guardian = guardian;
    Ok(())
}

/// A wallet that is lost would otherwise keep receiving every launch's platform leg, because
/// `sweep_platform` is a permissionless push to it. The owner's rule: funds can never get stuck.
pub fn set_platform_wallet_handler(ctx: Context<AdminOnly>, wallet: Pubkey) -> Result<()> {
    require!(wallet != Pubkey::default(), LaunchError::ZeroAddress);
    ctx.accounts.config.platform_wallet = wallet;
    Ok(())
}

pub fn propose_admin_handler(ctx: Context<AdminOnly>, new_admin: Pubkey) -> Result<()> {
    require!(new_admin != Pubkey::default(), LaunchError::ZeroAddress);
    ctx.accounts.config.pending_admin = new_admin;
    Ok(())
}

#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    pub pending_admin: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = pending_admin @ LaunchError::NotPendingAdmin,
    )]
    pub config: Account<'info, Config>,
}

pub fn accept_admin_handler(ctx: Context<AcceptAdmin>) -> Result<()> {
    let config = &mut ctx.accounts.config;
    config.admin = config.pending_admin;
    config.pending_admin = Pubkey::default();
    Ok(())
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SetQuoteParams {
    /// Meteora's `[b"token_badge", mint]`, or `Pubkey::default()` for a mint that needs none.
    /// A wrong one fails inside Meteora at launch.
    pub badge: Pubkey,
    pub enabled: bool,
    pub min_sqrt_price: u128,
    pub max_sqrt_price: u128,
}

#[derive(Accounts)]
pub struct SetQuote<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin)]
    pub config: Box<Account<'info, Config>>,

    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init_if_needed,
        payer = admin,
        space = 8 + QuoteConfig::INIT_SPACE,
        seeds = [QUOTE_SEED, quote_mint.key().as_ref()],
        bump,
    )]
    pub quote_config: Box<Account<'info, QuoteConfig>>,

    pub quote_token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

/// The allow-list: `launch` refuses any quote without an enabled entry here. A quote's token
/// program and decimals are read from the mint, never typed in.
pub fn set_quote_handler(ctx: Context<SetQuote>, params: SetQuoteParams) -> Result<()> {
    require!(
        params.min_sqrt_price != 0 && params.min_sqrt_price <= params.max_sqrt_price,
        LaunchError::InvalidPriceBand
    );

    let mint_info = ctx.accounts.quote_mint.to_account_info();
    let token_program = *mint_info.owner;
    require_keys_eq!(
        ctx.accounts.quote_token_program.key(),
        token_program,
        LaunchError::WrongTokenProgram
    );
    // Token-2022 can withdraw a vault's stray lamports and wSOL can wrap them; a plain SPL Token
    // vault can do neither, so §3.8 would have a hole.
    require!(
        token_program == anchor_spl::token_2022::ID || ctx.accounts.quote_mint.key() == WSOL_MINT,
        LaunchError::UnsupportedQuoteProgram
    );

    let mint = ctx.accounts.quote_mint.key();
    let decimals = ctx.accounts.quote_mint.decimals;
    let bump = ctx.bumps.quote_config;
    ctx.accounts.quote_config.set_inner(QuoteConfig {
        mint,
        token_program,
        decimals,
        badge: params.badge,
        enabled: params.enabled,
        min_sqrt_price: params.min_sqrt_price,
        max_sqrt_price: params.max_sqrt_price,
        bump,
    });
    Ok(())
}
