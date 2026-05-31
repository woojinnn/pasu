//! Aggregator cross-cutting concerns (1inch / 0x / Paraswap / Kyberswap / Odos /
//! OKX / `Uniswap Universal Router` / `CoW` solver).
//!
//! Aggregators have no pool math of their own — each hop in their `SwapRoute`
//! delegates to an underlying single-pool venue (`uniswap_v3`, `curve_v2`, ...).
//! This file holds the *aggregator-specific* hooks the swap reducer must call
//! when `AmmVenue::AggregatorRoute` is dispatched.
//!
//! ## Phase 2G scope — stub-grade verification
//!
//! The three hooks below are Phase 2 stubs that perform *structural* checks
//! only: known-executor allow-list lookup (`verify_executor`), 32-byte hex
//! sanity check on the recorded calldata hash (`verify_calldata_hash`), and a
//! `Permit2`-shaped allowance grant when the aggregator bundles a permit
//! (`apply_permit_bundle`). Actual calldata content verification requires the
//! raw bytes in `ActionMeta`, which is a follow-up phase.
//!
//! ### Known-safe executor allow-list
//!
//! Hard-coded today for 1inch v6 only (PDF §11 fixture #4). Other aggregators
//! either don't have a separate executor (`router == executor` — single-router
//! case, handled by the `meta.executor.is_none()` branch returning `Ok(())`),
//! or are deferred to follow-up fixture batches. `AggregatorKind::Custom`
//! always rejects executor verification — the reducer refuses to vouch for
//! an unmoderated aggregator.
//!
//! 1차 출처:
//!   * 1inch v6 router — <https://docs.1inch.io/docs/aggregation-protocol/api/swagger>
//!     and the canonical mainnet deployment at
//!     `0x111111125421ca6dc452d289314280a0f8842a65` (also reproduced in
//!     1inch's published deployment registry).

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::OnceLock;

use simulation_state::primitives::{Address, Duration, Spender};
use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::{AggregatorKind, AggregatorMeta, SwapAction, SwapDirection};
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

/// Hard-coded allow-list of known-safe `(aggregator, executor)` pairs.
///
/// Keyed by a stable string discriminator (one per `AggregatorKind` variant
/// that has a separate executor today). The Phase 2G batch only verifies
/// 1inch v6 (PDF §11 fixture #4); other aggregators either use a single
/// router contract (no separate executor — handled by an `Ok(())` short-
/// circuit) or are deferred to follow-up fixture batches.
fn known_safe_executors() -> &'static HashMap<&'static str, Vec<Address>> {
    static MAP: OnceLock<HashMap<&'static str, Vec<Address>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut m = HashMap::new();
        // 1inch v6 Aggregation Router — also acts as the canonical executor
        // when the calldata embeds executor selection internally. 1차 출처:
        // https://docs.1inch.io/docs/aggregation-protocol/api/swagger
        // (deployment registry).
        m.insert(
            "one_inch_v6",
            vec![
                Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65")
                    .expect("hard-coded 1inch v6 router literal is valid"),
            ],
        );
        m
    })
}

/// Stable string discriminator for `AggregatorKind` (variants that may carry
/// a separate executor). Returns `None` for variants we haven't catalogued
/// yet — callers map `None` to "unknown executor" rejection.
const fn discriminator(kind: &AggregatorKind) -> Option<&'static str> {
    match kind {
        AggregatorKind::OneInchV6 => Some("one_inch_v6"),
        // Other aggregators: not yet catalogued. Callers see `None` and
        // treat any non-None `executor` as unknown.
        _ => None,
    }
}

/// Default expiration for a bundled `Permit2` grant — 1 hour from `ctx.now`.
///
/// Phase 2 stub: the on-chain `Permit2` expiration field is carried inside
/// the user-signed permit blob, which isn't accessible to the reducer at
/// this phase. The 1-hour default is a conservative upper bound that lines
/// up with most aggregator SDK defaults (1inch / 0x / Paraswap publish
/// roughly comparable values in their reference flows).
const PERMIT_BUNDLE_DEFAULT_TTL: Duration = Duration::from_secs(3_600);

