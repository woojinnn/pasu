//! Enrichment-fact execution — the Rust counterpart of the Node.js policy-rpc
//! method dispatch, but for **state/reducer** facts (sim-server, not the
//! oracle/external policy-rpc host at :8787).
//!
//! A [`crate::dto::CallSpec`] carries a `method` name and resolved `params`; the
//! handler runs each spec against the simulated `state_after` to produce the
//! `$.result` payload the extension materializes into `context.custom`. Methods
//! are keyed by `spec.method` in [`dispatch`].
//!
//! ## Selector-wiring gap (design proposal D/F — surfaced, not resolved)
//!
//! The DTO contract says `CallSpec.params` arrive with selectors **already
//! resolved by the extension** (`dto::CallSpec::params` doc). For the GEN-01
//! manifest those resolved values are the *lowered Cedar* shapes, NOT the
//! `simulation-state` shapes:
//!   - `$.action.token`  → `{ "key": { "standard": "erc20", "chain": "<caip2>",
//!                          "address": "0x.." } }` (lowered `Core::TokenRef`),
//!   - `$.action.amount` → a `U256` hex String (e.g. `"0xffff…"`),
//!   - `$.action.spender`, `$.root.from`, `$.root.chain_id` → plain strings.
//!
//! This module therefore parses params in that **lowered** shape and maps them
//! onto the state's `(ChainId, contract Address)` + `Spender` keys itself. The
//! server does NOT re-resolve `$.action.*` selectors against a server-side
//! lowered action — it has none (it holds `wallet_id` + `envelopes` +
//! `eval_context`, not the Cedar-lowered context). Wiring a server-side selector
//! resolver (so the server, not the extension, resolves `$.action.token` from
//! its own `envelopes`) is the open plumbing item flagged in the design doc; the
//! fact here is kept as a pure, unit-testable `fn(state, params) -> Value` so it
//! is correct regardless of where the selectors are ultimately resolved.

use serde_json::{json, Value};

use simulation_state::primitives::{Address, ChainId, U256};
use simulation_state::WalletState;

/// Error from executing an enrichment fact against wallet state.
#[derive(Debug, PartialEq, Eq)]
pub enum FactError {
    /// `spec.method` has no registered fact implementation.
    UnknownMethod(String),
    /// A required param was absent or the wrong JSON shape.
    BadParams(String),
}

impl std::fmt::Display for FactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownMethod(m) => write!(f, "unknown enrichment method `{m}`"),
            Self::BadParams(why) => write!(f, "bad enrichment params: {why}"),
        }
    }
}

impl std::error::Error for FactError {}

/// Run the enrichment fact named `method` against `state`, returning the raw
/// `$.result` JSON payload the extension materializes.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] when no fact is registered for `method`,
/// or [`FactError::BadParams`] when `params` is missing a required field or has
/// the wrong shape.
pub fn dispatch(method: &str, params: &Value, state: &WalletState) -> Result<Value, FactError> {
    match method {
        "approval.unlimited_over_balance" => approval_unlimited_over_balance(params, state),
        other => Err(FactError::UnknownMethod(other.to_owned())),
    }
}

/// GEN-01 fact: is the approval to `spender` on `token` unlimited, and how many
/// times the wallet's available balance does the approved amount cover?
///
/// Reads the recorded ERC20 allowance
/// (`WalletState.approvals.erc20[(chain, contract)][spender]`) and the token's
/// available balance ([`WalletState::available_balance`]). Returns
/// `{ isUnlimited: bool, amountOverBalance: "<decimal 4dp>" }`.
///
/// `amountOverBalance = approved_amount / available_balance`, rendered to 4
/// decimal places. Divide-by-zero is handled explicitly: when the available
/// balance is zero (or the token is not held at all), any positive approval is
/// "infinitely over balance", so we return the sentinel [`OVER_BALANCE_SENTINEL`]
/// rather than erroring — semantically "approval vastly exceeds what the wallet
/// can back". A zero approval over a zero balance yields `"0.0000"`.
fn approval_unlimited_over_balance(
    params: &Value,
    state: &WalletState,
) -> Result<Value, FactError> {
    let chain = param_str(params, "chain_id")?;
    let chain = ChainId::new(chain);
    let token_contract = param_token_contract(params)?;
    let spender = param_addr(params, "spender")?;
    let amount = param_u256(params, "amount")?;

    let alw = state
        .approvals
        .allowance(&(chain.clone(), token_contract), &spender);
    // Prefer the recorded allowance's flag (which also accounts for "high cap"
    // approvals the reducer treats as unlimited); fall back to the param amount.
    let is_unlimited = alw.map_or(amount == U256::MAX, |a| a.is_unlimited);
    // Score against the param amount the user is approving NOW (the request under
    // evaluation), which is what the policy is reasoning about.
    let available = state
        .available_balance(&simulation_state::token::TokenKey::Erc20 {
            chain,
            address: token_contract,
        })
        .unwrap_or(U256::ZERO);

    Ok(json!({
        "isUnlimited": is_unlimited,
        "amountOverBalance": over_balance_4dp(amount, available),
    }))
}

