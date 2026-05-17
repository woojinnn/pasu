//! `opcode_stream_dispatch` strategy execution (spec §5.2.2).
//!
//! The outer call is `execute(bytes commands, bytes[] inputs, ...)` (Universal
//! Router style) — `commands` is one byte per step, `inputs[i]` carries the
//! ABI-encoded argument tuple for the opcode in `commands[i]`. The bundle's
//! `per_opcode_emit` maps each known opcode (after `mask`) to a `single_emit`-
//! shaped rule whose fields evaluate against the step's decoded `args`.
//!
//! Phase 5 PoC supports `dispatcher_id == "universal_router"` only. Other
//! routers (Pancake UR, Sushi RP, 0x Settler) reuse the same DSL but each
//! ships its own Tier B `OpcodeTable`; wiring those is a follow-up.
//!
//! Flow (spec §5.2:475-492):
//!
//! 1. Pull `commands` (`bytes`) and `inputs` (`bytes[]`) from `decoded.args`
//!    via Tier B's [`extract_commands_and_inputs`]. Both UR overloads put them
//!    at arg index 0 and 1.
//! 2. Dispatch through Tier B [`subdecode::opcode_stream::dispatch`] against
//!    [`subdecode::protocols::universal_router::UNISWAP_UR_TABLE`] → one
//!    `DecodedStep` per command byte (opcode already masked, name resolved,
//!    `inputs[i]` ABI-decoded against the opcode's tuple schema).
//! 3. For each `DecodedStep`, look up the bundle's `per_opcode_emit` entry by
//!    `format!("0x{:02x}", step.opcode)`. Miss → apply `unknown_opcode_policy`
//!    (`deny` errors out, `warn` logs to stderr and skips, `ignore_step`
//!    silently skips).
//! 4. Hit → build a synthetic `DecodedCall` whose `args` are the step's args
//!    (converted to the new pipeline's `DecodedValue` form) and dispatch
//!    through [`super::single_emit::execute`] with the per-opcode
//!    `(category, action, fields)` rephrased as a `SingleEmit` rule.
//! 5. Concatenate envelopes across steps.
//!
//! Tier B exposes the dispatch table and step decoding as a single source of
//! truth — this module doesn't replicate the opcode catalog. The bundle's
//! `per_opcode_emit` keys MUST match the opcodes Tier B knows about; mismatches
//! surface as `UnknownOpcodePolicy` outcomes here. Tier B also enforces the
//! mask / allow-revert-bit conventions, so the bundle's declared values are
//! treated as documentation rather than re-applied here. We do, however, fail
//! fast if the bundle disagrees with Tier B on either value — that points at a
//! bundle author bug.

use abi_resolver::bridge::convert_arg;
use abi_resolver::subdecode::opcode_stream as tier_b_opcode_stream;
use abi_resolver::subdecode::protocols::universal_router::{
    extract_commands_and_inputs, UNISWAP_UR_ALLOW_REVERT, UNISWAP_UR_MASK, UNISWAP_UR_TABLE,
};
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::ActionEnvelope;
use std::collections::BTreeMap;

use crate::mapper::{MapContext, MapperError};

use super::single_emit;
use super::types::{EmitRule, PerOpcodeEmit, UnknownOpcodePolicy, ValueExpr};

/// Dispatcher id supported by the Phase 5 PoC. Matches the value bundles
/// declare under `emit.dispatcher_id`.
pub const DISPATCHER_ID_UNIVERSAL_ROUTER: &str = "universal_router";

