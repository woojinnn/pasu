//! Approval helpers — `ERC20` allowance, `setApprovalForAll`, per-NFT `approve`, `Permit2`.
//!
//! Each helper produces one `TokenChange` row inside `delta`. `state` is read
//! only and is currently unused (kept in the signature for symmetry with the
//! other helper families and for future invariant checks).
//!
//! `set_at: Time` threads `AllowanceSpec::last_set_at` through every grant-side
//! dedicated time channel; revoke-side helpers do not need it because
//! `ApprovalRevoke` does not carry a timestamp.

use policy_state::approval::AllowanceSpec;
use policy_state::delta::token_change::ApprovalScope;
use policy_state::delta::TokenChange;
use policy_state::primitives::{Address, ChainId, Spender, Time, U256};
use policy_state::token::{TokenKey, TokenRef};
use policy_state::{StateDelta, WalletState};

use crate::error::{ReducerError, ReducerResult};

/// Set the `ERC20` allowance of `(token, spender)` to `amount`, emitting a
/// `TokenChange::ApprovalSet`. `U256::MAX` is recorded as `is_unlimited`.
///
/// `token.key` must be `TokenKey::Erc20`; any other variant returns
/// `ReducerError::Invariant`.
///
/// `amount == 0` is *accepted* here — the dispatcher in `effect/token.rs`
/// decides whether `approve(spender, 0)` is routed via this helper or via
/// [`revoke_erc20_allowance`]. We do not double-check the value so that
/// callers that always emit `ApprovalSet` (preserving the timestamp) stay
/// representable.
///
/// # Errors
///
/// Returns [`ReducerError::Invariant`] when `token.key` is not `TokenKey::Erc20`.
pub fn set_erc20_allowance(
    _state: &WalletState,
    delta: &mut StateDelta,
    set_at: Time,
    token: &TokenRef,
    spender: Address,
    amount: U256,
) -> ReducerResult<()> {
    if !matches!(token.key, TokenKey::Erc20 { .. }) {
        return Err(ReducerError::Invariant(
            "set_erc20_allowance on non-Erc20 token".into(),
        ));
    }

    let allowance = if amount == U256::MAX {
        AllowanceSpec::unlimited(set_at)
    } else {
        AllowanceSpec::new(amount, set_at)
    };

    delta.token_changes.push(TokenChange::ApprovalSet {
        key: token.key.clone(),
        spender: Spender::from(spender),
        allowance,
    });

    Ok(())
}

/// Revoke an `ERC20` allowance (`approve(spender, 0)` or a `RevokeApproval`
/// with `RevokeScope::Erc20`).
///
/// Emits `TokenChange::ApprovalRevoke { scope: Erc20 }`. No timestamp because
/// the revoke does not refresh `last_set_at`.
///
/// # Errors
///
/// Returns [`ReducerError::Invariant`] when `token.key` is not `TokenKey::Erc20`.
pub fn revoke_erc20_allowance(
    _state: &WalletState,
    delta: &mut StateDelta,
    token: &TokenRef,
    spender: Address,
) -> ReducerResult<()> {
    if !matches!(token.key, TokenKey::Erc20 { .. }) {
        return Err(ReducerError::Invariant(
            "revoke_erc20_allowance on non-Erc20 token".into(),
        ));
    }

    delta.token_changes.push(TokenChange::ApprovalRevoke {
        key: token.key.clone(),
        spender: Spender::from(spender),
        scope: ApprovalScope::Erc20,
    });

    Ok(())
}

/// Toggle `setApprovalForAll(spender, approved)` on an `ERC721` / `ERC1155`
/// contract.
///
/// `setApprovalForAll` is collection-scoped, but `TokenChange::ApprovalSet`
/// is keyed by `TokenKey` (a per-id fungibility unit). We therefore emit
/// `TokenKey::Erc721 { chain, contract, token_id: 0 }` as a *placeholder*
/// — `apply_delta` recognises an `Erc721`/`Erc1155` key combined with a
/// SetForAll-shaped `AllowanceSpec` (`is_unlimited`) as the set-for-all bucket.
/// `approved == false` emits `ApprovalRevoke { scope: SetForAll }`.
///
/// # Errors
///
/// This helper currently does not fail, but returns [`ReducerResult`] for a
/// uniform approval-helper API.
pub fn set_for_all(
    _state: &WalletState,
    delta: &mut StateDelta,
    set_at: Time,
    chain: &ChainId,
    contract: Address,
    spender: Address,
    approved: bool,
) -> ReducerResult<()> {
    let placeholder_key = TokenKey::Erc721 {
        chain: chain.clone(),
        contract,
        token_id: U256::ZERO,
    };

    if approved {
        delta.token_changes.push(TokenChange::ApprovalSet {
            key: placeholder_key,
            spender: Spender::from(spender),
            allowance: AllowanceSpec::unlimited(set_at),
        });
    } else {
        delta.token_changes.push(TokenChange::ApprovalRevoke {
            key: placeholder_key,
            spender: Spender::from(spender),
            scope: ApprovalScope::SetForAll,
        });
    }

    Ok(())
}

