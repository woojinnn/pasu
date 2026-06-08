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

/// Lowercase hex of `action.<field>.key.address` (a lowered token ref). `None`
/// for a native/missing token (no `address`).
fn action_token_address(action: &Value, field: &str) -> Option<String> {
    action
        .get(field)
        .and_then(|t| t.get("key"))
        .and_then(|k| k.get("address"))
        .and_then(asset_hex)
}

/// `intent.near_duplicate_pending`: is the new signed order a near-duplicate of an
/// already-open one — same venue + sell token + buy token? Re-signing a "did it go
/// through?" order is common and double-fills. Returns `{ "duplicate": bool }`.
///
/// Params are the manifest's `{ chain_id, owner, action }`. Reads the venue name
/// (`action.venue.name`) and the sell/buy token addresses (`action.<>.key.address`)
/// and tests membership against the live `OffchainLimitOrder` set. Matches tokens
/// by `(chain_id, address)` — a native sell OR buy (no `address`) yields `None` →
/// fail-open (the native-leg limitation is wider than just the sell side). Both
/// legs are matched against the single top-level `chain_id`; correct for today's
/// same-chain `IntentVenue`s (a per-leg cross-chain match is a follow-up).
///
/// The live set is `Active | PartiallyFilled | Unknown`. `Unknown` ("venue did not
/// respond / reconciliation failed") is INCLUDED — unlike the cap method, where it
/// is excluded to avoid double-counting a possibly-failed order: for duplicate
/// detection the risk polarity is reversed, and an `Unknown` order is exactly the
/// "did it go through?" state where re-signing the same order is most likely.
pub(crate) fn near_duplicate_pending(state: &WalletState, params: &Value) -> Option<Value> {
    let chain = params.get("chain_id").and_then(Value::as_str)?;
    let action = params.get("action")?;
    let venue = action
        .get("venue")
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)?;
    let sell = action_token_address(action, "sell")?;
    let buy = action_token_address(action, "buy")?;

    let token_matches = |t: &policy_state::token::TokenRef, addr: &str| -> bool {
        t.key.chain().as_str() == chain
            && t.key.contract().map(|a| format!("{a:#x}")).as_deref() == Some(addr)
    };

    let duplicate = state.pending.iter().any(|p| {
        matches!(
            p.lifecycle.status,
            PendingStatus::Active | PendingStatus::PartiallyFilled | PendingStatus::Unknown
        ) && match &p.kind {
            PendingKind::OffchainLimitOrder {
                venue: pv,
                sell: psell,
                buy: pbuy,
                ..
            } => pv.name == venue && token_matches(psell, &sell) && token_matches(pbuy, &buy),
            _ => false,
        }
    });
    Some(json!({ "duplicate": duplicate }))
}

/// `intent.validity_horizon_sec`: seconds from now until the order's `validUntil`
/// deadline — a long horizon means a long-lived off-chain signature (blind-sign /
/// stale-fill risk). Params: `valid_until` (unix-seconds `Long`); an optional `now`
/// (unix seconds) overrides the wall clock for deterministic tests. Returns
/// `{ "horizonSec": Long }`, clamped to ≥ 0. State is unused (pure on params).
pub(crate) fn validity_horizon_sec(_state: &WalletState, params: &Value) -> Option<Value> {
    let valid_until = params.get("valid_until").and_then(Value::as_i64)?;
    let now = params
        .get("now")
        .and_then(Value::as_i64)
        .unwrap_or_else(unix_now);
    let horizon = (valid_until - now).max(0);
    Some(json!({ "horizonSec": horizon }))
}