/// Sentinel returned when `amountOverBalance` is unbounded (zero available
/// balance backing a positive approval). A deliberately huge 4-dp value so a
/// Cedar `.greaterThan(...)` threshold always trips.
const OVER_BALANCE_SENTINEL: &str = "1000000000.0000";

/// Render `amount / divisor` to a 4-decimal-place string using U256 integer
/// math (no float — `amount` can be `U256::MAX`). Divisor zero → sentinel for a
/// positive numerator, `"0.0000"` for a zero numerator.
fn over_balance_4dp(amount: U256, divisor: U256) -> String {
    if divisor.is_zero() {
        return if amount.is_zero() {
            "0.0000".to_owned()
        } else {
            OVER_BALANCE_SENTINEL.to_owned()
        };
    }
    // scaled = amount * 10_000 / divisor, then split into whole.frac4.
    let scale = U256::from(10_000u64);
    let scaled = amount.saturating_mul(scale) / divisor;
    let whole = scaled / scale;
    let frac = scaled % scale;
    format!("{whole}.{frac:04}")
}

fn param_str(params: &Value, key: &str) -> Result<String, FactError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| FactError::BadParams(format!("missing string param `{key}`")))
}

fn param_addr(params: &Value, key: &str) -> Result<Address, FactError> {
    let s = param_str(params, key)?;
    s.parse::<Address>()
        .map_err(|e| FactError::BadParams(format!("param `{key}` is not an address: {e}")))
}

fn param_u256(params: &Value, key: &str) -> Result<U256, FactError> {
    let s = param_str(params, key)?;
    U256::from_str_radix(s.trim_start_matches("0x"), 16)
        .map_err(|e| FactError::BadParams(format!("param `{key}` is not a U256 hex: {e}")))
}

