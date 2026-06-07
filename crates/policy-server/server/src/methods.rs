//! Enrichment-method bodies executed server-side over a loaded `WalletState`.
//!
//! Each method is a pure `fn(state, params) -> Option<Value>`: it computes a
//! derived fact the extension materializes into the Cedar `context.custom`.
//! Returning `None` means "cannot serve" — the caller leaves the result absent
//! (fail-open for `optional` calls), never fabricating a value.

use serde_json::{json, Value};

use policy_state::pending::{AssetCommitment, PendingKind, PendingStatus};
use policy_state::primitives::U256;
use policy_state::WalletState;

/// Lowercase hex address from a param that is either a bare address string or an
/// object carrying an `address` field. Mirrors `handler::asset_address`.
fn asset_hex(v: &Value) -> Option<String> {
    let raw = match v {
        Value::String(s) => s.clone(),
        Value::Object(_) => v.get("address").and_then(Value::as_str)?.to_owned(),
        _ => return None,
    };
    Some(raw.to_lowercase())
}

/// Parse a `0x`-hex (or bare hex) `U256` amount, as the policy-rpc params carry it.
fn parse_hex_u256(v: &Value) -> Option<U256> {
    let s = v.as_str()?;
    U256::from_str_radix(s.trim_start_matches("0x"), 16).ok()
}

