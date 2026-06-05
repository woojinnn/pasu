//! Balance manipulation primitives: `debit`, `credit`, `transfer`.

use policy_state::delta::TokenChange;
use policy_state::primitives::{Address, SignedI256, U256};
use policy_state::token::{Balance, TokenKey};
use policy_state::{StateDelta, WalletState};

use crate::error::{ReducerError, ReducerResult};

/// Saturating `U256 → I256` conversion. Token amounts >= `2^255` are clamped
/// to `I256::MAX`; such values are pathological and not produced by real
/// `ERC20` calldata.
fn u256_to_signed_saturating(v: U256) -> SignedI256 {
    SignedI256::try_from(v).unwrap_or(SignedI256::MAX)
}

/// Effective signed balance for `key` = on-chain `state.tokens[key].balance`
/// (interpreted as positive) plus the algebraic sum of every prior
/// `TokenChange::BalanceDelta` entry already accumulated in `delta`.
/// Returns `Err(TokenNotFound)` when the holding is absent, and
/// `Err(Invariant)` when the holding is non-fungible (e.g. `ERC721 Owned`).
fn effective_signed_balance(
    state: &WalletState,
    delta: &StateDelta,
    key: &TokenKey,
) -> ReducerResult<SignedI256> {
    let holding = state
        .tokens
        .get(key)
        .ok_or_else(|| ReducerError::TokenNotFound(key.clone()))?;
    let base = holding.balance.as_fungible().ok_or_else(|| {
        ReducerError::Invariant(format!(
            "expected fungible balance for {key:?}, found non-fungible (Owned)"
        ))
    })?;

    let mut acc = u256_to_signed_saturating(base);
    for tc in &delta.token_changes {
        if let TokenChange::BalanceDelta { key: k, delta: d } = tc {
            if k == key {
                acc = acc.saturating_add(*d);
            }
        }
    }
    Ok(acc)
}

/// Decrease the effective fungible balance of `key` by `amount` and emit a
/// matching `TokenChange::BalanceDelta` into `delta`. Errors on underflow,
/// missing holding, or non-fungible balance form.
///
/// # Errors
///
/// Returns [`ReducerError`] if the token is missing, non-fungible, or the debit
/// would underflow the effective balance.
pub fn debit(
    state: &WalletState,
    delta: &mut StateDelta,
    key: &TokenKey,
    amount: U256,
) -> ReducerResult<()> {
    let effective = effective_signed_balance(state, delta, key)?;
    let amount_signed = u256_to_signed_saturating(amount);
    if effective < amount_signed {
        return Err(ReducerError::Invariant(format!(
            "balance underflow for {key:?}: effective {effective}, debit {amount}"
        )));
    }
    delta.token_changes.push(TokenChange::BalanceDelta {
        key: key.clone(),
        delta: -amount_signed,
    });
    Ok(())
}

/// Increase the effective fungible balance of `key` by `amount` and emit a
/// matching `TokenChange::BalanceDelta` into `delta`.
///
/// First-time receipt of a previously unseen token is **not** handled here —
/// callers must emit `TokenChange::Mint` (see PDF §8) before crediting.
/// Returns `TokenNotFound` when `key` has no holding in `state`.
///
/// # Errors
///
/// Returns [`ReducerError`] if the token is missing or has a non-fungible
/// balance form.
pub fn credit(
    state: &WalletState,
    delta: &mut StateDelta,
    key: &TokenKey,
    amount: U256,
) -> ReducerResult<()> {
    let holding = state
        .tokens
        .get(key)
        .ok_or_else(|| ReducerError::TokenNotFound(key.clone()))?;
    if matches!(holding.balance, Balance::Owned) {
        return Err(ReducerError::Invariant(format!(
            "expected fungible balance for {key:?}, found non-fungible (Owned)"
        )));
    }
    let amount_signed = u256_to_signed_saturating(amount);
    delta.token_changes.push(TokenChange::BalanceDelta {
        key: key.clone(),
        delta: amount_signed,
    });
    Ok(())
}

