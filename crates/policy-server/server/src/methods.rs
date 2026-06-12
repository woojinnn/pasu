//! Enrichment-method bodies executed server-side over a loaded `WalletState`.
//!
//! Each method is a pure `fn(state, params) -> Option<Value>`: it computes a
//! derived fact the extension materializes into the Cedar `context.custom`.
//! Returning `None` means "cannot serve" — the caller leaves the result absent
//! (fail-open for `optional` calls), never fabricating a value.

use serde_json::{json, Value};

use policy_state::pending::{AssetCommitment, PendingKind, PendingStatus};
use policy_state::primitives::U256;
use policy_state::{HlAccount, PositionKind, WalletState};

/// The reserved HL account position id — mirrors the reducer's
/// `effect/hyperliquid_core/common.rs::HL_ACCOUNT_ID` (which is `pub(super)`
/// there, so the literal is repeated rather than imported).
const HL_ACCOUNT_ID: &str = "hyperliquid/account";

/// The wallet's synced Hyperliquid account, when the sync layer has produced
/// one. Mirrors the reducer's (non-public) `find_hl_account`.
fn find_hl_account(state: &WalletState) -> Option<&HlAccount> {
    state.positions.iter().find_map(|p| match &p.kind {
        PositionKind::HyperliquidAccount(a) if p.id == HL_ACCOUNT_ID => Some(a),
        _ => None,
    })
}

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

/// `perp.equity_drawdown_bps`: the synced HL account's equity drawdown from
/// today's day-start baseline (`dayDrawdownBps`) and from its high-water mark
/// (`peakDrawdownBps`), in basis points, plus whether the baseline is a true
/// day-open anchor (`baselineTrusted`). Backs the daily-loss / max-drawdown
/// circuit-breaker policies — measured exactly the way prop-firm rulebooks
/// measure it: on EQUITY (unrealized `PnL` included), from a day-start anchor.
///
/// Pure read over the loaded `WalletState`; the anchors are rolled by the sync
/// layer (`HlAccount::roll_equity_anchors`), never here. Returns `None`
/// (fail-open) when there is no synced HL account, no current equity, no
/// baseline yet, or a stored decimal doesn't parse. NOTE: the loaded state
/// must be the HL MASTER's wallet — the extension passes the resolved master
/// as the venue `wallet_id`; an unregistered/sentinel wallet loads empty and
/// lands here as `None` (the policy stays dormant).
pub(crate) fn equity_drawdown_bps(state: &WalletState, params: &Value) -> Option<Value> {
    let _chain = params.get("chain_id").and_then(Value::as_str)?;
    let hl = find_hl_account(state)?;
    let raw: f64 = hl.perp_account_value_usd.as_ref()?.as_str().parse().ok()?;
    // Flow-neutral equity: net out the cumulative non-funding capital flow so a
    // deposit/withdrawal does not read as profit/drawdown. The anchors are rolled
    // on this same flow-neutral basis by the sync layer, so `current` must match.
    let flow: f64 = hl
        .cumulative_net_flow
        .as_str()
        .parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
        .unwrap_or(0.0);
    let current = raw - flow;
    let baseline = hl.equity_baseline.as_ref()?;
    let baseline_f: f64 = baseline.value.as_str().parse().ok()?;
    if !(current.is_finite() && baseline_f.is_finite()) || baseline_f <= 0.0 {
        return None;
    }

    let day = drawdown_bps(baseline_f, current);
    // HWM is rolled together with the baseline, so it is present whenever the
    // baseline is; the day-value fallback only covers a hand-edited blob.
    let peak = hl
        .equity_hwm
        .as_ref()
        .and_then(|h| h.as_str().parse::<f64>().ok())
        .filter(|h| h.is_finite() && *h > 0.0)
        .map_or(day, |h| drawdown_bps(h, current));

    Some(json!({
        "dayDrawdownBps": day,
        "peakDrawdownBps": peak,
        "baselineTrusted": baseline.trusted,
    }))
}