/// Verify that the aggregator's executor contract is on a known-safe allow
/// list. Returns `Err` for unknown or fake executors.
///
/// `meta.executor.is_none()` means the aggregator uses a single router
/// contract (no router/executor split) — no executor check is needed and
/// `Ok(())` is returned. `AggregatorKind::Custom` always rejects (the
/// reducer refuses to vouch for an unmoderated aggregator).
pub(super) fn verify_executor(meta: &AggregatorMeta) -> ReducerResult<()> {
    let Some(executor) = meta.executor else {
        // Single-router aggregator — no executor split, nothing to verify.
        return Ok(());
    };

    if matches!(meta.aggregator, AggregatorKind::Custom { .. }) {
        return Err(ReducerError::Invariant(format!(
            "custom aggregator executor verification not supported (executor {executor:?})"
        )));
    }

    let Some(key) = discriminator(&meta.aggregator) else {
        return Err(ReducerError::Invariant(format!(
            "unknown executor {executor:?} for aggregator {:?}",
            meta.aggregator
        )));
    };

    let map = known_safe_executors();
    let Some(allowed) = map.get(key) else {
        return Err(ReducerError::Invariant(format!(
            "no executor allow-list entry for aggregator {:?}",
            meta.aggregator
        )));
    };

    if allowed.contains(&executor) {
        Ok(())
    } else {
        Err(ReducerError::Invariant(format!(
            "unknown executor {executor:?} for aggregator {:?}",
            meta.aggregator
        )))
    }
}

/// Verify that the calldata hash recorded in `meta` matches what would be
/// generated from the user's signed intent (replay / audit guard).
///
/// **Phase 2 stub** — the actual content verification needs the raw
/// `ActionMeta.calldata` bytes. Today we only check the *format*: the hash
/// must be 32 bytes of hex prefixed with `0x` (i.e. 66 chars total).
pub(super) fn verify_calldata_hash(meta: &AggregatorMeta) -> ReducerResult<()> {
    let h = &meta.raw_calldata_hash;
    if !h.starts_with("0x") || h.len() != 66 {
        return Err(ReducerError::Invariant(format!(
            "invalid raw_calldata_hash format: expected 0x + 64 hex chars, got {h:?}"
        )));
    }
    if !h[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ReducerError::Invariant(format!(
            "invalid raw_calldata_hash format: non-hex digit in {h:?}"
        )));
    }
    Ok(())
}

