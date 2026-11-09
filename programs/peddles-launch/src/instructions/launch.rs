//! `launch` (§3.2): everything happens in this one instruction, or none of it does.
//!
//! The creator's first buy is not a swap (§2.5). Meteora's pool opens at the price the buy would
//! reach, and the coins it does not pull stay with the creator, so the opening 20% and Meteora's cut
//! never apply. The creator pays the ordinary rate, the EVM's `_effectiveTaxBps` rule, into the
//! `FeeVault` directly.

use crate::constants::*;
use crate::errors::{fee_err, LaunchError};
use crate::events::Launched;
use crate::meteora;
use crate::state::{Config, LaunchRecord, QuoteConfig};
use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Token;
use anchor_spl::token_2022::spl_token_2022::instruction::AuthorityType;
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_interface::{
    self, Mint, MintTo, SetAuthority, TokenAccount, TokenInterface, TransferChecked,
};
use peddles_fees::schedule::FeeSchedule;
use peddles_fees::{ix_data, mint as mint_checks, price, split, suffix};

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct LaunchParams {
    /// Ground so the coin mint PDA ends in `PEDL`.
    pub salt: [u8; 8],
    /// `false`: `[b"coin", salt]`, the instant salt pool. `true`: `[b"coin", creator, salt]`, a
    /// Pre-Launch address only this creator can launch at.
    pub bound_to_creator: bool,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    /// The rate traders pay, 125..=500 basis points, fixed forever.
    pub rate_bps: u16,
    /// The creator's share of everything above the two fixed legs, fixed forever.
    pub excess_to_creator_bps: u16,
    pub creator_payout: Pubkey,
    /// Must deposit exactly the whole supply at `sqrt_price`; see `peddles_fees::price`.
    pub liquidity: u128,
    /// `√P0`, the price BEFORE any first buy, inside the quote's band. A first buy moves the pool's
    /// opening price up from here; the band never sees that, so size cannot trip it or game it.
    pub sqrt_price: u128,
    /// The creator's first buy in raw quote, fee included. 0 for none: the pool stays one-sided.
    /// A transfer-fee quote costs its issuer's fee on top of this, on both the fee and the deposit.
    pub first_buy_quote: u64,
    /// The launch reverts if the first buy returns fewer coins.
    pub first_buy_min_coins: u64,
}

/// The middle seed of the coin mint PDA.
fn coin_binding_seed<'a>(bound_to_creator: bool, creator: &'a Pubkey) -> &'a [u8] {
    suffix::binding_seed(bound_to_creator, creator.as_ref())
}