/// Drawdown of `current` below `anchor` in basis points, clamped to ≥ 0 — an
/// account in profit reads 0, never a negative "gain" a `>=` cap would
/// misread. f64 per the bps precedent (`oracle_steth_peg_status_bps`).
#[allow(clippy::cast_possible_truncation)] // bounded to [0, 1e6] before the cast
fn drawdown_bps(anchor: f64, current: f64) -> i64 {
    (((anchor - current) / anchor * 10_000.0).clamp(0.0, 1_000_000.0)).round() as i64
}

/// `perp.session_fill_stats`: behavioral session aggregates over the synced HL
/// fill window. Four outputs, all UTC-day scoped (reset at midnight, like the
/// equity day-baseline):
/// - `lossStreak` — most-recent run of consecutive losing TRADES TODAY,
/// - `lossesToday` — count of losing trades today (cumulative, not consecutive),
/// - `tradesToday` — all fills since UTC day-start (frequency; spot included),
/// - `realizedPnlTodayUsd` — signed rounded USD sum of today's realized `PnL`.
///
/// Backs the cooldown / daily-loss-count / daily-realized-loss / overtrading
/// warn policies.
///
/// `lossStreak`/`lossesToday` count only MEANINGFUL trades: a close whose
/// `|closedPnl| >= min_loss` (manifest param `min_loss_usd`, default $1).
/// Sub-band "scratch" closes — and pure opens (`"0.0"`) — are INVISIBLE: they
/// neither count as a loss nor reset the streak. A win of `>= +min_loss` is the
/// first non-loss newest-first and ENDS the streak. `tradesToday`/realized are
/// size-agnostic (no band). Params: `chain_id`, optional `now` (unix SECONDS;
/// tests override), optional `min_loss_usd`.
///
/// Returns `None` (fail-open) when there is no synced HL account or the fill
/// window is EMPTY — "never polled" and "no fills in 24h" are indistinguishable
/// there, but both produce the same verdicts (0 trades / 0 streak fire
/// nothing), so no information is lost by staying dormant.
pub(crate) fn session_fill_stats(state: &WalletState, params: &Value) -> Option<Value> {
    let _chain = params.get("chain_id").and_then(Value::as_str)?;
    let now = params
        .get("now")
        .and_then(Value::as_i64)
        .unwrap_or_else(unix_now);
    let min_loss = min_loss_usd(params);
    let hl = find_hl_account(state)?;
    if hl.fill_window.is_empty() {
        return None;
    }

    // UTC day-start in fill time units (ms). `now` is seconds.
    let day_start_ms = (now / 86_400) * 86_400 * 1000;

    // The window is stored newest-first, but sort defensively — the streak is
    // order-sensitive and the blob could predate that guarantee.
    let mut fills: Vec<_> = hl.fill_window.iter().collect();
    fills.sort_by_key(|f| std::cmp::Reverse(f.time));

    let pnl_of = |f: &policy_state::HlFillSummary| f.closed_pnl.as_str().parse::<f64>().ok();
    let is_today =
        |f: &policy_state::HlFillSummary| i64::try_from(f.time).is_ok_and(|t| t >= day_start_ms);

    // `tradesToday` + `realizedPnlTodayUsd`: ALL of today's fills. Trade frequency
    // and the realized total are size-agnostic — the `min_loss` band does NOT
    // apply here. (`aggregateByTime` upstream already collapses partial fills of
    // one order into one trade row.)
    let trades_today = fills.iter().filter(|f| is_today(f)).count();
    let realized_today: f64 = fills
        .iter()
        .filter(|f| is_today(f))
        .filter_map(|f| pnl_of(f))
        .sum();

    // `lossStreak` + `lossesToday`: TODAY's MEANINGFUL trades only (|PnL| >=
    // min_loss), newest-first. Sub-band scratches (incl. opens at 0.0) are
    // invisible; a `>= +min_loss` win ends the streak. Calendar-day scoped, so a
    // streak does not span UTC midnight (accepted; honest-limit in the spec).
    let meaningful_today: Vec<f64> = fills
        .iter()
        .filter(|f| is_today(f))
        .filter_map(|f| pnl_of(f))
        .filter(|p| p.abs() >= min_loss)
        .collect();
    let loss_streak = meaningful_today.iter().take_while(|p| **p < 0.0).count();
    let losses_today = meaningful_today.iter().filter(|p| **p < 0.0).count();

    Some(json!({
        "lossStreak": loss_streak,
        "lossesToday": losses_today,
        "tradesToday": trades_today,
        "realizedPnlTodayUsd": round_usd(realized_today),
    }))
}

