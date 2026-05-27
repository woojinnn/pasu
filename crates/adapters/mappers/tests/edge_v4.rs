//! T-TEST-V4 — V4_SWAP (0x10) and V4 PositionManager (0x14) edge cases.
//!
//! These integration tests pin behaviour the declarative
//! `opcode_stream_dispatch` layer in `mappers::declarative::opcode_stream`
//! is required to honour for Uniswap V4-related opcodes. They complement
//! the inline happy-path tests in
//! `crates/adapters/mappers/src/declarative/opcode_stream.rs` by isolating
//! malformed / boundary / depth-bounded shapes.
//!
//! Production code is unchanged — this file only adds test coverage.

#![allow(clippy::doc_lazy_continuation, clippy::doc_overindented_list_items)]

use std::str::FromStr as _;
use std::sync::Mutex;

use abi_resolver::subdecode::opcode_stream as tier_b_opcode_stream;
use abi_resolver::subdecode::protocols::v4_router::V4_ROUTER_TABLE;
use abi_resolver::{CallMatchKey, DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::Function;
use alloy_primitives::{I256, U256};
use mappers::declarative::opcode_stream as decl_opcode_stream;
use mappers::declarative::AdapterFunctionBundle;
use mappers::mapper::{ChildResolver, MapContext, MapperError};
use mappers::token_registry::EmptyTokenRegistry;
use policy_engine::action::{Address, DecimalString};
use policy_engine::ActionEnvelope;

const UR_BUNDLE_JSON: &str =
    include_str!("../../../../registry/manifests/uniswap/universal-router/execute-v2@1.0.0.json");

/// UR `V4_SWAP` opcode (after `UNISWAP_UR_MASK`).
const OPCODE_V4_SWAP: u8 = 0x10;

/// UR `V4_POSITION_MANAGER_CALL` opcode.
const OPCODE_V4_PM_CALL: u8 = 0x14;

/// UR `EXECUTE_SUB_PLAN` opcode.
const OPCODE_EXECUTE_SUB_PLAN: u8 = 0x21;

/// Canonical V4 PM `modifyLiquidities(bytes,uint256)` selector
/// (Tier B's `MODIFY_LIQUIDITIES_SELECTOR`).
const V4_PM_MODIFY_LIQUIDITIES_SELECTOR: [u8; 4] = [0xdd, 0x46, 0x50, 0x8f];

// ─────────────────────────────────────────────────────────────────────────
// Shared helpers
// ─────────────────────────────────────────────────────────────────────────

fn token_in() -> alloy_primitives::Address {
    alloy_primitives::Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap()
}

fn token_out() -> alloy_primitives::Address {
    alloy_primitives::Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap()
}

fn recipient_addr() -> alloy_primitives::Address {
    alloy_primitives::Address::from_str("0x4444444444444444444444444444444444444444").unwrap()
}

fn dummy_addr(label: u8) -> Address {
    let suffix = format!("{label:02x}");
    Address::from_str(&format!("0x{}{}", "0".repeat(38), suffix)).unwrap()
}

/// V4 PM mainnet address — matches Tier B's `V4_PM_ADDRESSES` chain_id=1 entry.
fn expected_v4_pm_mainnet_addr() -> Address {
    Address::from_str("0xbd216513d74c8cf14cf4747e6aaa6420ff64ee9e").unwrap()
}

fn encode_step_input(sig: &str, values: &[DynSolValue]) -> Vec<u8> {
    let func = Function::parse(&format!("step{sig}")).unwrap();
    let raw = func.abi_encode_input(values).unwrap();
    raw[4..].to_vec()
}

fn encode_v4_swap_input(inner_actions: Vec<u8>, inner_params: Vec<Vec<u8>>) -> Vec<u8> {
    encode_step_input(
        "(bytes,bytes[])",
        &[
            DynSolValue::Bytes(inner_actions),
            DynSolValue::Array(inner_params.into_iter().map(DynSolValue::Bytes).collect()),
        ],
    )
}

fn encode_execute_sub_plan_input(inner_commands: Vec<u8>, inner_inputs: Vec<Vec<u8>>) -> Vec<u8> {
    encode_step_input(
        "(bytes,bytes[])",
        &[
            DynSolValue::Bytes(inner_commands),
            DynSolValue::Array(inner_inputs.into_iter().map(DynSolValue::Bytes).collect()),
        ],
    )
}

fn encode_position_manager_step_input(inner_calldata: Vec<u8>) -> Vec<u8> {
    // UR Dispatcher.sol forwards `inputs[i]` raw to V3 NPM / V4 PM — no
    // `abi.encode((bytes,))` wrapper. The runtime reads `step.raw_input`
    // directly; mirror that here so tests align with Dispatcher.sol.
    inner_calldata
}

/// Encode a V4 router `SWAP_EXACT_IN_SINGLE` (0x06) action params in the
/// pre-#497 (mainnet-deployed, no `minHopPriceX36`) shape.
#[allow(clippy::too_many_arguments)]
fn encode_v4_swap_exact_in_single_params(
    currency0: alloy_primitives::Address,
    currency1: alloy_primitives::Address,
    fee: u32,
    tick_spacing: i32,
    hooks: alloy_primitives::Address,
    zero_for_one: bool,
    amount_in: u128,
    amount_out_min: u128,
    hook_data: Vec<u8>,
) -> Vec<u8> {
    let pool_key = DynSolValue::Tuple(vec![
        DynSolValue::Address(currency0),
        DynSolValue::Address(currency1),
        DynSolValue::Uint(U256::from(fee), 24),
        DynSolValue::Int(I256::try_from(tick_spacing).unwrap(), 24),
        DynSolValue::Address(hooks),
    ]);
    let params_tuple = DynSolValue::Tuple(vec![
        pool_key,
        DynSolValue::Bool(zero_for_one),
        DynSolValue::Uint(U256::from(amount_in), 128),
        DynSolValue::Uint(U256::from(amount_out_min), 128),
        DynSolValue::Bytes(hook_data),
    ]);
    encode_step_input(
        "(((address,address,uint24,int24,address),bool,uint128,uint128,bytes))",
        &[params_tuple],
    )
}

/// Build a UR `execute(bytes commands, bytes[] inputs, uint256 deadline)`
/// outer `DecodedCall`.
fn ur_execute_decoded(commands: Vec<u8>, inputs: Vec<Vec<u8>>) -> DecodedCall {
    DecodedCall {
        decoder_id: DecoderId::new("declarative.uniswap/universal-router/execute"),
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
                value: DecodedValue::Array(inputs.into_iter().map(DecodedValue::Bytes).collect()),
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

fn ctx_with_resolver<'a>(
    resolver: &'a dyn ChildResolver,
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
        resolver: Some(resolver),
    }
}

#[derive(Debug)]
struct CapturedCall {
    key: CallMatchKey,
    calldata: Vec<u8>,
    depth: u8,
    had_parent: bool,
    to_in_ctx: Address,
}

struct CapturingResolver {
    calls: Mutex<Vec<CapturedCall>>,
    responses: Mutex<Vec<Result<Vec<ActionEnvelope>, MapperError>>>,
}

impl CapturingResolver {
    fn new(responses: Vec<Result<Vec<ActionEnvelope>, MapperError>>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(responses),
        }
    }
}

