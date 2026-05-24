//! `enum_tagged_dispatch` DSL strategy executor (Phase 12.0).
//!
//! Spec §4.3 / §5.1 BNF / §5.2 strategy flow.
//!
//! 본 module 의 책임 = (1) bundle 의 `tag_path` 의 bytes 추출, (2) `dispatcher_id`
//! ↔ `EnumTable` 의 lookup, (3) generic engine 의 dispatch 호출, (4) decoded
//! variant 의 args 를 single_emit fields 와 동일한 방식 의 ActionEnvelope 생성.
//!
//! Backend = `abi_resolver::subdecode::enum_tagged::{dispatch, EnumTable}`
//! (이미 generic engine 으로 구현 + tested in Phase 7).
//!
//! 본 strategy 는 Balancer V2 의 `userData` (Join/ExitKind) 와 Curve Router NG
//! 의 per-hop swap_type 의 양쪽 의 prerequisite. 본 Phase 12.0 의 결과 = 미활성
//! `EnumTaggedDispatch` arm 의 wiring + 4 unit test.

use abi_resolver::subdecode::enum_tagged::{dispatch as engine_dispatch, EnumTable};
use abi_resolver::subdecode::protocols::balancer_v2::{
    BALANCER_V2_EXIT_KIND_STABLE, BALANCER_V2_EXIT_KIND_WEIGHTED, BALANCER_V2_JOIN_KIND,
};
use abi_resolver::subdecode::protocols::curve::CURVE_ROUTER_NG_SWAP_TYPES;
use abi_resolver::{DecodedCall, DecodedValue};
use alloy_dyn_abi::DynSolValue;
use policy_engine::ActionEnvelope;
use serde_json::Value;

use crate::mapper::{MapContext, MapperError};

use super::eval::decoded_value_to_json;
use super::single_emit;
use super::types::{EmitRule, UnknownVariantPolicy};

/// Execute the `enum_tagged_dispatch` strategy.
///
/// Flow (per spec §5.2.3):
///   1. Extract the tagged `bytes` field from `decoded.args` by `tag_path`
///      (PoC: `$.args.<name>` only — no chained `[idx]`).
///   2. Look up the static [`EnumTable`] referenced by `dispatcher_id`.
///   3. Call the generic [`engine_dispatch`] to decode `(kind, …payload)`.
///   4. Look up the per-variant emit rule via `kind.to_string()`.
///   5. Build an `args_json` view of the *decoded enum* args and run the same
///      field-tree → envelope machinery as `single_emit`.
///
/// Unknown variants (no entry in the table, no entry in `per_variant_emit`,
/// or a `< 32 byte` input) honour the bundle's `unknown_variant_policy`.
pub fn execute(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
    emit_rule: &EmitRule,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    let EmitRule::EnumTaggedDispatch {
        dispatcher_id,
        tag_path,
        tag_decoder: _,
        per_variant_emit,
        unknown_variant_policy,
    } = emit_rule
    else {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "enum_tagged::execute called with non-EnumTaggedDispatch rule"
        )));
    };

    let user_data = extract_tag_bytes(decoded, tag_path)?;
    let table = lookup_dispatcher_table(dispatcher_id)?;

    let Some(decoded_enum) = engine_dispatch(&user_data, table) else {
        return handle_unknown_variant(unknown_variant_policy, dispatcher_id, &user_data);
    };

    let variant_key = decoded_enum.kind.to_string();
    let Some(per_variant) = per_variant_emit.get(&variant_key) else {
        return handle_unknown_variant(unknown_variant_policy, dispatcher_id, &user_data);
    };

    let args_json = enum_args_to_json(&decoded_enum.args)?;
    let envelope = single_emit::execute_with_args(
        ctx,
        &args_json,
        &per_variant.category,
        &per_variant.action,
        &per_variant.fields,
    )?;
    Ok(vec![envelope])
}

