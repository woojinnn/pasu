//! Bridge: convert legacy `decode::DecodedCall` (DynSolValue-based) to the new
//! `decoder::DecodedCall` (DecodedValue-based) so legacy Sourcify-driven
//! decodes can feed the new `Mapper` trait pipeline.
//!
//! Used as the fallback path in `request-router::route_request` when no
//! per-function `Decoder` matches.

use std::str::FromStr as _;

use alloy_dyn_abi::DynSolValue;
use alloy_primitives::Address as AlloyAddress;
use policy_engine::action::Address;

use crate::decode::{DecodedArg as LegacyArg, DecodedCall as LegacyCall};
use crate::decoder::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use crate::ids::{
    APPROVE_SELECTOR, ERC20_APPROVE_DECODER_ID, ERC20_TRANSFER_DECODER_ID,
    ERC20_TRANSFER_FROM_DECODER_ID, SET_APPROVAL_FOR_ALL_DECODER_ID, SET_APPROVAL_FOR_ALL_SELECTOR,
    TRANSFER_FROM_SELECTOR, TRANSFER_SELECTOR,
};
use crate::ids::{
    EXACT_INPUT_SELECTOR, EXACT_INPUT_SINGLE_SELECTOR, EXACT_OUTPUT_DECODER_ID,
    EXACT_OUTPUT_SELECTOR, EXACT_OUTPUT_SINGLE_DECODER_ID, EXACT_OUTPUT_SINGLE_SELECTOR,
    UNISWAP_V3_DECODER_ID,
};
use crate::ids::{
    SR02_EXACT_INPUT_DECODER_ID, SR02_EXACT_INPUT_SELECTOR, SR02_EXACT_INPUT_SINGLE_DECODER_ID,
    SR02_EXACT_INPUT_SINGLE_SELECTOR, SR02_EXACT_OUTPUT_DECODER_ID, SR02_EXACT_OUTPUT_SELECTOR,
    SR02_EXACT_OUTPUT_SINGLE_DECODER_ID, SR02_EXACT_OUTPUT_SINGLE_SELECTOR,
};
use crate::ids::{
    SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID, SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR,
    SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID, SWAP_EXACT_ETH_FOR_TOKENS_FOT_DECODER_ID,
    SWAP_EXACT_ETH_FOR_TOKENS_FOT_SELECTOR, SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR,
    SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID, SWAP_EXACT_TOKENS_FOR_ETH_FOT_DECODER_ID,
    SWAP_EXACT_TOKENS_FOR_ETH_FOT_SELECTOR, SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR,
    SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID, SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_DECODER_ID,
    SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_SELECTOR, SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR,
    SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID, SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR,
    SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID, SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR,
};
use crate::ids::{
    WETH_DEPOSIT_DECODER_ID, WETH_DEPOSIT_SELECTOR, WETH_WITHDRAW_DECODER_ID,
    WETH_WITHDRAW_SELECTOR,
};

/// Convert a legacy `DecodedCall` (Sourcify-decoded) into the new shape so it
/// can be dispatched against the new `MapperRegistry`.
///
/// `selector` is the 4-byte selector extracted from the original calldata.
/// We use it to look up a known `decoder_id` so existing mappers (keyed by
/// decoder_id) match without modification. Unknown selectors get a synthetic
/// `fallback/0x<selector>` ID — these will not match any registered mapper
/// today, which is the expected behaviour for a function we don't know yet.
pub fn convert_legacy_call(
    legacy: LegacyCall,
    selector: [u8; 4],
) -> Result<DecodedCall, BridgeError> {
    let decoder_id = decoder_id_for_selector(selector)
        .map(DecoderId::new)
        .unwrap_or_else(|| fallback_decoder_id(selector));

    // V3 / SR02 style: function takes a single `params` struct. Sourcify hands
    // it back as one top-level `Tuple` arg with field metadata on
    // `components`. The new-pipeline mappers were written against the
    // `sol!`-flattened layout (one top-level arg per struct field), so we
    // flatten here to keep the mapper API stable.
    let args = if legacy.args.len() == 1
        && matches!(legacy.args[0].value, DynSolValue::Tuple(_))
        && !legacy.args[0].components.is_empty()
    {
        let only = legacy.args.into_iter().next().expect("len == 1 checked");
        flatten_tuple_arg(only)?
    } else {
        legacy
            .args
            .into_iter()
            .map(convert_arg)
            .collect::<Result<Vec<_>, _>>()?
    };

    Ok(DecodedCall {
        decoder_id,
        function_signature: legacy.signature,
        args,
        nested: vec![],
    })
}