/// Set a single-NFT `approve(spender)` on a specific `Erc721` token id.
///
/// `nft_key` must be `TokenKey::Erc721`. ERC1155 does not have a per-token
/// `approve` (only `setApprovalForAll`), so `Erc1155` keys are rejected with
/// `ReducerError::Invariant`.
///
/// `spender == Address::ZERO` is treated as a *revoke* (mapped to
/// `Erc721ApprovedTo { spender: None }`), matching `ERC721.approve(address(0), tokenId)`.
/// Any non-zero spender is kept as `Some(spender)`.
///
/// `set_at` is currently unused (per-NFT approve has no timestamped
/// `AllowanceSpec`) but kept in the signature so all approve-side helpers
/// have a uniform shape. Marked `_set_at` to silence unused-variable warnings
/// while preserving the API.
///
/// # Errors
///
/// Returns [`ReducerError::Invariant`] when `nft_key` is not `TokenKey::Erc721`.
pub fn set_nft_approve(
    _state: &WalletState,
    delta: &mut StateDelta,
    _set_at: Time,
    nft_key: &TokenKey,
    spender: Address,
) -> ReducerResult<()> {
    if !matches!(nft_key, TokenKey::Erc721 { .. }) {
        return Err(ReducerError::Invariant(
            "set_nft_approve requires TokenKey::Erc721".into(),
        ));
    }

    let mapped = if spender == Address::ZERO {
        None
    } else {
        Some(spender)
    };

    delta.token_changes.push(TokenChange::Erc721ApprovedTo {
        key: nft_key.clone(),
        spender: mapped,
    });

    Ok(())
}

/// Revoke a `Permit2` on-chain allowance entry (e.g. `Permit2.lockdown`).
///
/// Emits `TokenChange::ApprovalRevoke { scope: Permit2 }`. The owning
/// `apply_delta` routes the revoke to `approvals.permit2`.
///
/// `token.key` must be `TokenKey::Erc20` — `Permit2` allowances are always
/// keyed by the underlying ERC20.
///
/// # Errors
///
/// Returns [`ReducerError::Invariant`] when `token.key` is not `TokenKey::Erc20`.
pub fn revoke_permit2_allowance(
    _state: &WalletState,
    delta: &mut StateDelta,
    token: &TokenRef,
    spender: Address,
) -> ReducerResult<()> {
    if !matches!(token.key, TokenKey::Erc20 { .. }) {
        return Err(ReducerError::Invariant(
            "revoke_permit2_allowance on non-Erc20 token".into(),
        ));
    }

    delta.token_changes.push(TokenChange::ApprovalRevoke {
        key: token.key.clone(),
        spender: Spender::from(spender),
        scope: ApprovalScope::Permit2,
    });

    Ok(())
}

