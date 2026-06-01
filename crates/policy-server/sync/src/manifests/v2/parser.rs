//! V2 manifest 의 `live_inputs` JSON → 우리 `LiveField` source 모양.
//!
//! 파싱은 두 단계:
//! 1. `live_inputs` 안 각 슬롯의 source 를 JSON 그대로 들고 옴 ([`LiveInputSpec`])
//! 2. placeholder ($chain, $inputs.X, ...) 를 [`super::resolver`] 에서 치환 후
//!    [`simulation_state::DataSource`] 로 변환

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::SyncError;

/// `live_inputs` 안의 한 슬롯 spec — JSON 그대로 (placeholder 미해결).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LiveInputSpec {
    /// source 의 JSON object (kind / chain / contract / ...)
    pub source: Value,
    /// 권장 갱신 주기 (초).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_s: Option<u64>,
}

/// 한 manifest 의 전체 `live_inputs` 섹션 = `{slot_name: spec, ...}`.
pub type LiveInputsSpec = HashMap<String, LiveInputSpec>;

/// manifest 의 JSON 에서 `live_inputs` 섹션을 추출 + 파싱.
///
/// 입력 예 (manifest body 안의 일부):
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
        // registryV2 manifest 의 V3 swap 변형
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