/// Map a `dispatcher_id` to its static [`EnumTable`].
///
/// Phase 12.0 wired three Balancer V2 tables; Phase 13 (P2-5) wires the Curve
/// Router NG per-hop `swap_type` table — `CURVE_ROUTER_NG_SWAP_TYPES` is
/// defined + unit-tested in `subdecode/protocols/curve.rs`.
fn lookup_dispatcher_table(dispatcher_id: &str) -> Result<&'static EnumTable, MapperError> {
    match dispatcher_id {
        "balancer_v2_join" => Ok(&BALANCER_V2_JOIN_KIND),
        "balancer_v2_exit_weighted" => Ok(&BALANCER_V2_EXIT_KIND_WEIGHTED),
        "balancer_v2_exit_stable" => Ok(&BALANCER_V2_EXIT_KIND_STABLE),
        "curve_router_ng_swap_types" => Ok(&CURVE_ROUTER_NG_SWAP_TYPES),
        other => Err(MapperError::Unsupported(format!(
            "enum_tagged dispatcher_id {other:?} not wired"
        ))),
    }
}

/// Extract a `bytes` argument from `decoded.args` selected by `tag_path`.
///
/// PoC simplification: `tag_path` must be exactly `$.args.<name>` — no chained
/// `[idx]` suffix and no `$.tx.*` / `$.context.*` roots (which never carry
/// `bytes` in the current pipeline). Other shapes return `Internal` so the
/// orchestrator can surface a bundle-author error rather than a runtime fault.
fn extract_tag_bytes(decoded: &DecodedCall, tag_path: &str) -> Result<Vec<u8>, MapperError> {
    let body = tag_path.strip_prefix("$.args.").ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "tag_path must be $.args.<name>, got {tag_path:?}"
        ))
    })?;
    if body.contains('.') || body.contains('[') {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "tag_path {tag_path:?}: nested or indexed access not supported (PoC)"
        )));
    }

    let arg = decoded
        .args
        .iter()
        .find(|a| a.name == body)
        .ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "tag_path arg {body:?} not found in decoded call"
            ))
        })?;

    match &arg.value {
        DecodedValue::Bytes(b) => Ok(b.clone()),
        other => Err(MapperError::Internal(anyhow::anyhow!(
            "tag_path arg {body:?} must be bytes, got {other:?}"
        ))),
    }
}

/// Build the `args_json` object that [`single_emit::execute_with_args`]
/// expects, from the enum-decoded args returned by [`engine_dispatch`].
///
/// `engine_dispatch` returns `Vec<abi_resolver::decode::DecodedArg>` carrying
/// `DynSolValue`, while the mapper pipeline (`single_emit` / `eval`) operates
/// on `abi_resolver::decoder::DecodedValue` (via `decoded_value_to_json`). We
/// convert each value through `abi_resolver::bridge::convert_value` and then
/// reuse the same JSON encoder so the resulting object is byte-identical to
/// what `args_to_json` would produce for an equivalent outer call.
fn enum_args_to_json(args: &[abi_resolver::decode::DecodedArg]) -> Result<Value, MapperError> {
    let mut obj = serde_json::Map::with_capacity(args.len());
    for arg in args {
        let decoded_value = convert_dyn_sol_value(&arg.value)?;
        obj.insert(arg.name.clone(), decoded_value_to_json(&decoded_value));
    }
    Ok(Value::Object(obj))
}

