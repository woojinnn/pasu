//! `Action::Dex` to `PolicyRequest` conversion.

use crate::context_keys::{
    ALLOWANCES_COVER_INPUTS, HAS_EXTERNAL_RECIPIENT, HAS_ZERO_MIN_OUTPUT, INPUT_TOKENS,
    MAX_FEE_BPS, OUTPUT_TOKENS, PROTOCOL_IDS, SWAP_COUNT_24H, SWAP_VOLUME_USD_24H, TARGET,
    TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS, TOTAL_INPUT_USD, TOTAL_MIN_OUTPUT_USD, VALUE_WEI,
    WINDOW_STATS,
};
use crate::core::{DexAction, DexFacts, WindowStatsContext};
use crate::policy::PolicyRequest;
use serde_json::{json, Map, Value};

use super::amount::{decimal_json, token_json, usd_valuation_json};

pub(super) fn request(action: &DexAction) -> PolicyRequest {
    let principal = format!(r#"Wallet::"{}""#, action.actor.as_str());
    let resource = r#"Protocol::"dex""#.to_string();
    let entities = json!([
        { "uid": { "type": "Wallet", "id": action.actor.as_str() }, "attrs": {}, "parents": [] },
        { "uid": { "type": "Protocol", "id": "dex" }, "attrs": {}, "parents": [] },
    ]);

    PolicyRequest::new(
        principal,
        r#"Action::"dex""#,
        resource,
        entities,
        context(action),
    )
}

fn context(action: &DexAction) -> Value {
    let facts = &action.facts;
    let mut context = Map::new();
    context.insert(TARGET.into(), Value::from(action.target.as_str()));
    context.insert(VALUE_WEI.into(), Value::from(action.value_wei.as_str()));
    context.insert(PROTOCOL_IDS.into(), json!(facts.protocol_ids));
    context.insert(INPUT_TOKENS.into(), tokens_json(facts, TokenSide::Input));
    context.insert(OUTPUT_TOKENS.into(), tokens_json(facts, TokenSide::Output));
    context.insert(
        HAS_ZERO_MIN_OUTPUT.into(),
        Value::from(facts.has_zero_min_output),
    );
    context.insert(
        HAS_EXTERNAL_RECIPIENT.into(),
        Value::from(facts.has_external_recipient),
    );

    if let Some(usd) = &facts.total_input_usd {
        context.insert(TOTAL_INPUT_USD.into(), usd_valuation_json(usd));
    }
    if let Some(usd) = &facts.total_min_output_usd {
        context.insert(TOTAL_MIN_OUTPUT_USD.into(), usd_valuation_json(usd));
    }
    if let Some(max_fee_bps) = facts.max_fee_bps {
        context.insert(MAX_FEE_BPS.into(), Value::from(max_fee_bps));
    }
    if let Some(fraction_bps) = facts.total_input_fraction_of_portfolio_bps {
        context.insert(
            TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS.into(),
            cedar_long_u64(fraction_bps),
        );
    }
    if let Some(allowances_cover_inputs) = facts.allowances_cover_inputs {
        context.insert(
            ALLOWANCES_COVER_INPUTS.into(),
            Value::from(allowances_cover_inputs),
        );
    }
    if let Some(window_stats) = &facts.window_stats {
        let value = window_stats_json(window_stats);
        if value.as_object().is_some_and(|fields| !fields.is_empty()) {
            context.insert(WINDOW_STATS.into(), value);
        }
    }

    Value::Object(context)
}

#[derive(Clone, Copy)]
enum TokenSide {
    Input,
    Output,
}

fn tokens_json(facts: &DexFacts, side: TokenSide) -> Value {
    let tokens = match side {
        TokenSide::Input => &facts.input_tokens,
        TokenSide::Output => &facts.output_tokens,
    };
    Value::Array(tokens.iter().map(token_json).collect())
}

fn window_stats_json(stats: &WindowStatsContext) -> Value {
    let mut out = Map::new();
    if let Some(value) = &stats.swap_volume_usd_24h {
        out.insert(SWAP_VOLUME_USD_24H.into(), decimal_json(value));
    }
    if let Some(value) = stats.swap_count_24h {
        out.insert(SWAP_COUNT_24H.into(), cedar_long_u64(value));
    }
    Value::Object(out)
}

fn cedar_long_u64(value: u64) -> Value {
    Value::from(i64::try_from(value).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Address, DexAction, DexFacts, DexTrace};

    fn action_with_facts(facts: DexFacts) -> DexAction {
        DexAction {
            actor: Address::new("0x1111111111111111111111111111111111111111").unwrap(),
            target: Address::new("0x2222222222222222222222222222222222222222").unwrap(),
            value_wei: "0".into(),
            facts,
            oracle_requirements: Vec::new(),
            trace: DexTrace::default(),
        }
    }

    #[test]
    fn u64_long_context_fields_are_clamped_to_cedar_long_range() {
        let action = action_with_facts(DexFacts {
            total_input_fraction_of_portfolio_bps: Some(u64::MAX),
            window_stats: Some(WindowStatsContext {
                swap_volume_usd_24h: None,
                swap_count_24h: Some(u64::MAX),
            }),
            ..DexFacts::default()
        });

        let context = context(&action);

        assert_eq!(
            context
                .get(TOTAL_INPUT_FRACTION_OF_PORTFOLIO_BPS)
                .and_then(Value::as_i64),
            Some(i64::MAX)
        );
        assert_eq!(
            context
                .get(WINDOW_STATS)
                .and_then(Value::as_object)
                .and_then(|stats| stats.get(SWAP_COUNT_24H))
                .and_then(Value::as_i64),
            Some(i64::MAX)
        );
    }
}