/// Flatten a `Tuple`-valued top-level argument into one new-pipeline
/// [`DecodedArg`] per field, using `components` for field names. The function
/// signature for `legacy.sol_type` is expected to be a tuple type like
/// `(address,address,uint24,...)`.
fn flatten_tuple_arg(arg: LegacyArg) -> Result<Vec<DecodedArg>, BridgeError> {
    let DynSolValue::Tuple(items) = arg.value else {
        return Err(BridgeError::UnsupportedValue(
            "flatten_tuple_arg called on non-tuple".to_string(),
        ));
    };
    if items.len() != arg.components.len() {
        // Defensive: mismatch between tuple values and parameter metadata.
        // Fall back to keeping it as a single nested Tuple arg under the
        // original parameter name so the mapper at least sees something.
        let value = convert_value(DynSolValue::Tuple(items))?;
        return Ok(vec![DecodedArg {
            name: arg.name,
            // We hold the full `Param` here (with `components`), so emit the
            // canonical parenthesised type (e.g. `(address,uint160,uint48)`)
            // rather than the bare `"tuple"` — see `canonical_abi_type`.
            abi_type: canonical_abi_type(&arg.sol_type, &arg.components),
            value,
        }]);
    }
    let mut out = Vec::with_capacity(items.len());
    for (value, component) in items.into_iter().zip(arg.components) {
        let name = if component.name.is_empty() {
            format!("arg{}", out.len())
        } else {
            component.name.clone()
        };
        out.push(DecodedArg {
            // Each flattened field keeps its own canonical type so a NESTED
            // tuple field (V4 `modifyLiquidities` params) threads its inner
            // widths instead of collapsing to a bare `"tuple"`.
            abi_type: canonical_abi_type(&component.ty, &component.components),
            name,
            value: convert_value(value)?,
        });
    }
    Ok(out)
}

/// Convert a single legacy `decode::DecodedArg` (the form Tier B
/// `subdecode::opcode_stream::dispatch` emits) into a new-pipeline
/// [`DecodedArg`]. Wraps [`convert_value`] for the value field while preserving
/// the argument's `name` and `sol_type`.
pub fn convert_arg(legacy: LegacyArg) -> Result<DecodedArg, BridgeError> {
    Ok(DecodedArg {
        // A bare `"tuple"` / `"tuple[]"` (alloy keeps the field types out of
        // band on `components`) is rebuilt into the full parenthesised type so
        // the downstream `eval::decoded_value_to_json_typed` can thread each
        // nested component's ABI width — without it a `uint48` / `int24`
        // component collapses to a decimal string and a `u64`/`i32`-typed
        // `ActionBody` field rejects it. Scalars pass through unchanged.
        abi_type: canonical_abi_type(&legacy.sol_type, &legacy.components),
        name: legacy.name,
        value: convert_value(legacy.value)?,
    })
}

/// Rebuild a parameter's **canonical** ABI type string from its `components`.
///
/// alloy's `Param.ty` for a tuple input is the bare word `"tuple"` (or
/// `"tuple[]"` / `"tuple[2]"`), with the inner field types carried out of band
/// on `Param.components`. For a NON-compound type (`components` empty) `ty` is
/// already canonical (`"address"`, `"uint48"`, `"bytes"`, …).
///
/// [`alloy_json_abi::Param::selector_type`] does exactly this reconstruction —
/// it returns `Cow::Borrowed(&ty)` when `components` is empty and otherwise
/// emits the fully parenthesised form (`"(address,uint160,uint48,uint48)[]"`),
/// recursing into nested tuples. We thread that string onto
/// [`DecodedArg::abi_type`] so the JSON encoder (`eval.rs`) recovers per-field
/// widths for nested tuple / tuple-array components.
fn canonical_abi_type(ty: &str, components: &[alloy_json_abi::Param]) -> String {
    if components.is_empty() {
        // Scalars / `bytes` / `string` / non-tuple arrays — `ty` is canonical.
        return ty.to_owned();
    }
    // Build a throwaway `Param` so we can reuse alloy's canonical formatter
    // (which handles arbitrary nesting + the `tuple[..]` array suffix).
    alloy_json_abi::Param {
        name: String::new(),
        ty: ty.to_owned(),
        components: components.to_vec(),
        internal_type: None,
    }
    .selector_type()
    .into_owned()
}

