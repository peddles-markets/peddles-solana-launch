use anchor_lang::prelude::*;
use peddles_fees::FeeError;

#[error_code]
pub enum LaunchError {
    #[msg("The rate must be between 1.25% and 5%")]
    InvalidRate,
    #[msg("The creator's share of the excess must be at most 100%")]
    InvalidExcessShare,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("The fee legs do not sum to the claim")]
    SplitMismatch,
    #[msg("The vault balance fell during a claim")]
    VaultDecreased,
    #[msg("Nothing to pay")]
    NothingToPay,
    #[msg("The claim exceeds the published total")]
    ClaimAboveRootTotal,
    #[msg("Recovery asked for more than that bucket holds")]
    RecoverAboveBucket,
    #[msg("A root must carry the next epoch")]
    RootEpochOutOfOrder,
    #[msg("A root's cumulative total cannot go down")]
    RootTotalDecreased,
    #[msg("A root cannot promise more than has arrived")]
    RootTotalAboveFunded,
    #[msg("A root cannot be empty")]
    RootEmpty,
    #[msg("Too soon since the last root")]
    RootTooSoon,
    #[msg("A root must raise the total")]
    RootTotalNotRaised,
    #[msg("No root is active yet")]
    NoActiveRoot,
    #[msg("The proof does not verify")]
    InvalidProof,
    #[msg("The proof is too long")]
    ProofTooLong,
    #[msg("The quote token is paused")]
    MintPaused,
    #[msg("New accounts of the quote token start frozen")]
    MintDefaultNotInitialized,
    #[msg("The quote token has a transfer hook set")]
    MintHookSet,
    #[msg("The quote token cannot be transferred")]
    MintNonTransferable,
    #[msg("The quote token's mint could not be read")]
    MalformedMint,
    #[msg("The name, symbol or uri is too long")]
    MetadataTooLong,

    #[msg("Only the program's upgrade authority can initialize it")]
    NotUpgradeAuthority,
    #[msg("This quote token is not enabled")]
    QuoteDisabled,
    #[msg("The token program does not match the quote")]
    WrongTokenProgram,
    #[msg("The opening price is outside the band set for this quote")]
    OpeningPriceOutOfBand,
    #[msg("The price band is invalid")]
    InvalidPriceBand,
    #[msg("Only wrapped SOL may use the plain SPL Token program")]
    UnsupportedQuoteProgram,
    #[msg("The coin address must end in PEDL")]
    CoinSuffix,
    #[msg("The pool did not take exactly the whole supply")]
    SupplyMismatch,
    #[msg("The launch fee is above the cap")]
    LaunchFeeAboveCap,
    #[msg("An address cannot be the zero address")]
    ZeroAddress,
    #[msg("This signer has not been proposed as admin")]
    NotPendingAdmin,
    #[msg("This account is not ours to touch")]
    NotProgramOwned,
    #[msg("The rescue seeds do not match the account")]
    RescueSeedsMismatch,
    #[msg("The position NFT and the fee vault can never be rescued")]
    ProtectedAccount,
    #[msg("The keeper and the guardian must be different keys")]
    KeeperIsGuardian,
    #[msg("Holder publishing and claims are paused")]
    Paused,
    #[msg("Liquidity must deposit exactly the whole supply at the opening price")]
    LiquidityNotExact,
    #[msg("The first buy is too small to buy any coins")]
    FirstBuyTooSmall,
    #[msg("The first buy is above the limit")]
    FirstBuyTooLarge,
    #[msg("The first buy returned fewer coins than the minimum")]
    FirstBuyBelowMinimum,
    #[msg("A recovery cannot pay the account it takes from")]
    RecoverToSource,
    #[msg("The fee vault or the destination is frozen by the quote token's issuer")]
    AccountFrozen,
    #[msg("The vault's balance did not fall by exactly the amount sent")]
    RecoverMismatch,
    #[msg("Nothing to sweep")]
    NothingToSweep,
    #[msg("This vault's lamports are recovered through sync_vault")]
    VaultLamportsViaSync,
}

impl From<FeeError> for LaunchError {
    fn from(error: FeeError) -> Self {
        match error {
            FeeError::InvalidRate => LaunchError::InvalidRate,
            FeeError::InvalidExcessShare => LaunchError::InvalidExcessShare,
            FeeError::MathOverflow => LaunchError::MathOverflow,
            FeeError::SplitMismatch => LaunchError::SplitMismatch,
            FeeError::VaultDecreased => LaunchError::VaultDecreased,
            FeeError::NothingToPay => LaunchError::NothingToPay,
            FeeError::ClaimAboveRootTotal => LaunchError::ClaimAboveRootTotal,
            FeeError::RecoverAboveBucket => LaunchError::RecoverAboveBucket,
            FeeError::RootEpochOutOfOrder => LaunchError::RootEpochOutOfOrder,
            FeeError::RootTotalDecreased => LaunchError::RootTotalDecreased,
            FeeError::RootTotalAboveFunded => LaunchError::RootTotalAboveFunded,
            FeeError::RootEmpty => LaunchError::RootEmpty,
            FeeError::RootTooSoon => LaunchError::RootTooSoon,
            FeeError::RootTotalNotRaised => LaunchError::RootTotalNotRaised,
            FeeError::NoActiveRoot => LaunchError::NoActiveRoot,
            FeeError::InvalidProof => LaunchError::InvalidProof,
            FeeError::ProofTooLong => LaunchError::ProofTooLong,
            FeeError::MintPaused => LaunchError::MintPaused,
            FeeError::MintDefaultNotInitialized => LaunchError::MintDefaultNotInitialized,
            FeeError::MintHookSet => LaunchError::MintHookSet,
            FeeError::MintNonTransferable => LaunchError::MintNonTransferable,
            FeeError::MalformedMint => LaunchError::MalformedMint,
            FeeError::MetadataTooLong => LaunchError::MetadataTooLong,
        }
    }
}

/// Turn the arithmetic crate's refusal into this program's error.
pub fn fee_err(error: FeeError) -> anchor_lang::error::Error {
    LaunchError::from(error).into()
}
