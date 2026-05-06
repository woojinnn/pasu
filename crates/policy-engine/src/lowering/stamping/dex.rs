//! Dex action enrichment and projected stat-window stamping.

use crate::core::{
    Address, DexAction, OracleRequirementKind, Token, UsdValuation, WindowStatsContext,
};
use crate::host::stat_windows::{StatDelta, StatKey, StatValue};
use crate::host::{HostCapabilities, Oracle};
use crate::lowering::decimal::{try_add_decimal_strings, try_multiply_decimal_strings};
use alloy_primitives::U256;
use std::collections::HashMap;

/// Enrich a DEX action with host facts and projected window stats.
pub fn enrich_dex_action(action: &mut DexAction, host: &HostCapabilities<'_>) {
    enrich_dex_action_base(action, host);
    let deltas = compute_dex_window_deltas(action);
    enrich_dex_window_stats(action, host, &deltas);
}

/// Enrich a DEX action with oracle, portfolio, and allowance facts.
pub fn enrich_dex_action_base(action: &mut DexAction, host: &HostCapabilities<'_>) {
    action.facts.total_input_usd =
        total_usd_for_kind(action, OracleRequirementKind::Input, host.oracle());
    action.facts.total_min_output_usd =
        total_usd_for_kind(action, OracleRequirementKind::MinOutput, host.oracle());
    action.facts.total_input_fraction_of_portfolio_bps =
        total_input_fraction_of_portfolio_bps(action, host);
    action.facts.allowances_cover_inputs = allowances_cover_inputs(action, host);
}

/// Stamp projected stat-window facts onto a DEX action.
pub fn enrich_dex_window_stats(
    action: &mut DexAction,
    host: &HostCapabilities<'_>,
    pending_deltas: &[StatDelta],
) {
    action.facts.window_stats = projected_window_stats(&action.actor, host, pending_deltas);
}

/// Compute stat-window deltas represented by this DEX action.
#[must_use]
pub fn compute_dex_window_deltas(action: &DexAction) -> Vec<StatDelta> {
    let mut deltas = Vec::new();
    if let Some(usd) = &action.facts.total_input_usd {
        deltas.push(StatDelta {
            key: StatKey::SWAP_VOLUME_USD_24H,
            value: StatValue::Decimal(usd.value.clone()),
        });
    }
    deltas.push(StatDelta {
        key: StatKey::SWAP_COUNT_24H,
        value: StatValue::Count(1),
    });
    deltas
}

fn total_usd_for_kind(
    action: &DexAction,
    kind: OracleRequirementKind,
    oracle: &dyn Oracle,
) -> Option<UsdValuation> {
    let mut total = None;

    for requirement in action
        .oracle_requirements
        .iter()
        .filter(|requirement| requirement.kind == kind)
    {
        let Some(unit_price) = oracle.price(&requirement.token).ok() else {
            continue;
        };
        let Some(valuation) = scaled_usd(
            &requirement.raw_amount,
            requirement.token.decimals,
            &unit_price,
        ) else {
            continue;
        };
        total = Some(match total.take() {
            Some(previous) => sum_valuations(previous, valuation),
            None => valuation,
        });
    }

    total
}

fn scaled_usd(raw: &str, decimals: u32, valuation: &UsdValuation) -> Option<UsdValuation> {
    let value = try_multiply_decimal_strings(raw, decimals, &valuation.value)?;
    Some(UsdValuation {
        value,
        as_of_ts: valuation.as_of_ts,
        sources: valuation.sources.clone(),
        stale_sec: valuation.stale_sec,
    })
}

fn sum_valuations(mut left: UsdValuation, right: UsdValuation) -> UsdValuation {
    let Some(value) = try_add_decimal_strings(&left.value, &right.value) else {
        return left;
    };
    left.value = value;
    left.as_of_ts = left.as_of_ts.min(right.as_of_ts);
    left.stale_sec = left.stale_sec.max(right.stale_sec);
    left.sources.extend(right.sources);
    left.sources.sort();
    left.sources.dedup();
    left
}

fn total_input_fraction_of_portfolio_bps(
    action: &DexAction,
    host: &HostCapabilities<'_>,
) -> Option<u64> {
    let portfolio = host.portfolio()?;
    let inputs = grouped_input_requirements(action)?;
    if inputs.is_empty() {
        return None;
    }

    let mut total_bps = 0u64;
    for (token, input_raw) in inputs {
        let balance = portfolio.balance(&action.actor, &token).ok()?;
        let balance_raw = amount_raw_u256(&balance.raw)?;
        let token_bps = fraction_bps(input_raw, balance_raw)?;
        total_bps = total_bps.saturating_add(token_bps);
    }

    Some(total_bps)
}