/// Convert a single `alloy_dyn_abi::DynSolValue` into the policy-engine /
/// mapper-pipeline [`DecodedValue`]. Exposed for callers (e.g. the declarative
/// Phase 5 opcode-stream dispatcher) that consume Tier B `subdecode` output —
/// which carries `DynSolValue` — and need to feed it into the new pipeline.
pub fn convert_value(value: DynSolValue) -> Result<DecodedValue, BridgeError> {
    Ok(match value {
        DynSolValue::Address(addr) => DecodedValue::Address(address_to_policy(addr)?),
        DynSolValue::Uint(v, _) => DecodedValue::Uint(v),
        DynSolValue::Int(v, _) => DecodedValue::Int(v),
        DynSolValue::Bool(b) => DecodedValue::Bool(b),
        DynSolValue::Bytes(b) => DecodedValue::Bytes(b),
        DynSolValue::FixedBytes(word, len) => DecodedValue::Bytes(word.as_slice()[..len].to_vec()),
        DynSolValue::String(s) => DecodedValue::String(s),
        DynSolValue::Array(items) | DynSolValue::FixedArray(items) => DecodedValue::Array(
            items
                .into_iter()
                .map(convert_value)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        DynSolValue::Tuple(items) => DecodedValue::Tuple(
            items
                .into_iter()
                .map(convert_value)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        DynSolValue::Function(_) => return Err(BridgeError::UnsupportedValue("function".into())),
    })
}

fn address_to_policy(addr: AlloyAddress) -> Result<Address, BridgeError> {
    Address::from_str(&format!("0x{}", hex::encode(addr.0)))
        .map_err(|e| BridgeError::AddressFormat(e.to_string()))
}

fn fallback_decoder_id(selector: [u8; 4]) -> DecoderId {
    DecoderId::new(format!("fallback/0x{}", hex::encode(selector)))
}

/// Lookup table: selector → existing `decoder_id`. Mirrors the IDs declared
/// per-function in `crates/adapters/abi-resolver/src/decoders/*.rs`. Keeping
/// this central means we only need to update one place when a new per-function
/// decoder lands.
pub fn decoder_id_for_selector(selector: [u8; 4]) -> Option<&'static str> {
    match selector {
        // ── erc20 ─────────────────────────────────────────────────────────────
        APPROVE_SELECTOR => Some(ERC20_APPROVE_DECODER_ID),
        TRANSFER_SELECTOR => Some(ERC20_TRANSFER_DECODER_ID),
        TRANSFER_FROM_SELECTOR => Some(ERC20_TRANSFER_FROM_DECODER_ID),
        SET_APPROVAL_FOR_ALL_SELECTOR => Some(SET_APPROVAL_FOR_ALL_DECODER_ID),

        // ── uniswap v2 ────────────────────────────────────────────────────────
        SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR => Some(SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID),
        SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR => Some(SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID),
        SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR => Some(SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID),
        SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR => Some(SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID),
        SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR => Some(SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID),
        SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR => Some(SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID),
        SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_SELECTOR => {
            Some(SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_DECODER_ID)
        }
        SWAP_EXACT_ETH_FOR_TOKENS_FOT_SELECTOR => Some(SWAP_EXACT_ETH_FOR_TOKENS_FOT_DECODER_ID),
        SWAP_EXACT_TOKENS_FOR_ETH_FOT_SELECTOR => Some(SWAP_EXACT_TOKENS_FOR_ETH_FOT_DECODER_ID),

        // ── uniswap v3 ────────────────────────────────────────────────────────
        // ExactInputSingle / ExactInput both share UNISWAP_V3_DECODER_ID in the
        // current new-pipeline mapper layout. The mapper distinguishes by
        // selector internally.
        EXACT_INPUT_SINGLE_SELECTOR | EXACT_INPUT_SELECTOR => Some(UNISWAP_V3_DECODER_ID),
        EXACT_OUTPUT_SINGLE_SELECTOR => Some(EXACT_OUTPUT_SINGLE_DECODER_ID),
        EXACT_OUTPUT_SELECTOR => Some(EXACT_OUTPUT_DECODER_ID),

        // ── swap-router-02 ────────────────────────────────────────────────────
        SR02_EXACT_INPUT_SINGLE_SELECTOR => Some(SR02_EXACT_INPUT_SINGLE_DECODER_ID),
        SR02_EXACT_INPUT_SELECTOR => Some(SR02_EXACT_INPUT_DECODER_ID),
        SR02_EXACT_OUTPUT_SINGLE_SELECTOR => Some(SR02_EXACT_OUTPUT_SINGLE_DECODER_ID),
        SR02_EXACT_OUTPUT_SELECTOR => Some(SR02_EXACT_OUTPUT_DECODER_ID),

        // Universal Router is handled directly by `MultiRouterCallAdapter`
        // (which does opcode dispatch in `call-adapter`), so its selectors
        // intentionally don't appear here — fallback would never be reached
        // for UR addresses, and even if it were, there's no UR Mapper to
        // route to.

        // ── weth ──────────────────────────────────────────────────────────────
        WETH_DEPOSIT_SELECTOR => Some(WETH_DEPOSIT_DECODER_ID),
        WETH_WITHDRAW_SELECTOR => Some(WETH_WITHDRAW_DECODER_ID),

        _ => None,
    }
}

/// Decode raw `calldata` against a JSON ABI `Function` value (as carried in a
/// declarative bundle's `abi_fragment.abi`) and convert the result to the new
/// `decoder::DecodedCall` shape ready for `Mapper::map`.
///
/// `abi_json` is the raw JSON value taken from `AdapterFunctionBundle.abi_fragment.abi`
/// (alloy parses it via `alloy_json_abi::Function::deserialize`). `selector` is
/// the 4-byte selector taken from the first 4 bytes of `calldata`, used to
/// look up a known `decoder_id`. When the selector is unknown the function
/// returns a synthetic `fallback/0x<selector>` decoder id — callers that
/// dispatch declarative bundles by their own `(chain, to, selector)` bridge
/// can override the resulting `decoder_id` with the canonical declarative
/// one.
///
/// Used by the WASM-side `ChildResolver` impl to decode each inner
/// `multicall(bytes[])` sub-call against the bundle the parent bridge resolved.
///
/// # Errors
///
/// Returns [`DecodeWithJsonAbiError`] when the JSON ABI is malformed, the
/// calldata is malformed (short, selector mismatch, ABI decode failure), or
/// the legacy-to-new conversion fails.
pub fn decode_with_json_abi(
    abi_json: &serde_json::Value,
    calldata: &[u8],
) -> Result<DecodedCall, DecodeWithJsonAbiError> {
    // `alloy_json_abi::Function` requires `outputs`. Declarative bundle
    // `abi_fragment.abi` payloads (as shipped in `registry/manifests/`) omit
    // it because outputs are irrelevant to calldata decoding. Inject a
    // default empty `outputs` and `stateMutability` so the deserialiser
    // accepts the payload as-is.
    let mut patched = abi_json.clone();
    if let serde_json::Value::Object(ref mut obj) = patched {
        obj.entry("outputs")
            .or_insert_with(|| serde_json::Value::Array(vec![]));
        obj.entry("stateMutability")
            .or_insert_with(|| serde_json::Value::String("nonpayable".into()));
    }
    let function: alloy_json_abi::Function = serde_json::from_value(patched)
        .map_err(|error| DecodeWithJsonAbiError::InvalidAbi(error.to_string()))?;
    let legacy = crate::decode::decode_with_function(&function, calldata)
        .map_err(|error| DecodeWithJsonAbiError::Decode(error.to_string()))?;

    if calldata.len() < 4 {
        return Err(DecodeWithJsonAbiError::Decode(
            "calldata too short for selector".into(),
        ));
    }
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&calldata[..4]);

    convert_legacy_call(legacy, selector)
        .map_err(|error| DecodeWithJsonAbiError::Convert(error.to_string()))
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeWithJsonAbiError {
    #[error("invalid abi json: {0}")]
    InvalidAbi(String),
    #[error("calldata decode failed: {0}")]
    Decode(String),
    #[error("bridge conversion failed: {0}")]
    Convert(String),
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("address format: {0}")]
    AddressFormat(String),
    #[error("unsupported value: {0}")]
    UnsupportedValue(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;

    #[test]
    fn test_decoder_id_for_known_selectors() {
        assert_eq!(
            decoder_id_for_selector(SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR),
            Some(SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID)
        );
        assert_eq!(
            decoder_id_for_selector(APPROVE_SELECTOR),
            Some(ERC20_APPROVE_DECODER_ID)
        );
    }

    #[test]
    fn test_decoder_id_for_unknown_selector_is_none() {
        assert_eq!(decoder_id_for_selector([0x00, 0x00, 0x00, 0x00]), None);
    }

    #[test]
    fn test_fallback_decoder_id_format() {
        let id = fallback_decoder_id([0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(id.as_str(), "fallback/0xdeadbeef");
    }

    #[test]
    fn test_convert_value_primitives() {
        let v = convert_value(DynSolValue::Uint(U256::from(42u64), 256)).unwrap();
        assert!(matches!(v, DecodedValue::Uint(u) if u == U256::from(42u64)));

        let v = convert_value(DynSolValue::Bool(true)).unwrap();
        assert!(matches!(v, DecodedValue::Bool(true)));

        let v = convert_value(DynSolValue::String("hello".into())).unwrap();
        assert!(matches!(v, DecodedValue::String(s) if s == "hello"));
    }

    #[test]
    fn test_convert_value_array() {
        let v = convert_value(DynSolValue::Array(vec![
            DynSolValue::Uint(U256::from(1u64), 256),
            DynSolValue::Uint(U256::from(2u64), 256),
        ]))
        .unwrap();
        let DecodedValue::Array(items) = v else {
            panic!("expected array");
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_convert_legacy_call_uses_decoder_id_lookup() {
        let legacy = LegacyCall {
            function_name: "approve".into(),
            signature: "approve(address,uint256)".into(),
            args: vec![],
        };
        let converted = convert_legacy_call(legacy, APPROVE_SELECTOR).unwrap();
        assert_eq!(converted.decoder_id.as_str(), ERC20_APPROVE_DECODER_ID);
    }

    #[test]
    fn test_convert_legacy_call_unknown_uses_fallback_id() {
        let legacy = LegacyCall {
            function_name: "foo".into(),
            signature: "foo()".into(),
            args: vec![],
        };
        let converted = convert_legacy_call(legacy, [0xde, 0xad, 0xbe, 0xef]).unwrap();
        assert_eq!(converted.decoder_id.as_str(), "fallback/0xdeadbeef");
    }

    #[test]
    fn canonical_abi_type_rebuilds_nested_tuple() {
        // Bare `"tuple"` + nested `"tuple[]"` components → the full
        // parenthesised canonical string. Drives the b1-infra width fix.
        let abi = serde_json::json!({
            "name": "permit", "type": "function",
            "outputs": [], "stateMutability": "nonpayable",
            "inputs": [
                { "name": "owner", "type": "address" },
                { "name": "permitBatch", "type": "tuple", "components": [
                    { "name": "details", "type": "tuple[]", "components": [
                        { "name": "token", "type": "address" },
                        { "name": "amount", "type": "uint160" },
                        { "name": "expiration", "type": "uint48" },
                        { "name": "nonce", "type": "uint48" }
                    ]},
                    { "name": "spender", "type": "address" },
                    { "name": "sigDeadline", "type": "uint256" }
                ]},
                { "name": "signature", "type": "bytes" }
            ]
        });
        let function: alloy_json_abi::Function = serde_json::from_value(abi).unwrap();
        let owner = &function.inputs[0];
        let permit_batch = &function.inputs[1];
        let details = &permit_batch.components[0];

        // Scalar passthrough.
        assert_eq!(canonical_abi_type(&owner.ty, &owner.components), "address");
        // Nested tuple[] keeps its inner widths + array suffix.
        assert_eq!(
            canonical_abi_type(&details.ty, &details.components),
            "(address,uint160,uint48,uint48)[]"
        );
        // Top-level tuple recurses into the nested array.
        assert_eq!(
            canonical_abi_type(&permit_batch.ty, &permit_batch.components),
            "((address,uint160,uint48,uint48)[],address,uint256)"
        );
    }

    #[test]
    fn decode_with_json_abi_threads_nested_tuple_canonical_type() {
        // End-to-end through the bridge: a calldata `permit(owner, PermitBatch,
        // signature)` with a nested `PermitDetails[]` — assert the decoded
        // `permitBatch` arg carries the CANONICAL parenthesised `abi_type`
        // (rebuilt from `components`) rather than the bare `"tuple"` alloy
        // emits. The downstream `eval::args_to_json` (covered in the mappers
        // crate + the v3-route integration test) relies on this string to
        // render the nested uint48 `expiration` as a JSON number.
        //
        // The arg's VALUE is also verified to be a correctly-shaped nested
        // Tuple/Array so the positional `$inputs[2]` walk lands on the uint48.
        use alloy_dyn_abi::DynSolValue;
        use alloy_primitives::Address as AlloyAddress;

        let abi = serde_json::json!({
            "name": "permit", "type": "function",
            "inputs": [
                { "name": "owner", "type": "address" },
                { "name": "permitBatch", "type": "tuple", "components": [
                    { "name": "details", "type": "tuple[]", "components": [
                        { "name": "token", "type": "address" },
                        { "name": "amount", "type": "uint160" },
                        { "name": "expiration", "type": "uint48" },
                        { "name": "nonce", "type": "uint48" }
                    ]},
                    { "name": "spender", "type": "address" },
                    { "name": "sigDeadline", "type": "uint256" }
                ]},
                { "name": "signature", "type": "bytes" }
            ]
        });

        let token = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
            .parse::<AlloyAddress>()
            .unwrap();
        let spender = "0x00000000000000000000000000000000deadbeef"
            .parse::<AlloyAddress>()
            .unwrap();
        let owner = "0x000000000000000000000000000000000000a01c"
            .parse::<AlloyAddress>()
            .unwrap();
        let details = DynSolValue::Tuple(vec![
            DynSolValue::Address(token),
            DynSolValue::Uint(U256::from(1000u64), 160),
            DynSolValue::Uint(U256::from(1_738_001_800u64), 48),
            DynSolValue::Uint(U256::from(0u64), 48),
        ]);
        let permit_batch = DynSolValue::Tuple(vec![
            DynSolValue::Array(vec![details]),
            DynSolValue::Address(spender),
            DynSolValue::Uint(U256::from(1_738_002_000u64), 256),
        ]);
        // Structural ABI encoding (no Function-type coercion) — mirrors the
        // route test's `encode_calldata`: selector ++ abi_encode_params over a
        // top-level Tuple of the three args. The `permit` selector is
        // `0x2a2d80d1`.
        let body = DynSolValue::Tuple(vec![
            DynSolValue::Address(owner),
            permit_batch,
            DynSolValue::Bytes(vec![0xab, 0xcd]),
        ])
        .abi_encode_params();
        let mut calldata = vec![0x2a, 0x2d, 0x80, 0xd1];
        calldata.extend_from_slice(&body);

        let decoded = decode_with_json_abi(&abi, &calldata).unwrap();
        let pb = decoded
            .args
            .iter()
            .find(|a| a.name == "permitBatch")
            .expect("permitBatch arg");
        // The load-bearing assertion: canonical parenthesised type, NOT "tuple".
        assert_eq!(
            pb.abi_type,
            "((address,uint160,uint48,uint48)[],address,uint256)"
        );

        // Value shape: permitBatch = Tuple[ Array[ Tuple[token, amount, exp, nonce] ], spender, sigDeadline ].
        let DecodedValue::Tuple(pb_fields) = &pb.value else {
            panic!("permitBatch must be a Tuple, got {:?}", pb.value);
        };
        let DecodedValue::Array(details_arr) = &pb_fields[0] else {
            panic!("details must be an Array, got {:?}", pb_fields[0]);
        };
        let DecodedValue::Tuple(d0) = &details_arr[0] else {
            panic!("details[0] must be a Tuple, got {:?}", details_arr[0]);
        };
        assert!(
            matches!(&d0[2], DecodedValue::Uint(v) if *v == U256::from(1_738_001_800u64)),
            "details[0][2] (uint48 expiration) value mismatch: {:?}",
            d0[2]
        );
    }

    #[test]
    fn decode_with_json_abi_decodes_approve_calldata() {
        // approve(address,uint256) — JSON ABI form mirroring an
        // `abi_fragment.abi` payload in a declarative bundle.
        let abi = serde_json::json!({
            "name": "approve",
            "type": "function",
            "inputs": [
                { "name": "spender", "type": "address" },
                { "name": "amount",  "type": "uint256" }
            ]
        });

        // selector for approve(address,uint256) = 0x095ea7b3
        let mut calldata = vec![0x09, 0x5e, 0xa7, 0xb3];
        let mut spender_word = [0u8; 32];
        spender_word[12..].copy_from_slice(&[0x11; 20]);
        calldata.extend_from_slice(&spender_word);
        let amount_bytes: [u8; 32] = U256::from(42u64).to_be_bytes();
        calldata.extend_from_slice(&amount_bytes);

        let decoded = decode_with_json_abi(&abi, &calldata).unwrap();
        assert_eq!(decoded.decoder_id.as_str(), ERC20_APPROVE_DECODER_ID);
        assert_eq!(decoded.args.len(), 2);
        assert_eq!(decoded.args[0].name, "spender");
        assert_eq!(decoded.args[1].name, "amount");
    }

    #[test]
    fn decode_with_json_abi_surfaces_invalid_abi_error() {
        // Plain string is not an object → cannot be deserialised as a
        // `Function`. The helper auto-injects `outputs`/`stateMutability`
        // when the value IS an object (matching real bundle payloads), but
        // a non-object value still surfaces an InvalidAbi error.
        let abi = serde_json::Value::String("not an abi".into());
        let calldata = vec![0xde, 0xad, 0xbe, 0xef];
        let err = decode_with_json_abi(&abi, &calldata).unwrap_err();
        assert!(
            matches!(err, DecodeWithJsonAbiError::InvalidAbi(_)),
            "expected InvalidAbi, got {err:?}"
        );
    }

    #[test]
    fn decode_with_json_abi_accepts_bundle_shaped_abi_without_outputs() {
        // Real bundle abi_fragment.abi payloads (registry/manifests/*) omit
        // `outputs` and `stateMutability`. The helper must auto-inject defaults
        // and still decode the calldata.
        let abi = serde_json::json!({
            "name": "burn",
            "type": "function",
            "inputs": [
                { "name": "tokenId", "type": "uint256" }
            ]
        });
        // selector for burn(uint256) = 0x42966c68
        let mut calldata = vec![0x42, 0x96, 0x6c, 0x68];
        let token_id: [u8; 32] = U256::from(42u64).to_be_bytes();
        calldata.extend_from_slice(&token_id);

        let decoded = decode_with_json_abi(&abi, &calldata).unwrap();
        assert_eq!(decoded.args.len(), 1);
        assert_eq!(decoded.args[0].name, "tokenId");
    }

    #[test]
    fn decode_with_json_abi_surfaces_selector_mismatch() {
        let abi = serde_json::json!({
            "name": "approve",
            "type": "function",
            "inputs": [
                { "name": "spender", "type": "address" },
                { "name": "amount",  "type": "uint256" }
            ]
        });
        // Selector 0xdeadbeef does not match approve's 0x095ea7b3.
        let mut calldata = vec![0xde, 0xad, 0xbe, 0xef];
        calldata.extend_from_slice(&[0u8; 64]);
        let err = decode_with_json_abi(&abi, &calldata).unwrap_err();
        assert!(
            matches!(err, DecodeWithJsonAbiError::Decode(_)),
            "expected Decode, got {err:?}"
        );
    }
}
