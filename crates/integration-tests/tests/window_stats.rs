use alloy_primitives::{Address as AlloyAddress, U256};
use policy_engine::{
    Address, HostCapabilities, MockAdapterRegistry, MockOracle, MockStatWindows, Pipeline,
    PolicyEngine, RequestKind, StatDelta, StatKey, StatValue, StatWindows, TransactionRequest,
    Verdict,
};
use policy_engine_adapter_uniswap_v2::{
    encode_swap_exact_tokens_for_tokens, SwapExactTokensForTokensParams,
    UniswapV2SwapExactTokensForTokensAdapter, UNISWAP_V2_ROUTER_MAINNET,
};
use std::str::FromStr;
use std::sync::Arc;

const POLICY_WINDOW_VOLUME_CAP: &str =
    include_str!("../../../policies/dex/window-swap-volume-usd-24h-cap-5000.cedar");
const FROM: &str = "0x0000000000000000000000000000000000000001";
const USDT: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const RECIPIENT: &str = "0x1111111111111111111111111111111111111111";

fn usdt() -> policy_engine::Token {
    policy_engine::Token {
        chain_id: 1,
        address: Address::new(USDT).unwrap(),
        symbol: "USDT".into(),
        decimals: 6,
        is_native: false,
    }
}

fn from_address() -> Address {
    Address::new(FROM).unwrap()
}

fn oracle() -> MockOracle {
    MockOracle::new().with_simple_price(&usdt(), "1.0000", 5)
}

fn v2_registry() -> MockAdapterRegistry {
    MockAdapterRegistry::new()
        .with_adapter(Arc::new(UniswapV2SwapExactTokensForTokensAdapter::new()))
}

fn v2_swap_tx(amount_in: u64) -> TransactionRequest {
    let params = SwapExactTokensForTokensParams {
        amount_in: U256::from(amount_in),
        amount_out_min: U256::ZERO,
        path: vec![
            AlloyAddress::from_str(USDT).unwrap(),
            AlloyAddress::from_str(WETH).unwrap(),
        ],
        to: AlloyAddress::from_str(RECIPIENT).unwrap(),
        deadline: U256::from(9_999_999_999u64),
    };

    TransactionRequest {
        chain_id: 1,
        from: from_address(),
        to: Address::new(UNISWAP_V2_ROUTER_MAINNET).unwrap(),
        value_wei: "0".into(),
        data: encode_swap_exact_tokens_for_tokens(&params),
        gas: None,
        nonce: None,
    }
}

fn stats_key() -> StatKey {
    StatKey::SWAP_VOLUME_USD_24H
}

#[test]
fn window_stats_snapshot_reflects_confirmed_history() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let stats = MockStatWindows::new();
    let orc = oracle();
    let registry = v2_registry();
    let host = HostCapabilities::new(&orc).with_stats(&stats);
    let pipe = Pipeline::new(&registry, host, &policies);

    let actor = from_address();
    let seed = stats.reserve(
        &actor,
        vec![StatDelta {
            key: stats_key(),
            value: StatValue::Decimal("6000.00".into()),
        }],
    );
    stats.settle(seed);

    let verdict = pipe.evaluate(&v2_swap_tx(1_100_000_000)).unwrap();
    match verdict {
        Verdict::Fail(matched) => {
            assert!(matched.iter().any(|m| m.policy_id
                == "user/window-swap-volume-usd-24h-cap-5000"
                && matches!(m.origin, RequestKind::Action)));
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn window_stats_reservation_is_visible_to_next_evaluation() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let stats = MockStatWindows::new();
    let orc = oracle();
    let registry = v2_registry();
    let host = HostCapabilities::new(&orc).with_stats(&stats);
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(3_000_000_000);

    let first = pipe.evaluate_with_reservation(&tx).unwrap();
    assert!(first.reservation.is_some());
    assert_eq!(first.verdict, Verdict::Pass);
    let volume_key = stats_key();
    assert_eq!(
        stats
            .snapshot(&from_address(), &[volume_key])
            .get(&volume_key),
        Some(&StatValue::Decimal("3000.0000".into()))
    );

    let second = pipe.evaluate(&v2_swap_tx(1_000_000_000)).unwrap();
    assert_eq!(second, Verdict::Pass);

    assert_eq!(
        stats
            .snapshot(&from_address(), &[volume_key])
            .get(&volume_key),
        Some(&StatValue::Decimal("3000.0000".into()))
    );
}

#[test]
fn window_cap_boundary_crossing_uses_projected_post_tx_state() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let stats = MockStatWindows::new();
    let orc = oracle();
    let registry = v2_registry();
    let host = HostCapabilities::new(&orc).with_stats(&stats);
    let pipe = Pipeline::new(&registry, host, &policies);
    let actor = from_address();

    let seed = stats.reserve(
        &actor,
        vec![StatDelta {
            key: stats_key(),
            value: StatValue::Decimal("4900.00".into()),
        }],
    );
    stats.settle(seed);

    let verdict = pipe.evaluate(&v2_swap_tx(200_000_000)).unwrap();
    match verdict {
        Verdict::Fail(matched) => {
            assert_eq!(matched.len(), 1);
            let match_info = &matched[0];
            assert_eq!(
                match_info.policy_id,
                "user/window-swap-volume-usd-24h-cap-5000"
            );
            assert!(matches!(match_info.origin, RequestKind::Action));
        }
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn window_cap_enforce_sequential_reserved_evals() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let stats = MockStatWindows::new();
    let orc = oracle();
    let registry = v2_registry();
    let host = HostCapabilities::new(&orc).with_stats(&stats);
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(3_000_000_000);

    let first = pipe.evaluate_with_reservation(&tx).unwrap();
    assert_eq!(first.verdict, Verdict::Pass);
    assert!(first.reservation.is_some());

    let second = pipe.evaluate_with_reservation(&tx).unwrap();
    match second.verdict {
        Verdict::Fail(_) => {}
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn window_cap_evaluate_and_reservation_share_projected_state() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let stats = MockStatWindows::new();
    let orc = oracle();
    let registry = v2_registry();
    let host = HostCapabilities::new(&orc).with_stats(&stats);
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(3_000_000_000);

    let first = pipe.evaluate_with_reservation(&tx).unwrap();
    assert_eq!(first.verdict, Verdict::Pass);
    assert!(first.reservation.is_some());

    let plain = pipe.evaluate(&tx).unwrap();
    match plain {
        Verdict::Fail(_) => {}
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }

    let second = pipe.evaluate_with_reservation(&tx).unwrap();
    match second.verdict {
        Verdict::Fail(_) => {}
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

#[test]
fn window_cap_without_stats_capability_remains_fail_open() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let orc = oracle();
    let registry = v2_registry();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&orc), &policies);
    let tx = v2_swap_tx(4_900_000_000);
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
}

#[test]
fn window_cap_evaluate_with_reservation_releases_on_fail() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let stats = MockStatWindows::new();
    let orc = oracle();
    let registry = v2_registry();
    let host = HostCapabilities::new(&orc).with_stats(&stats);
    let pipe = Pipeline::new(&registry, host, &policies);
    let actor = from_address();
    let seed = stats.reserve(
        &actor,
        vec![StatDelta {
            key: stats_key(),
            value: StatValue::Decimal("4900.00".into()),
        }],
    );
    stats.settle(seed);

    let outcome = pipe
        .evaluate_with_reservation(&v2_swap_tx(200_000_000))
        .unwrap();
    assert!(matches!(outcome.verdict, Verdict::Fail(_)));
    assert!(outcome.reservation.is_none());
    assert_eq!(
        stats.confirmed(&from_address(), &stats_key()),
        Some(StatValue::Decimal("4900.0000".into()))
    );
}

