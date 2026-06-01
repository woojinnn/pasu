//! Placeholder 치환 — `$chain`, `$inputs.X`, `$resolved.X`, `$derived.X`, `$tx.from`
//! 을 context 에서 실제 값으로 풀어준다.
//!
//! 변환된 source JSON 은 그대로 `serde_json::from_value::<DataSource>()` 가능.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::error::SyncError;

/// 한 manifest 실행 시점에 제공되는 context.
#[derive(Clone, Debug, Default)]
pub struct ResolveContext {
    /// `$chain` 의 값.
    pub chain: Option<String>,
    /// `$inputs.<field>` — calldata 디코드 결과.
    pub inputs: HashMap<String, Value>,
    /// `$resolved.<field>` — host 가 추가로 해결한 값 (pool 주소 등).
    pub resolved: HashMap<String, Value>,
    /// `$derived.<field>` — manifest 의 derived 규칙으로 계산된 값.
    pub derived: HashMap<String, Value>,
    /// `$tx.<field>` — transaction 자체의 메타 (from/to/value 등).
    pub tx: HashMap<String, Value>,
}

impl ResolveContext {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_chain(mut self, chain: impl Into<String>) -> Self {
        self.chain = Some(chain.into());
        self
    }

    pub fn insert_input(mut self, key: impl Into<String>, value: Value) -> Self {
        self.inputs.insert(key.into(), value);
        self
    }

    pub fn insert_resolved(mut self, key: impl Into<String>, value: Value) -> Self {
        self.resolved.insert(key.into(), value);
        self
    }
}

/// `value` 안의 모든 placeholder 를 context 로 치환 (recursive).
///
/// 지원 placeholder:
/// * `"$chain"`        → ctx.chain
/// * `"$inputs.X"`     → ctx.inputs[X]
/// * `"$resolved.X"`   → ctx.resolved[X]
/// * `"$derived.X"`    → ctx.derived[X]
/// * `"$tx.X"`         → ctx.tx[X]
///
/// 치환 못 하면 `SyncError::FetchFailed` 반환.
pub fn resolve_placeholders(value: &Value, ctx: &ResolveContext) -> Result<Value, SyncError> {
    match value {
        Value::String(s) => resolve_string(s, ctx),

        Value::Object(obj) => {
            let mut new_obj = Map::with_capacity(obj.len());
            for (k, v) in obj {
                new_obj.insert(k.clone(), resolve_placeholders(v, ctx)?);
            }
            Ok(Value::Object(new_obj))
        }

        Value::Array(arr) => {
            let mut new_arr = Vec::with_capacity(arr.len());
            for v in arr {
                new_arr.push(resolve_placeholders(v, ctx)?);
            }
            Ok(Value::Array(new_arr))
        }

        // 숫자/bool/null 은 placeholder 아니라 그대로
        other => Ok(other.clone()),
    }
}

fn resolve_string(s: &str, ctx: &ResolveContext) -> Result<Value, SyncError> {
    // placeholder 형태가 아니면 그대로
    if !s.starts_with('$') {
        return Ok(Value::String(s.to_string()));
    }

    // 단일 placeholder 만 처리 — "$chain", "$inputs.amountIn" 등.
    // 복합 문자열 ("prefix_$X_suffix") 은 지원 X (V2 spec 도 단일 사용).
    if s == "$chain" {
        return ctx
            .chain
            .as_ref()
            .map(|c| Value::String(c.clone()))
            .ok_or_else(|| SyncError::FetchFailed {
                source_id: "manifest_v2".into(),
                reason: "$chain referenced but not set in ResolveContext".into(),
            });
    }

    // "$inputs.X", "$resolved.X" 등 — dot 으로 scope 분리
    if let Some(rest) = s.strip_prefix('$') {
        if let Some((scope, field)) = rest.split_once('.') {
            let map = match scope {
                "inputs" => &ctx.inputs,
                "resolved" => &ctx.resolved,
                "derived" => &ctx.derived,
                "tx" => &ctx.tx,
                _ => {
                    return Err(SyncError::FetchFailed {
                        source_id: "manifest_v2".into(),
                        reason: format!("unknown scope: ${scope}"),
                    });
                }
            };
            return map
                .get(field)
                .cloned()
                .ok_or_else(|| SyncError::FetchFailed {
                    source_id: "manifest_v2".into(),
                    reason: format!("${scope}.{field} not in ResolveContext"),
                });
        }
    }

    Err(SyncError::FetchFailed {
        source_id: "manifest_v2".into(),
        reason: format!("unrecognized placeholder: {s}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolves_chain() {
        let ctx = ResolveContext::new().with_chain("eip155:1");
        let v = Value::String("$chain".into());
        let r = resolve_placeholders(&v, &ctx).unwrap();
        assert_eq!(r, Value::String("eip155:1".into()));
    }

    #[test]
    fn resolves_inputs() {
        let ctx = ResolveContext::new()
            .insert_input("amountIn", json!("1000"))
            .insert_input("recipient", json!("0xUser..."));
        let v = json!({
            "amount": "$inputs.amountIn",
            "to":     "$inputs.recipient"
        });
        let r = resolve_placeholders(&v, &ctx).unwrap();
        assert_eq!(r["amount"], Value::String("1000".into()));
        assert_eq!(r["to"], Value::String("0xUser...".into()));
    }

    #[test]
    fn resolves_nested_source() {
        // 실제 V2 manifest source 구조와 같은 형태
        let ctx = ResolveContext::new()
            .with_chain("eip155:1")
            .insert_resolved("pool", json!("0xUniV3Pool..."));
        let v = json!({
            "kind": "onchain_view",
            "chain": "$chain",
            "contract": "$resolved.pool",
            "function": "slot0()",
            "decoder_id": "uniswap_v3_slot0"
        });
        let r = resolve_placeholders(&v, &ctx).unwrap();
        assert_eq!(r["chain"], Value::String("eip155:1".into()));
        assert_eq!(r["contract"], Value::String("0xUniV3Pool...".into()));
        assert_eq!(r["function"], Value::String("slot0()".into()));
    }

    #[test]
    fn unknown_scope_errors() {
        let ctx = ResolveContext::new();
        let v = Value::String("$nonexistent.field".into());
        let err = resolve_placeholders(&v, &ctx).unwrap_err();
        assert!(format!("{err}").contains("unknown scope"));
    }

    #[test]
    fn missing_value_errors() {
        let ctx = ResolveContext::new(); // chain 없음
        let v = Value::String("$chain".into());
        let err = resolve_placeholders(&v, &ctx).unwrap_err();
        assert!(format!("{err}").contains("$chain referenced"));
    }

    #[test]
    fn non_placeholder_passthrough() {
        let ctx = ResolveContext::new();
        let v = json!({ "kind": "oracle_feed", "feed_id": "USDC/USD" });
        let r = resolve_placeholders(&v, &ctx).unwrap();
        assert_eq!(r, v);
    }
}