/// Upsert a `Permit2` on-chain allowance entry (`Permit2.approve`).
///
/// `token.key` must be `TokenKey::Erc20` — `Permit2` allowances are always
/// keyed by the underlying ERC20.
///
/// The on-chain `Permit2` allowance carries `(amount, expiration, nonce)`,
/// but `TokenChange::ApprovalSet.allowance` is `AllowanceSpec`
/// (`amount + is_unlimited + last_set_at`). We therefore emit
/// `AllowanceSpec { last_set_at: expires_at }` — i.e. the expiration is
/// stored in the `last_set_at` slot. Downstream `apply_delta`, when it sees
/// an ERC20 `ApprovalSet` whose owning spender is the Permit2 contract (or
/// is recorded via a Permit2-shaped channel from the caller), reinterprets
/// `last_set_at` as the on-chain `expiration` and writes a `Permit2Allowance`
/// row. Nonce is not representable in the current `TokenChange` variant set
/// and must be threaded by the caller or supplied separately by `apply_delta`
///
/// # Errors
///
/// Returns [`ReducerError::Invariant`] when `token.key` is not `TokenKey::Erc20`.
pub fn upsert_permit2_allowance(
    _state: &WalletState,
    delta: &mut StateDelta,
    token: &TokenRef,
    spender: Spender,
    amount: U256,
    expires_at: Time,
) -> ReducerResult<()> {
    if !matches!(token.key, TokenKey::Erc20 { .. }) {
        return Err(ReducerError::Invariant(
            "upsert_permit2_allowance on non-Erc20 token".into(),
        ));
    }

    let allowance = AllowanceSpec {
        amount,
        is_unlimited: amount == U256::MAX,
        last_set_at: expires_at,
    };

    delta.token_changes.push(TokenChange::ApprovalSet {
        key: token.key.clone(),
        spender,
        allowance,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::primitives::{Address, ChainId};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::wallet::{WalletId, WalletState};
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn usdc_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    fn spender() -> Address {
        Address::from_str("0x00000000000000000000000000000000DeaDBeef").unwrap()
    }

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]))
    }

    fn nft_collection() -> Address {
        Address::from_str("0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d").unwrap()
    }

    fn nft_key(token_id: u64) -> TokenKey {
        TokenKey::Erc721 {
            chain: ChainId::ethereum_mainnet(),
            contract: nft_collection(),
            token_id: U256::from(token_id),
        }
    }

    // ---------- set_erc20_allowance ----------

    #[test]
    fn set_erc20_allowance_emits_bounded_approval_set() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        set_erc20_allowance(
            &state,
            &mut delta,
            now(),
            &usdc_ref(),
            spender(),
            U256::from(1_000_000_000u64),
        )
        .unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::ApprovalSet {
            key,
            spender: s,
            allowance,
        } = &delta.token_changes[0]
        else {
            panic!("expected ApprovalSet, got {:?}", delta.token_changes[0]);
        };
        assert_eq!(*key, usdc_ref().key);
        assert_eq!(*s, spender());
        assert_eq!(allowance.amount, U256::from(1_000_000_000u64));
        assert!(!allowance.is_unlimited);
        assert_eq!(allowance.last_set_at, now());
    }

    #[test]
    fn set_erc20_allowance_unlimited_flag() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        set_erc20_allowance(&state, &mut delta, now(), &usdc_ref(), spender(), U256::MAX).unwrap();

        let TokenChange::ApprovalSet { allowance, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalSet");
        };
        assert!(allowance.is_unlimited);
        assert_eq!(allowance.amount, U256::MAX);
    }

    #[test]
    fn set_erc20_allowance_rejects_non_erc20() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        let native = TokenRef {
            key: TokenKey::Native {
                chain: ChainId::ethereum_mainnet(),
            },
        };
        let err = set_erc20_allowance(
            &state,
            &mut delta,
            now(),
            &native,
            spender(),
            U256::from(1u8),
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
        assert!(delta.token_changes.is_empty());
    }

    // ---------- revoke_erc20_allowance ----------

    #[test]
    fn revoke_erc20_allowance_emits_scoped_revoke() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        revoke_erc20_allowance(&state, &mut delta, &usdc_ref(), spender()).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::ApprovalRevoke {
            key,
            spender: s,
            scope,
        } = &delta.token_changes[0]
        else {
            panic!("expected ApprovalRevoke");
        };
        assert_eq!(*key, usdc_ref().key);
        assert_eq!(*s, spender());
        assert_eq!(*scope, ApprovalScope::Erc20);
    }

    #[test]
    fn revoke_erc20_allowance_rejects_non_erc20() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        let bad = TokenRef { key: nft_key(1) };
        let err = revoke_erc20_allowance(&state, &mut delta, &bad, spender()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
        assert!(delta.token_changes.is_empty());
    }

    // ---------- set_for_all ----------

    #[test]
    fn set_for_all_true_emits_unlimited_approval_set_with_placeholder_key() {
        let state = empty_state();
        let mut delta = StateDelta::new();
        let chain = ChainId::ethereum_mainnet();

        set_for_all(
            &state,
            &mut delta,
            now(),
            &chain,
            nft_collection(),
            spender(),
            true,
        )
        .unwrap();

        let TokenChange::ApprovalSet {
            key,
            spender: s,
            allowance,
        } = &delta.token_changes[0]
        else {
            panic!("expected ApprovalSet");
        };
        let TokenKey::Erc721 {
            chain: c,
            contract,
            token_id,
        } = key
        else {
            panic!("expected Erc721 placeholder key");
        };
        assert_eq!(*c, chain);
        assert_eq!(*contract, nft_collection());
        assert_eq!(*token_id, U256::ZERO);
        assert_eq!(*s, spender());
        assert!(allowance.is_unlimited);
        assert_eq!(allowance.last_set_at, now());
    }

    #[test]
    fn set_for_all_false_emits_revoke_with_set_for_all_scope() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        set_for_all(
            &state,
            &mut delta,
            now(),
            &ChainId::ethereum_mainnet(),
            nft_collection(),
            spender(),
            false,
        )
        .unwrap();

        let TokenChange::ApprovalRevoke { scope, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalRevoke");
        };
        assert_eq!(*scope, ApprovalScope::SetForAll);
    }

    // ---------- set_nft_approve ----------

    #[test]
    fn set_nft_approve_emits_erc721_approved_to_with_some_spender() {
        let state = empty_state();
        let mut delta = StateDelta::new();
        let nft = nft_key(42);

        set_nft_approve(&state, &mut delta, now(), &nft, spender()).unwrap();

        let TokenChange::Erc721ApprovedTo { key, spender: s } = &delta.token_changes[0] else {
            panic!("expected Erc721ApprovedTo");
        };
        assert_eq!(*key, nft);
        assert_eq!(*s, Some(spender()));
    }

    #[test]
    fn set_nft_approve_zero_spender_maps_to_none() {
        let state = empty_state();
        let mut delta = StateDelta::new();
        let nft = nft_key(42);

        set_nft_approve(&state, &mut delta, now(), &nft, Address::ZERO).unwrap();

        let TokenChange::Erc721ApprovedTo { spender: s, .. } = &delta.token_changes[0] else {
            panic!("expected Erc721ApprovedTo");
        };
        assert_eq!(*s, None);
    }

    #[test]
    fn set_nft_approve_rejects_non_erc721_key() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        let erc1155 = TokenKey::Erc1155 {
            chain: ChainId::ethereum_mainnet(),
            contract: nft_collection(),
            token_id: U256::from(7u8),
        };
        let err = set_nft_approve(&state, &mut delta, now(), &erc1155, spender()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
        assert!(delta.token_changes.is_empty());
    }

    // ---------- upsert_permit2_allowance ----------

    #[test]
    fn upsert_permit2_allowance_packs_expiration_into_last_set_at() {
        let state = empty_state();
        let mut delta = StateDelta::new();
        let expires_at = Time::from_unix(1_738_086_400);

        upsert_permit2_allowance(
            &state,
            &mut delta,
            &usdc_ref(),
            Spender::from(spender()),
            U256::from(5_000_000u64),
            expires_at,
        )
        .unwrap();

        let TokenChange::ApprovalSet {
            key,
            spender: s,
            allowance,
        } = &delta.token_changes[0]
        else {
            panic!("expected ApprovalSet");
        };
        assert_eq!(*key, usdc_ref().key);
        assert_eq!(*s, spender());
        assert_eq!(allowance.amount, U256::from(5_000_000u64));
        assert!(!allowance.is_unlimited);
        // Permit2 stuffs the expiration into the AllowanceSpec timestamp slot.
        assert_eq!(allowance.last_set_at, expires_at);
    }

    #[test]
    fn upsert_permit2_allowance_max_amount_sets_unlimited() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        upsert_permit2_allowance(
            &state,
            &mut delta,
            &usdc_ref(),
            Spender::from(spender()),
            U256::MAX,
            now(),
        )
        .unwrap();

        let TokenChange::ApprovalSet { allowance, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalSet");
        };
        assert!(allowance.is_unlimited);
    }

    #[test]
    fn upsert_permit2_allowance_rejects_non_erc20() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        let nft = TokenRef { key: nft_key(1) };
        let err = upsert_permit2_allowance(
            &state,
            &mut delta,
            &nft,
            Spender::from(spender()),
            U256::from(1u8),
            now(),
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
        assert!(delta.token_changes.is_empty());
    }

    // ---------- revoke_permit2_allowance ----------

    #[test]
    fn revoke_permit2_allowance_emits_permit2_scoped_revoke() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        revoke_permit2_allowance(&state, &mut delta, &usdc_ref(), spender()).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::ApprovalRevoke {
            key,
            spender: s,
            scope,
        } = &delta.token_changes[0]
        else {
            panic!("expected ApprovalRevoke");
        };
        assert_eq!(*key, usdc_ref().key);
        assert_eq!(*s, spender());
        assert_eq!(*scope, ApprovalScope::Permit2);
    }

    #[test]
    fn revoke_permit2_allowance_rejects_non_erc20() {
        let state = empty_state();
        let mut delta = StateDelta::new();

        let bad = TokenRef { key: nft_key(1) };
        let err = revoke_permit2_allowance(&state, &mut delta, &bad, spender()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
        assert!(delta.token_changes.is_empty());
    }
}