/// Minimum `|realized PnL|` (USD) for a closed trade to count toward the loss
/// streak / daily loss count — sub-threshold "scratch" trades are invisible
/// (neither extend nor reset the streak). Per-policy configurable via the
/// manifest `policy_rpc` literal param `min_loss_usd` (string or number);
/// default `$1`. Non-finite / non-positive → `$1`.
fn min_loss_usd(params: &Value) -> f64 {
    params
        .get("min_loss_usd")
        .and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or_else(|| v.as_f64())
        })
        .filter(|x| x.is_finite() && *x > 0.0)
        .unwrap_or(1.0)
}

/// Round a USD float to a whole-dollar `i64` (the Cedar-comparable grain).
#[allow(clippy::cast_possible_truncation)] // clamped before the cast
fn round_usd(v: f64) -> i64 {
    v.round().clamp(-1e15, 1e15) as i64
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

    // --- perp.equity_drawdown_bps ---

    use policy_state::primitives::{Decimal, ProtocolRef};
    use policy_state::{EquityAnchor, HlAccount, Position, PositionKind};

    /// A `WalletState` carrying a synced HL account with the given equity and
    /// (optionally) anchors. Mirrors the sync layer's reserved position shape.
    fn hl_state(
        equity: Option<&str>,
        baseline: Option<(&str, bool)>,
        hwm: Option<&str>,
    ) -> WalletState {
        hl_state_flow(equity, baseline, hwm, "0")
    }

    /// `hl_state` with a non-zero `cumulative_net_flow` (signed USD): the running
    /// non-funding capital flow the drawdown method nets out of raw equity.
    fn hl_state_flow(
        equity: Option<&str>,
        baseline: Option<(&str, bool)>,
        hwm: Option<&str>,
        flow: &str,
    ) -> WalletState {
        let mut s = state(vec![], vec![]);
        s.positions.push(Position {
            id: "hyperliquid/account".into(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                perp_account_value_usd: equity.map(Decimal::new),
                equity_baseline: baseline.map(|(v, trusted)| EquityAnchor {
                    value: Decimal::new(v),
                    anchored_at: Time::from_unix(864_000),
                    trusted,
                }),
                equity_hwm: hwm.map(Decimal::new),
                cumulative_net_flow: Decimal::new(flow),
                ..Default::default()
            }),
            primitives_synced_at: Time::from_unix(864_000),
            primitives_source: DataSource::UserSupplied,
        });
        s
    }

    fn drawdown(st: &WalletState) -> Option<Value> {
        super::equity_drawdown_bps(st, &serde_json::json!({ "chain_id": "hl-mainnet" }))
    }

    #[test]
    fn five_pct_daily_loss_reads_500_bps() {
        let st = hl_state(Some("950"), Some(("1000", true)), Some("1000"));
        let v = drawdown(&st).expect("served");
        assert_eq!(v["dayDrawdownBps"], 500);
        assert_eq!(v["peakDrawdownBps"], 500);
        assert_eq!(v["baselineTrusted"], true);
    }

    #[test]
    fn peak_drawdown_measures_from_hwm_not_baseline() {
        // Day: (940-920)/940 ≈ 213 bps; peak: (1000-920)/1000 = 800 bps.
        let st = hl_state(Some("920"), Some(("940", false)), Some("1000"));
        let v = drawdown(&st).expect("served");
        assert_eq!(v["dayDrawdownBps"], 213);
        assert_eq!(v["peakDrawdownBps"], 800);
        assert_eq!(v["baselineTrusted"], false);
    }

    #[test]
    fn account_in_profit_reads_zero_not_negative() {
        let st = hl_state(Some("1100"), Some(("1000", true)), Some("1100"));
        let v = drawdown(&st).expect("served");
        assert_eq!(v["dayDrawdownBps"], 0);
        assert_eq!(v["peakDrawdownBps"], 0);
    }

    #[test]
    fn no_baseline_is_none() {
        // Watch started this request — no anchor yet → fail-open, not 0.
        let st = hl_state(Some("950"), None, Some("1000"));
        assert!(drawdown(&st).is_none());
    }

    #[test]
    fn withdrawal_does_not_read_as_drawdown() {
        // Withdrew $200: raw equity 800, cumulative flow −200 → flow-neutral 1000.
        // Anchors are already on the flow-neutral scale, so drawdown must be 0 —
        // a pure capital withdrawal is not a trading loss.
        let st = hl_state_flow(Some("800"), Some(("1000", true)), Some("1000"), "-200");
        let v = drawdown(&st).expect("served");
        assert_eq!(v["dayDrawdownBps"], 0, "a withdrawal is not a daily loss");
        assert_eq!(v["peakDrawdownBps"], 0, "a withdrawal is not a drawdown");
    }

    #[test]
    fn real_loss_after_withdrawal_is_still_measured() {
        // Withdrew $200 (flow −200) AND lost on trades: raw 760 → flow-neutral
        // 960, against the 1000 peak = (1000−960)/1000 = 400 bps. The withdrawal
        // is netted out; the genuine loss is not.
        let st = hl_state_flow(Some("760"), Some(("1000", true)), Some("1000"), "-200");
        let v = drawdown(&st).expect("served");
        assert_eq!(v["peakDrawdownBps"], 400);
        assert_eq!(v["dayDrawdownBps"], 400);
    }

    #[test]
    fn deposit_does_not_mask_real_drawdown() {
        // Down to 500 on a 1000 peak (real 50% drawdown), then DEPOSITED 500 to
        // refill: raw equity 1000, flow +500 → flow-neutral 500. The breaker must
        // still see (1000−500)/1000 = 5000 bps — a top-up cannot reset it.
        let st = hl_state_flow(Some("1000"), Some(("1000", true)), Some("1000"), "500");
        let v = drawdown(&st).expect("served");
        assert_eq!(
            v["peakDrawdownBps"], 5000,
            "deposit must not erase the drawdown"
        );
    }

    #[test]
    fn no_hl_account_is_none() {
        let st = state(vec![], vec![]);
        assert!(drawdown(&st).is_none());
    }

    #[test]
    fn zero_or_garbage_baseline_is_none() {
        assert!(drawdown(&hl_state(Some("950"), Some(("0", true)), None)).is_none());
        assert!(drawdown(&hl_state(Some("950"), Some(("nope", true)), None)).is_none());
    }

    #[test]
    fn equity_drawdown_missing_chain_id_is_none() {
        let st = hl_state(Some("950"), Some(("1000", true)), Some("1000"));
        assert!(super::equity_drawdown_bps(&st, &serde_json::json!({})).is_none());
    }

    // --- perp.session_fill_stats ---

    use policy_state::HlFillSummary;

    /// Day-start used by the fill tests: NOW is 01:00 UTC into day 20615.
    const DAY_START_MS: u64 = 20_615 * 86_400 * 1000;
    const NOW_SECS: i64 = 20_615 * 86_400 + 3_600;

    fn fill(tid: u64, time: u64, pnl: &str) -> HlFillSummary {
        HlFillSummary {
            tid,
            time,
            coin: "BTC".to_owned(),
            closed_pnl: Decimal::new(pnl),
            px: Decimal::new("60000"),
            sz: Decimal::new("0.1"),
        }
    }

    fn fills_state(window: Vec<HlFillSummary>) -> WalletState {
        let mut s = state(vec![], vec![]);
        s.positions.push(Position {
            id: "hyperliquid/account".into(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::HyperliquidAccount(HlAccount {
                fill_window: window,
                ..Default::default()
            }),
            primitives_synced_at: Time::from_unix(864_000),
            primitives_source: DataSource::UserSupplied,
        });
        s
    }

    fn stats(st: &WalletState) -> Option<Value> {
        super::session_fill_stats(
            st,
            &serde_json::json!({ "chain_id": "hl-mainnet", "now": NOW_SECS }),
        )
    }

    #[test]
    fn three_consecutive_meaningful_losses_read_streak_3() {
        // Newest → oldest: loss, loss, loss, PROFIT (breaks), loss. Every
        // |PnL| >= $1 so the default band keeps them all.
        let st = fills_state(vec![
            fill(5, DAY_START_MS + 5000, "-1.0"),
            fill(4, DAY_START_MS + 4000, "-2.5"),
            fill(3, DAY_START_MS + 3000, "-1.5"),
            fill(2, DAY_START_MS + 2000, "3.0"),
            fill(1, DAY_START_MS + 1000, "-9.0"),
        ]);
        let v = stats(&st).expect("served");
        assert_eq!(v["lossStreak"], 3); // -1.0 -2.5 -1.5 then +3.0 ends it
        assert_eq!(v["lossesToday"], 4); // all four negatives (cumulative)
        assert_eq!(v["tradesToday"], 5);
        // -1.0 -2.5 -1.5 +3.0 -9.0 = -11.0.
        assert_eq!(v["realizedPnlTodayUsd"], -11);
    }

    #[test]
    fn sub_dollar_scratch_closes_are_invisible_to_streak() {
        // A +$0.40 scratch win and a -$0.10 scratch loss sit among real losses;
        // the $1 band drops both → the streak is the run of >= $1 losses, and a
        // tiny win does NOT reset it.
        let st = fills_state(vec![
            fill(5, DAY_START_MS + 5000, "-2.0"),
            fill(4, DAY_START_MS + 4000, "0.4"), // scratch win < $1 → invisible
            fill(3, DAY_START_MS + 3000, "-3.0"),
            fill(2, DAY_START_MS + 2000, "-0.1"), // scratch loss < $1 → invisible
            fill(1, DAY_START_MS + 1000, "-4.0"),
        ]);
        let v = stats(&st).expect("served");
        assert_eq!(v["lossStreak"], 3); // -2.0 -3.0 -4.0 (scratches skipped)
        assert_eq!(v["lossesToday"], 3); // -0.1 is not a meaningful loss
        assert_eq!(v["tradesToday"], 5); // scratches still count as activity
    }

    #[test]
    fn a_meaningful_win_resets_the_streak() {
        // A >= $1 win newest-first ends the streak even with older losses.
        let st = fills_state(vec![
            fill(3, DAY_START_MS + 3000, "5.0"),
            fill(2, DAY_START_MS + 2000, "-2.0"),
            fill(1, DAY_START_MS + 1000, "-3.0"),
        ]);
        let v = stats(&st).expect("served");
        assert_eq!(v["lossStreak"], 0); // newest is a >= $1 win
        assert_eq!(v["lossesToday"], 2); // both losses still counted
    }

    #[test]
    fn min_loss_usd_param_raises_the_band() {
        // min_loss_usd = 25: only the -$30 close clears the band.
        let st = fills_state(vec![
            fill(3, DAY_START_MS + 3000, "-10.0"),
            fill(2, DAY_START_MS + 2000, "-20.0"),
            fill(1, DAY_START_MS + 1000, "-30.0"),
        ]);
        let v = super::session_fill_stats(
            &st,
            &serde_json::json!({ "chain_id": "hl-mainnet", "now": NOW_SECS, "min_loss_usd": "25" }),
        )
        .expect("served");
        assert_eq!(v["lossStreak"], 1); // only -30 is meaningful at $25
        assert_eq!(v["lossesToday"], 1);
        assert_eq!(v["tradesToday"], 3); // band doesn't affect frequency
                                         // Default ($1) band sees all three.
        let d = stats(&st).expect("served");
        assert_eq!(d["lossStreak"], 3);
        assert_eq!(d["lossesToday"], 3);
    }

    #[test]
    fn pure_opens_neither_extend_nor_break_the_streak() {
        // Newest → oldest: OPEN (0.0), loss, OPEN, loss → streak 2.
        let st = fills_state(vec![
            fill(4, DAY_START_MS + 4000, "0.0"),
            fill(3, DAY_START_MS + 3000, "-1.0"),
            fill(2, DAY_START_MS + 2000, "0.0"),
            fill(1, DAY_START_MS + 1000, "-2.0"),
        ]);
        let v = stats(&st).expect("served");
        assert_eq!(v["lossStreak"], 2);
        assert_eq!(v["lossesToday"], 2);
        assert_eq!(v["tradesToday"], 4); // opens still count as activity
    }

    #[test]
    fn streak_and_losses_are_today_scoped() {
        // A loss today preceded by a loss YESTERDAY. Today-scope means the streak
        // and loss-count see only today's run — no cross-midnight span.
        let st = fills_state(vec![
            fill(2, DAY_START_MS + 1000, "-1.0"),
            fill(1, DAY_START_MS - 1000, "-2.0"), // yesterday
        ]);
        let v = stats(&st).expect("served");
        assert_eq!(v["tradesToday"], 1);
        assert_eq!(v["realizedPnlTodayUsd"], -1);
        assert_eq!(v["lossStreak"], 1); // today only — yesterday excluded
        assert_eq!(v["lossesToday"], 1);
    }

    #[test]
    fn fill_at_exact_day_start_counts_as_today() {
        // Pins the `>=` boundary: a fill at exactly UTC midnight is TODAY.
        let st = fills_state(vec![fill(1, DAY_START_MS, "-1.0")]);
        let v = stats(&st).expect("served");
        assert_eq!(v["tradesToday"], 1);
        assert_eq!(v["realizedPnlTodayUsd"], -1);
        assert_eq!(v["lossStreak"], 1);
        assert_eq!(v["lossesToday"], 1);
    }

    #[test]
    fn garbage_hwm_falls_back_to_day_drawdown_for_peak() {
        // A corrupt stored HWM must not kill the method — the peak axis falls
        // back to the day value (the baseline still parsed).
        let st = hl_state(Some("950"), Some(("1000", true)), Some("not-a-number"));
        let v = drawdown(&st).expect("served");
        assert_eq!(v["dayDrawdownBps"], 500);
        assert_eq!(v["peakDrawdownBps"], 500); // fallback = day axis
    }

    #[test]
    fn unsorted_window_is_handled() {
        // Oldest-first storage (pre-guarantee blob) must not corrupt the streak.
        let st = fills_state(vec![
            fill(1, DAY_START_MS + 1000, "5.0"),
            fill(2, DAY_START_MS + 2000, "-1.0"),
        ]);
        let v = stats(&st).expect("served");
        assert_eq!(v["lossStreak"], 1); // newest is the loss
        assert_eq!(v["lossesToday"], 1);
    }

    #[test]
    fn empty_window_is_none() {
        // Never-polled and no-fills look identical — both stay dormant, which
        // yields the same verdicts as serving zeros.
        assert!(stats(&fills_state(vec![])).is_none());
    }

    #[test]
    fn fill_stats_no_hl_account_is_none() {
        assert!(stats(&state(vec![], vec![])).is_none());
    }

    #[test]
    fn fill_stats_missing_chain_id_is_none() {
        let st = fills_state(vec![fill(1, DAY_START_MS + 1000, "-1.0")]);
        assert!(super::session_fill_stats(&st, &serde_json::json!({ "now": NOW_SECS })).is_none());
    }
}