#[test]
fn window_stats_settle_promotes_reservations() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let stats = MockStatWindows::new();
    let orc = oracle();
    let registry = v2_registry();
    let host = HostCapabilities::new(&orc).with_stats(&stats);
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(3_000_000_000);

    let outcome = pipe.evaluate_with_reservation(&tx).unwrap();
    let reservation = outcome.reservation.expect("expected reservation");
    stats.settle(reservation);

    assert_eq!(
        stats.confirmed(&from_address(), &stats_key()),
        Some(StatValue::Decimal("3000.0000".into()))
    );
}

#[test]
fn window_stats_release_rolls_back_snapshot() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let stats = MockStatWindows::new();
    let orc = oracle();
    let registry = v2_registry();
    let host = HostCapabilities::new(&orc).with_stats(&stats);
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(3_000_000_000);

    let outcome = pipe.evaluate_with_reservation(&tx).unwrap();
    let reservation = outcome.reservation.expect("expected reservation");
    stats.release(reservation);

    match pipe.evaluate(&tx).unwrap() {
        Verdict::Pass => {}
        other => panic!("expected Verdict::Pass, got {other:?}"),
    }
}

#[test]
fn window_stats_evaluation_does_not_reserve_on_fail() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let stats = MockStatWindows::new();
    let orc = oracle();
    let registry = v2_registry();
    let host = HostCapabilities::new(&orc).with_stats(&stats);
    let pipe = Pipeline::new(&registry, host, &policies);
    let actor = from_address();
    let seed = stats.reserve(
        &actor,
        vec![StatDelta {
            key: stats_key(),
            value: StatValue::Decimal("6001.00".into()),
        }],
    );
    stats.settle(seed);

    let tx = v2_swap_tx(1_000_000_000);
    let first = pipe.evaluate_with_reservation(&tx).unwrap();
    assert!(first.reservation.is_none());

    let second = pipe.evaluate_with_reservation(&tx).unwrap();
    assert!(second.reservation.is_none());
}

#[test]
fn window_stats_absent_without_capability() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let orc = oracle();
    let registry = v2_registry();
    let pipe = Pipeline::new(&registry, HostCapabilities::new(&orc), &policies);
    assert_eq!(
        pipe.evaluate(&v2_swap_tx(10_000_000_000)).unwrap(),
        Verdict::Pass
    );
}

#[test]
fn evaluate_does_not_create_window_reservations() {
    let policies = PolicyEngine::from_sources([POLICY_WINDOW_VOLUME_CAP]).unwrap();
    let stats = MockStatWindows::new();
    let orc = oracle();
    let registry = v2_registry();
    let host = HostCapabilities::new(&orc).with_stats(&stats);
    let pipe = Pipeline::new(&registry, host, &policies);
    let tx = v2_swap_tx(3_000_000_000);

    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);
    assert_eq!(pipe.evaluate(&tx).unwrap(), Verdict::Pass);

    let outcome = pipe.evaluate_with_reservation(&tx).unwrap();
    assert!(outcome.reservation.is_some());
    stats.settle(outcome.reservation.unwrap());

    assert_eq!(
        stats.confirmed(&from_address(), &stats_key()),
        Some(StatValue::Decimal("3000.0000".into()))
    );
}
