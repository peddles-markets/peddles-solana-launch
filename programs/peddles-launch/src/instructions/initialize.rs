use crate::constants::{CONFIG_SEED, MAX_LAUNCH_FEE_LAMPORTS};
use crate::errors::LaunchError;
use crate::state::Config;
use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitializeParams {
    pub admin: Pubkey,
    pub keeper: Pubkey,
    pub guardian: Pubkey,
    pub platform_wallet: Pubkey,
    pub launch_fee_lamports: u64,
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Account<'info, Config>,

    /// Without this, whoever got here first after deploy would own the program's settings.
    #[account(
        constraint = program.programdata_address()? == Some(program_data.key())
            @ LaunchError::NotUpgradeAuthority,
    )]
    pub program: Program<'info, crate::program::PeddlesLaunch>,

    #[account(
        constraint = program_data.upgrade_authority_address == Some(authority.key())
            @ LaunchError::NotUpgradeAuthority,
    )]
    pub program_data: Account<'info, ProgramData>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_handler(ctx: Context<Initialize>, params: InitializeParams) -> Result<()> {
    let zero = Pubkey::default();
    require!(
        params.admin != zero
            && params.keeper != zero
            && params.guardian != zero
            && params.platform_wallet != zero,
        LaunchError::ZeroAddress
    );
    require_keys_neq!(params.keeper, params.guardian, LaunchError::KeeperIsGuardian);
    require!(
        params.launch_fee_lamports <= MAX_LAUNCH_FEE_LAMPORTS,
        LaunchError::LaunchFeeAboveCap
    );

    let bump = ctx.bumps.config;
    ctx.accounts.config.set_inner(Config {
        admin: params.admin,
        pending_admin: zero,
        keeper: params.keeper,
        guardian: params.guardian,
        platform_wallet: params.platform_wallet,
        launch_fee_lamports: params.launch_fee_lamports,
        paused: false,
        bump,
    });
    Ok(())
}
