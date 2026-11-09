//! The exits of §3.8 that are not payouts: stray tokens and forced lamports.
//!
//! `rescue_foreign_token` is the one instruction that can move a token out of a `LaunchRecord`'s
//! authority without a ledger entry, so it refuses by ADDRESS and by MINT everything that matters:
//! the `FeeVault`, which only `admin_recover` touches because it keeps the ledger, and the position
//! NFT, which is the locked liquidity itself. A rescued position NFT would hand the admin every
//! future fee of that launch.

use crate::constants::*;
use crate::errors::LaunchError;
use crate::events::AdminRescued;
use crate::state::{Config, LaunchRecord};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    self, Mint, TokenAccount, TokenInterface, TransferChecked, WithdrawExcessLamports,
};

/// Which of our PDAs holds the stray tokens.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum RescueOwner {
    Config,
    Quote { mint: Pubkey },
    Launch { coin_mint: Pubkey },
    FeeVault { launch: Pubkey },
    Claim { launch: Pubkey, holder: Pubkey },
}

impl RescueOwner {
    fn seeds(&self) -> Vec<Vec<u8>> {
        match self {
            RescueOwner::Config => vec![CONFIG_SEED.to_vec()],
            RescueOwner::Quote { mint } => vec![QUOTE_SEED.to_vec(), mint.to_bytes().to_vec()],
            RescueOwner::Launch { coin_mint } => {
                vec![LAUNCH_SEED.to_vec(), coin_mint.to_bytes().to_vec()]
            }
            RescueOwner::FeeVault { launch } => {
                vec![FEE_VAULT_SEED.to_vec(), launch.to_bytes().to_vec()]
            }
            RescueOwner::Claim { launch, holder } => vec![
                CLAIM_SEED.to_vec(),
                launch.to_bytes().to_vec(),
                holder.to_bytes().to_vec(),
            ],
        }
    }
}

#[derive(Accounts)]
pub struct RescueForeignToken<'info> {
    pub admin: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = admin)]
    pub config: Box<Account<'info, Config>>,

    /// CHECK: proved to be our PDA by the seeds in `owner`.
    pub pda_owner: UncheckedAccount<'info>,

    #[account(
        mut,
        token::mint = mint,
        token::authority = pda_owner,
        token::token_program = token_program,
    )]
    pub source: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mint::token_program = token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,

    /// Named in the call: a token account of the same mint.
    #[account(mut, token::mint = mint, token::token_program = token_program)]
    pub destination: Box<InterfaceAccount<'info, TokenAccount>>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn rescue_foreign_token_handler(
    ctx: Context<RescueForeignToken>,
    owner: RescueOwner,
) -> Result<()> {
    let seed_values = owner.seeds();
    let mut seeds: Vec<&[u8]> = seed_values.iter().map(|seed| seed.as_slice()).collect();
    let (expected, bump) = Pubkey::find_program_address(&seeds, &crate::ID);
    require_keys_eq!(
        ctx.accounts.pda_owner.key(),
        expected,
        LaunchError::RescueSeedsMismatch
    );

    if matches!(owner, RescueOwner::Launch { .. }) {
        let info = ctx.accounts.pda_owner.to_account_info();
        require!(info.owner == &crate::ID, LaunchError::NotProgramOwned);
        let data = info.try_borrow_data()?;
        let record = LaunchRecord::try_deserialize(&mut &data[..])?;
        let source = ctx.accounts.source.key();
        require!(
            source != record.fee_vault
                && source != record.position_nft_account
                && ctx.accounts.source.mint != record.position_nft_mint,
            LaunchError::ProtectedAccount
        );
    }

    require_keys_neq!(
        ctx.accounts.source.key(),
        ctx.accounts.destination.key(),
        LaunchError::RecoverToSource
    );
    let amount = ctx.accounts.source.amount;
    require!(amount > 0, LaunchError::NothingToSweep);

    let bump_seed = [bump];
    seeds.push(&bump_seed);
    let signer: &[&[&[u8]]] = &[seeds.as_slice()];

    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.source.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.destination.to_account_info(),
                authority: ctx.accounts.pda_owner.to_account_info(),
            },
            signer,
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    emit!(AdminRescued {
        pda_owner: ctx.accounts.pda_owner.key(),
        source: ctx.accounts.source.key(),
        mint: ctx.accounts.mint.key(),
        amount,
        destination: ctx.accounts.destination.key(),
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SweepExcessLamports<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, Config>>,

    /// CHECK: any account this program owns; only lamports above rent move, and only to the wallet.
    #[account(mut)]
    pub target: UncheckedAccount<'info>,

    /// CHECK: must be the configured wallet.
    #[account(mut, address = config.platform_wallet)]
    pub platform_wallet: UncheckedAccount<'info>,
}

pub fn sweep_excess_lamports_handler(ctx: Context<SweepExcessLamports>) -> Result<()> {
    let target = ctx.accounts.target.to_account_info();
    require!(target.owner == &crate::ID, LaunchError::NotProgramOwned);

    let keep = Rent::get()?.minimum_balance(target.data_len());
    let excess = target.lamports().saturating_sub(keep);
    require!(excess > 0, LaunchError::NothingToSweep);

    target.sub_lamports(excess)?;
    ctx.accounts
        .platform_wallet
        .to_account_info()
        .add_lamports(excess)?;
    Ok(())
}

#[derive(Accounts)]
pub struct SweepVaultLamports<'info> {
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, Config>>,

    #[account(
        seeds = [LAUNCH_SEED, launch_record.coin_mint.as_ref()],
        bump = launch_record.bump,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,

    /// CHECK: must be this launch's vault; the token program does the rent arithmetic.
    #[account(mut, address = launch_record.fee_vault)]
    pub fee_vault: UncheckedAccount<'info>,

    /// CHECK: must be the configured wallet.
    #[account(mut, address = config.platform_wallet)]
    pub platform_wallet: UncheckedAccount<'info>,

    #[account(
        address = launch_record.quote_token_program @ LaunchError::WrongTokenProgram,
    )]
    pub quote_token_program: Interface<'info, TokenInterface>,
}

/// A `FeeVault` is owned by the token program, so `sweep_excess_lamports` cannot reach it. Only
/// Token-2022 has this instruction; a wrapped-SOL vault wraps stray lamports through `sync_vault`.
pub fn sweep_vault_lamports_handler(ctx: Context<SweepVaultLamports>) -> Result<()> {
    require!(
        ctx.accounts.quote_token_program.key() == anchor_spl::token_2022::ID,
        LaunchError::VaultLamportsViaSync
    );

    let coin_key = ctx.accounts.launch_record.coin_mint;
    let bump = [ctx.accounts.launch_record.bump];
    let seeds: &[&[u8]] = &[LAUNCH_SEED, coin_key.as_ref(), &bump];
    let signer: &[&[&[u8]]] = &[seeds];

    token_interface::withdraw_excess_lamports(CpiContext::new_with_signer(
        ctx.accounts.quote_token_program.key(),
        WithdrawExcessLamports {
            source: ctx.accounts.fee_vault.to_account_info(),
            destination: ctx.accounts.platform_wallet.to_account_info(),
            authority: ctx.accounts.launch_record.to_account_info(),
        },
        signer,
    ))
}
