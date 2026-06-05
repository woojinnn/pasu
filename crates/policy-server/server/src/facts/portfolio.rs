//! Enrichment-fact execution for the `portfolio.*` namespace (sim-server fact
//! host, NOT the oracle/external policy-rpc daemon).
//!
//! Each fact runs a resolved [`CallSpec`] against the simulated wallet state and
//! returns the raw `$.result` JSON payload the extension materializes into
//! `context.custom`. Methods are keyed by `spec.method` in [`dispatch`].
//!
//! Params arrive in the **lowered Cedar** shape (resolved by the extension), not
//! the `simulation-state` shape — see the shared param helpers in
//! `super::params`.
//!
//! Bodies are stubbed (`FactError::NotImplemented`) so the server boots and
//! serves already-implemented methods; the [`dispatch`] match is FROZEN at
//! scaffold time and must not be edited when filling in bodies.

use serde_json::{json, Value};

use policy_state::primitives::U256;
use policy_state::token::holding::TokenHolding;
use policy_state::token::kind::{BaseCategory, FiatCurrency, PegTarget, TokenKind};

use super::params::{over_balance_4dp, param_chain_id, param_str};
use super::FactCtx;
use super::FactError;

/// Run the `portfolio.*` enrichment fact named `method` against `ctx`.
///
/// The inner match is COMPLETE and FROZEN: it has exactly one arm per
/// sim-server method in this namespace plus the unknown-method catch-all. Do not
/// edit it when implementing fact bodies.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] when no fact in this namespace is
/// registered for `method`, [`FactError::NotImplemented`] for a registered but
/// not-yet-implemented method, or [`FactError::BadParams`] when `params` is
/// missing a required field or has the wrong shape.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "portfolio.group_pct" => group_pct(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// Fixed-point scale applied to a price [`Decimal`] string so USD valuation can
/// be done in `U256` integer math (no float, no `Decimal` arithmetic). A USD
/// value summed across holdings is carried in units of `1 / 10^PRICE_SCALE` USD.
const PRICE_SCALE: u32 = 18;