/// Extract the ERC20 contract address from the lowered `Core::TokenRef` param
/// shape (`{ "key": { "standard": "erc20", "address": "0x.." } }`). Only ERC20
/// is meaningful for an `approve` allowance; other standards are rejected.
fn param_token_contract(params: &Value) -> Result<Address, FactError> {
    let key = params
        .get("token")
        .and_then(|t| t.get("key"))
        .ok_or_else(|| FactError::BadParams("missing param `token.key`".to_owned()))?;
    let standard = key.get("standard").and_then(Value::as_str);
    if standard != Some("erc20") {
        return Err(FactError::BadParams(format!(
            "token.key.standard is {standard:?}, expected \"erc20\""
        )));
    }
    let addr = key
        .get("address")
        .and_then(Value::as_str)
        .ok_or_else(|| FactError::BadParams("missing param `token.key.address`".to_owned()))?;
    addr.parse::<Address>()
        .map_err(|e| FactError::BadParams(format!("token.key.address is not an address: {e}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use simulation_state::approval::AllowanceSpec;
    use simulation_state::live_field::DataSource;
    use simulation_state::primitives::Time;
    use simulation_state::token::holding::{Balance, TokenHolding};
    use simulation_state::token::kind::{BaseCategory, TokenKind};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::{WalletId, WalletState};

    const TOKEN: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const SPENDER: &str = "0x00000000000000000000000000000000deadbeef";

    fn chain() -> ChainId {
        ChainId::ethereum_mainnet()
    }

    fn token_addr() -> Address {
        Address::from_str(TOKEN).unwrap()
    }

    fn wallet_id() -> WalletId {
        WalletId::new(
            Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            [chain()],
        )
    }

    fn token_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: chain(),
            address: token_addr(),
        }
    }

    /// A `WalletState` holding `balance` of the token and an allowance to
    /// `SPENDER` (`unlimited` toggles `AllowanceSpec::unlimited` vs a bounded
    /// amount equal to `approved`).
    fn state_with(balance: u64, unlimited: bool, approved: u64) -> WalletState {
        let mut state = WalletState::new(wallet_id());
        state.tokens.insert(
            token_key(),
            TokenHolding {
                key: token_key(),
                kind: TokenKind::Base {
                    category: BaseCategory::Stable,
                    peg_to: None,
                },
                symbol: "USDC".to_owned(),
                decimals: 6,
                balance: Balance::fungible(U256::from(balance)),
                committed: Balance::zero_fungible(),
                approved_to: None,
                price_usd: None,
                last_synced_at: Time::from_unix(1_700_000_000),
                primitives_source: DataSource::OnchainView {
                    chain: chain(),
                    contract: token_addr(),
                    function: "balanceOf(address)".into(),
                    decoder_id: "erc20_balance".into(),
                },
            },
        );
        let spec = if unlimited {
            AllowanceSpec::unlimited(Time::from_unix(1_700_000_000))
        } else {
            AllowanceSpec::new(U256::from(approved), Time::from_unix(1_700_000_000))
        };
        state.approvals.erc20.insert(
            (chain(), token_addr()),
            [(Address::from_str(SPENDER).unwrap(), spec)]
                .into_iter()
                .collect(),
        );
        state
    }

    /// Lowered `Core::TokenRef` param for the ERC20 token (the shape the
    /// extension forwards for `$.action.token`).
    fn token_param() -> Value {
        let lowered = TokenRef { key: token_key() };
        // Mirror the policy-engine lowering's `{ key: { standard, chain, address } }`.
        json!({
            "key": {
                "standard": "erc20",
                "chain": lowered.key.chain().to_string(),
                "address": TOKEN
            }
        })
    }

    fn params(amount_hex: &str) -> Value {
        json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "token": token_param(),
            "spender": SPENDER,
            "amount": amount_hex
        })
    }

    #[test]
    fn unlimited_approval_over_balance_reports_unlimited() {
        // Unlimited (U256::MAX) approval, 1_000 balance → isUnlimited true.
        let state = state_with(1_000, true, 0);
        let max_hex = format!("{:#x}", U256::MAX);
        let out = dispatch("approval.unlimited_over_balance", &params(&max_hex), &state).unwrap();
        assert_eq!(out["isUnlimited"], json!(true));
        // MAX / 1000 is astronomically over balance.
        assert_ne!(out["amountOverBalance"], json!("0.0000"));
    }

    #[test]
    fn bounded_approval_over_balance_ratio_is_4dp() {
        // Approve 5_000 against a 2_000 balance → 2.5000× over balance.
        let state = state_with(2_000, false, 5_000);
        let amount_hex = format!("{:#x}", U256::from(5_000u64));
        let out = dispatch(
            "approval.unlimited_over_balance",
            &params(&amount_hex),
            &state,
        )
        .unwrap();
        assert_eq!(out["isUnlimited"], json!(false));
        assert_eq!(out["amountOverBalance"], json!("2.5000"));
    }

    #[test]
    fn zero_balance_positive_approval_returns_sentinel() {
        // No holding at all (token absent) → available 0 → sentinel.
        let mut state = WalletState::new(wallet_id());
        state.approvals.erc20.insert(
            (chain(), token_addr()),
            [(
                Address::from_str(SPENDER).unwrap(),
                AllowanceSpec::new(U256::from(100u64), Time::from_unix(1_700_000_000)),
            )]
            .into_iter()
            .collect(),
        );
        let amount_hex = format!("{:#x}", U256::from(100u64));
        let out = dispatch(
            "approval.unlimited_over_balance",
            &params(&amount_hex),
            &state,
        )
        .unwrap();
        assert_eq!(out["amountOverBalance"], json!(OVER_BALANCE_SENTINEL));
    }

    #[test]
    fn unknown_method_errors() {
        let state = WalletState::new(wallet_id());
        let err = dispatch("oracle.usd_value", &json!({}), &state).unwrap_err();
        assert!(matches!(err, FactError::UnknownMethod(_)), "{err:?}");
    }

    #[test]
    fn over_balance_4dp_math() {
        assert_eq!(
            over_balance_4dp(U256::from(5_000u64), U256::from(2_000u64)),
            "2.5000"
        );
        assert_eq!(
            over_balance_4dp(U256::from(1u64), U256::from(3u64)),
            "0.3333"
        );
        assert_eq!(over_balance_4dp(U256::ZERO, U256::ZERO), "0.0000");
        assert_eq!(
            over_balance_4dp(U256::from(1u64), U256::ZERO),
            OVER_BALANCE_SENTINEL
        );
    }
}