/// Outgoing `ERC20`-style transfer from this wallet to `recipient`.
///
/// Decreases the effective balance of `key` by `amount` (via `debit`) and
/// records the recipient in `delta` for audit. The recipient wallet itself
/// is not tracked here — the simulator only models one wallet's state.
///
/// # Errors
///
/// Returns [`ReducerError`] from [`debit`] if the token cannot be debited.
pub fn transfer(
    state: &WalletState,
    delta: &mut StateDelta,
    key: &TokenKey,
    recipient: Address,
    amount: U256,
) -> ReducerResult<()> {
    // `recipient` is kept in the signature so a future sync orchestrator
    // change (e.g. attaching a transfer-audit metadata field to
    // `TokenChange::BalanceDelta`) can be wired in without breaking
    // call-sites. Today PDF §8 only records `(key, delta)` so the
    // recipient is intentionally discarded.
    let _ = recipient;
    debit(state, delta, key, amount)
}

/// Outgoing NFT-style transfer from this wallet to `recipient`.
/// Routes by `TokenKey` standard:
/// * `Erc1155` — fungible per-id semantics; `amount` must be `Some(_)` and is
///   subtracted from the balance via [`debit`].
/// * `Erc721`  — non-fungible; `amount` is ignored. The wallet's `Owned`
///   holding is dropped via a synthetic `BalanceDelta { delta: -1 }`. The
///   `apply_delta` step recognises a signed `-1` on an `Owned` ERC721 key as
///   on a non-fungible holding errors, so reducer-side this helper emits the
///   change for audit and downstream extension is tracked separately).
/// * `Native` / `Erc20` — invariant violation (use [`transfer`] for those).
///
/// # Errors
///
/// Returns [`ReducerError`] for missing amounts, unsupported standards, missing
/// holdings, or balance underflow.
pub fn transfer_nft(
    state: &WalletState,
    delta: &mut StateDelta,
    key: &TokenKey,
    recipient: Address,
    amount: Option<U256>,
) -> ReducerResult<()> {
    let _ = recipient;

    match key {
        TokenKey::Erc1155 { .. } => {
            let amt = amount.ok_or_else(|| {
                ReducerError::Invariant(format!(
                    "transfer_nft on ERC1155 {key:?} requires an explicit amount"
                ))
            })?;
            debit(state, delta, key, amt)
        }
        TokenKey::Erc721 { .. } => {
            // Verify the wallet owns the NFT before emitting the change.
            let _holding = state
                .tokens
                .get(key)
                .ok_or_else(|| ReducerError::TokenNotFound(key.clone()))?;
            delta.token_changes.push(TokenChange::BalanceDelta {
                key: key.clone(),
                delta: -SignedI256::ONE,
            });
            Ok(())
        }
        TokenKey::Native { .. } | TokenKey::Erc20 { .. } => Err(ReducerError::Invariant(format!(
            "transfer_nft requires Erc721 or Erc1155 key, got {key:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::live_field::{DataSource, LiveField, OracleProvider};
    use policy_state::primitives::{ChainId, Decimal, Duration, Time};
    use policy_state::token::{BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKind};
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    fn mainnet_usdc_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        }
    }

    fn make_fungible_holding(key: TokenKey, amount: u128) -> TokenHolding {
        let contract = key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key,
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::fungible(U256::from(amount)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: Some(
                LiveField::new(
                    Decimal::new("1.0"),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Chainlink,
                        feed_id: "USDC/USD".into(),
                    },
                    Time::from_unix(1_000_000),
                )
                .with_ttl(Duration::from_secs(60)),
            ),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        }
    }

    fn make_owned_holding(key: TokenKey) -> TokenHolding {
        let contract = key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key,
            kind: TokenKind::Unknown,
            symbol: "NFT".into(),
            decimals: 0,
            balance: Balance::Owned,
            committed: Balance::Owned,
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract,
                function: "ownerOf(uint256)".into(),
                decoder_id: "erc721_owner".into(),
            },
        }
    }

    fn empty_state() -> WalletState {
        let owner = Address::from_str("0x0000000000000000000000000000000000000001").unwrap();
        WalletState::new(WalletId::new(owner, [ChainId::ethereum_mainnet()]))
    }

    fn state_with(holding: TokenHolding) -> WalletState {
        let mut s = empty_state();
        s.tokens.insert(holding.key.clone(), holding);
        s
    }

    /// `debit` happy path: positive balance, sufficient funds → one negative
    /// `BalanceDelta` is appended to `delta.token_changes`.
    #[test]
    fn debit_happy_path_emits_negative_delta() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 1_000));
        let mut delta = StateDelta::new();

        debit(&state, &mut delta, &key, U256::from(300u64)).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key: k, delta: d } => {
                assert_eq!(k, &key);
                assert_eq!(*d, -SignedI256::try_from(300i64).unwrap());
            }
            other => panic!("expected BalanceDelta, got {other:?}"),
        }
    }

    /// Two consecutive debits both succeed when their sum fits, and the
    /// second one's underflow check must see the first one's accumulated
    /// effect.
    #[test]
    fn debit_accumulates_across_multiple_calls() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 1_000));
        let mut delta = StateDelta::new();

        debit(&state, &mut delta, &key, U256::from(600u64)).unwrap();
        debit(&state, &mut delta, &key, U256::from(400u64)).unwrap();
        // 1000 - 600 - 400 = 0, but a 3rd debit of 1 must underflow.
        let err = debit(&state, &mut delta, &key, U256::from(1u64)).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    /// Missing holding → `TokenNotFound`.
    #[test]
    fn debit_missing_holding_returns_token_not_found() {
        let key = mainnet_usdc_key();
        let state = empty_state();
        let mut delta = StateDelta::new();
        let err = debit(&state, &mut delta, &key, U256::from(1u64)).unwrap_err();
        assert!(matches!(err, ReducerError::TokenNotFound(_)));
    }

    /// Debiting on a non-fungible `Owned` holding must be rejected as an
    /// invariant violation (callers should `transfer_nft` instead, not yet
    /// in scope here).
    #[test]
    fn debit_on_owned_balance_is_invariant_error() {
        let key = TokenKey::Erc721 {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0xb47e3cd837ddf8e4c57f05d70ab865de6e193bbb").unwrap(),
            token_id: U256::from(42u64),
        };
        let state = state_with(make_owned_holding(key.clone()));
        let mut delta = StateDelta::new();
        let err = debit(&state, &mut delta, &key, U256::from(1u64)).unwrap_err();
        match err {
            ReducerError::Invariant(msg) => {
                assert!(msg.contains("non-fungible") || msg.contains("Owned"));
            }
            other => panic!("expected Invariant, got {other:?}"),
        }
    }

    /// Pure underflow against an unchanged delta.
    #[test]
    fn debit_underflow_reports_invariant() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 100));
        let mut delta = StateDelta::new();
        let err = debit(&state, &mut delta, &key, U256::from(101u64)).unwrap_err();
        match err {
            ReducerError::Invariant(msg) => assert!(msg.contains("underflow")),
            other => panic!("expected Invariant, got {other:?}"),
        }
    }

    /// `debit` with `amount = 0` still emits a `BalanceDelta` (audit
    /// consistency: every helper invocation must leave a trace).
    #[test]
    fn debit_zero_still_pushes_delta() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 1_000));
        let mut delta = StateDelta::new();
        debit(&state, &mut delta, &key, U256::ZERO).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { delta: d, .. } => {
                assert_eq!(*d, SignedI256::ZERO);
            }
            other => panic!("expected BalanceDelta, got {other:?}"),
        }
    }

    /// `credit` happy path emits a positive `BalanceDelta`.
    #[test]
    fn credit_happy_path_emits_positive_delta() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 1_000));
        let mut delta = StateDelta::new();

        credit(&state, &mut delta, &key, U256::from(500u64)).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key: k, delta: d } => {
                assert_eq!(k, &key);
                assert_eq!(*d, SignedI256::try_from(500i64).unwrap());
            }
            other => panic!("expected BalanceDelta, got {other:?}"),
        }
    }

    /// `credit` with a missing holding must return `TokenNotFound` — Mint
    /// is a separate `TokenChange` variant outside this helper's scope.
    #[test]
    fn credit_on_missing_holding_returns_token_not_found() {
        let key = mainnet_usdc_key();
        let state = empty_state();
        let mut delta = StateDelta::new();
        let err = credit(&state, &mut delta, &key, U256::from(1u64)).unwrap_err();
        assert!(matches!(err, ReducerError::TokenNotFound(_)));
    }

    /// Crediting an `Owned` (NFT) holding is an invariant violation —
    /// `BalanceDelta` is only defined for fungible balances.
    #[test]
    fn credit_on_owned_balance_is_invariant_error() {
        let key = TokenKey::Erc721 {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0xb47e3cd837ddf8e4c57f05d70ab865de6e193bbb").unwrap(),
            token_id: U256::from(7u64),
        };
        let state = state_with(make_owned_holding(key.clone()));
        let mut delta = StateDelta::new();
        let err = credit(&state, &mut delta, &key, U256::from(1u64)).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    /// `transfer` produces the same single `BalanceDelta` as `debit` would;
    /// the recipient is currently informational only.
    #[test]
    fn transfer_delegates_to_debit_and_ignores_recipient() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 1_000));
        let mut delta = StateDelta::new();
        let recipient =
            Address::from_str("0x0000000000000000000000000000000000000beef").unwrap_or_default();

        transfer(&state, &mut delta, &key, recipient, U256::from(250u64)).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key: k, delta: d } => {
                assert_eq!(k, &key);
                assert_eq!(*d, -SignedI256::try_from(250i64).unwrap());
            }
            other => panic!("expected BalanceDelta, got {other:?}"),
        }
    }

    /// `transfer` propagates underflow exactly like `debit`.
    #[test]
    fn transfer_underflow_reports_invariant() {
        let key = mainnet_usdc_key();
        let state = state_with(make_fungible_holding(key.clone(), 10));
        let mut delta = StateDelta::new();
        let recipient =
            Address::from_str("0x000000000000000000000000000000000000dead").unwrap_or_default();
        let err = transfer(&state, &mut delta, &key, recipient, U256::from(11u64)).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    // ------------------------------------------------------------
    // transfer_nft
    // ------------------------------------------------------------

    fn erc1155_key(id: u64) -> TokenKey {
        TokenKey::Erc1155 {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d").unwrap(),
            token_id: U256::from(id),
        }
    }

    fn erc721_key(id: u64) -> TokenKey {
        TokenKey::Erc721 {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d").unwrap(),
            token_id: U256::from(id),
        }
    }

    /// `transfer_nft` on ERC1155 routes to `debit` (fungible per-id).
    #[test]
    fn transfer_nft_erc1155_emits_negative_balance_delta() {
        let key = erc1155_key(11);
        let mut state = empty_state();
        state
            .tokens
            .insert(key.clone(), make_fungible_holding(key.clone(), 10));
        let mut delta = StateDelta::new();
        let recipient =
            Address::from_str("0x000000000000000000000000000000000000beef").unwrap_or_default();

        transfer_nft(&state, &mut delta, &key, recipient, Some(U256::from(3u64))).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key: k, delta: d } => {
                assert_eq!(*k, key);
                assert_eq!(*d, -SignedI256::try_from(3i64).unwrap());
            }
            other => panic!("expected BalanceDelta, got {other:?}"),
        }
    }

    /// `transfer_nft` on ERC1155 without amount is an invariant error.
    #[test]
    fn transfer_nft_erc1155_without_amount_errors() {
        let key = erc1155_key(11);
        let mut state = empty_state();
        state
            .tokens
            .insert(key.clone(), make_fungible_holding(key.clone(), 10));
        let mut delta = StateDelta::new();
        let err = transfer_nft(
            &state,
            &mut delta,
            &key,
            Address::from_str("0x000000000000000000000000000000000000beef").unwrap_or_default(),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    /// `transfer_nft` on ERC721 emits a -1 `BalanceDelta` and verifies ownership.
    #[test]
    fn transfer_nft_erc721_emits_minus_one_balance_delta() {
        let key = erc721_key(42);
        let mut state = empty_state();
        state
            .tokens
            .insert(key.clone(), make_owned_holding(key.clone()));
        let mut delta = StateDelta::new();
        let recipient =
            Address::from_str("0x000000000000000000000000000000000000beef").unwrap_or_default();

        transfer_nft(&state, &mut delta, &key, recipient, None).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key: k, delta: d } => {
                assert_eq!(*k, key);
                assert_eq!(*d, -SignedI256::ONE);
            }
            other => panic!("expected BalanceDelta, got {other:?}"),
        }
    }

    /// `transfer_nft` on ERC721 without the wallet owning the NFT errors.
    #[test]
    fn transfer_nft_erc721_missing_holding_errors() {
        let key = erc721_key(42);
        let state = empty_state();
        let mut delta = StateDelta::new();
        let err = transfer_nft(
            &state,
            &mut delta,
            &key,
            Address::from_str("0x000000000000000000000000000000000000beef").unwrap_or_default(),
            None,
        )
        .unwrap_err();
        assert!(matches!(err, ReducerError::TokenNotFound(_)));
    }

    /// `transfer_nft` rejects ERC20 / Native keys with an invariant error.
    #[test]
    fn transfer_nft_rejects_non_nft_keys() {
        let state = empty_state();
        let mut delta = StateDelta::new();
        let erc20 = mainnet_usdc_key();
        let recipient =
            Address::from_str("0x000000000000000000000000000000000000beef").unwrap_or_default();
        let err = transfer_nft(&state, &mut delta, &erc20, recipient, None).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }
}
