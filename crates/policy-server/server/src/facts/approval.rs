//! `approval.*` enrichment-fact namespace (sim-server fact host).
//!
//! Generated scaffold mirroring the `planned` sim-server `approval.*` methods in
//! `schema/method-catalog.json`. One real migrated fact
//! (`approval.unlimited_over_balance`) plus not-implemented stubs for the rest;
//! dev fills the stub bodies. The inner [`dispatch`] match is FROZEN at scaffold
//! time — do not edit it when filling bodies.

use serde_json::{json, Value};

use policy_state::primitives::{ChainId, U256};

use super::params::{over_balance_4dp, param_addr, param_str, param_token_contract, param_u256};
use super::FactCtx;
use super::FactError;

/// Dispatch an `approval.*` method to its fact implementation.
///
/// FROZEN: one arm per sim-server `approval.*` method in the catalog, plus the
/// catch-all. Devs filling in stub bodies must never edit this match.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "approval.unlimited_over_balance" => unlimited_over_balance(params, ctx),
        "approval.set_for_all_state" => set_for_all_state(params, ctx),
        "approval.resulting_allowance_state" => resulting_allowance_state(params, ctx),
        "approval.already_granted" => already_granted(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// GEN-01 fact: is the approval to `spender` on `token` unlimited, and how many
/// times the wallet's available balance does the approved amount cover?
///
/// Reads the recorded ERC20 allowance
/// (`WalletState.approvals.erc20[(chain, contract)][spender]`) and the token's
/// available balance ([`policy_state::WalletState::available_balance`]).
/// Returns
/// `{ isUnlimited: bool, amountOverBalance: "<decimal 4dp>" }`.
///
/// `amountOverBalance = approved_amount / available_balance`, rendered to 4
/// decimal places. Divide-by-zero is handled explicitly: when the available
/// balance is zero (or the token is not held at all), any positive approval is
/// "infinitely over balance", so we return the sentinel [`OVER_BALANCE_SENTINEL`]
/// rather than erroring — semantically "approval vastly exceeds what the wallet
/// can back". A zero approval over a zero balance yields `"0.0000"`.
fn unlimited_over_balance(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = param_str(params, "chain_id")?;
    let chain = ChainId::new(chain);
    let token_contract = param_token_contract(params)?;
    let spender = param_addr(params, "spender")?;
    let amount = param_u256(params, "amount")?;

    let alw = ctx
        .state
        .approvals
        .allowance(&(chain.clone(), token_contract), &spender);
    // Prefer the recorded allowance's flag (which also accounts for "high cap"
    // approvals the reducer treats as unlimited); fall back to the param amount.
    let is_unlimited = alw.map_or(amount == U256::MAX, |a| a.is_unlimited);
    // Score against the param amount the user is approving NOW (the request under
    // evaluation), which is what the policy is reasoning about.
    let available = ctx
        .state
        .available_balance(&policy_state::token::TokenKey::Erc20 {
            chain,
            address: token_contract,
        })
        .unwrap_or(U256::ZERO);

    Ok(json!({
        "isUnlimited": is_unlimited,
        "amountOverBalance": over_balance_4dp(amount, available),
    }))
}

/// `approval.set_for_all_state` (GEN-02 boost) — readKind: direct.
///
/// Whether `(collection, operator)` already holds a setApprovalForAll grant in
/// wallet state.
///
/// Catalog params:
///   - `chain_id`: Long (required) — `$.root.chain_id`
///   - `owner`: String (required) — `$.root.from`
///   - `collection`: String (required) — `$.action.contract` (NFT/1155 contract address)
///   - `operator`: String (required) — `$.action.spender`
///
/// Catalog outputs:
///   - `alreadyGranted`: Bool — `$.result.alreadyGranted`
///
/// `WalletState` accessors to call:
///   - `ApprovalSet::has_set_for_all(&self, key: &(ChainId, Address), spender: &Spender) -> bool`
///     i.e. `state.approvals.has_set_for_all(&(chain, collection), &operator)`.
fn set_for_all_state(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = ChainId::new(param_str(params, "chain_id")?);
    let collection = param_addr(params, "collection")?;
    let operator = param_addr(params, "operator")?;

    let already_granted = ctx
        .state
        .approvals
        .has_set_for_all(&(chain, collection), &operator);

    Ok(json!({ "alreadyGranted": already_granted }))
}

/// `approval.resulting_allowance_state` (GEN-15) — readKind: reducer.
///
/// Fold the existing on-chain ERC20 allowance (State1) with this
/// `increaseAllowance`/`approve` added amount, then score the U256 sum. Emits the
/// post-increase over-balance ratio and the unlimited flag (two context fields
/// from one call).
///
/// Catalog params:
///   - `chain_id`: Long (required) — `$.root.chain_id`
///   - `owner`: String (required) — `$.root.from`
///   - `token`: `AssetRef` (required) — `$.action.token` (lowered `TokenRef`; use
///     [`param_token_contract`] to get the ERC20 contract address)
///   - `spender`: String (required) — `$.action.spender`
///   - `added_amount`: String (required) — `$.action.amount` (U256 hex folded
///     onto the current allowance)
///
/// Catalog outputs:
///   - `overBalance`: decimal — `$.result.overBalance`
///   - `isUnlimited`: Bool — `$.result.isUnlimited`
///
/// `WalletState` accessors to call:
///   - `ApprovalSet::allowance(&self, key: &(ChainId, Address), spender: &Spender) -> Option<&AllowanceSpec>`
///     for the current `AllowanceSpec.amount` / `AllowanceSpec.is_unlimited` (State1).
///   - `WalletState::available_balance(&self, key: &TokenKey) -> Option<U256>`
///     for the over-balance denominator.
///
/// Then `resulting = current_amount.saturating_add(added_amount)`, render via
/// [`over_balance_4dp`]; `isUnlimited = current.is_unlimited || resulting == U256::MAX`.
fn resulting_allowance_state(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = ChainId::new(param_str(params, "chain_id")?);
    let token_contract = param_token_contract(params)?;
    let spender = param_addr(params, "spender")?;
    let added_amount = param_u256(params, "added_amount")?;

    let current = ctx
        .state
        .approvals
        .allowance(&(chain.clone(), token_contract), &spender);
    let current_amount = current.map_or(U256::ZERO, |a| a.amount);
    let current_unlimited = current.is_some_and(|a| a.is_unlimited);

    let resulting = current_amount.saturating_add(added_amount);
    let is_unlimited = current_unlimited || resulting == U256::MAX;

    let available = ctx
        .state
        .available_balance(&policy_state::token::TokenKey::Erc20 {
            chain,
            address: token_contract,
        })
        .unwrap_or(U256::ZERO);

    Ok(json!({
        "overBalance": over_balance_4dp(resulting, available),
        "isUnlimited": is_unlimited,
    }))
}

/// `approval.already_granted` (GEN-20) — readKind: direct.
///
/// Does a non-zero ERC20 allowance for `(token, spender)` already exist in wallet
/// state (`approvals_erc20.amount > 0`)? Flags redundant / surprise
/// re-approvals. ERC20-flavored sibling of [`set_for_all_state`] (NFT operator case).
///
/// Catalog params:
///   - `chain_id`: Long (required) — `$.root.chain_id`
///   - `owner`: String (required) — `$.root.from`
///   - `token`: `AssetRef` (required) — `$.action.token` (lowered `TokenRef`; use
///     [`param_token_contract`] to get the ERC20 contract address)
///   - `spender`: String (required) — `$.action.spender`
///
/// Catalog outputs:
///   - `alreadyGranted`: Bool — `$.result.alreadyGranted`
///
/// `WalletState` accessors to call:
///   - `ApprovalSet::allowance(&self, key: &(ChainId, Address), spender: &Spender) -> Option<&AllowanceSpec>`
///     then `alreadyGranted = allowance.map_or(false, |a| !a.amount.is_zero())`.
fn already_granted(params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    let chain = ChainId::new(param_str(params, "chain_id")?);
    let token_contract = param_token_contract(params)?;
    let spender = param_addr(params, "spender")?;

    let already_granted = ctx
        .state
        .approvals
        .allowance(&(chain, token_contract), &spender)
        .is_some_and(|a| !a.amount.is_zero());

    Ok(json!({ "alreadyGranted": already_granted }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use super::super::params::OVER_BALANCE_SENTINEL;
    use policy_state::approval::AllowanceSpec;
    use policy_state::live_field::DataSource;
    use policy_state::primitives::{Address, Time};
    use policy_state::token::holding::{Balance, TokenHolding};
    use policy_state::token::kind::{BaseCategory, TokenKind};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::{WalletId, WalletState};

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
                metadata: None,
                value_usd: None,
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
        let out = dispatch(
            "approval.unlimited_over_balance",
            &params(&max_hex),
            &FactCtx { state: &state },
        )
        .unwrap();
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
            &FactCtx { state: &state },
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
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["amountOverBalance"], json!(OVER_BALANCE_SENTINEL));
    }

    #[test]
    fn unknown_method_errors() {
        let state = WalletState::new(wallet_id());
        let err = dispatch("oracle.usd_value", &json!({}), &FactCtx { state: &state }).unwrap_err();
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

    fn nft_set_for_all_params(operator: &str) -> Value {
        json!({
            "chain_id": chain().to_string(),
            "owner": "0x000000000000000000000000000000000000a01c",
            "collection": TOKEN,
            "operator": operator,
        })
    }

    #[test]
    fn set_for_all_state_reports_existing_grant() {
        let mut state = state_with(1_000, false, 100);
        state.approvals.set_for_all.insert(
            (chain(), token_addr()),
            [Address::from_str(SPENDER).unwrap()].into_iter().collect(),
        );
        let granted = dispatch(
            "approval.set_for_all_state",
            &nft_set_for_all_params(SPENDER),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(granted["alreadyGranted"], json!(true));

        let other = "0x0000000000000000000000000000000000000bad";
        let absent = dispatch(
            "approval.set_for_all_state",
            &nft_set_for_all_params(other),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(absent["alreadyGranted"], json!(false));
    }

    #[test]
    fn already_granted_distinguishes_nonzero_from_zero() {
        let with_allowance = state_with(1_000, false, 100);
        let out = dispatch(
            "approval.already_granted",
            &params("0x0"),
            &FactCtx {
                state: &with_allowance,
            },
        )
        .unwrap();
        assert_eq!(out["alreadyGranted"], json!(true));

        let zero_allowance = state_with(1_000, false, 0);
        let out = dispatch(
            "approval.already_granted",
            &params("0x0"),
            &FactCtx {
                state: &zero_allowance,
            },
        )
        .unwrap();
        assert_eq!(out["alreadyGranted"], json!(false));

        // Spender with no recorded allowance at all → not granted.
        let mut no_spender = WalletState::new(wallet_id());
        no_spender.tokens = with_allowance.tokens.clone();
        let out = dispatch(
            "approval.already_granted",
            &params("0x0"),
            &FactCtx { state: &no_spender },
        )
        .unwrap();
        assert_eq!(out["alreadyGranted"], json!(false));
    }

    #[test]
    fn resulting_allowance_state_folds_added_amount() {
        // Current 2_000 allowance + add 3_000 = 5_000 over a 2_000 balance → 2.5000×.
        let state = state_with(2_000, false, 2_000);
        let added_hex = format!("{:#x}", U256::from(3_000u64));
        let out = dispatch(
            "approval.resulting_allowance_state",
            &json!({
                "chain_id": chain().to_string(),
                "owner": "0x000000000000000000000000000000000000a01c",
                "token": token_param(),
                "spender": SPENDER,
                "added_amount": added_hex,
            }),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["overBalance"], json!("2.5000"));
        assert_eq!(out["isUnlimited"], json!(false));
    }

    #[test]
    fn resulting_allowance_state_carries_unlimited_flag() {
        // Existing unlimited allowance stays unlimited after any fold.
        let state = state_with(1_000, true, 0);
        let out = dispatch(
            "approval.resulting_allowance_state",
            &json!({
                "chain_id": chain().to_string(),
                "owner": "0x000000000000000000000000000000000000a01c",
                "token": token_param(),
                "spender": SPENDER,
                "added_amount": "0x0",
            }),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["isUnlimited"], json!(true));
    }
}