/// Execute an `opcode_stream_dispatch` rule against `decoded`.
///
/// Returns the flattened envelopes the per-opcode rules emit, or an error if
/// the rule shape is unsupported, the bundle disagrees with Tier B on
/// mask/allow_revert_bit, the outer args don't carry a `(bytes, bytes[])`
/// pair, or any per-step `single_emit` rule fails.
pub fn execute(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
    rule: &EmitRule,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    let (dispatcher_id, mask, allow_revert_bit, per_opcode_emit, unknown_opcode_policy) = match rule
    {
        EmitRule::OpcodeStreamDispatch {
            dispatcher_id,
            mask,
            allow_revert_bit,
            per_opcode_emit,
            unknown_opcode_policy,
        } => (
            dispatcher_id.as_str(),
            mask.as_str(),
            allow_revert_bit.as_str(),
            per_opcode_emit,
            *unknown_opcode_policy,
        ),
        other => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "opcode_stream::execute called with non-opcode_stream_dispatch rule: {other:?}"
            )));
        }
    };

    if dispatcher_id != DISPATCHER_ID_UNIVERSAL_ROUTER {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "opcode_stream_dispatch dispatcher_id {dispatcher_id:?} not implemented in Phase 5 PoC \
             (only {DISPATCHER_ID_UNIVERSAL_ROUTER:?} supported)"
        )));
    }

    // Bundle's declared mask / allow_revert_bit must agree with Tier B's
    // UNISWAP_UR_TABLE — otherwise the per-opcode keys we're about to look up
    // are computed against a different bit layout than Tier B dispatched
    // against. Detecting this here points authors at a bundle bug rather than
    // surfacing as silent unknown-opcode misses.
    let bundle_mask = parse_hex_byte(mask, "mask")?;
    let bundle_allow_revert_bit = parse_hex_byte(allow_revert_bit, "allow_revert_bit")?;
    if bundle_mask != UNISWAP_UR_MASK {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "bundle mask {bundle_mask:#04x} disagrees with Tier B UNISWAP_UR_TABLE mask {UNISWAP_UR_MASK:#04x}"
        )));
    }
    if bundle_allow_revert_bit != UNISWAP_UR_ALLOW_REVERT {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "bundle allow_revert_bit {bundle_allow_revert_bit:#04x} disagrees with Tier B \
             UNISWAP_UR_TABLE allow_revert_bit {UNISWAP_UR_ALLOW_REVERT:#04x}"
        )));
    }

    // Bridge from the new-pipeline `DecodedCall` back to the legacy form Tier B
    // exposes. The two share field semantics but use different value enums
    // (DecodedValue ↔ DynSolValue) — we need the legacy view here because
    // `extract_commands_and_inputs` and the `OpcodeTable` schemas were defined
    // against `crate::decode::DecodedCall`.
    let legacy_decoded = to_legacy_decoded(decoded)?;
    let (commands, inputs) = extract_commands_and_inputs(&legacy_decoded).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "opcode_stream_dispatch: outer args do not match (bytes commands, bytes[] inputs) \
             — got function_signature {:?}",
            decoded.function_signature
        ))
    })?;

    let steps = tier_b_opcode_stream::dispatch(&commands, &inputs, &UNISWAP_UR_TABLE);

    let mut envelopes = Vec::new();
    for step in steps {
        let key = format!("0x{:02x}", step.opcode);
        let Some(rule) = per_opcode_emit.get(&key) else {
            match unknown_opcode_policy {
                UnknownOpcodePolicy::Deny => {
                    return Err(MapperError::Internal(anyhow::anyhow!(
                        "opcode_stream_dispatch: opcode {key} (step index {}, Tier B name {:?}) \
                         has no per_opcode_emit entry and unknown_opcode_policy=deny",
                        step.index,
                        step.name
                    )));
                }
                UnknownOpcodePolicy::Warn => {
                    eprintln!(
                        "[opcode_stream_dispatch] warn: opcode {key} (step index {}, Tier B name \
                         {:?}) has no per_opcode_emit entry — skipping (policy=warn)",
                        step.index, step.name
                    );
                    continue;
                }
                UnknownOpcodePolicy::IgnoreStep => continue,
            }
        };

        // Skip steps Tier B couldn't ABI-decode — we have no `args` to feed
        // the per-opcode rule. Surface as an error rather than silently
        // dropping so authors notice the schema mismatch.
        let step_args = step.args.ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "opcode_stream_dispatch: opcode {key} (step index {}, Tier B name {:?}) \
                 has no decoded args — Tier B error: {:?}",
                step.index,
                step.name,
                step.error
            ))
        })?;

        // Build a synthetic per-step `DecodedCall` so the existing single_emit
        // pipeline can evaluate the per-opcode fields against the step's args.
        let inner_args = step_args
            .into_iter()
            .map(convert_arg)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                MapperError::Internal(anyhow::anyhow!(
                    "opcode_stream_dispatch: opcode {key} step args bridge failed: {error}"
                ))
            })?;
        let step_decoded = DecodedCall {
            decoder_id: DecoderId::new(format!("opcode_stream::{}", step.name)),
            function_signature: format!("{}({})", step.name, inner_args_signature(&inner_args)),
            args: inner_args,
            nested: Vec::new(),
        };

        let inner_rule = per_opcode_rule_to_single_emit(rule);
        let envelope = single_emit::execute(ctx, &step_decoded, &inner_rule).map_err(|error| {
            MapperError::Internal(anyhow::anyhow!(
                "opcode_stream_dispatch: opcode {key} (step index {}, Tier B name {:?}) emit failed: {error}",
                step.index,
                step.name
            ))
        })?;
        envelopes.push(envelope);
    }

    Ok(envelopes)
}

