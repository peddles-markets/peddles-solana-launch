//! What `launch` checks on a quote mint's raw account data (§3.2 step 1, §5.2).
//!
//! Token-2022 lays a mint out as the 82-byte base, zero padding to 165, an account-type byte at 165,
//! then TLV entries of `type: u16 LE, length: u16 LE, value`. Parsed by hand so the check can be
//! tested on real mint bytes without an SDK. Extension numbers and layouts are from
//! `interface/src/extension/mod.rs` at token-2022 commit `f1a6c88`, and were confirmed against the
//! live TSLAx and PreStocks ANTHROPIC mints on 2026-11-06 (slots 466,188,414 and 466,188,415).

use crate::FeeError;

const MINT_BASE_LEN: usize = 82;
const ACCOUNT_TYPE_OFFSET: usize = 165;
const TLV_START: usize = 166;
const ACCOUNT_TYPE_MINT: u8 = 1;

const EXT_UNINITIALIZED: u16 = 0;
const EXT_DEFAULT_ACCOUNT_STATE: u16 = 6;
const EXT_NON_TRANSFERABLE: u16 = 9;
const EXT_TRANSFER_HOOK: u16 = 14;
const EXT_PAUSABLE: u16 = 26;

const ACCOUNT_STATE_INITIALIZED: u8 = 1;

/// The facts a launch depends on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MintFacts {
    pub paused: bool,
    /// `None` when the mint has no `DefaultAccountState` extension.
    pub default_account_state: Option<u8>,
    pub transfer_hook_program_set: bool,
    pub non_transferable: bool,
}

/// Parse a mint account's data. A plain SPL mint (82 bytes) has no extensions.
pub fn inspect_mint(data: &[u8]) -> Result<MintFacts, FeeError> {
    let mut facts = MintFacts::default();
    if data.len() == MINT_BASE_LEN {
        return Ok(facts);
    }
    if data.len() <= ACCOUNT_TYPE_OFFSET || data[ACCOUNT_TYPE_OFFSET] != ACCOUNT_TYPE_MINT {
        return Err(FeeError::MalformedMint);
    }

    let mut offset = TLV_START;
    while offset + 4 <= data.len() {
        let ext_type = u16::from_le_bytes([data[offset], data[offset + 1]]);
        let length = u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;
        if ext_type == EXT_UNINITIALIZED {
            break;
        }
        let start = offset + 4;
        let end = start.checked_add(length).ok_or(FeeError::MalformedMint)?;
        let value = data.get(start..end).ok_or(FeeError::MalformedMint)?;

        match ext_type {
            EXT_DEFAULT_ACCOUNT_STATE => {
                facts.default_account_state = Some(*value.first().ok_or(FeeError::MalformedMint)?);
            }
            EXT_NON_TRANSFERABLE => facts.non_transferable = true,
            EXT_TRANSFER_HOOK => {
                // authority (32) then program_id (32); all zeroes means unset.
                let program = value.get(32..64).ok_or(FeeError::MalformedMint)?;
                facts.transfer_hook_program_set = program.iter().any(|byte| *byte != 0);
            }
            EXT_PAUSABLE => {
                // authority (32) then paused (1).
                facts.paused = *value.get(32).ok_or(FeeError::MalformedMint)? != 0;
            }
            _ => {}
        }
        offset = end;
    }
    Ok(facts)
}

/// Refuse a transfer the mint would refuse: paused, or a hook whose accounts this program never
/// passes. Used by recovery, where a default-frozen mint no longer matters.
pub fn require_transferable(data: &[u8]) -> Result<(), FeeError> {
    let facts = inspect_mint(data)?;
    if facts.paused {
        return Err(FeeError::MintPaused);
    }
    if facts.transfer_hook_program_set {
        return Err(FeeError::MintHookSet);
    }
    if facts.non_transferable {
        return Err(FeeError::MintNonTransferable);
    }
    Ok(())
}

/// Refuse a quote the pool could not move (§3.2 step 1).
pub fn require_launchable(data: &[u8]) -> Result<(), FeeError> {
    let facts = inspect_mint(data)?;
    if facts.paused {
        return Err(FeeError::MintPaused);
    }
    if matches!(facts.default_account_state, Some(state) if state != ACCOUNT_STATE_INITIALIZED) {
        return Err(FeeError::MintDefaultNotInitialized);
    }
    if facts.transfer_hook_program_set {
        return Err(FeeError::MintHookSet);
    }
    if facts.non_transferable {
        return Err(FeeError::MintNonTransferable);
    }
    Ok(())
}