/// Current unix time in seconds (wall clock).
fn unix_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
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

    // --- intent.near_duplicate_pending ---

    /// An active `OffchainLimitOrder` with a chosen venue / sell / buy.
    fn dup_pending(
        id: &str,
        venue_name: &str,
        sell: &str,
        buy: &str,
        status: PendingStatus,
    ) -> PendingTx {
        let mut p = intent_pending(id, sell, 1, status);
        if let PendingKind::OffchainLimitOrder {
            venue, buy: pbuy, ..
        } = &mut p.kind
        {
            *venue = VenueRef::new(venue_name);
            *pbuy = TokenRef { key: key(buy) };
        }
        p
    }

    /// Manifest-shaped params for the new order: `action.{venue.name, sell, buy}`.
    fn dup_params(venue: &str, sell: &str, buy: &str) -> Value {
        serde_json::json!({
            "chain_id": "eip155:1",
            "owner": "0x0000000000000000000000000000000000000000",
            "action": {
                "venue": { "name": venue },
                "sell": { "key": { "standard": "erc20", "chain": "eip155:1", "address": sell } },
                "buy": { "key": { "standard": "erc20", "chain": "eip155:1", "address": buy } },
                "sellAmount": "0x1",
                "validUntil": 2_000_000_000_i64,
            }
        })
    }

    fn dup(state: &WalletState, p: &Value) -> Option<bool> {
        super::near_duplicate_pending(state, p).map(|v| v["duplicate"].as_bool().unwrap())
    }

    #[test]
    fn same_venue_sell_buy_is_duplicate() {
        let st = state(
            vec![],
            vec![dup_pending(
                "a",
                "one_inch_fusion",
                SELL,
                OTHER,
                PendingStatus::Active,
            )],
        );
        assert_eq!(
            dup(&st, &dup_params("one_inch_fusion", SELL, OTHER)),
            Some(true)
        );
    }

    #[test]
    fn different_venue_is_not_duplicate() {
        let st = state(
            vec![],
            vec![dup_pending(
                "a",
                "cow_swap",
                SELL,
                OTHER,
                PendingStatus::Active,
            )],
        );
        assert_eq!(
            dup(&st, &dup_params("one_inch_fusion", SELL, OTHER)),
            Some(false)
        );
    }

    #[test]
    fn different_buy_is_not_duplicate() {
        let st = state(
            vec![],
            vec![dup_pending(
                "a",
                "one_inch_fusion",
                SELL,
                SELL,
                PendingStatus::Active,
            )],
        );
        assert_eq!(
            dup(&st, &dup_params("one_inch_fusion", SELL, OTHER)),
            Some(false)
        );
    }

    #[test]
    fn terminal_pending_is_not_duplicate() {
        let st = state(
            vec![],
            vec![dup_pending(
                "a",
                "one_inch_fusion",
                SELL,
                OTHER,
                PendingStatus::Filled,
            )],
        );
        assert_eq!(
            dup(&st, &dup_params("one_inch_fusion", SELL, OTHER)),
            Some(false)
        );
    }

    #[test]
    fn unknown_status_pending_is_duplicate() {
        // Unknown ("did it go through?" reconciliation failure) is the prime
        // re-sign candidate → must be in the live set for duplicate detection.
        let st = state(
            vec![],
            vec![dup_pending(
                "a",
                "one_inch_fusion",
                SELL,
                OTHER,
                PendingStatus::Unknown,
            )],
        );
        assert_eq!(
            dup(&st, &dup_params("one_inch_fusion", SELL, OTHER)),
            Some(true)
        );
    }

    #[test]
    fn no_open_orders_is_not_duplicate() {
        let st = state(vec![], vec![]);
        assert_eq!(
            dup(&st, &dup_params("one_inch_fusion", SELL, OTHER)),
            Some(false)
        );
    }

    #[test]
    fn near_duplicate_unparseable_action_is_none() {
        let st = state(vec![], vec![]);
        assert!(
            super::near_duplicate_pending(&st, &serde_json::json!({ "chain_id": "eip155:1" }))
                .is_none()
        );
    }

    // --- intent.validity_horizon_sec ---

    fn horizon(valid_until: i64, now: i64) -> Option<i64> {
        let st = state(vec![], vec![]);
        let p = serde_json::json!({ "valid_until": valid_until, "now": now });
        super::validity_horizon_sec(&st, &p).map(|v| v["horizonSec"].as_i64().unwrap())
    }

    #[test]
    fn horizon_is_valid_until_minus_now() {
        assert_eq!(horizon(5000, 1000), Some(4000));
    }

    #[test]
    fn horizon_clamped_to_zero_when_already_past() {
        assert_eq!(horizon(1000, 5000), Some(0));
    }

    #[test]
    fn horizon_missing_valid_until_is_none() {
        let st = state(vec![], vec![]);
        assert!(super::validity_horizon_sec(&st, &serde_json::json!({ "now": 1000 })).is_none());
    }
}
