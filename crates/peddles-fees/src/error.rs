//! Every way the arithmetic refuses. The program maps each one to its own error code.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeeError {
    /// `rate_bps` is outside 125..=500.
    InvalidRate,
    /// `excess_to_creator_bps` is above 10,000.
    InvalidExcessShare,
    /// A checked operation overflowed or underflowed.
    MathOverflow,
    /// The four legs did not sum to the claim. Unreachable by construction; checked anyway.
    SplitMismatch,
    /// The vault balance went down across a claim.
    VaultDecreased,
    /// There is nothing owed to this recipient.
    NothingToPay,
    /// A holder payout would exceed what the active root, or the vault, allows.
    ClaimAboveRootTotal,
    /// Recovery asked for more than that bucket of the vault holds.
    RecoverAboveBucket,
    /// A root must carry the next epoch number.
    RootEpochOutOfOrder,
    /// A root's cumulative total may not go down.
    RootTotalDecreased,
    /// A root may not promise more than has arrived.
    RootTotalAboveFunded,
    /// A root is the zero hash.
    RootEmpty,
    /// Less than the minimum interval since the last root.
    RootTooSoon,
    /// A root must raise the cumulative total.
    RootTotalNotRaised,
    /// No root has been published yet.
    NoActiveRoot,
    /// The merkle proof does not verify against the active root.
    InvalidProof,
    /// The merkle proof is longer than `merkle::MAX_PROOF_LEN`.
    ProofTooLong,
    /// The quote mint is paused.
    MintPaused,
    /// The quote mint's default account state is not `initialized`.
    MintDefaultNotInitialized,
    /// The quote mint has a transfer hook program set.
    MintHookSet,
    /// The quote mint is non-transferable.
    MintNonTransferable,
    /// The mint's account data could not be parsed.
    MalformedMint,
    /// A metadata string does not fit.
    MetadataTooLong,
}