/// `intent.pending_cap_over_balance`: do the wallet's already-open signed orders
/// selling this token, plus the new order, commit more than the held balance?
///
/// Pure fold over the loaded `WalletState`. Sums `PermitCap.max_out` of every
/// active/partially-filled `OffchainLimitOrder` whose sell token matches the new
/// order's sell token, adds the new order's sell amount, and compares to the
/// synced balance of that token. Returns `None` (fail-open) when the params don't
/// parse or the sell token has no synced holding.
///
/// Params are exactly what the manifest projects (`{ chain_id, owner, action }`):
/// `action` is the lowered `SignIntentOrder` context, so the sell token lives at
/// `action.sell.key.address` and the amount at `action.sellAmount` (camelCase
/// `0x`-hex). A native sell (the token key carries no `address`) yields `None`,
/// the documented v1 limitation. `chain_id` is the CAIP-2 string, matching a
/// `TokenKey`'s `chain().as_str()`.
pub(crate) fn pending_cap_over_balance(state: &WalletState, params: &Value) -> Option<Value> {
    let chain = params.get("chain_id").and_then(Value::as_str)?;
    let action = params.get("action")?;
    let sell = action
        .get("sell")
        .and_then(|s| s.get("key"))
        .and_then(|k| k.get("address"))
        .and_then(asset_hex)?;
    let sell_amount = action.get("sellAmount").and_then(parse_hex_u256)?;

    let matches_sell = |k: &policy_state::token::TokenKey| -> bool {
        k.chain().as_str() == chain
            && k.contract().map(|a| format!("{a:#x}")).as_deref() == Some(sell.as_str())
    };

    let cap_sum = state
        .pending
        .iter()
        .filter_map(|p| match (&p.kind, &p.commitment, &p.lifecycle.status) {
            (
                PendingKind::OffchainLimitOrder { .. },
                AssetCommitment::PermitCap { token, max_out, .. },
                PendingStatus::Active | PendingStatus::PartiallyFilled,
            ) if matches_sell(&token.key) => Some(*max_out),
            _ => None,
        })
        .fold(U256::ZERO, U256::saturating_add);

    // First matching holding with a fungible balance (skips a non-fungible
    // holding — e.g. an ERC721 — that happens to share the contract address).
    let balance = state.tokens.values().find_map(|h| {
        matches_sell(&h.key)
            .then(|| h.balance.as_fungible())
            .flatten()
    })?;

    let over = cap_sum.saturating_add(sell_amount) > balance;
    Some(json!({ "capSumOverBalance": over }))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use serde_json::Value;

    use policy_state::pending::{
        AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
    };
    use policy_state::primitives::{Address, ChainId, Time, VenueRef, U256};
    use policy_state::token::{Balance, BaseCategory, TokenHolding, TokenKey, TokenKind, TokenRef};
    use policy_state::{DataSource, StateDelta, WalletId, WalletState};

    const SELL: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"; // USDC
    const OTHER: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"; // WETH

    fn key(addr: &str) -> TokenKey {
        TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str(addr).unwrap(),
        }
    }

    fn holding(addr: &str, balance: u64) -> TokenHolding {
        TokenHolding {
            key: key(addr),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: None,
            },
            symbol: "T".into(),
            decimals: 6,
            balance: Balance::fungible(U256::from(balance)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(0),
            primitives_source: DataSource::UserSupplied,
        }
    }

    /// An `OffchainLimitOrder` selling `sell_addr` with a `PermitCap` of `cap`.
    fn intent_pending(id: &str, sell_addr: &str, cap: u64, status: PendingStatus) -> PendingTx {
        let token = TokenRef {
            key: key(sell_addr),
        };
        PendingTx {
            id: id.into(),
            kind: PendingKind::OffchainLimitOrder {
                venue: VenueRef::new("one_inch_fusion"),
                sell: token.clone(),
                buy: TokenRef { key: key(OTHER) },
                sell_max: U256::from(cap),
                buy_min: U256::from(1u64),
                order_kind: OrderKind::Dutch,
            },
            commitment: AssetCommitment::PermitCap {
                token,
                spender: Address::ZERO,
                max_out: U256::from(cap),
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status,
                valid_until: None,
                nonce: None,
                on_chain_tx: None,
                raw_status: None,
            },
            sync: DataSource::UserSupplied,
            signed_at: Time::from_unix(0),
            signature_payload: Vec::new(),
        }
    }

    fn state(holdings: Vec<TokenHolding>, pendings: Vec<PendingTx>) -> WalletState {
        let mut s = WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        for h in holdings {
            s.tokens.insert(h.key.clone(), h);
        }
        s.pending = pendings;
        s
    }

    /// The exact shape the manifest + lowering produce: `{chain_id, owner,
    /// action}` where `action` is the lowered `SignIntentOrder` (sell token nested
    /// at `action.sell.key.address`, amount at camelCase `action.sellAmount`).
    /// Mirrors `lower_token_ref`/`sign_intent_order::lower` in policy-engine.
    fn params(sell: &str, amount: u64) -> Value {
        serde_json::json!({
            "chain_id": "eip155:1",
            "owner": "0x0000000000000000000000000000000000000000",
            "action": {
                "sell": { "key": { "standard": "erc20", "chain": "eip155:1", "address": sell } },
                "sellAmount": format!("0x{amount:x}"),
            }
        })
    }

    fn over(state: &WalletState, p: &Value) -> Option<bool> {
        super::pending_cap_over_balance(state, p).map(|v| v["capSumOverBalance"].as_bool().unwrap())
    }

    #[test]
    fn under_balance_is_false() {
        let st = state(
            vec![holding(SELL, 100)],
            vec![intent_pending("a", SELL, 30, PendingStatus::Active)],
        );
        assert_eq!(over(&st, &params(SELL, 20)), Some(false));
    }

    #[test]
    fn cap_sum_plus_new_over_balance_is_true() {
        let st = state(
            vec![holding(SELL, 100)],
            vec![
                intent_pending("a", SELL, 80, PendingStatus::Active),
                intent_pending("b", SELL, 30, PendingStatus::PartiallyFilled),
            ],
        );
        assert_eq!(over(&st, &params(SELL, 5)), Some(true));
    }

    #[test]
    fn caps_on_other_sell_token_excluded() {
        let st = state(
            vec![holding(SELL, 100), holding(OTHER, 100)],
            vec![
                intent_pending("a", SELL, 10, PendingStatus::Active),
                intent_pending("b", OTHER, 90, PendingStatus::Active),
            ],
        );
        assert_eq!(over(&st, &params(SELL, 10)), Some(false));
    }

    #[test]
    fn terminal_and_non_intent_pendings_excluded() {
        let st = state(
            vec![holding(SELL, 100)],
            vec![intent_pending("a", SELL, 90, PendingStatus::Filled)],
        );
        assert_eq!(over(&st, &params(SELL, 20)), Some(false));
    }

    #[test]
    fn unknown_balance_is_none() {
        let st = state(vec![holding(OTHER, 100)], vec![]);
        assert_eq!(over(&st, &params(SELL, 20)), None);
    }

    #[test]
    fn unparseable_params_is_none() {
        let st = state(vec![holding(SELL, 100)], vec![]);
        // No `action` → cannot find the sell token/amount → fail-open.
        let bad = serde_json::json!({ "chain_id": "eip155:1" });
        assert!(super::pending_cap_over_balance(&st, &bad).is_none());
    }

    #[test]
    fn native_sell_token_is_none() {
        // A native sell (key has no `address`, only `standard:"native"`) cannot be
        // matched by (chain, contract) → None (documented v1 limitation).
        let st = state(vec![holding(SELL, 100)], vec![]);
        let native = serde_json::json!({
            "chain_id": "eip155:1",
            "action": {
                "sell": { "key": { "standard": "native", "chain": "eip155:1" } },
                "sellAmount": "0x14",
            }
        });
        assert!(super::pending_cap_over_balance(&st, &native).is_none());
    }

    #[test]
    fn saturating_add_does_not_panic_on_overflow() {
        let mut p = intent_pending("a", SELL, 0, PendingStatus::Active);
        if let AssetCommitment::PermitCap { max_out, .. } = &mut p.commitment {
            *max_out = U256::MAX;
        }
        let st = state(vec![holding(SELL, 100)], vec![p]);
        assert_eq!(over(&st, &params(SELL, 5)), Some(true));
    }
}