#[derive(Accounts)]
#[instruction(params: LaunchParams)]
pub struct Launch<'info> {
    /// Pays every deposit, and holds the coin for the moment before the pool takes its share.
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, Config>>,

    #[account(
        seeds = [QUOTE_SEED, quote_mint.key().as_ref()],
        bump = quote_config.bump,
        constraint = quote_config.enabled @ LaunchError::QuoteDisabled,
    )]
    pub quote_config: Box<Account<'info, QuoteConfig>>,

    #[account(
        address = quote_config.mint,
        mint::token_program = quote_token_program,
    )]
    pub quote_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Created here, so nobody can have taken the pool address first (§4.7). No freeze authority.
    #[account(
        init,
        payer = creator,
        seeds = [
            COIN_SEED,
            coin_binding_seed(params.bound_to_creator, creator.key),
            params.salt.as_ref(),
        ],
        bump,
        mint::decimals = COIN_DECIMALS,
        mint::authority = launch_record,
        mint::token_program = coin_token_program,
    )]
    pub coin_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        init,
        payer = creator,
        space = 8 + LaunchRecord::INIT_SPACE,
        seeds = [LAUNCH_SEED, coin_mint.key().as_ref()],
        bump,
    )]
    pub launch_record: Box<Account<'info, LaunchRecord>>,

    #[account(
        init,
        payer = creator,
        seeds = [FEE_VAULT_SEED, launch_record.key().as_ref()],
        bump,
        token::mint = quote_mint,
        token::authority = launch_record,
        token::token_program = quote_token_program,
    )]
    pub fee_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init,
        payer = creator,
        associated_token::mint = coin_mint,
        associated_token::authority = creator,
        associated_token::token_program = coin_token_program,
    )]
    pub creator_coin_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Must hold at least 1 raw unit (Meteora transfers one to prove the mint, §1.6), or enough for
    /// the first buy.
    #[account(
        mut,
        token::mint = quote_mint,
        token::authority = creator,
        token::token_program = quote_token_program,
    )]
    pub creator_quote_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: Metaplex's metadata PDA for the coin; the seeds are checked here and by Metaplex.
    #[account(
        mut,
        seeds = [METADATA_SEED, metadata_program.key().as_ref(), coin_mint.key().as_ref()],
        seeds::program = metadata_program.key(),
        bump,
    )]
    pub metadata: UncheckedAccount<'info>,

    /// CHECK: our PDA, holds nothing. Signs as Metaplex update authority and verified creator.
    #[account(seeds = [CREATOR_SEED], bump)]
    pub peddles_creator: UncheckedAccount<'info>,

    /// CHECK: our PDA, created by Meteora, which requires it to sign. See `invoke_signed` below.
    #[account(mut, seeds = [POSITION_NFT_SEED, coin_mint.key().as_ref()], bump)]
    pub position_nft_mint: UncheckedAccount<'info>,

    /// CHECK: `init` with seeds on Meteora's side.
    #[account(mut)]
    pub position_nft_account: UncheckedAccount<'info>,

    /// CHECK: `init` with seeds on Meteora's side.
    #[account(mut)]
    pub pool: UncheckedAccount<'info>,

    /// CHECK: `init` with seeds on Meteora's side.
    #[account(mut)]
    pub position: UncheckedAccount<'info>,

    /// CHECK: `init` with seeds on Meteora's side.
    #[account(mut)]
    pub token_a_vault: UncheckedAccount<'info>,

    /// CHECK: `init` with seeds on Meteora's side.
    #[account(mut)]
    pub token_b_vault: UncheckedAccount<'info>,

    /// CHECK: fixed address.
    #[account(address = METEORA_POOL_AUTHORITY)]
    pub pool_authority: UncheckedAccount<'info>,

    /// CHECK: the badge recorded for this quote; Meteora loads and checks it.
    #[account(address = quote_config.badge)]
    pub token_badge: UncheckedAccount<'info>,

    /// CHECK: receives the launch fee; must be the configured wallet.
    #[account(mut, address = config.platform_wallet)]
    pub platform_wallet: UncheckedAccount<'info>,

    /// CHECK: fixed address.
    #[account(address = METEORA_EVENT_AUTHORITY)]
    pub meteora_event_authority: UncheckedAccount<'info>,

    /// CHECK: fixed address.
    #[account(address = METEORA_DAMM_V2)]
    pub meteora_program: UncheckedAccount<'info>,

    /// CHECK: fixed address.
    #[account(address = METAPLEX_TOKEN_METADATA)]
    pub metadata_program: UncheckedAccount<'info>,

    pub coin_token_program: Program<'info, Token>,

    #[account(
        constraint = quote_token_program.key() == quote_config.token_program
            @ LaunchError::WrongTokenProgram,
    )]
    pub quote_token_program: Interface<'info, TokenInterface>,

    pub token_2022_program: Program<'info, Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> Launch<'info> {
    /// Step 1, everything that can be refused before a lamport moves.
    fn validate(&self, params: &LaunchParams) -> Result<()> {
        split::validate_terms(params.rate_bps, params.excess_to_creator_bps).map_err(fee_err)?;
        require!(
            suffix::ends_with_pedl(&self.coin_mint.key().to_bytes()),
            LaunchError::CoinSuffix
        );
        require!(
            params.creator_payout != Pubkey::default(),
            LaunchError::ZeroAddress
        );
        require!(
            price::in_band(
                params.sqrt_price,
                self.quote_config.min_sqrt_price,
                self.quote_config.max_sqrt_price
            ),
            LaunchError::OpeningPriceOutOfBand
        );
        // This is what makes a first buy safe: `L` is fixed by the supply at the OPENING price, so a
        // thin `L` cannot buy most of the supply for a little quote.
        require!(
            price::liquidity_deposits_exactly(
                params.sqrt_price,
                price::MAX_SQRT_PRICE,
                params.liquidity,
                COIN_SUPPLY_RAW
            ),
            LaunchError::LiquidityNotExact
        );
        let quote_info = self.quote_mint.to_account_info();
        let quote_data = quote_info.try_borrow_data()?;
        mint_checks::require_launchable(&quote_data).map_err(fee_err)?;
        Ok(())
    }

    /// Step 2. The program never holds the fee.
    fn collect_launch_fee(&self) -> Result<()> {
        let lamports = self.config.launch_fee_lamports;
        if lamports == 0 {
            return Ok(());
        }
        system_program::transfer(
            CpiContext::new(
                self.system_program.key(),
                system_program::Transfer {
                    from: self.creator.to_account_info(),
                    to: self.platform_wallet.to_account_info(),
                },
            ),
            lamports,
        )
    }

    /// Step 3. `is_mutable = false`, so nobody can ever change it. The creator is verified because
    /// `peddles_creator` signs as update authority; Metaplex refuses `verified` any other way.
    #[inline(never)]
    fn create_metadata(&self, params: &LaunchParams, signer: &[&[&[u8]]]) -> Result<()> {
        let mut data = [0u8; ix_data::CREATE_METADATA_MAX_LEN];
        let length = ix_data::create_metadata_account_v3(
            &mut data,
            params.name.as_bytes(),
            params.symbol.as_bytes(),
            params.uri.as_bytes(),
            &self.peddles_creator.key().to_bytes(),
        )
        .map_err(fee_err)?;
        meteora::create_metadata_account_v3(
            &meteora::CreateMetadataAccount {
                metadata: self.metadata.to_account_info(),
                mint: self.coin_mint.to_account_info(),
                mint_authority: self.launch_record.to_account_info(),
                payer: self.creator.to_account_info(),
                update_authority: self.peddles_creator.to_account_info(),
                system_program: self.system_program.to_account_info(),
                program: self.metadata_program.to_account_info(),
            },
            &data[..length],
            signer,
        )
    }

    /// Step 4.
    fn mint_supply(&self, signer: &[&[&[u8]]]) -> Result<()> {
        token_interface::mint_to(
            CpiContext::new_with_signer(
                self.coin_token_program.key(),
                MintTo {
                    mint: self.coin_mint.to_account_info(),
                    to: self.creator_coin_account.to_account_info(),
                    authority: self.launch_record.to_account_info(),
                },
                signer,
            ),
            COIN_SUPPLY_RAW,
        )?;
        token_interface::set_authority(
            CpiContext::new_with_signer(
                self.coin_token_program.key(),
                SetAuthority {
                    current_authority: self.launch_record.to_account_info(),
                    account_or_mint: self.coin_mint.to_account_info(),
                },
                signer,
            ),
            AuthorityType::MintTokens,
            None,
        )
    }

    /// Step 5. `sqrt_open` is `√P0` with no first buy, which leaves the pool one-sided, and `√P1`
    /// with one, which makes Meteora deposit both sides.
    #[inline(never)]
    fn create_pool(
        &self,
        params: &LaunchParams,
        schedule: &FeeSchedule,
        sqrt_open: u128,
        signer: &[&[&[u8]]],
    ) -> Result<()> {
        meteora::initialize_customizable_pool(
            &meteora::InitializeCustomizablePool {
                creator: self.launch_record.to_account_info(),
                position_nft_mint: self.position_nft_mint.to_account_info(),
                position_nft_account: self.position_nft_account.to_account_info(),
                payer: self.creator.to_account_info(),
                pool_authority: self.pool_authority.to_account_info(),
                pool: self.pool.to_account_info(),
                position: self.position.to_account_info(),
                token_a_mint: self.coin_mint.to_account_info(),
                token_b_mint: self.quote_mint.to_account_info(),
                token_a_vault: self.token_a_vault.to_account_info(),
                token_b_vault: self.token_b_vault.to_account_info(),
                payer_token_a: self.creator_coin_account.to_account_info(),
                payer_token_b: self.creator_quote_account.to_account_info(),
                token_a_program: self.coin_token_program.to_account_info(),
                token_b_program: self.quote_token_program.to_account_info(),
                token_2022_program: self.token_2022_program.to_account_info(),
                system_program: self.system_program.to_account_info(),
                event_authority: self.meteora_event_authority.to_account_info(),
                program: self.meteora_program.to_account_info(),
                token_a_badge: self.system_program.to_account_info(),
                token_b_badge: self.token_badge.to_account_info(),
            },
            schedule,
            // The bottom of the range is always the opening price.
            params.sqrt_price,
            price::MAX_SQRT_PRICE,
            params.liquidity,
            sqrt_open,
            signer,
        )
    }

    /// Step 6. The whole position, forever.
    fn lock_position(&self, liquidity: u128, signer: &[&[&[u8]]]) -> Result<()> {
        meteora::permanent_lock_position(
            &meteora::PermanentLockPosition {
                pool: self.pool.to_account_info(),
                position: self.position.to_account_info(),
                position_nft_account: self.position_nft_account.to_account_info(),
                signer: self.launch_record.to_account_info(),
                event_authority: self.meteora_event_authority.to_account_info(),
                program: self.meteora_program.to_account_info(),
            },
            liquidity,
            signer,
        )
    }

    /// Step 8. The first buy's fee, straight into the vault. Nothing here is a pool fee, so no part
    /// of it reaches Meteora.
    fn charge_first_buy_fee(&self, fee: u64) -> Result<()> {
        token_interface::transfer_checked(
            CpiContext::new(
                self.quote_token_program.key(),
                TransferChecked {
                    from: self.creator_quote_account.to_account_info(),
                    mint: self.quote_mint.to_account_info(),
                    to: self.fee_vault.to_account_info(),
                    authority: self.creator.to_account_info(),
                },
            ),
            fee,
            self.quote_mint.decimals,
        )
    }
}

