use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::SyncError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveInputSpec {
    pub source: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_s: Option<u64>,
}

pub type LiveInputsSpec = HashMap<String, LiveInputSpec>;

/// ```ignore
/// {
///   "live_inputs": {
///     "route": { "source": { "kind": "onchain_view", ... }, "ttl_s": 12 },
///     ...
///   }
/// }
/// ```
pub fn parse_live_inputs(manifest_subtree: &Value) -> Result<LiveInputsSpec, SyncError> {
    let live = manifest_subtree
        .get("live_inputs")
        .ok_or_else(|| SyncError::FetchFailed {
            source_id: "manifest_v2".into(),
            reason: "missing 'live_inputs' field".into(),
        })?;

    let obj = live.as_object().ok_or_else(|| SyncError::FetchFailed {
        source_id: "manifest_v2".into(),
        reason: "'live_inputs' is not an object".into(),
    })?;

    let mut out = HashMap::with_capacity(obj.len());
    for (slot_name, slot_json) in obj {
        let spec: LiveInputSpec =
            serde_json::from_value(slot_json.clone()).map_err(|e| SyncError::FetchFailed {
                source_id: "manifest_v2".into(),
                reason: format!("live_inputs[{slot_name}] parse: {e}"),
            })?;
        out.insert(slot_name.clone(), spec);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_uniswap_v3_swap_style() {
        let manifest = json!({
            "live_inputs": {
                "route": {
                    "source": {
                        "kind": "onchain_view",
                        "chain": "$chain",
                        "contract": "$resolved.pool",
                        "function": "slot0()",
                        "decoder_id": "uniswap_v3_slot0"
                    },
                    "ttl_s": 12
                },
                "expected_amount_out": {
                    "source": {
                        "kind": "venue_api",
                        "endpoint": "https://api.uniswap.org/v2/quote",
                        "parser_id": "uniswapx_quote"
                    },
                    "ttl_s": 6
                },
                "gas_estimate": {
                    "source": {
                        "kind": "oracle_feed",
                        "provider": "pyth",
                        "feed_id": "gas/ethereum"
                    },
                    "ttl_s": 6
                }
            }
        });

        let parsed = parse_live_inputs(&manifest).unwrap();
        assert_eq!(parsed.len(), 3);
        assert!(parsed.contains_key("route"));
        assert!(parsed.contains_key("expected_amount_out"));
        assert!(parsed.contains_key("gas_estimate"));
        assert_eq!(parsed["route"].ttl_s, Some(12));
        assert_eq!(parsed["expected_amount_out"].ttl_s, Some(6));
    }

    #[test]
    fn missing_live_inputs_errors() {
        let manifest = json!({ "match": {}, "abi_fragment": {} });
        let err = parse_live_inputs(&manifest).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("missing 'live_inputs'"));
    }

    #[test]
    fn live_inputs_not_object_errors() {
        let manifest = json!({ "live_inputs": [] });
        let err = parse_live_inputs(&manifest).unwrap_err();
        assert!(format!("{err}").contains("not an object"));
    }
}