/// When `meta.permit_bundled == true`, apply the bundled `permit` step
/// (allowance grant) before the swap proceeds. Emits an `ApprovalSet`
/// change to `delta`.
///
/// Spender choice: when the aggregator has a separate executor we approve
/// that executor (the contract that actually pulls the funds); otherwise we
/// approve the router contract directly. Amount: `ExactInput.amount_in`
/// for exact-in swaps, `ExactOutput.max_amount_in` for exact-out swaps
/// (the user-signed spend cap). Expiration: `ctx.now + 1 hour` Phase 2 stub
/// default — the actual on-chain expiration lives inside the off-chain
/// `Permit2` signature, which isn't reachable here yet.
pub(super) fn apply_permit_bundle(
    state: &WalletState,
    delta: &mut StateDelta,
    ctx: &EvalContext,
    meta: &AggregatorMeta,
    swap_action: &SwapAction,
) -> ReducerResult<()> {
    if !meta.permit_bundled {
        // Caller guards on this too, but keep the no-op symmetric so the
        // helper is safe to call unconditionally.
        return Ok(());
    }

    let token = &swap_action.params.token_in;
    // Approve the executor when present (it's the actual `transferFrom`
    // caller); fall back to the router contract otherwise.
    let spender_addr = meta.executor.unwrap_or(meta.router);
    let amount = match &swap_action.params.direction {
        SwapDirection::ExactInput { amount_in, .. } => *amount_in,
        SwapDirection::ExactOutput { max_amount_in, .. } => *max_amount_in,
    };
    let expires_at = ctx.now.saturating_add(PERMIT_BUNDLE_DEFAULT_TTL);

    helpers::approval::upsert_permit2_allowance(
        state,
        delta,
        token,
        Spender::from(spender_addr),
        amount,
        expires_at,
    )
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::amm::{
        AmmVenue, PoolState, RouteHop, RoutePath, SwapDirection, SwapLiveInputs, SwapParams,
        SwapRoute,
    };
    use simulation_state::delta::TokenChange;
    use simulation_state::eval_context::RequestKind;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::primitives::{Address, ChainId, Time, U256};
    use simulation_state::token::{TokenKey, TokenRef};
    use simulation_state::wallet::WalletId;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn ctx() -> EvalContext {
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn usdc_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    fn weth_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
            },
        }
    }

    fn one_inch_router() -> Address {
        Address::from_str("0x111111125421ca6dc452d289314280a0f8842a65").unwrap()
    }

    fn fake_executor() -> Address {
        Address::from_str("0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef").unwrap()
    }

    fn zero_hash() -> String {
        format!("0x{}", "00".repeat(32))
    }

    fn meta_1inch_v6(executor: Option<Address>, permit_bundled: bool) -> AggregatorMeta {
        AggregatorMeta {
            aggregator: AggregatorKind::OneInchV6,
            router: one_inch_router(),
            executor,
            raw_calldata_hash: zero_hash(),
            permit_bundled,
            referrer: None,
            referrer_fee_bp: 0,
        }
    }

    // -----------------------------------------------------------------------
    // verify_executor
    // -----------------------------------------------------------------------

    /// `executor.is_none()` — single-router aggregator — must pass.
    #[test]
    fn verify_executor_none_passes() {
        let m = meta_1inch_v6(None, false);
        verify_executor(&m).expect("None executor should pass");
    }

    /// 1inch v6 with the known router-as-executor address passes.
    #[test]
    fn verify_executor_one_inch_v6_known_address_passes() {
        let m = meta_1inch_v6(Some(one_inch_router()), false);
        verify_executor(&m).expect("known 1inch v6 executor should pass");
    }

    /// 1inch v6 with an unknown executor address surfaces as `Invariant`.
    #[test]
    fn verify_executor_one_inch_v6_unknown_address_rejected() {
        let m = meta_1inch_v6(Some(fake_executor()), false);
        let err = verify_executor(&m).unwrap_err();
        assert!(
            matches!(&err, ReducerError::Invariant(msg) if msg.contains("unknown executor")),
            "unexpected err: {err:?}"
        );
    }

    /// `AggregatorKind::Custom` always rejects when an executor is present.
    #[test]
    fn verify_executor_custom_aggregator_rejected() {
        let mut m = meta_1inch_v6(Some(fake_executor()), false);
        m.aggregator = AggregatorKind::Custom {
            name: "myagg".into(),
        };
        let err = verify_executor(&m).unwrap_err();
        assert!(
            matches!(&err, ReducerError::Invariant(msg) if msg.contains("custom aggregator")),
            "unexpected err: {err:?}"
        );
    }

    /// An aggregator without a catalogued discriminator (e.g. `OneInchV5`)
    /// rejects any executor — these are deferred to follow-up fixture batches.
    #[test]
    fn verify_executor_uncatalogued_aggregator_rejected() {
        let mut m = meta_1inch_v6(Some(fake_executor()), false);
        m.aggregator = AggregatorKind::OneInchV5;
        let err = verify_executor(&m).unwrap_err();
        assert!(
            matches!(&err, ReducerError::Invariant(msg) if msg.contains("unknown executor")),
            "unexpected err: {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // verify_calldata_hash
    // -----------------------------------------------------------------------

    /// Well-formed 0x-prefixed 32-byte hex hash passes.
    #[test]
    fn verify_calldata_hash_valid_passes() {
        let m = meta_1inch_v6(None, false);
        verify_calldata_hash(&m).expect("valid hash should pass");
    }

    /// Wrong length (missing chars) rejected.
    #[test]
    fn verify_calldata_hash_too_short_rejected() {
        let mut m = meta_1inch_v6(None, false);
        m.raw_calldata_hash = "0xdeadbeef".into();
        let err = verify_calldata_hash(&m).unwrap_err();
        assert!(
            matches!(&err, ReducerError::Invariant(msg) if msg.contains("invalid raw_calldata_hash"))
        );
    }

    /// Missing `0x` prefix rejected.
    #[test]
    fn verify_calldata_hash_missing_prefix_rejected() {
        let mut m = meta_1inch_v6(None, false);
        m.raw_calldata_hash = "00".repeat(32);
        let err = verify_calldata_hash(&m).unwrap_err();
        assert!(
            matches!(&err, ReducerError::Invariant(msg) if msg.contains("invalid raw_calldata_hash"))
        );
    }

    /// Non-hex character rejected (correct length, correct prefix).
    #[test]
    fn verify_calldata_hash_non_hex_rejected() {
        let mut m = meta_1inch_v6(None, false);
        // 64 chars, but a 'z' instead of a hex digit.
        m.raw_calldata_hash = format!("0x{}z", "0".repeat(63));
        let err = verify_calldata_hash(&m).unwrap_err();
        assert!(matches!(&err, ReducerError::Invariant(msg) if msg.contains("non-hex")));
    }

    // -----------------------------------------------------------------------
    // apply_permit_bundle
    // -----------------------------------------------------------------------

    fn make_holding(
        token: &TokenRef,
        balance: U256,
        symbol: &str,
    ) -> simulation_state::token::TokenHolding {
        let contract = token
            .key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        simulation_state::token::TokenHolding {
            key: token.key.clone(),
            kind: simulation_state::token::TokenKind::Base {
                category: simulation_state::token::BaseCategory::Stable,
                peg_to: Some(simulation_state::token::PegTarget::Fiat(
                    simulation_state::token::FiatCurrency::Usd,
                )),
            },
            symbol: symbol.into(),
            decimals: 18,
            balance: simulation_state::token::Balance::fungible(balance),
            committed: simulation_state::token::Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
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

    fn state_with_usdc(amount: U256) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens
            .insert(usdc_ref().key, make_holding(&usdc_ref(), amount, "USDC"));
        s
    }

    fn aggregator_venue() -> AmmVenue {
        AmmVenue::AggregatorRoute {
            chain: ChainId::ethereum_mainnet(),
            router: one_inch_router(),
            route_hash: zero_hash(),
        }
    }

    fn v2_venue() -> AmmVenue {
        AmmVenue::UniswapV2 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc").unwrap(),
            factory: Address::from_str("0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f").unwrap(),
        }
    }

    fn xy_pool(reserve_in: u128, reserve_out: u128, fee_bp: u32) -> PoolState {
        PoolState::XyConstant {
            reserve_in: U256::from(reserve_in),
            reserve_out: U256::from(reserve_out),
            fee_bp,
        }
    }

    fn aggregator_swap(amount_in: U256, min_out: U256, meta: AggregatorMeta) -> SwapAction {
        let route = SwapRoute {
            paths: vec![RoutePath {
                share_bp: 10_000,
                hops: vec![RouteHop {
                    token_in: usdc_ref(),
                    token_out: weth_ref(),
                    venue: v2_venue(),
                    pool_state: xy_pool(10_000, 10_000, 30),
                    effective_fee_bp: 30,
                    estimated_out: U256::ZERO,
                }],
                estimated_out: U256::ZERO,
            }],
            aggregator: Some(meta),
        };
        SwapAction {
            venue: aggregator_venue(),
            params: SwapParams {
                token_in: usdc_ref(),
                token_out: weth_ref(),
                direction: SwapDirection::ExactInput {
                    amount_in,
                    min_amount_out: min_out,
                },
                recipient: user(),
                slippage_bp: 50,
            },
            live_inputs: SwapLiveInputs {
                route: LiveField::new(route, DataSource::UserSupplied, now()),
                expected_amount_out: LiveField::new(U256::ZERO, DataSource::UserSupplied, now()),
                price_impact_bp: LiveField::new(0u32, DataSource::UserSupplied, now()),
                gas_estimate: LiveField::new(U256::ZERO, DataSource::UserSupplied, now()),
            },
        }
    }

    /// Permit bundle emits a `Permit2`-shaped `ApprovalSet` on `token_in`
    /// to the executor (when present), with the user-signed `amount_in` and
    /// the 1-hour default expiration.
    #[test]
    fn apply_permit_bundle_emits_approval_set_on_token_in() {
        let state = state_with_usdc(U256::from(1_000_000u64));
        let mut delta = StateDelta::new();
        let meta = meta_1inch_v6(Some(one_inch_router()), true);
        let swap = aggregator_swap(U256::from(1_000u64), U256::from(800u64), meta.clone());

        apply_permit_bundle(&state, &mut delta, &ctx(), &meta, &swap).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        let TokenChange::ApprovalSet {
            key,
            spender,
            allowance,
        } = &delta.token_changes[0]
        else {
            panic!("expected ApprovalSet, got {:?}", delta.token_changes[0]);
        };
        assert_eq!(*key, usdc_ref().key);
        assert_eq!(*spender, one_inch_router());
        assert_eq!(allowance.amount, U256::from(1_000u64));
        assert!(!allowance.is_unlimited);
        // 1-hour default TTL.
        assert_eq!(
            allowance.last_set_at,
            now().saturating_add(PERMIT_BUNDLE_DEFAULT_TTL)
        );
    }

    /// `executor.is_none()` falls back to the router address as the
    /// approved spender.
    #[test]
    fn apply_permit_bundle_executor_none_uses_router_as_spender() {
        let state = state_with_usdc(U256::from(1_000_000u64));
        let mut delta = StateDelta::new();
        let meta = meta_1inch_v6(None, true);
        let swap = aggregator_swap(U256::from(2_500u64), U256::ZERO, meta.clone());

        apply_permit_bundle(&state, &mut delta, &ctx(), &meta, &swap).unwrap();

        let TokenChange::ApprovalSet { spender, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalSet");
        };
        assert_eq!(*spender, one_inch_router());
    }

    /// `permit_bundled == false` is a no-op (no `ApprovalSet` emitted).
    #[test]
    fn apply_permit_bundle_disabled_is_noop() {
        let state = state_with_usdc(U256::from(1_000_000u64));
        let mut delta = StateDelta::new();
        let meta = meta_1inch_v6(Some(one_inch_router()), false);
        let swap = aggregator_swap(U256::from(1_000u64), U256::ZERO, meta.clone());

        apply_permit_bundle(&state, &mut delta, &ctx(), &meta, &swap).unwrap();

        assert!(delta.token_changes.is_empty());
    }

    /// `ExactOutput` direction uses `max_amount_in` as the approval amount
    /// (the user-signed spend cap).
    #[test]
    fn apply_permit_bundle_exact_output_uses_max_amount_in() {
        let state = state_with_usdc(U256::from(1_000_000u64));
        let mut delta = StateDelta::new();
        let meta = meta_1inch_v6(Some(one_inch_router()), true);
        let mut swap = aggregator_swap(U256::ZERO, U256::ZERO, meta.clone());
        swap.params.direction = SwapDirection::ExactOutput {
            max_amount_in: U256::from(5_000u64),
            amount_out: U256::from(900u64),
        };

        apply_permit_bundle(&state, &mut delta, &ctx(), &meta, &swap).unwrap();

        let TokenChange::ApprovalSet { allowance, .. } = &delta.token_changes[0] else {
            panic!("expected ApprovalSet");
        };
        assert_eq!(allowance.amount, U256::from(5_000u64));
    }
}