/// Render a synthetic function signature string from the bridged arg list.
/// Used purely for diagnostic messages — the single_emit pipeline matches on
/// arg *names*, not on this signature.
fn inner_args_signature(args: &[abi_resolver::DecodedArg]) -> String {
    args.iter()
        .map(|arg| arg.abi_type.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// Convert a `PerOpcodeEmit` into the `SingleEmit` rule variant the
/// `single_emit::execute` interpreter expects.
fn per_opcode_rule_to_single_emit(rule: &PerOpcodeEmit) -> EmitRule {
    // Clone the field map because `EmitRule::SingleEmit` owns its fields.
    let fields: BTreeMap<String, ValueExpr> = rule.fields.clone();
    EmitRule::SingleEmit {
        category: rule.category.clone(),
        action: rule.action.clone(),
        fields,
    }
}

/// Bridge `decoder::DecodedCall` → `decode::DecodedCall` (legacy view used by
/// Tier B). The two share field semantics but use different value enums; we
/// rebuild the legacy form so `extract_commands_and_inputs` (which pattern-
/// matches on `DynSolValue`) can pull `(commands, inputs)`.
fn to_legacy_decoded(
    decoded: &DecodedCall,
) -> Result<abi_resolver::decode::DecodedCall, MapperError> {
    let mut legacy_args = Vec::with_capacity(decoded.args.len());
    for arg in &decoded.args {
        let dyn_value = decoded_value_to_dyn(&arg.value)?;
        legacy_args.push(abi_resolver::decode::DecodedArg {
            name: arg.name.clone(),
            sol_type: arg.abi_type.clone(),
            value: dyn_value,
            components: Vec::new(),
        });
    }
    Ok(abi_resolver::decode::DecodedCall {
        function_name: decoded.decoder_id.as_str().to_owned(),
        signature: decoded.function_signature.clone(),
        args: legacy_args,
    })
}

/// `DecodedValue` (new pipeline) → `DynSolValue` (Tier B). Inverse of
/// `bridge::convert_value`. Phase 5 only needs the value classes that
/// `extract_commands_and_inputs` matches on (`Bytes`, `Array<Bytes>`) plus the
/// `Uint` we'd see for `deadline`; we cover the full enum for safety but use
/// the minimum bit-widths that decoder consumers tolerate.
fn decoded_value_to_dyn(
    value: &abi_resolver::DecodedValue,
) -> Result<alloy_dyn_abi::DynSolValue, MapperError> {
    use abi_resolver::DecodedValue;
    use alloy_dyn_abi::DynSolValue;
    Ok(match value {
        DecodedValue::Address(addr) => {
            let hex_str = addr.to_string();
            let no_prefix = hex_str.strip_prefix("0x").unwrap_or(&hex_str);
            let mut bytes = [0u8; 20];
            let raw = hex::decode(no_prefix)
                .map_err(|e| MapperError::Internal(anyhow::anyhow!("address hex decode: {e}")))?;
            if raw.len() != 20 {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "address byte length {} != 20",
                    raw.len()
                )));
            }
            bytes.copy_from_slice(&raw);
            DynSolValue::Address(alloy_primitives::Address::from(bytes))
        }
        DecodedValue::Uint(value) => DynSolValue::Uint(*value, 256),
        DecodedValue::Int(value) => DynSolValue::Int(*value, 256),
        DecodedValue::Bool(b) => DynSolValue::Bool(*b),
        DecodedValue::Bytes(b) => DynSolValue::Bytes(b.clone()),
        DecodedValue::String(s) => DynSolValue::String(s.clone()),
        DecodedValue::Array(items) => {
            let inner: Vec<DynSolValue> = items
                .iter()
                .map(decoded_value_to_dyn)
                .collect::<Result<_, _>>()?;
            DynSolValue::Array(inner)
        }
        DecodedValue::Tuple(items) => {
            let inner: Vec<DynSolValue> = items
                .iter()
                .map(decoded_value_to_dyn)
                .collect::<Result<_, _>>()?;
            DynSolValue::Tuple(inner)
        }
    })
}