/// Convert a borrowed [`DynSolValue`] into the mapper-pipeline
/// [`DecodedValue`]. Mirrors `abi_resolver::bridge::convert_value` but borrows
/// instead of consuming (the engine_dispatch result is borrowed by reference
/// here so we can avoid an extra clone of large `Bytes` payloads in the common
/// path).
fn convert_dyn_sol_value(value: &DynSolValue) -> Result<DecodedValue, MapperError> {
    use std::str::FromStr as _;
    Ok(match value {
        DynSolValue::Address(addr) => {
            let hex = format!("0x{}", hex::encode(addr.0));
            DecodedValue::Address(policy_engine::action::Address::from_str(&hex).map_err(|e| {
                MapperError::Internal(anyhow::anyhow!(
                    "enum_tagged_dispatch: invalid address {hex}: {e}"
                ))
            })?)
        }
        DynSolValue::Uint(v, _) => DecodedValue::Uint(*v),
        DynSolValue::Int(v, _) => DecodedValue::Int(*v),
        DynSolValue::Bool(b) => DecodedValue::Bool(*b),
        DynSolValue::Bytes(b) => DecodedValue::Bytes(b.clone()),
        DynSolValue::FixedBytes(word, len) => DecodedValue::Bytes(word.as_slice()[..*len].to_vec()),
        DynSolValue::String(s) => DecodedValue::String(s.clone()),
        DynSolValue::Array(items) | DynSolValue::FixedArray(items) => DecodedValue::Array(
            items
                .iter()
                .map(convert_dyn_sol_value)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        DynSolValue::Tuple(items) => DecodedValue::Tuple(
            items
                .iter()
                .map(convert_dyn_sol_value)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        DynSolValue::Function(_) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "enum_tagged_dispatch: DynSolValue::Function unsupported"
            )))
        }
    })
}