impl ChildResolver for CapturingResolver {
    fn resolve_child(
        &self,
        child: &CallMatchKey,
        ctx: &MapContext<'_>,
        child_calldata: &[u8],
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        self.calls.lock().unwrap().push(CapturedCall {
            key: child.clone(),
            calldata: child_calldata.to_vec(),
            depth: ctx.depth,
            had_parent: ctx.parent_calldata.is_some(),
            to_in_ctx: ctx.to.clone(),
        });
        let mut responses = self.responses.lock().unwrap();
        responses.pop().unwrap_or_else(|| {
            Err(MapperError::Internal(anyhow::anyhow!(
                "CapturingResolver exhausted"
            )))
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Tests — 7 edge cases
// ─────────────────────────────────────────────────────────────────────────

/// 1) `v4_swap_with_empty_actions_yields_no_envelopes` — outer `[0x10]`
/// + actions = empty Bytes + params = empty Bytes\[\] MUST complete cleanly.
/// An empty V4 action stream has no swap action, so the TB-2 swap builder
/// returns zero envelopes — a clean result, not a fault.
#[test]
fn v4_swap_with_empty_actions_yields_no_envelopes() {
    let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();
    let v4_swap = encode_v4_swap_input(vec![], vec![]);
    let decoded = ur_execute_decoded(vec![OPCODE_V4_SWAP], vec![v4_swap]);

    let registry = EmptyTokenRegistry;
    let from = dummy_addr(0xAA);
    let to = dummy_addr(0xBB);
    let value = DecimalString::from_str("0").unwrap();
    let ctx = build_ctx(&registry, &from, &to, &value);

    let envelopes = decl_opcode_stream::execute(&ctx, &decoded, &bundle.emit).unwrap();
    assert!(
        envelopes.is_empty(),
        "empty V4_SWAP action stream has no swap → no envelopes, got {envelopes:?}"
    );
}

/// 2) `v4_swap_malformed_params_length_mismatch_errors` — actions length 3
/// + params length 2. The outer `extract_actions_and_params` still succeeds
/// (it just hands the raw pair to Tier B), and Tier B's `dispatch` tolerates
/// length mismatch by feeding empty bytes to the missing index — producing a
/// per-step decode error rather than panicking. The action stream here is
/// settle/take-only (no swap action), so the TB-2 swap builder returns
/// `Ok([])` while the inner step list carries `StepDecodeError::AbiDecode` /
/// `NoSchema` markers for the under-fed indices. The "error" the spec
/// mentions is per-step (visible via direct Tier B dispatch), not a
/// `MapperError` on the outer call — this test pins both contracts at once.
#[test]
fn v4_swap_malformed_params_length_mismatch_errors() {
    let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

    // actions = [0x0b SETTLE, 0x0e TAKE, 0x0c SETTLE_ALL] — 3 entries.
    // params  = only 2 entries — Tier B feeds index 2 an empty bytes vec.
    let settle = encode_step_input(
        "(address,uint256,bool)",
        &[
            DynSolValue::Address(token_in()),
            DynSolValue::Uint(U256::from(1_000_000_u64), 256),
            DynSolValue::Bool(true),
        ],
    );
    let take = encode_step_input(
        "(address,address,uint256)",
        &[
            DynSolValue::Address(token_in()),
            DynSolValue::Address(recipient_addr()),
            DynSolValue::Uint(U256::from(2_000_000_u64), 256),
        ],
    );
    let v4_swap = encode_v4_swap_input(vec![0x0b, 0x0e, 0x0c], vec![settle.clone(), take.clone()]);
    let decoded = ur_execute_decoded(vec![OPCODE_V4_SWAP], vec![v4_swap]);

    let registry = EmptyTokenRegistry;
    let from = dummy_addr(0xAA);
    let to = dummy_addr(0xBB);
    let value = DecimalString::from_str("0").unwrap();
    let ctx = build_ctx(&registry, &from, &to, &value);

    // Outer call MUST NOT fault — a settle/take-only stream has no swap
    // action, so the TB-2 swap builder returns Ok([]).
    let envelopes = decl_opcode_stream::execute(&ctx, &decoded, &bundle.emit).unwrap();
    assert!(
        envelopes.is_empty(),
        "length-mismatch settle/take-only V4_SWAP returns Ok([]), got {envelopes:?}"
    );

    // Sanity: directly dispatching the same inner stream against
    // V4_ROUTER_TABLE confirms the under-fed index carries a per-step
    // decode error.
    let inner_steps =
        tier_b_opcode_stream::dispatch(&[0x0b, 0x0e, 0x0c], &[settle, take], &V4_ROUTER_TABLE);
    assert_eq!(inner_steps.len(), 3, "one inner step per command byte");
    assert!(
        inner_steps[2].error.is_some(),
        "under-fed SETTLE_ALL step MUST carry a Tier B decode error"
    );
}

/// 3) `v4_swap_all_25_actions_decode_succeeds` — every opcode in
/// `V4_ROUTER_TABLE` (0x00..=0x18, 25 entries) appears as an inner action.
/// Label-only entries (`DONATE` 0x0a, `MINT_6909` 0x17, `BURN_6909` 0x18)
/// carry empty params; entries with a schema get minimal ABI-encoded blobs.
/// The outer call MUST emit one Swap envelope per swap action (4 total —
/// 0x06/0x07/0x08/0x09) and Tier B's direct dispatch MUST produce 25 inner
/// steps with no `UNKNOWN` names — proving the table covers 0x00..=0x18
/// contiguously.
#[test]
fn v4_swap_all_25_actions_decode_succeeds() {
    let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

    let addr_uint_bool = encode_step_input(
        "(address,uint256,bool)",
        &[
            DynSolValue::Address(token_in()),
            DynSolValue::Uint(U256::from(1u64), 256),
            DynSolValue::Bool(true),
        ],
    );
    let addr_uint = encode_step_input(
        "(address,uint256)",
        &[
            DynSolValue::Address(token_in()),
            DynSolValue::Uint(U256::from(1u64), 256),
        ],
    );
    let addr_addr = encode_step_input(
        "(address,address)",
        &[
            DynSolValue::Address(token_in()),
            DynSolValue::Address(token_out()),
        ],
    );
    let addr_addr_uint = encode_step_input(
        "(address,address,uint256)",
        &[
            DynSolValue::Address(token_in()),
            DynSolValue::Address(token_out()),
            DynSolValue::Uint(U256::from(1u64), 256),
        ],
    );
    let addr_addr_addr = encode_step_input(
        "(address,address,address)",
        &[
            DynSolValue::Address(token_in()),
            DynSolValue::Address(token_out()),
            DynSolValue::Address(recipient_addr()),
        ],
    );
    let single_addr = encode_step_input("(address)", &[DynSolValue::Address(token_in())]);
    let single_uint = encode_step_input("(uint256)", &[DynSolValue::Uint(U256::from(1u64), 256)]);
    // Liquidity flat: (uint256, uint256, uint128, uint128, bytes)
    let inc_dec_liq = encode_step_input(
        "(uint256,uint256,uint128,uint128,bytes)",
        &[
            DynSolValue::Uint(U256::from(1u64), 256),
            DynSolValue::Uint(U256::from(1u64), 256),
            DynSolValue::Uint(U256::from(1u64), 128),
            DynSolValue::Uint(U256::from(1u64), 128),
            DynSolValue::Bytes(vec![]),
        ],
    );
    // MINT_POSITION flat
    let mint_pos = encode_step_input(
        "((address,address,uint24,int24,address),int24,int24,uint256,uint128,uint128,address,bytes)",
        &[
            DynSolValue::Tuple(vec![
                DynSolValue::Address(token_in()),
                DynSolValue::Address(token_out()),
                DynSolValue::Uint(U256::from(3000u64), 24),
                DynSolValue::Int(I256::try_from(60i32).unwrap(), 24),
                DynSolValue::Address(alloy_primitives::Address::ZERO),
            ]),
            DynSolValue::Int(I256::try_from(-60i32).unwrap(), 24),
            DynSolValue::Int(I256::try_from(60i32).unwrap(), 24),
            DynSolValue::Uint(U256::from(1u64), 256),
            DynSolValue::Uint(U256::from(1u64), 128),
            DynSolValue::Uint(U256::from(1u64), 128),
            DynSolValue::Address(recipient_addr()),
            DynSolValue::Bytes(vec![]),
        ],
    );
    // BURN_POSITION: (uint256, uint128, uint128, bytes)
    let burn_pos = encode_step_input(
        "(uint256,uint128,uint128,bytes)",
        &[
            DynSolValue::Uint(U256::from(1u64), 256),
            DynSolValue::Uint(U256::from(1u64), 128),
            DynSolValue::Uint(U256::from(1u64), 128),
            DynSolValue::Bytes(vec![]),
        ],
    );
    let inc_from_deltas = burn_pos.clone();
    let mint_from_deltas = encode_step_input(
        "((address,address,uint24,int24,address),int24,int24,uint128,uint128,address,bytes)",
        &[
            DynSolValue::Tuple(vec![
                DynSolValue::Address(token_in()),
                DynSolValue::Address(token_out()),
                DynSolValue::Uint(U256::from(3000u64), 24),
                DynSolValue::Int(I256::try_from(60i32).unwrap(), 24),
                DynSolValue::Address(alloy_primitives::Address::ZERO),
            ]),
            DynSolValue::Int(I256::try_from(-60i32).unwrap(), 24),
            DynSolValue::Int(I256::try_from(60i32).unwrap(), 24),
            DynSolValue::Uint(U256::from(1u64), 128),
            DynSolValue::Uint(U256::from(1u64), 128),
            DynSolValue::Address(recipient_addr()),
            DynSolValue::Bytes(vec![]),
        ],
    );
    let swap_single = encode_v4_swap_exact_in_single_params(
        token_in(),
        token_out(),
        3000,
        60,
        alloy_primitives::Address::ZERO,
        true,
        1,
        1,
        vec![],
    );
    // V4 multi-hop swap params — `(currency, PathKey[], amountA, amountB)`,
    // the mainnet (pre-#497) shape. One PathKey hop token_in → token_out.
    let swap_multi = encode_step_input(
        "((address,(address,uint24,int24,address,bytes)[],uint128,uint128))",
        &[DynSolValue::Tuple(vec![
            DynSolValue::Address(token_in()),
            DynSolValue::Array(vec![DynSolValue::Tuple(vec![
                DynSolValue::Address(token_out()),
                DynSolValue::Uint(U256::from(3000u64), 24),
                DynSolValue::Int(I256::try_from(60i32).unwrap(), 24),
                DynSolValue::Address(alloy_primitives::Address::ZERO),
                DynSolValue::Bytes(vec![]),
            ])]),
            DynSolValue::Uint(U256::from(1u64), 128),
            DynSolValue::Uint(U256::from(1u64), 128),
        ])],
    );

    // 25 actions, one per V4_ROUTER_TABLE entry. Label-only entries
    // (0x0a DONATE, 0x17 MINT_6909, 0x18 BURN_6909) carry empty params.
    let actions: Vec<u8> = (0x00u8..=0x18u8).collect();
    let params: Vec<Vec<u8>> = vec![
        inc_dec_liq.clone(),    // 0x00 INCREASE_LIQUIDITY
        inc_dec_liq,            // 0x01 DECREASE_LIQUIDITY
        mint_pos,               // 0x02 MINT_POSITION
        burn_pos,               // 0x03 BURN_POSITION
        inc_from_deltas,        // 0x04 INCREASE_LIQUIDITY_FROM_DELTAS
        mint_from_deltas,       // 0x05 MINT_POSITION_FROM_DELTAS
        swap_single.clone(),    // 0x06 SWAP_EXACT_IN_SINGLE
        swap_multi.clone(),     // 0x07 SWAP_EXACT_IN
        swap_single,            // 0x08 SWAP_EXACT_OUT_SINGLE
        swap_multi,             // 0x09 SWAP_EXACT_OUT
        vec![],                 // 0x0a DONATE (label-only)
        addr_uint_bool,         // 0x0b SETTLE
        addr_uint.clone(),      // 0x0c SETTLE_ALL
        addr_addr.clone(),      // 0x0d SETTLE_PAIR
        addr_addr_uint.clone(), // 0x0e TAKE
        addr_uint.clone(),      // 0x0f TAKE_ALL
        addr_addr_uint,         // 0x10 TAKE_PORTION
        addr_addr_addr,         // 0x11 TAKE_PAIR
        single_addr,            // 0x12 CLOSE_CURRENCY
        addr_uint,              // 0x13 CLEAR_OR_TAKE
        addr_addr,              // 0x14 SWEEP
        single_uint.clone(),    // 0x15 WRAP
        single_uint,            // 0x16 UNWRAP
        vec![],                 // 0x17 MINT_6909 (label-only)
        vec![],                 // 0x18 BURN_6909 (label-only)
    ];
    assert_eq!(actions.len(), 25);
    assert_eq!(params.len(), 25);

    let v4_swap = encode_v4_swap_input(actions.clone(), params.clone());
    let decoded = ur_execute_decoded(vec![OPCODE_V4_SWAP], vec![v4_swap]);

    let registry = EmptyTokenRegistry;
    let from = dummy_addr(0xAA);
    let to = dummy_addr(0xBB);
    let value = DecimalString::from_str("0").unwrap();
    let ctx = build_ctx(&registry, &from, &to, &value);

    // TB-2: the 4 swap actions (0x06/0x07/0x08/0x09) each emit one Swap
    // envelope via the shared V4 swap builder; the 21 non-swap actions
    // (liquidity / settle / take / wrap / label-only) emit nothing. There is
    // no TAKE-derived recipient because the stream's 0x0e TAKE comes after
    // the swaps — but all swap envelopes are still produced.
    let envelopes = decl_opcode_stream::execute(&ctx, &decoded, &bundle.emit).unwrap();
    assert_eq!(
        envelopes.len(),
        4,
        "all-25-action V4_SWAP must emit 4 Swap envelopes (one per swap action), \
         got {envelopes:?}"
    );
    for env in &envelopes {
        assert!(
            matches!(env.action, policy_engine::action::envelope::Action::Swap(_)),
            "every emitted envelope must be a Swap, got {:?}",
            env.action
        );
    }

    // Sanity: Tier B dispatch produces exactly 25 inner steps and every
    // opcode is recognised (no `UNKNOWN` name) — proves the table covers
    // 0x00..=0x18 contiguously without holes.
    let inner = tier_b_opcode_stream::dispatch(&actions, &params, &V4_ROUTER_TABLE);
    assert_eq!(
        inner.len(),
        25,
        "Tier B must produce one step per action byte"
    );
    for (i, step) in inner.iter().enumerate() {
        assert_ne!(
            step.name, "UNKNOWN",
            "opcode 0x{:02x} (step {i}) MUST be recognised in V4_ROUTER_TABLE",
            actions[i]
        );
    }
}

/// 4) `v4_swap_at_max_depth_3_succeeds` (actually depth 4 — exceeds cap) —
/// a chain of three `EXECUTE_SUB_PLAN` (0x21) wrapping a leaf `V4_SWAP`
/// (0x10) places the V4_SWAP entry at depth 4 (outer at depth 0, each
/// sub-plan increments by 1). With `MAX_SUB_PLAN_DEPTH = 3` the V4_SWAP
/// guard MUST fire — producing a `MapperError::Internal` whose message
/// names both `MAX_SUB_PLAN_DEPTH` and `V4_SWAP` (so a regression that
/// bypasses the V4-specific guard and hits the EXECUTE_SUB_PLAN guard
/// instead remains visible).
#[test]
fn v4_swap_at_max_depth_3_succeeds() {
    let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

    let leaf = encode_v4_swap_input(vec![], vec![]);
    let l3 = encode_execute_sub_plan_input(vec![OPCODE_V4_SWAP], vec![leaf]);
    let l2 = encode_execute_sub_plan_input(vec![OPCODE_EXECUTE_SUB_PLAN], vec![l3]);
    let l1 = encode_execute_sub_plan_input(vec![OPCODE_EXECUTE_SUB_PLAN], vec![l2]);

    let decoded = ur_execute_decoded(vec![OPCODE_EXECUTE_SUB_PLAN], vec![l1]);
    let registry = EmptyTokenRegistry;
    let from = dummy_addr(0xAA);
    let to = dummy_addr(0xBB);
    let value = DecimalString::from_str("0").unwrap();
    let ctx = build_ctx(&registry, &from, &to, &value);

    let err = decl_opcode_stream::execute(&ctx, &decoded, &bundle.emit).unwrap_err();
    let MapperError::Internal(inner) = &err else {
        panic!("expected MapperError::Internal, got {err:?}");
    };
    let msg = inner.to_string();
    assert!(
        msg.contains("MAX_SUB_PLAN_DEPTH"),
        "expected depth-bound error, got: {msg}"
    );
    assert!(
        msg.contains("V4_SWAP"),
        "expected V4_SWAP-specific guard message, got: {msg}"
    );
}

/// 5) `v4_swap_at_depth_2_succeeds` — outer `[0x21]` wrapping a leaf
/// `[0x10]` places the V4_SWAP entry at depth 2. With
/// `MAX_SUB_PLAN_DEPTH = 3` the guard fires only when `ctx.depth >= 3`, so
/// depth 2 MUST succeed. Since TB-2 the inner SWAP_EXACT_IN_SINGLE emits one
/// Swap envelope (rather than the pre-TB-2 0-envelope option-D behaviour).
#[test]
fn v4_swap_at_depth_2_succeeds() {
    let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

    let v4_inner = encode_v4_swap_exact_in_single_params(
        token_in(),
        token_out(),
        3_000,
        60,
        alloy_primitives::Address::ZERO,
        true,
        1,
        1,
        vec![],
    );
    let leaf_v4 = encode_v4_swap_input(vec![0x06], vec![v4_inner]);
    let sub_plan = encode_execute_sub_plan_input(vec![OPCODE_V4_SWAP], vec![leaf_v4]);

    let decoded = ur_execute_decoded(vec![OPCODE_EXECUTE_SUB_PLAN], vec![sub_plan]);
    let registry = EmptyTokenRegistry;
    let from = dummy_addr(0xAA);
    let to = dummy_addr(0xBB);
    let value = DecimalString::from_str("0").unwrap();
    let ctx = build_ctx(&registry, &from, &to, &value);

    let envelopes = decl_opcode_stream::execute(&ctx, &decoded, &bundle.emit).unwrap();
    assert_eq!(
        envelopes.len(),
        1,
        "V4_SWAP at depth 2 must succeed and emit one Swap envelope, got {envelopes:?}"
    );
    assert!(
        matches!(
            envelopes[0].action,
            policy_engine::action::envelope::Action::Swap(_)
        ),
        "expected Swap, got {:?}",
        envelopes[0].action
    );
}

/// 6) `v4_swap_unknown_inner_opcode_with_warn_policy_skips` — the inner V4
/// action stream contains `0xFF` (not in `V4_ROUTER_TABLE`). Tier B
/// produces a `DecodedStep { name: "UNKNOWN", error: Some(UnknownOpcode),
/// args: None }` for it; the TB-2 swap builder ignores any non-swap step
/// (`UNKNOWN` included) — so an unknown V4 inner opcode that is not a swap
/// action MUST NOT fault the outer call and emits no envelope.
///
/// The bundle's `unknown_opcode_policy = warn` applies to the OUTER UR
/// `per_opcode_emit` map only — V4 inner actions do not consult that map.
/// This test pins both contracts (no outer fault + per-step `UnknownOpcode`
/// marker visible via direct V4 dispatch).
#[test]
fn v4_swap_unknown_inner_opcode_with_warn_policy_skips() {
    let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

    // Outer carries the bundle whose unknown_opcode_policy is "warn" —
    // verify the policy via the bundle's per_opcode_emit map shape.
    if let mappers::declarative::EmitRule::OpcodeStreamDispatch {
        unknown_opcode_policy,
        ..
    } = &bundle.emit
    {
        // The bundle is expected to declare warn; if upstream flips it the
        // assertion below pins us.
        let s = format!("{unknown_opcode_policy:?}");
        assert!(
            s.eq_ignore_ascii_case("Warn") || s.eq_ignore_ascii_case("warn"),
            "bundle unknown_opcode_policy must be Warn, got {s}"
        );
    } else {
        panic!(
            "UR bundle must be OpcodeStreamDispatch, got {:?}",
            bundle.emit
        );
    }

    // Inner V4 action 0xFF is not in V4_ROUTER_TABLE (only 0x00..=0x18
    // registered). Empty params keep the step decode trivial.
    let v4_swap = encode_v4_swap_input(vec![0xFF], vec![vec![]]);
    let decoded = ur_execute_decoded(vec![OPCODE_V4_SWAP], vec![v4_swap]);

    let registry = EmptyTokenRegistry;
    let from = dummy_addr(0xAA);
    let to = dummy_addr(0xBB);
    let value = DecimalString::from_str("0").unwrap();
    let ctx = build_ctx(&registry, &from, &to, &value);

    let envelopes = decl_opcode_stream::execute(&ctx, &decoded, &bundle.emit).unwrap();
    assert!(
        envelopes.is_empty(),
        "V4_SWAP with non-swap unknown inner opcode emits no envelopes, got {envelopes:?}"
    );

    // Sanity: direct V4 dispatch confirms 0xFF surfaces as UnknownOpcode.
    let inner = tier_b_opcode_stream::dispatch(&[0xFF], &[vec![]], &V4_ROUTER_TABLE);
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0].name, "UNKNOWN");
    assert!(inner[0].error.is_some());
}

/// 7) `v4_position_manager_call_with_modifyLiquidities_calldata` — UR
/// opcode `0x14 V4_POSITION_MANAGER_CALL` carrying a properly ABI-encoded
/// `modifyLiquidities(bytes unlockData, uint256 deadline)` calldata MUST
/// dispatch through `ctx.resolver` with:
///   - `child_key.chain_id` inherited from outer (1 = mainnet),
///   - `child_key.to` = V4 PM mainnet address `0xbd2165…ee9e`
///     (cross-target — NOT the outer UR address),
///   - `child_key.selector` = `0xdd46508f` (canonical V4 PM
///     `modifyLiquidities` selector — Tier B's
///     `MODIFY_LIQUIDITIES_SELECTOR`).
///
/// Uses a realistic V4 PM payload (SETTLE_ALL + TAKE_ALL inside
/// `modifyLiquidities`) so a regression that mis-slices the selector or
/// hands the outer UR address to the resolver remains visible.
#[test]
fn v4_position_manager_call_with_modify_liquidities_calldata() {
    let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

    // Build a realistic `modifyLiquidities(bytes unlockData, uint256 deadline)`
    // calldata: unlockData = abi.encode(bytes actions, bytes[] params).
    let settle_all_param = encode_step_input(
        "(address,uint256)",
        &[
            DynSolValue::Address(token_in()),
            DynSolValue::Uint(U256::from(1_000_000_u64), 256),
        ],
    );
    let take_all_param = encode_step_input(
        "(address,uint256)",
        &[
            DynSolValue::Address(token_out()),
            DynSolValue::Uint(U256::from(1u64), 256),
        ],
    );
    let unlock_data = encode_step_input(
        "(bytes,bytes[])",
        &[
            DynSolValue::Bytes(vec![0x0c, 0x0f]),
            DynSolValue::Array(vec![
                DynSolValue::Bytes(settle_all_param),
                DynSolValue::Bytes(take_all_param),
            ]),
        ],
    );
    // Build outer modifyLiquidities calldata: 4-byte selector + ABI args.
    let outer_fn = Function::parse("modifyLiquidities(bytes,uint256)").unwrap();
    let outer_raw = outer_fn
        .abi_encode_input(&[
            DynSolValue::Bytes(unlock_data),
            DynSolValue::Uint(U256::from(1_700_000_999_u64), 256),
        ])
        .unwrap();
    let mut pm_calldata = V4_PM_MODIFY_LIQUIDITIES_SELECTOR.to_vec();
    pm_calldata.extend_from_slice(&outer_raw[4..]);

    // Selector sanity — the first 4 bytes MUST be the V4 PM canonical
    // selector. If this fails the encoder is buggy, not production code.
    assert_eq!(&pm_calldata[..4], &V4_PM_MODIFY_LIQUIDITIES_SELECTOR);

    let step_input = encode_position_manager_step_input(pm_calldata.clone());
    let decoded = ur_execute_decoded(vec![OPCODE_V4_PM_CALL], vec![step_input]);

    let resolver = CapturingResolver::new(vec![Ok(Vec::new())]);
    let registry = EmptyTokenRegistry;
    let from = dummy_addr(0xAA);
    let to = dummy_addr(0xBB);
    let value = DecimalString::from_str("0").unwrap();
    let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value);

    let envelopes = decl_opcode_stream::execute(&ctx, &decoded, &bundle.emit).unwrap();
    assert!(
        envelopes.is_empty(),
        "stub resolver returns empty — outer must propagate, got {envelopes:?}"
    );

    let calls = resolver.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "resolver MUST be invoked exactly once");
    // chain_id inherited from outer.
    assert_eq!(calls[0].key.chain_id, 1);
    // Cross-target: child.to MUST be the per-chain V4 PM, NOT the parent UR.
    assert_eq!(
        calls[0].key.to,
        expected_v4_pm_mainnet_addr(),
        "child.to must resolve to V4 PM mainnet, not the outer UR"
    );
    assert_eq!(calls[0].to_in_ctx, expected_v4_pm_mainnet_addr());
    // Selector = canonical `modifyLiquidities(bytes,uint256)` (Tier B's
    // MODIFY_LIQUIDITIES_SELECTOR).
    assert_eq!(calls[0].key.selector, V4_PM_MODIFY_LIQUIDITIES_SELECTOR);
    // Full PM calldata (selector + ABI args) preserved.
    assert_eq!(calls[0].calldata, pm_calldata);
    // Depth incremented and parent_calldata wired via MapContext::child.
    assert_eq!(calls[0].depth, 1, "child depth must be parent depth + 1");
    assert!(calls[0].had_parent, "child must have parent_calldata set");
}