/// Parse `"0x" + 1-2 hex chars` into a single byte. Used for the bundle's
/// `mask` / `allow_revert_bit` strings, which we sanity-check against the
/// Tier B table.
fn parse_hex_byte(s: &str, field: &str) -> Result<u8, MapperError> {
    let no_prefix = s.strip_prefix("0x").ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "{field}: expected \"0x\"-prefixed hex byte, got {s:?}"
        ))
    })?;
    let raw = hex::decode(format!("{:0>2}", no_prefix))
        .map_err(|e| MapperError::Internal(anyhow::anyhow!("{field} hex decode: {e}")))?;
    if raw.len() != 1 {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected a single byte, got {} bytes",
            raw.len()
        )));
    }
    Ok(raw[0])
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use abi_resolver::{DecodedArg, DecodedValue, DecoderId};
    use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
    use alloy_json_abi::Function;
    use alloy_primitives::U256;
    use policy_engine::action::dex::SwapMode;
    use policy_engine::action::misc::PermitKind;
    use policy_engine::action::{Action, Address, AmountKind, AssetKind, Category, DecimalString};

    use crate::mapper::MapContext;
    use crate::token_registry::EmptyTokenRegistry;

    use super::super::types::AdapterFunctionBundle;
    use super::*;

    const UR_BUNDLE_JSON: &str =
        include_str!("../../tests/fixtures/uniswap-ur-execute.json");

    fn build_ctx<'a>(
        registry: &'a EmptyTokenRegistry,
        from: &'a Address,
        to: &'a Address,
        value: &'a DecimalString,
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

    fn token_in() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")
            .unwrap()
    }

    fn token_out() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")
            .unwrap()
    }

    fn recipient_addr() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0x4444444444444444444444444444444444444444")
            .unwrap()
    }

    /// Encode the 5-tuple `(recipient, amountIn, amountOutMin, bytes path,
    /// payerIsUser)` for the V3_SWAP_EXACT_IN opcode (older deployments).
    /// Tier B's fallback chain accepts the 5-tuple when the 6-tuple
    /// `minHopPriceX36` shape doesn't decode. The opcode's `inputs[i]` carries
    /// the 5 args at the top level (not wrapped in an outer tuple) — matching
    /// Tier B's `Function::parse("step(address,uint256,uint256,bytes,bool)")`.
    fn encode_v3_swap_exact_in_input(
        recipient: alloy_primitives::Address,
        amount_in: u128,
        amount_out_min: u128,
        path: Vec<u8>,
        payer_is_user: bool,
    ) -> Vec<u8> {
        let func =
            Function::parse("step(address,uint256,uint256,bytes,bool)").unwrap();
        let values = vec![
            DynSolValue::Address(recipient),
            DynSolValue::Uint(U256::from(amount_in), 256),
            DynSolValue::Uint(U256::from(amount_out_min), 256),
            DynSolValue::Bytes(path),
            DynSolValue::Bool(payer_is_user),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        // strip synthetic 4-byte selector
        raw[4..].to_vec()
    }

    /// Encode SWEEP input `(address token, address recipient, uint256 amountMin)`.
    fn encode_sweep_input(
        token: alloy_primitives::Address,
        recipient: alloy_primitives::Address,
        amount_min: u128,
    ) -> Vec<u8> {
        let func = Function::parse("step(address,address,uint256)").unwrap();
        let values = vec![
            DynSolValue::Address(token),
            DynSolValue::Address(recipient),
            DynSolValue::Uint(U256::from(amount_min), 256),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    /// Encode PAY_PORTION input — used to exercise the unknown_opcode path
    /// (the Phase 5 bundle does NOT include 0x06 in per_opcode_emit).
    fn encode_pay_portion_input(
        token: alloy_primitives::Address,
        recipient: alloy_primitives::Address,
        bips: u128,
    ) -> Vec<u8> {
        let func = Function::parse("step(address,address,uint256)").unwrap();
        let values = vec![
            DynSolValue::Address(token),
            DynSolValue::Address(recipient),
            DynSolValue::Uint(U256::from(bips), 256),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    /// `[USDC][fee=3000][WETH]` — single-hop V3 packed path.
    fn v3_packed_path_usdc_weth() -> Vec<u8> {
        hex::decode(concat!(
            "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "000bb8",
            "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        ))
        .unwrap()
    }

    /// Build an outer `DecodedCall` that mirrors `execute(bytes commands,
    /// bytes[] inputs, uint256 deadline)` as a Sourcify-decoded call would
    /// reach the declarative mapper.
    fn ur_execute_decoded(
        decoder_id: DecoderId,
        commands: Vec<u8>,
        inputs: Vec<Vec<u8>>,
    ) -> DecodedCall {
        DecodedCall {
            decoder_id,
            function_signature: "execute(bytes,bytes[],uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "commands".into(),
                    abi_type: "bytes".into(),
                    value: DecodedValue::Bytes(commands),
                },
                DecodedArg {
                    name: "inputs".into(),
                    abi_type: "bytes[]".into(),
                    value: DecodedValue::Array(
                        inputs.into_iter().map(DecodedValue::Bytes).collect(),
                    ),
                },
                DecodedArg {
                    name: "deadline".into(),
                    abi_type: "uint256".into(),
                    value: DecodedValue::Uint(U256::from(9_999_999_999_u64)),
                },
            ],
            nested: vec![],
        }
    }

    fn dummy_addr(label: u8) -> Address {
        Address::from_str(&format!("0x{}{}", "0".repeat(38), format!("{label:02x}"))).unwrap()
    }

    #[test]
    fn single_v3_swap_exact_in_yields_one_swap_envelope() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let input = encode_v3_swap_exact_in_input(
            recipient_addr(),
            1_000_000,
            900_000,
            v3_packed_path_usdc_weth(),
            true,
        );
        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x00],
            vec![input],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert_eq!(envelopes.len(), 1);
        let Action::Swap(action) = &envelopes[0].action else {
            panic!("expected swap, got {:?}", envelopes[0].action);
        };
        assert_eq!(envelopes[0].category, Category::Dex);
        assert_eq!(action.swap_mode, SwapMode::ExactIn);
        assert_eq!(action.input_token.asset.kind, AssetKind::Erc20);
        assert_eq!(
            action.input_token.asset.address.as_ref().map(|a| a.to_string()),
            Some(format!("0x{}", hex::encode(token_in()))),
        );
        assert_eq!(action.input_token.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.input_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("1000000".to_owned())
        );
        assert_eq!(
            action.output_token.asset.address.as_ref().map(|a| a.to_string()),
            Some(format!("0x{}", hex::encode(token_out()))),
        );
        assert_eq!(action.output_token.amount.kind, AmountKind::Min);
        assert_eq!(
            action.output_token.amount.value.as_ref().map(|v| v.to_string()),
            Some("900000".to_owned())
        );
        assert_eq!(
            action.recipient.to_string(),
            format!("0x{}", hex::encode(recipient_addr()))
        );
    }

    #[test]
    fn multi_step_permit_swap_sweep_yields_three_envelopes() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        // Step 1: PERMIT2_PERMIT (0x0a). We encode with a deliberately small
        // signature blob — Tier B doesn't validate signature content.
        let permit_input = encode_permit2_permit_input(
            token_in(),
            1_000_000,
            1_700_000_000,
            0,
            recipient_addr(),
            1_700_000_900,
            vec![0xab, 0xcd],
        );
        // Step 2: V3_SWAP_EXACT_IN (0x00)
        let swap_input = encode_v3_swap_exact_in_input(
            recipient_addr(),
            500_000,
            450_000,
            v3_packed_path_usdc_weth(),
            true,
        );
        // Step 3: SWEEP (0x04)
        let sweep_input = encode_sweep_input(token_out(), recipient_addr(), 1);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x0a, 0x00, 0x04],
            vec![permit_input, swap_input, sweep_input],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert_eq!(envelopes.len(), 3, "expected 3 envelopes, got {envelopes:?}");

        // Order MUST match commands order.
        assert!(matches!(envelopes[0].action, Action::Permit(_)));
        assert!(matches!(envelopes[1].action, Action::Swap(_)));
        assert!(matches!(envelopes[2].action, Action::Transfer(_)));

        if let Action::Permit(permit) = &envelopes[0].action {
            assert_eq!(permit.permit_kind, PermitKind::Permit2Single);
            assert_eq!(permit.token.kind, AssetKind::Erc20);
            assert_eq!(
                permit.token.address.as_ref().map(|a| a.to_string()),
                Some(format!("0x{}", hex::encode(token_in())))
            );
        }
        if let Action::Transfer(transfer) = &envelopes[2].action {
            assert_eq!(transfer.token.asset.kind, AssetKind::Erc20);
            assert_eq!(
                transfer.token.asset.address.as_ref().map(|a| a.to_string()),
                Some(format!("0x{}", hex::encode(token_out())))
            );
        }
    }

    #[test]
    fn unknown_opcode_with_warn_policy_skips_step() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        // PAY_PORTION (0x06) is NOT in the bundle's per_opcode_emit; the
        // bundle declares unknown_opcode_policy=warn so the step is skipped.
        let pay_portion = encode_pay_portion_input(token_out(), recipient_addr(), 50);
        let swap = encode_v3_swap_exact_in_input(
            recipient_addr(),
            1_000_000,
            900_000,
            v3_packed_path_usdc_weth(),
            true,
        );

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x06, 0x00],
            vec![pay_portion, swap],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        // Only the swap remains.
        assert_eq!(envelopes.len(), 1);
        assert!(matches!(envelopes[0].action, Action::Swap(_)));
    }

    #[test]
    fn empty_commands_yields_no_envelopes() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();
        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![],
            vec![],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert!(envelopes.is_empty());
    }

    #[test]
    fn allow_revert_high_bit_is_stripped_by_tier_b() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();
        // 0x00 | 0x80 == 0x80 → opcode 0x00 (V3_SWAP_EXACT_IN) with allowRevert.
        let input = encode_v3_swap_exact_in_input(
            recipient_addr(),
            1,
            1,
            v3_packed_path_usdc_weth(),
            true,
        );
        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x80],
            vec![input],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert_eq!(envelopes.len(), 1);
        assert!(matches!(envelopes[0].action, Action::Swap(_)));
    }

    /// Encode `((address token, uint160 amount, uint48 expiration, uint48 nonce),
    /// address spender, uint256 sigDeadline) permitSingle, bytes signature`.
    fn encode_permit2_permit_input(
        token: alloy_primitives::Address,
        amount: u128,
        expiration: u64,
        nonce: u64,
        spender: alloy_primitives::Address,
        sig_deadline: u64,
        signature: Vec<u8>,
    ) -> Vec<u8> {
        // permitSingle: tuple of (details_tuple, spender, sigDeadline)
        // details_tuple: (token, amount uint160, expiration uint48, nonce uint48)
        let details = DynSolValue::Tuple(vec![
            DynSolValue::Address(token),
            DynSolValue::Uint(U256::from(amount), 160),
            DynSolValue::Uint(U256::from(expiration), 48),
            DynSolValue::Uint(U256::from(nonce), 48),
        ]);
        let permit_single = DynSolValue::Tuple(vec![
            details,
            DynSolValue::Address(spender),
            DynSolValue::Uint(U256::from(sig_deadline), 256),
        ]);
        let func = Function::parse(
            "step(((address,uint160,uint48,uint48),address,uint256),bytes)",
        )
        .unwrap();
        let values = vec![permit_single, DynSolValue::Bytes(signature)];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }
}