fn allowances_cover_inputs(action: &DexAction, host: &HostCapabilities<'_>) -> Option<bool> {
    let approvals = host.approvals();
    let Some(inputs) = grouped_input_requirements(action) else {
        return approvals.map(|_| false);
    };
    let non_native_inputs: Vec<_> = inputs
        .into_iter()
        .filter(|(token, _)| !token.is_native)
        .collect();

    if non_native_inputs.is_empty() {
        return Some(true);
    }

    let approvals = approvals?;
    for (token, input_raw) in non_native_inputs {
        let Ok(allowance) = approvals.allowance(&action.actor, &token, &action.target) else {
            return Some(false);
        };
        let Some(allowance_raw) = amount_raw_u256(&allowance.raw) else {
            return Some(false);
        };
        if allowance_raw < input_raw {
            return Some(false);
        }
    }

    Some(true)
}

fn grouped_input_requirements(action: &DexAction) -> Option<Vec<(Token, U256)>> {
    let mut groups: Vec<(Token, U256)> = Vec::new();
    for requirement in action
        .oracle_requirements
        .iter()
        .filter(|requirement| requirement.kind == OracleRequirementKind::Input)
    {
        let raw = amount_raw_u256(&requirement.raw_amount)?;
        if let Some((_, total_raw)) = groups
            .iter_mut()
            .find(|(token, _)| token.key() == requirement.token.key())
        {
            *total_raw = total_raw.saturating_add(raw);
        } else {
            groups.push((requirement.token.clone(), raw));
        }
    }
    Some(groups)
}

fn fraction_bps(input_raw: U256, balance_raw: U256) -> Option<u64> {
    if balance_raw.is_zero() {
        return None;
    }

    let ratio = input_raw.saturating_mul(U256::from(10_000u64)) / balance_raw;
    let max = U256::from(u64::MAX);
    if ratio > max {
        Some(u64::MAX)
    } else {
        ratio.to_string().parse::<u64>().ok()
    }
}

fn amount_raw_u256(raw: &str) -> Option<U256> {
    U256::from_str_radix(raw, 10).ok()
}

fn projected_window_stats(
    actor: &Address,
    host: &HostCapabilities<'_>,
    pending_deltas: &[StatDelta],
) -> Option<WindowStatsContext> {
    let stats = host.stats()?;
    let keys = [StatKey::SWAP_VOLUME_USD_24H, StatKey::SWAP_COUNT_24H];
    let mut snapshot = stats.snapshot(actor, &keys);

    for delta in pending_deltas {
        let value = snapshot.remove(&delta.key);
        snapshot.insert(delta.key, merge_stat_value(value, &delta.value));
    }

    window_stats_from_snapshot(&snapshot)
}

fn merge_stat_value(base: Option<StatValue>, delta: &StatValue) -> StatValue {
    match (base, delta) {
        (Some(StatValue::Decimal(left)), StatValue::Decimal(right)) => {
            StatValue::Decimal(crate::lowering::decimal::add_decimal_strings(&left, right))
        }
        (Some(StatValue::Count(left)), StatValue::Count(right)) => {
            StatValue::Count(left.saturating_add(*right))
        }
        (Some(value), _) => value,
        (None, value) => value.clone(),
    }
}