pub fn launch_handler(ctx: Context<Launch>, params: LaunchParams) -> Result<()> {
    ctx.accounts.validate(&params)?;
    let schedule = FeeSchedule::for_rate(params.rate_bps).map_err(fee_err)?;

    let first_buy = split::first_buy_fee(params.first_buy_quote, params.rate_bps).map_err(fee_err)?;
    let sqrt_open = price::sqrt_price_after_buy(
        params.sqrt_price,
        price::MAX_SQRT_PRICE,
        params.liquidity,
        first_buy.net,
    )
    .ok_or(LaunchError::FirstBuyTooLarge)?;
    let has_first_buy = params.first_buy_quote > 0;
    require!(
        !has_first_buy || sqrt_open > params.sqrt_price,
        LaunchError::FirstBuyTooSmall
    );

    let coin_key = ctx.accounts.coin_mint.key();
    let record_bump = [ctx.bumps.launch_record];
    let record_seeds: &[&[u8]] = &[LAUNCH_SEED, coin_key.as_ref(), &record_bump];
    let record_signer: &[&[&[u8]]] = &[record_seeds];
    let nft_bump = [ctx.bumps.position_nft_mint];
    let nft_seeds: &[&[u8]] = &[POSITION_NFT_SEED, coin_key.as_ref(), &nft_bump];
    let nft_signer: &[&[&[u8]]] = &[nft_seeds];
    let creator_bump = [ctx.bumps.peddles_creator];
    let creator_seeds: &[&[u8]] = &[CREATOR_SEED, &creator_bump];
    let metadata_signer: &[&[&[u8]]] = &[record_seeds, creator_seeds];

    let record = LaunchRecord {
        coin_mint: coin_key,
        quote_mint: ctx.accounts.quote_mint.key(),
        quote_token_program: ctx.accounts.quote_token_program.key(),
        pool: ctx.accounts.pool.key(),
        position: ctx.accounts.position.key(),
        position_nft_mint: ctx.accounts.position_nft_mint.key(),
        position_nft_account: ctx.accounts.position_nft_account.key(),
        fee_vault: ctx.accounts.fee_vault.key(),
        creator: ctx.accounts.creator.key(),
        rate_bps: params.rate_bps,
        excess_to_creator_bps: params.excess_to_creator_bps,
        // Meteora activates the pool at this same `Clock::unix_timestamp`.
        activation_point: Clock::get()?.unix_timestamp,
        cliff_fee_numerator: schedule.cliff_fee_numerator,
        reduction_factor: schedule.reduction_factor,
        bump: ctx.bumps.launch_record,
        fee_vault_bump: ctx.bumps.fee_vault,
        creator_payout: params.creator_payout,
        platform_owed: 0,
        creator_owed: 0,
        holder_funded: 0,
        holder_claimed: 0,
        holder_recovered: 0,
        active_root: [0u8; 32],
        active_total: 0,
        active_epoch: 0,
        last_root_at: 0,
        paused: false,
    };
    ctx.accounts.launch_record.set_inner(record);

    ctx.accounts.collect_launch_fee()?;
    ctx.accounts.create_metadata(&params, metadata_signer)?;
    ctx.accounts.mint_supply(record_signer)?;
    ctx.accounts
        .create_pool(&params, &schedule, sqrt_open, nft_signer)?;

    // The creator's ATA was created in this instruction and held exactly the supply, so what is
    // left is exactly what the pool did not take. This reads what moved, and every unit is either
    // in the pool's vault or here.
    ctx.accounts.creator_coin_account.reload()?;
    let first_buy_coins = ctx.accounts.creator_coin_account.amount;
    if has_first_buy {
        require!(first_buy_coins > 0, LaunchError::FirstBuyTooSmall);
        require!(
            first_buy_coins >= params.first_buy_min_coins,
            LaunchError::FirstBuyBelowMinimum
        );
        let limit = (COIN_SUPPLY_RAW as u128)
            .checked_mul(MAX_FIRST_BUY_BPS_OF_SUPPLY as u128)
            .ok_or(LaunchError::MathOverflow)?
            / 10_000;
        require!(
            first_buy_coins as u128 <= limit,
            LaunchError::FirstBuyTooLarge
        );
    } else {
        require!(first_buy_coins == 0, LaunchError::SupplyMismatch);
    }

    ctx.accounts.lock_position(params.liquidity, record_signer)?;

    if first_buy.fee > 0 {
        let vault_before = ctx.accounts.fee_vault.amount;
        ctx.accounts.charge_first_buy_fee(first_buy.fee)?;
        ctx.accounts.fee_vault.reload()?;
        let vault_after = ctx.accounts.fee_vault.amount;

        let record = &mut ctx.accounts.launch_record;
        let mut book = record.ledger();
        book.credit_first_buy_fee(
            vault_before,
            vault_after,
            params.rate_bps,
            params.excess_to_creator_bps,
        )
        .map_err(fee_err)?;
        record.set_ledger(book);
    }

    emit!(Launched {
        launch: ctx.accounts.launch_record.key(),
        coin_mint: coin_key,
        quote_mint: ctx.accounts.quote_mint.key(),
        pool: ctx.accounts.pool.key(),
        creator: ctx.accounts.creator.key(),
        rate_bps: params.rate_bps,
        excess_to_creator_bps: params.excess_to_creator_bps,
        activation_point: ctx.accounts.launch_record.activation_point,
        launch_fee_lamports: ctx.accounts.config.launch_fee_lamports,
        bound_to_creator: params.bound_to_creator,
        first_buy_coins,
        first_buy_fee: first_buy.fee,
    });
    Ok(())
}
