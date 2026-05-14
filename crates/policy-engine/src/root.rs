//! Top-level request envelope. Mirrors schema/schema/root.json.

use crate::action::{ActionEnvelope, Address, DecimalString};
use serde::{Deserialize, Serialize};

/// Top-level request transport kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    /// On-chain transaction request.
    Transaction,
    /// Off-chain signature request.
    Signature,
    /// ERC-4337 user operation request.
    UserOperation,
}

/// Protocol metadata associated with a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolRef {
    /// Protocol name.
    pub name: String,
    /// Protocol version, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Protocol component, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
}

/// Root request envelope containing transaction context and normalized actions.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootRequest {
    /// Schema version used to encode this request.
    pub schema_version: String,
    /// Request transport kind.
    pub request_kind: RequestKind,
    /// EVM chain id.
    pub chain_id: u64,
    /// Request sender.
    pub from: Address,
    /// Request target.
    pub to: Address,
    /// Native value attached to the request.
    pub value: DecimalString,
    /// Calldata or signature selector.
    pub selector: String,
    /// Protocol metadata, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<ProtocolRef>,
    /// Normalized action envelopes.
    pub actions: Vec<ActionEnvelope>,
    /// Block timestamp used for request evaluation, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_timestamp: Option<u64>,
}

impl RootRequest {
    /// Current root request schema version.
    pub const SCHEMA_VERSION: &'static str = "1.0.1";
}

#[cfg(test)]
mod tests {
    use super::{RequestKind, RootRequest};
    use crate::action::{Address, Category, DecimalString};
    use serde_json::{json, Value};

    #[test]
    fn test_root_request_minimal_serde() {
        let request = root_request();

        let json = serde_json::to_string(&request).unwrap();
        let roundtrip = serde_json::from_str::<RootRequest>(&json).unwrap();

        assert_eq!(roundtrip, request);
    }

    #[test]
    fn test_root_request_omits_optionals() {
        let value = serde_json::to_value(root_request()).unwrap();

        assert!(value.get("protocol").is_none());
        assert!(value.get("blockTimestamp").is_none());
    }

    #[test]
    fn test_root_request_with_one_swap_action() {
        let request = serde_json::from_value::<RootRequest>(json!({
            "schemaVersion": RootRequest::SCHEMA_VERSION,
            "requestKind": "transaction",
            "chainId": 1,
            "from": address(0x01),
            "to": address(0x02),
            "value": "0",
            "selector": "0x38ed1739",
            "actions": [
                {
                    "category": "dex",
                    "action": "swap",
                    "fields": swap_fields()
                }
            ]
        }))
        .unwrap();

        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["actions"][0]["category"], json!("dex"));
        assert_eq!(value["actions"][0]["action"], json!("swap"));
        assert_eq!(request.actions[0].category, Category::Dex);
        assert_eq!(request.actions[0].action.kind(), "swap");

        let roundtrip = serde_json::from_value::<RootRequest>(value).unwrap();
        assert_eq!(roundtrip, request);
    }

    fn root_request() -> RootRequest {
        RootRequest {
            schema_version: RootRequest::SCHEMA_VERSION.to_owned(),
            request_kind: RequestKind::Transaction,
            chain_id: 1,
            from: address_value(0x01),
            to: address_value(0x02),
            value: decimal_string("0"),
            selector: "0x38ed1739".to_owned(),
            protocol: None,
            actions: Vec::new(),
            block_timestamp: None,
        }
    }

    fn address_value(value: u8) -> Address {
        serde_json::from_value(json!(address(value))).unwrap()
    }

    fn decimal_string(value: &str) -> DecimalString {
        serde_json::from_value(json!(value)).unwrap()
    }

    fn address(value: u8) -> String {
        format!("0x{value:040x}")
    }

    fn erc20(symbol: &str) -> Value {
        json!({
            "kind": "erc20",
            "address": address(0x10),
            "symbol": symbol,
            "decimals": 18
        })
    }

    fn amount(kind: &str, value: &str) -> Value {
        json!({ "kind": kind, "value": value })
    }

    fn swap_fields() -> Value {
        json!({
            "swapMode": "exact_in",
            "tokenIn": erc20("WETH"),
            "tokenOut": erc20("USDC"),
            "amountIn": amount("exact", "1000"),
            "amountOut": amount("min", "900"),
            "recipient": address(0x30)
        })
    }
}