fn window_stats_from_snapshot(
    snapshot: &HashMap<StatKey, StatValue>,
) -> Option<WindowStatsContext> {
    let swap_volume_usd_24h = match snapshot.get(&StatKey::SWAP_VOLUME_USD_24H) {
        Some(StatValue::Decimal(value)) => Some(value.clone()),
        _ => None,
    };
    let swap_count_24h = match snapshot.get(&StatKey::SWAP_COUNT_24H) {
        Some(StatValue::Count(value)) => u64::try_from(*value).ok(),
        _ => None,
    };

    if swap_volume_usd_24h.is_none() && swap_count_24h.is_none() {
        None
    } else {
        Some(WindowStatsContext {
            swap_volume_usd_24h,
            swap_count_24h,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{DexFacts, DexTrace, OracleRequirement};
    use crate::host::{MockApprovals, MockOracle, MockStatWindows};

    fn actor() -> Address {
        Address::new("0x1111111111111111111111111111111111111111").unwrap()
    }

    fn target() -> Address {
        Address::new("0x2222222222222222222222222222222222222222").unwrap()
    }

    fn usdc() -> Token {
        Token {
            chain_id: 1,
            address: Address::new("0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(),
            symbol: "USDC".into(),
            decimals: 6,
            is_native: false,
        }
    }

    fn usdt() -> Token {
        Token {
            chain_id: 1,
            address: Address::new("0xdAC17F958D2ee523a2206206994597C13D831ec7").unwrap(),
            symbol: "USDT".into(),
            decimals: 6,
            is_native: false,
        }
    }

    fn weth() -> Token {
        Token {
            chain_id: 1,
            address: Address::new("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2").unwrap(),
            symbol: "WETH".into(),
            decimals: 18,
            is_native: false,
        }
    }

    fn eth() -> Token {
        Token {
            chain_id: 1,
            address: Address::new("0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE").unwrap(),
            symbol: "ETH".into(),
            decimals: 18,
            is_native: true,
        }
    }

    fn input_requirement(token: Token, raw_amount: &str) -> OracleRequirement {
        OracleRequirement {
            kind: OracleRequirementKind::Input,
            token,
            raw_amount: raw_amount.into(),
        }
    }

    fn dex_action(oracle_requirements: Vec<OracleRequirement>) -> DexAction {
        DexAction {
            actor: actor(),
            target: target(),
            value_wei: "0".into(),
            facts: DexFacts::default(),
            oracle_requirements,
            trace: DexTrace::default(),
        }
    }

    #[test]
    fn malformed_raw_amount_omits_usd_aggregate_without_panic() {
        let token = usdc();
        let oracle = MockOracle::new().with_simple_price(&token, "1.00", 5);
        let host = HostCapabilities::new(&oracle);
        let mut action = dex_action(vec![input_requirement(token, "not-a-u256")]);

        enrich_dex_action_base(&mut action, &host);

        assert!(action.facts.total_input_usd.is_none());
    }

    #[test]
    fn total_usd_sums_successful_requirements_and_skips_unpriced_or_malformed_requirements() {
        let priced = usdc();
        let malformed = usdt();
        let unpriced = weth();
        let oracle = MockOracle::new()
            .with_simple_price(&priced, "1.00", 5)
            .with_simple_price(&malformed, "1.00", 5);
        let host = HostCapabilities::new(&oracle);
        let mut action = dex_action(vec![
            input_requirement(priced, "200000000"),
            input_requirement(unpriced, "1000000000000000000"),
            input_requirement(malformed, "not-a-u256"),
        ]);

        enrich_dex_action_base(&mut action, &host);

        assert_eq!(
            action
                .facts
                .total_input_usd
                .as_ref()
                .map(|valuation| valuation.value.as_str()),
            Some("200.0000")
        );
    }

    #[test]
    fn missing_non_native_allowance_marks_inputs_not_covered_when_provider_exists() {
        let token = usdc();
        let oracle = MockOracle::new();
        let approvals = MockApprovals::new();
        let host = HostCapabilities::new(&oracle).with_approvals(&approvals);
        let mut action = dex_action(vec![input_requirement(token, "100")]);

        enrich_dex_action_base(&mut action, &host);

        assert_eq!(action.facts.allowances_cover_inputs, Some(false));
    }

    #[test]
    fn missing_approvals_provider_keeps_allowance_coverage_unknown() {
        let token = usdc();
        let oracle = MockOracle::new();
        let host = HostCapabilities::new(&oracle);
        let mut action = dex_action(vec![input_requirement(token, "100")]);

        enrich_dex_action_base(&mut action, &host);

        assert_eq!(action.facts.allowances_cover_inputs, None);
    }

    #[test]
    fn native_only_inputs_are_covered_without_approvals() {
        let oracle = MockOracle::new();
        let host = HostCapabilities::new(&oracle);
        let mut action = dex_action(vec![input_requirement(eth(), "100")]);

        enrich_dex_action_base(&mut action, &host);

        assert_eq!(action.facts.allowances_cover_inputs, Some(true));
    }

    #[test]
    fn enrich_dex_action_projects_current_action_into_window_stats() {
        let token = usdc();
        let oracle = MockOracle::new().with_simple_price(&token, "1.00", 5);
        let stats = MockStatWindows::new();
        let host = HostCapabilities::new(&oracle).with_stats(&stats);
        let mut action = dex_action(vec![input_requirement(token, "2000000")]);

        enrich_dex_action(&mut action, &host);

        let window_stats = action.facts.window_stats.as_ref().unwrap();
        assert_eq!(window_stats.swap_volume_usd_24h.as_deref(), Some("2.0000"));
        assert_eq!(window_stats.swap_count_24h, Some(1));
    }
}
