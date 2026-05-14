//! Swap action.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, AmountConstraint, AssetRef, Validity};

use super::{SwapEnrichment, SwapMode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Single-hop token swap action.
pub struct SwapAction {
    /// Swap amount mode.
    #[serde(rename = "swapMode")]
    pub swap_mode: SwapMode,
    /// Asset sent by the user.
    pub token_in: AssetRef,
    /// Asset received by the user.
    pub token_out: AssetRef,
    /// Input amount constraint.
    pub amount_in: AmountConstraint,
    /// Output amount constraint.
    pub amount_out: AmountConstraint,
    /// Recipient of the output asset.
    pub recipient: Address,
    /// Validity window, when present in calldata or wrapper context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
    /// Pool fee in basis points, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_bps: Option<u32>,
    /// Host-derived enrichment facts.
    #[serde(default, skip_serializing_if = "SwapEnrichment::is_empty")]
    pub enrichment: SwapEnrichment,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::common::AmountKind;
    use crate::action::dex::test_support::{
        address, amount, assert_roundtrip, token_pair, usd, validity,
    };
    use serde_json::{json, Value};

    #[test]
    fn test_swap_action_serde_roundtrip_minimal() {
        let action = SwapAction {
            swap_mode: SwapMode::ExactIn,
            token_in: token_pair()[0].clone(),
            token_out: token_pair()[1].clone(),
            amount_in: amount(AmountKind::Exact, "1000000000000000000"),
            amount_out: amount(AmountKind::Min, "1900000"),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: None,
            fee_bps: None,
            enrichment: SwapEnrichment::default(),
        };

        assert_roundtrip(&action);
        let value = serde_json::to_value(&action).unwrap();
        assert_eq!(value.get("swapMode"), Some(&json!("exact_in")));
        assert!(value.get("mode").is_none());
    }

    #[test]
    fn test_swap_action_serde_roundtrip_full() {
        let action = SwapAction {
            swap_mode: SwapMode::ExactOut,
            token_in: token_pair()[0].clone(),
            token_out: token_pair()[1].clone(),
            amount_in: amount(AmountKind::Max, "1100000000000000000"),
            amount_out: amount(AmountKind::Exact, "2000000"),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: Some(validity()),
            fee_bps: Some(5),
            enrichment: SwapEnrichment {
                value_in_usd: Some(usd("2000.00")),
                min_value_out_usd: Some(usd("1990.00")),
                expected_value_out_usd: Some(usd("2001.00")),
                input_fraction_of_portfolio_bps: Some(125),
            },
        };

        assert_roundtrip(&action);
        let value = serde_json::to_value(&action).unwrap();
        assert_eq!(value.get("swapMode"), Some(&json!("exact_out")));
        assert!(value.get("mode").is_none());
    }

    #[test]
    fn test_swap_enrichment_omitted_when_empty() {
        let action = SwapAction {
            swap_mode: SwapMode::ExactIn,
            token_in: token_pair()[0].clone(),
            token_out: token_pair()[1].clone(),
            amount_in: amount(AmountKind::Exact, "1000000000000000000"),
            amount_out: amount(AmountKind::Min, "1900000"),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: None,
            fee_bps: None,
            enrichment: SwapEnrichment::default(),
        };

        let value = serde_json::to_value(action).unwrap();

        assert!(value.get("enrichment").is_none());
    }

    #[test]
    fn test_swap_action_matches_schema_fixture() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../../schema/schema/actions/dex/swap.json"
        ))
        .unwrap();
        let fixture = schema
            .get("examples")
            .and_then(Value::as_array)
            .and_then(|examples| examples.first())
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "swapMode": "exact_in",
                    "tokenIn": {
                        "kind": "erc20",
                        "address": "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                        "symbol": "WETH",
                        "decimals": 18
                    },
                    "tokenOut": {
                        "kind": "erc20",
                        "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                        "symbol": "USDC",
                        "decimals": 6
                    },
                    "amountIn": {
                        "kind": "exact",
                        "value": "1000000000000000000"
                    },
                    "amountOut": {
                        "kind": "min",
                        "value": "1900000"
                    },
                    "recipient": "0x2222222222222222222222222222222222222222"
                })
            });

        let action = serde_json::from_value::<SwapAction>(fixture).unwrap();

        assert_eq!(action.swap_mode, SwapMode::ExactIn);
        assert!(action.enrichment.is_empty());
    }
}