/// Parse a price decimal string (`"2500.5"`, `"0.999"`, `"1"`) into a `U256`
/// fixed-point integer scaled by `10^PRICE_SCALE`, truncating any digits past
/// `PRICE_SCALE`. Returns `None` for a non-numeric / malformed string so the
/// caller can skip that holding rather than fabricate a value.
fn price_to_scaled(price: &str) -> Option<U256> {
    let price = price.trim();
    let (sign_ok, price) = match price.strip_prefix('-') {
        // Negative price is nonsensical for a holding valuation; reject.
        Some(_) => (false, price),
        None => (true, price),
    };
    if !sign_ok {
        return None;
    }
    let (int_part, frac_part) = match price.split_once('.') {
        Some((i, f)) => (i, f),
        None => (price, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return None;
    }
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let scale = PRICE_SCALE as usize;
    let mut frac = frac_part.to_owned();
    if frac.len() > scale {
        frac.truncate(scale);
    } else {
        frac.push_str(&"0".repeat(scale - frac.len()));
    }
    let combined = format!("{int_part}{frac}");
    let combined = combined.trim_start_matches('0');
    if combined.is_empty() {
        return Some(U256::ZERO);
    }
    U256::from_str_radix(combined, 10).ok()
}

/// USD value of one holding in `1 / 10^PRICE_SCALE` USD units, computed as
/// `raw_balance * price_scaled / 10^decimals` with `U256` integer math. Returns
/// `None` when the holding is unpriced, non-fungible (ERC721 `Owned`), or its
/// price string is malformed — those holdings are simply excluded from both the
/// numerator and denominator (conservative, never fabricated).
fn holding_usd(h: &TokenHolding) -> Option<U256> {
    let raw = h.balance.as_fungible()?;
    let price = h.price_usd.as_ref()?;
    let price_scaled = price_to_scaled(price.value.as_str())?;
    let numer = raw.checked_mul(price_scaled)?;
    let denom = U256::from(10u64).checked_pow(U256::from(h.decimals))?;
    if denom.is_zero() {
        return None;
    }
    Some(numer / denom)
}

/// Does this holding belong to the group identified by (`group_axis`, `asset`)?
///
/// - `token`: match the holding's contract address (lowercase hex) against
///   `asset`; `asset` may also be the native marker `"native"`.
/// - `category`: match `TokenKind::Base.category` against `asset`
///   (`stable` | `volatile` | `native_wrap` | `governance`).
/// - `fiat_peg`: match `TokenKind::Base.peg_to` fiat currency against `asset`
///   (`usd` | `eur` | `krw` | `jpy` | `gbp`).
fn holding_in_group(h: &TokenHolding, group_axis: &str, asset: &str) -> bool {
    let asset = asset.trim().to_ascii_lowercase();
    match group_axis {
        "token" => match h.key.contract() {
            Some(addr) => format!("{addr:#x}").to_ascii_lowercase() == asset,
            None => asset == "native",
        },
        "category" => match &h.kind {
            TokenKind::Base { category, .. } => {
                let name = match category {
                    BaseCategory::Stable => "stable",
                    BaseCategory::Volatile => "volatile",
                    BaseCategory::NativeWrap => "native_wrap",
                    BaseCategory::Governance { .. } => "governance",
                };
                name == asset
            }
            _ => false,
        },
        "fiat_peg" => match &h.kind {
            TokenKind::Base {
                peg_to: Some(PegTarget::Fiat(fiat)),
                ..
            } => {
                let name = match fiat {
                    FiatCurrency::Usd => "usd",
                    FiatCurrency::Eur => "eur",
                    FiatCurrency::Krw => "krw",
                    FiatCurrency::Jpy => "jpy",
                    FiatCurrency::Gbp => "gbp",
                };
                name == asset
            }
            _ => false,
        },
        _ => false,
    }
}

/// `portfolio.group_pct` (readKind: fold) — whole-wallet fold: percentage of
/// total portfolio USD held in a token/category/fiat-peg group.
///
/// ## Params (catalog)
/// - `chain_id`: Long, required (`$.root.chain_id`)
/// - `owner`: String, required (`$.root.from`)
/// - `group_axis`: String, required (enum: `token` | `category` | `fiat_peg`)
/// - `asset`: String, required (the group key being measured)
/// - `basis`: String, required (enum: `state1` | `state2`, default `state2`)
///
/// ## Outputs (catalog)
/// - `pct`: decimal, from `$.result.pct`
///
/// ## Implementation
/// Folds `WalletState.tokens`: each priced fungible holding is valued in USD via
/// [`holding_usd`] (`raw_balance * price / 10^decimals`, all `U256` integer
/// math). The numerator sums holdings matching (`group_axis`, `asset`) per
/// [`holding_in_group`]; the denominator sums all priced holdings. `pct` is the
/// `group / total` ratio scaled to a 0..100 PERCENTAGE (the group USD is
/// multiplied by 100 in `U256` space *before* the 4-dp render) so the consuming
/// Cedar policies, which compare against 0..100 thresholds, see e.g. `"75.0000"`
/// for a 75%-of-portfolio holding — not `"0.7500"`. Rendered via
/// [`over_balance_4dp`].
///
/// `basis = state1` is fully supported (the current `WalletState` snapshot).
///
/// `basis = state2` is NOT supported here: it requires applying the proposed
/// action via the reducer to obtain post-action holdings, and (a) no reducer /
/// `apply()->delta` accessor exists in the state surface, and (b) this method's
/// catalog params do not even carry the `action` body needed to apply. The
/// sampleCall uses `basis=state2`, so this fact is PARTIAL until a post-action
/// `state_after` snapshot is threaded onto `FactCtx`.
// BLOCKED (state2 branch): needs post-action holdings — no reducer apply()
// accessor on the state surface, and no `action` param on this method.
fn group_pct(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    // chain_id / owner are required by the catalog and validated for shape, but
    // the fold ranges over the whole wallet snapshot (which is already scoped to
    // this owner). `owner` is checked-present, not used to re-filter holdings.
    let _chain = param_chain_id(params, "chain_id")?;
    let _owner = param_str(params, "owner")?;
    let group_axis = param_str(params, "group_axis")?;
    let asset = param_str(params, "asset")?;
    let basis = param_str(params, "basis")?;

    if !matches!(group_axis.as_str(), "token" | "category" | "fiat_peg") {
        return Err(FactError::BadParams(format!(
            "group_axis `{group_axis}` is not one of token|category|fiat_peg"
        )));
    }

    if basis != "state1" {
        // state2 = post-reducer holdings; unavailable on the State_1-only ctx.
        return Err(FactError::BadParams(format!(
            "basis `{basis}` unsupported: state2 requires post-action holdings \
             (no reducer apply()->delta accessor and no `action` param)"
        )));
    }

    let mut group_usd = U256::ZERO;
    let mut total_usd = U256::ZERO;
    for holding in ctx.state.tokens.values() {
        let Some(usd) = holding_usd(holding) else {
            continue;
        };
        total_usd = total_usd.saturating_add(usd);
        if holding_in_group(holding, &group_axis, &asset) {
            group_usd = group_usd.saturating_add(usd);
        }
    }

    // Scale the group/total ratio to a 0..100 percentage (×100 in U256 space,
    // before the 4-dp render) — the consuming Cedar policies threshold on 0..100.
    let pct = over_balance_4dp(group_usd.saturating_mul(U256::from(100u64)), total_usd);
    Ok(json!({ "pct": pct }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::primitives::{Address, ChainId, Price, Time};
    use policy_state::token::holding::{Balance, TokenHolding};
    use policy_state::token::kind::{BaseCategory, FiatCurrency, PegTarget, TokenKind};
    use policy_state::token::TokenKey;
    use policy_state::{WalletId, WalletState};

    use serde_json::json;

    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

    fn chain() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    fn wallet_id() -> WalletId {
        WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [chain()],
        )
    }

    fn erc20_key(addr: &str) -> TokenKey {
        TokenKey::Erc20 {
            chain: chain(),
            address: Address::from_str(addr).unwrap(),
        }
    }

    /// Insert an ERC20 holding with a price into `state`.
    fn insert(
        state: &mut WalletState,
        addr: &str,
        symbol: &str,
        decimals: u8,
        raw_balance: u128,
        price: &str,
        kind: TokenKind,
    ) {
        let key = erc20_key(addr);
        state.tokens.insert(
            key.clone(),
            TokenHolding {
                key: key.clone(),
                kind,
                symbol: symbol.to_owned(),
                decimals,
                balance: Balance::fungible(U256::from(raw_balance)),
                committed: Balance::zero_fungible(),
                approved_to: None,
                price_usd: Some(LiveField::new(
                    Price::new(price.to_owned()),
                    DataSource::OracleFeed {
                        provider: policy_state::live_field::OracleProvider::Pyth,
                        feed_id: "test".into(),
                    },
                    Time::from_unix(1_700_000_000),
                )),
                metadata: None,
                value_usd: None,
                last_synced_at: Time::from_unix(1_700_000_000),
                primitives_source: DataSource::UserSupplied,
            },
        );
    }

    fn stable_usd() -> TokenKind {
        TokenKind::Base {
            category: BaseCategory::Stable,
            peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
        }
    }

    fn volatile() -> TokenKind {
        TokenKind::Base {
            category: BaseCategory::Volatile,
            peg_to: None,
        }
    }

    fn params(group_axis: &str, asset: &str, basis: &str) -> Value {
        json!({
            "chain_id": "eip155:1",
            "owner": "0x000000000000000000000000000000000000a01c",
            "group_axis": group_axis,
            "asset": asset,
            "basis": basis,
        })
    }

    /// Two holdings: 1000 USDC ($1 each = $1000) + 1 WETH ($3000) → $4000 total.
    fn two_token_state() -> WalletState {
        let mut state = WalletState::new(wallet_id());
        // 1000 USDC, 6 decimals → raw 1_000_000_000.
        insert(
            &mut state,
            USDC,
            "USDC",
            6,
            1_000_000_000,
            "1",
            stable_usd(),
        );
        // 1 WETH, 18 decimals → raw 1e18.
        insert(
            &mut state,
            WETH,
            "WETH",
            18,
            1_000_000_000_000_000_000,
            "3000",
            volatile(),
        );
        state
    }

    #[test]
    fn token_axis_shares_by_contract_address() {
        let state = two_token_state();
        // WETH is $3000 / $4000 = 75% → "75.0000".
        let out = dispatch(
            "portfolio.group_pct",
            &params("token", WETH, "state1"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["pct"], json!("75.0000"));

        // USDC is $1000 / $4000 = 25% → "25.0000".
        let out = dispatch(
            "portfolio.group_pct",
            &params("token", USDC, "state1"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["pct"], json!("25.0000"));
    }

    #[test]
    fn category_axis_buckets_by_base_category() {
        let state = two_token_state();
        // Stable bucket = USDC only = 25% → "25.0000".
        let out = dispatch(
            "portfolio.group_pct",
            &params("category", "stable", "state1"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["pct"], json!("25.0000"));

        // Volatile bucket = WETH = 75% → "75.0000".
        let out = dispatch(
            "portfolio.group_pct",
            &params("category", "volatile", "state1"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["pct"], json!("75.0000"));
    }

    #[test]
    fn fiat_peg_axis_buckets_by_peg_currency() {
        let state = two_token_state();
        // USD-pegged = USDC = 25% → "25.0000"; WETH has no fiat peg.
        let out = dispatch(
            "portfolio.group_pct",
            &params("fiat_peg", "usd", "state1"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["pct"], json!("25.0000"));

        // KRW-pegged = nothing = 0.0000.
        let out = dispatch(
            "portfolio.group_pct",
            &params("fiat_peg", "krw", "state1"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["pct"], json!("0.0000"));
    }

    #[test]
    fn unpriced_holdings_are_excluded_from_denominator() {
        let mut state = WalletState::new(wallet_id());
        insert(
            &mut state,
            USDC,
            "USDC",
            6,
            1_000_000_000,
            "1",
            stable_usd(),
        );
        // WETH with NO price → excluded from both numerator and denominator.
        let key = erc20_key(WETH);
        state.tokens.insert(
            key.clone(),
            TokenHolding {
                key: key.clone(),
                kind: volatile(),
                symbol: "WETH".to_owned(),
                decimals: 18,
                balance: Balance::fungible(U256::from(1_000_000_000_000_000_000u128)),
                committed: Balance::zero_fungible(),
                approved_to: None,
                price_usd: None,
                metadata: None,
                value_usd: None,
                last_synced_at: Time::from_unix(1_700_000_000),
                primitives_source: DataSource::UserSupplied,
            },
        );
        // USDC is the only priced holding → it is 100% of the priced portfolio.
        let out = dispatch(
            "portfolio.group_pct",
            &params("token", USDC, "state1"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["pct"], json!("100.0000"));
    }

    #[test]
    fn empty_portfolio_is_zero_not_error() {
        let state = WalletState::new(wallet_id());
        let out = dispatch(
            "portfolio.group_pct",
            &params("category", "stable", "state1"),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["pct"], json!("0.0000"));
    }

    #[test]
    fn state2_basis_is_bad_params() {
        let state = two_token_state();
        let err = dispatch(
            "portfolio.group_pct",
            &params("token", WETH, "state2"),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }

    #[test]
    fn unknown_group_axis_is_bad_params() {
        let state = two_token_state();
        let err = dispatch(
            "portfolio.group_pct",
            &params("sector", WETH, "state1"),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }

    #[test]
    fn unknown_method_in_namespace_is_unknown_method() {
        let state = two_token_state();
        let err = dispatch(
            "portfolio.not_a_method",
            &Value::Null,
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::UnknownMethod(_)), "{err:?}");
    }

    #[test]
    fn price_to_scaled_parses_and_truncates() {
        assert_eq!(
            price_to_scaled("1"),
            Some(U256::from(10u64).pow(U256::from(18)))
        );
        assert_eq!(price_to_scaled("0"), Some(U256::ZERO));
        assert_eq!(price_to_scaled("0.0"), Some(U256::ZERO));
        assert!(price_to_scaled("2500.5").is_some());
        assert_eq!(price_to_scaled(""), None);
        assert_eq!(price_to_scaled("abc"), None);
        assert_eq!(price_to_scaled("-1"), None);
    }
}