/// Apply the bundle's [`UnknownVariantPolicy`] when the discriminator does not
/// match an [`EnumTable`] entry or `per_variant_emit` does not declare a rule
/// for the decoded kind. `Deny` → fail-closed; `Warn` → no envelopes.
fn handle_unknown_variant(
    policy: &UnknownVariantPolicy,
    dispatcher_id: &str,
    user_data: &[u8],
) -> Result<Vec<ActionEnvelope>, MapperError> {
    match policy {
        UnknownVariantPolicy::Deny => Err(MapperError::Unsupported(format!(
            "enum_tagged_dispatch/{dispatcher_id}: unknown variant in {} bytes",
            user_data.len()
        ))),
        UnknownVariantPolicy::Warn => Ok(vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use abi_resolver::{DecodedArg, DecoderId};
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function;
    use alloy_primitives::U256;
    use policy_engine::action::Action;
    use std::collections::BTreeMap;
    use std::str::FromStr;

    use crate::declarative::types::{PerVariantEmit, ValueExpr};
    use crate::token_registry::EmptyTokenRegistry;

    /// Encode `(kind, …)` payload exactly as Balancer V2 `userData` is packed.
    fn encode_payload(sig: &str, values: Vec<DynSolValue>) -> Vec<u8> {
        let func = Function::parse(&format!("step{sig}")).unwrap();
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    /// Build a `DecodedCall` whose single `userData` arg carries the given
    /// pre-encoded enum payload — mirroring how `Vault.joinPool` reaches the
    /// mapper after standard-ABI decoding.
    fn decoded_with_user_data(user_data: Vec<u8>) -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new("test/balancer-v2/joinPool"),
            function_signature:
                "joinPool(bytes32,address,address,(address[],uint256[],bytes,bool))".into(),
            args: vec![DecodedArg {
                name: "userData".into(),
                abi_type: "bytes".into(),
                value: DecodedValue::Bytes(user_data),
            }],
            nested: vec![],
        }
    }

    fn dummy_addr(label: u8) -> policy_engine::action::Address {
        let zeros = "0".repeat(38);
        policy_engine::action::Address::from_str(&format!("0x{zeros}{label:02x}")).unwrap()
    }

    fn build_ctx<'a>(
        registry: &'a EmptyTokenRegistry,
        from: &'a policy_engine::action::Address,
        to: &'a policy_engine::action::Address,
        value: &'a policy_engine::action::DecimalString,
    ) -> MapContext<'a> {
        MapContext {
            chain_id: 1,
            from,
            to,
            value_wei: value,
            block_timestamp: Some(1_700_000_000),
            token_registry: registry,
            parent_calldata: None,
            depth: 0,
            resolver: None,
        }
    }

    /// Build a `PerVariantEmit` whose only emitted field is a literal —
    /// sufficient to prove the dispatch path picks the right variant. The
    /// envelope built is shape-compatible with `add_liquidity` only when the
    /// caller pre-populates every other required field via `extra_fields`.
    fn add_liquidity_emit_for_init() -> PerVariantEmit {
        // Balancer V2 INIT payload = (kind=0 uint256, amountsIn=uint256[]).
        // We synthesize a minimal add_liquidity field set: pool literal, two
        // input asset literals, an outputLp literal, and a recipient literal.
        // The literal-only shape avoids depending on JsonPath walking of the
        // enum args (which would require additional decoded array helpers we
        // don't yet need in Phase 12.0).
        let mut fields: BTreeMap<String, ValueExpr> = BTreeMap::new();
        // pool
        fields.insert(
            "pool.address".into(),
            ValueExpr::Literal {
                literal: serde_json::json!("0x1111111111111111111111111111111111111111"),
            },
        );
        // inputTokens (one element)
        fields.insert(
            "inputTokens".into(),
            ValueExpr::Literal {
                literal: serde_json::json!([
                    {
                        "asset": {
                            "kind": "erc20",
                            "address": "0x2222222222222222222222222222222222222222"
                        },
                        "amount": {
                            "kind": "max",
                            "value": "100"
                        }
                    }
                ]),
            },
        );
        // outputLp
        fields.insert(
            "outputLp.asset.kind".into(),
            ValueExpr::Literal {
                literal: serde_json::json!("erc20"),
            },
        );
        fields.insert(
            "outputLp.asset.address".into(),
            ValueExpr::Literal {
                literal: serde_json::json!("0x3333333333333333333333333333333333333333"),
            },
        );
        fields.insert(
            "outputLp.amount.kind".into(),
            ValueExpr::Literal {
                literal: serde_json::json!("min"),
            },
        );
        fields.insert(
            "outputLp.amount.value".into(),
            ValueExpr::Literal {
                literal: serde_json::json!("0"),
            },
        );
        // recipient
        fields.insert(
            "recipient".into(),
            ValueExpr::FromArg {
                from: "$.tx.from".into(),
                via: None,
                kind: None,
            },
        );

        PerVariantEmit {
            name: "INIT".into(),
            category: "dex".into(),
            action: "add_liquidity".into(),
            fields,
        }
    }

    fn bundle_with_join_kind(
        per_variant_emit: BTreeMap<String, PerVariantEmit>,
        unknown_variant_policy: UnknownVariantPolicy,
    ) -> EmitRule {
        EmitRule::EnumTaggedDispatch {
            dispatcher_id: "balancer_v2_join".into(),
            tag_path: "$.args.userData".into(),
            tag_decoder: "uint256_at_offset_0".into(),
            per_variant_emit,
            unknown_variant_policy,
        }
    }

    #[test]
    fn enum_tagged_dispatch_with_balancer_v2_join_kind_init() {
        // Pack a Balancer V2 INIT JoinKind payload: (kind=0, amountsIn=[100, 200]).
        let user_data = encode_payload(
            "(uint256,uint256[])",
            vec![
                DynSolValue::Uint(U256::ZERO, 256),
                DynSolValue::Array(vec![
                    DynSolValue::Uint(U256::from(100u64), 256),
                    DynSolValue::Uint(U256::from(200u64), 256),
                ]),
            ],
        );
        let decoded = decoded_with_user_data(user_data);

        let mut per_variant: BTreeMap<String, PerVariantEmit> = BTreeMap::new();
        per_variant.insert("0".into(), add_liquidity_emit_for_init());
        let rule = bundle_with_join_kind(per_variant, UnknownVariantPolicy::Deny);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = execute(&ctx, &decoded, &rule).expect("INIT decodes");
        assert_eq!(envelopes.len(), 1, "single envelope per variant emit");
        match &envelopes[0].action {
            Action::AddLiquidity(_) => {} // happy path
            other => panic!("expected AddLiquidity, got {other:?}"),
        }
    }

    #[test]
    fn enum_tagged_dispatch_with_unknown_variant_denies() {
        // kind=999 → no entry in BALANCER_V2_JOIN_KIND (entries 0..=3). The
        // dispatcher's `Deny` policy must surface as `Unsupported`.
        // engine_dispatch's leading-zero gate forces kind values to fit in u32
        // (rightmost 4 bytes of the 32-byte word), so 999 must be encoded as a
        // u256. The Balancer table only declares kinds 0..=3.
        let user_data = encode_payload(
            "(uint256)",
            vec![DynSolValue::Uint(U256::from(999u64), 256)],
        );
        let decoded = decoded_with_user_data(user_data);

        let mut per_variant: BTreeMap<String, PerVariantEmit> = BTreeMap::new();
        per_variant.insert("0".into(), add_liquidity_emit_for_init());
        let rule = bundle_with_join_kind(per_variant, UnknownVariantPolicy::Deny);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let err = execute(&ctx, &decoded, &rule).unwrap_err();
        match err {
            MapperError::Unsupported(msg) => {
                assert!(
                    msg.contains("balancer_v2_join"),
                    "error message should name the dispatcher (got {msg:?})"
                );
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn enum_tagged_dispatch_with_unknown_variant_warns() {
        // Same kind=999 payload, but the bundle's policy = warn → return
        // `Ok(vec![])`. Policy authors who set `warn` accept that some kinds
        // silently produce no envelope.
        let user_data = encode_payload(
            "(uint256)",
            vec![DynSolValue::Uint(U256::from(999u64), 256)],
        );
        let decoded = decoded_with_user_data(user_data);

        let mut per_variant: BTreeMap<String, PerVariantEmit> = BTreeMap::new();
        per_variant.insert("0".into(), add_liquidity_emit_for_init());
        let rule = bundle_with_join_kind(per_variant, UnknownVariantPolicy::Warn);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = execute(&ctx, &decoded, &rule).expect("warn policy returns Ok");
        assert!(envelopes.is_empty(), "warn policy emits no envelopes");
    }

    #[test]
    fn enum_tagged_dispatch_with_short_input_denies() {
        // < 32 bytes: engine_dispatch returns None — apply Deny policy.
        let user_data: Vec<u8> = vec![0u8; 16];
        let decoded = decoded_with_user_data(user_data);

        let mut per_variant: BTreeMap<String, PerVariantEmit> = BTreeMap::new();
        per_variant.insert("0".into(), add_liquidity_emit_for_init());
        let rule = bundle_with_join_kind(per_variant, UnknownVariantPolicy::Deny);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let err = execute(&ctx, &decoded, &rule).unwrap_err();
        match err {
            MapperError::Unsupported(msg) => {
                assert!(
                    msg.contains("16 bytes"),
                    "error message should mention the short length (got {msg:?})"
                );
            }
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    /// Phase 13 P2-5 — the Curve Router NG dispatcher resolves to the real
    /// `CURVE_ROUTER_NG_SWAP_TYPES` table (previously an `Unsupported` stub).
    #[test]
    fn curve_router_ng_dispatcher_is_wired() {
        let table = lookup_dispatcher_table("curve_router_ng_swap_types")
            .expect("curve_router_ng_swap_types must be wired");
        assert_eq!(table.name, "Curve Router NG swap_type");
        // swap_type = 8 (WRAPPED_ASSET_CONVERT) decodes via the generic engine.
        let payload = encode_payload("(uint256)", vec![DynSolValue::Uint(U256::from(8u64), 256)]);
        let decoded = engine_dispatch(&payload, table).expect("kind 8 dispatches");
        assert_eq!(decoded.kind, 8);
        assert_eq!(decoded.kind_name, "WRAPPED_ASSET_CONVERT");
    }
}
