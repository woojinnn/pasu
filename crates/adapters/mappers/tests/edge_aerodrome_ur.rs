//! Aerodrome / Velodrome Universal Router edge-case integration tests
//! (Phase 3 — A1, Tier A/B integration track).
//!
//! Phase 2 wired the Tier B side: `opcode_stream.rs` learned the
//! `dispatcher_id = "aerodrome_universal_router"` dispatcher backed by
//! `AERODROME_UR_MAIN_TABLE` (`aerodrome_ur.rs`, `mask 0x3f`), and the
//! `unfold_velo_v2_path` BuiltinFn was added. Phase 3 ships the Tier A bundle
//! that drives them — `registry/manifests/aerodrome/universal-router/
//! execute@1.0.0.json` — and these tests pin the full declarative route:
//!
//!   bundle JSON → `DeclarativeMapper::map` → `opcode_stream::execute` →
//!   `AERODROME_UR_MAIN_TABLE` dispatch → per-opcode `single_emit` →
//!   `ActionEnvelope[]`.
//!
//! The bundle is `opcode_stream_dispatch`, so unlike the `single_emit` /
//! `enum_tagged` Aerodrome bundles the outer call is `execute(bytes commands,
//! bytes[] inputs, uint256 deadline)` and each `inputs[i]` is the ABI-encoded
//! argument tuple for the opcode in `commands[i]`. The encoders below mirror
//! the `opcode_stream.rs` `mod tests` calldata builders — each `inputs[i]` is
//! produced by `Function::parse(...).abi_encode_input(...)` with the synthetic
//! 4-byte selector sliced off.
//!
//! Coverage focus:
//!   * **`unfold_slipstream_path`** — V3_SWAP_EXACT_IN endpoint extraction
//!     from the packed `token ++ tickSpacing(3) ++ token` path.
//!   * **`unfold_velo_v2_path`** — V2_SWAP_EXACT_IN endpoint extraction across
//!     both the UniV2 (`token ++ token`) and VeloV2
//!     (`token ++ stable(1) ++ token`) packed layouts.
//!   * **WRAP_ETH / UNWRAP_WETH** — Base WETH `0x4200…0006` literal, the
//!     `amount` / `amountMin` arg-name divergence pinned against the Tier B
//!     `AERODROME_UR_MAIN_TABLE` signatures.
//!   * **PERMIT2_PERMIT** — nested Permit2 `PermitSingle` struct decode.
//!   * **multi-step streams** — command order preserved across opcodes.
//!   * **unmodeled opcodes** — `0x10 V4_SWAP` / `0x12 BRIDGE_TOKEN` are NOT in
//!     the bundle's `per_opcode_emit`; `unknown_opcode_policy = deny` faults
//!     the whole route so a partial envelope set never drives a verdict
//!     (AUDIT_AERODROME_UR A-01/A-02 — `warn`-skip would let the orchestrator
//!     PASS a tx with the unmodeled opcode's intent silently omitted).
//!
//! Production code is unchanged — this file only adds test coverage. Bundle
//! JSON is loaded via `include_str!` from `registry/manifests/`.

use std::str::FromStr as _;

use abi_resolver::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_dyn_abi::{DynSolValue, JsonAbiExt};
use alloy_json_abi::Function;
use alloy_primitives::U256;
use mappers::declarative::{types::AdapterFunctionBundle, DeclarativeMapper};
use mappers::mapper::{MapContext, Mapper};
use mappers::EmptyTokenRegistry;
use policy_engine::action::dex::SwapMode;
use policy_engine::action::misc::PermitKind;
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountKind, AssetKind, Category, DecimalString,
};

// ───────────────────────────────────────────────────────────────────────────
// Bundle fixture — the Aerodrome UR `execute` bundle from the registry.
// Loaded directly so the tests track whatever the registry ships.
// ───────────────────────────────────────────────────────────────────────────

const AERO_UR_EXECUTE_BUNDLE: &str =
    include_str!("../../../../registry/manifests/aerodrome/universal-router/execute@1.0.0.json");

// ───────────────────────────────────────────────────────────────────────────
// Address fixtures. The UR opcode decoders only care about byte positions, so
// distinct dummy addresses suffice — except the Base WETH literal which the
// WRAP/UNWRAP bundle entries hard-code.
// ───────────────────────────────────────────────────────────────────────────

/// Base canonical WETH — `0x4200000000000000000000000000000000000006`. The
/// WRAP_ETH / UNWRAP_WETH bundle entries hard-code this exact literal (NOT
/// mainnet WETH `0xC02a…`).
fn base_weth() -> Address {
    Address::from_str("0x4200000000000000000000000000000000000006").unwrap()
}

fn token_a() -> Address {
    Address::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01").unwrap()
}

fn token_b() -> Address {
    Address::from_str("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02").unwrap()
}

fn token_c() -> Address {
    Address::from_str("0xcccccccccccccccccccccccccccccccccccccc03").unwrap()
}

fn recipient() -> Address {
    Address::from_str("0x4444444444444444444444444444444444444444").unwrap()
}

/// Aerodrome UR `main`-lineage deployment on Base — the bundle's first `to`.
fn aero_ur_addr() -> Address {
    Address::from_str("0xc5b6786d7b64767d775877b0b6a319ad946b11b5").unwrap()
}

// ───────────────────────────────────────────────────────────────────────────
// MapContext helper — Base chain (8453), empty token registry. `from` is the
// EOA signer (PERMIT2_PERMIT binds `owner = $.tx.from`), `to` the UR address.
// ───────────────────────────────────────────────────────────────────────────

struct Ctx {
    registry: EmptyTokenRegistry,
    from: Address,
    to: Address,
    value: DecimalString,
}

impl Ctx {
    fn new() -> Self {
        Self {
            registry: EmptyTokenRegistry,
            from: Address::from_str("0x00000000000000000000000000000000000000aa").unwrap(),
            to: aero_ur_addr(),
            value: DecimalString::from_str("0").unwrap(),
        }
    }

    fn map_ctx(&self) -> MapContext<'_> {
        MapContext::new(
            8453, // Base mainnet — matches bundle.match.chain_ids[0]
            &self.from,
            &self.to,
            &self.value,
            Some(1_700_000_000),
            &self.registry,
        )
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Calldata builders — mirror `opcode_stream.rs` `mod tests`.
//
// The outer call is `execute(bytes commands, bytes[] inputs, uint256
// deadline)`. Each `inputs[i]` is the ABI-encoded argument tuple for the
// opcode in `commands[i]`, produced via `Function::parse(...)` with the
// synthetic 4-byte selector sliced off (Tier B's `dispatch` decodes
// `inputs[i]` directly against the opcode schema, no selector).
// ───────────────────────────────────────────────────────────────────────────

/// Raw 20-byte big-endian form of an `Address` (for packed-path assembly).
fn address_bytes(addr: &Address) -> [u8; 20] {
    let s = addr.to_string();
    let stripped = s.strip_prefix("0x").expect("Address is 0x-prefixed");
    let decoded = hex::decode(stripped).expect("Address hex is valid");
    let mut out = [0u8; 20];
    out.copy_from_slice(&decoded);
    out
}

/// Build the outer `execute(bytes,bytes[],uint256)` `DecodedCall` — the shape
/// the declarative mapper receives for an Aerodrome UR transaction.
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
                value: DecodedValue::Array(inputs.into_iter().map(DecodedValue::Bytes).collect()),
            },
            DecodedArg {
                name: "deadline".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(U256::from(1_700_000_900_u64)),
            },
        ],
        nested: vec![],
    }
}

/// Encode a V3 / V2 swap `inputs[i]` — the Aerodrome UR `main` 6-tuple
/// `(address recipient, uint256 amountIn/amountOut, uint256
/// amountOutMin/amountInMax, bytes path, bool payerIsUser, bool isUni)`.
/// Used for opcodes `0x00`/`0x01`/`0x08`/`0x09` (same shape per
/// `AERODROME_UR_MAIN_TABLE`).
fn encode_swap_6tuple_input(
    recipient_addr: Address,
    amount_first: u128,
    amount_second: u128,
    path: Vec<u8>,
    payer_is_user: bool,
    is_uni: bool,
) -> Vec<u8> {
    let func = Function::parse("step(address,uint256,uint256,bytes,bool,bool)").unwrap();
    let values = vec![
        DynSolValue::Address(alloy_primitives::Address::from(address_bytes(
            &recipient_addr,
        ))),
        DynSolValue::Uint(U256::from(amount_first), 256),
        DynSolValue::Uint(U256::from(amount_second), 256),
        DynSolValue::Bytes(path),
        DynSolValue::Bool(payer_is_user),
        DynSolValue::Bool(is_uni),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

/// Encode WRAP_ETH (`0x0b`) input `(address recipient, uint256 amount)`.
/// Note the arg name is `amount` per `AERODROME_UR_MAIN_TABLE` — distinct
/// from Uniswap UR's `amountMin`.
fn encode_wrap_eth_input(recipient_addr: Address, amount: u128) -> Vec<u8> {
    let func = Function::parse("step(address,uint256)").unwrap();
    let values = vec![
        DynSolValue::Address(alloy_primitives::Address::from(address_bytes(
            &recipient_addr,
        ))),
        DynSolValue::Uint(U256::from(amount), 256),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

/// Encode UNWRAP_WETH (`0x0c`) input `(address recipient, uint256
/// amountMin)`. Same wire shape as WRAP_ETH but the arg name is `amountMin`.
fn encode_unwrap_weth_input(recipient_addr: Address, amount_min: u128) -> Vec<u8> {
    // Identical encoding to WRAP_ETH — only the Tier B arg name differs, and
    // the bundle's `0x0c` rule sources `$.args.amountMin`.
    encode_wrap_eth_input(recipient_addr, amount_min)
}

/// Encode PERMIT2_PERMIT (`0x0a`) input — the nested Permit2 `PermitSingle`
/// struct `(((address,uint160,uint48,uint48) details, address spender,
/// uint256 sigDeadline) permitSingle, bytes signature)`.
fn encode_permit2_permit_input(
    token: Address,
    amount: u128,
    expiration: u64,
    nonce: u64,
    spender: Address,
    sig_deadline: u64,
    signature: Vec<u8>,
) -> Vec<u8> {
    let details = DynSolValue::Tuple(vec![
        DynSolValue::Address(alloy_primitives::Address::from(address_bytes(&token))),
        DynSolValue::Uint(U256::from(amount), 160),
        DynSolValue::Uint(U256::from(expiration), 48),
        DynSolValue::Uint(U256::from(nonce), 48),
    ]);
    let permit_single = DynSolValue::Tuple(vec![
        details,
        DynSolValue::Address(alloy_primitives::Address::from(address_bytes(&spender))),
        DynSolValue::Uint(U256::from(sig_deadline), 256),
    ]);
    let func =
        Function::parse("step(((address,uint160,uint48,uint48),address,uint256),bytes)").unwrap();
    let values = vec![permit_single, DynSolValue::Bytes(signature)];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

/// Encode a V4_SWAP (`0x10`) input `(bytes actions, bytes[] params)` — used
/// purely to exercise the warn-opcode path (the bundle omits `0x10`).
fn encode_v4_swap_input(actions: Vec<u8>, params: Vec<Vec<u8>>) -> Vec<u8> {
    let func = Function::parse("step(bytes,bytes[])").unwrap();
    let values = vec![
        DynSolValue::Bytes(actions),
        DynSolValue::Array(params.into_iter().map(DynSolValue::Bytes).collect()),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

/// Encode a BRIDGE_TOKEN (`0x12`) input — the 8-tuple `(uint8 bridgeType,
/// address recipient, address token, address bridge, uint256 amount, uint256
/// msgFee, uint32 domain, bool payerIsUser)`. Used to exercise the
/// warn-opcode path (the bundle omits `0x12`).
#[allow(clippy::too_many_arguments)]
fn encode_bridge_token_input(
    bridge_type: u8,
    recipient_addr: Address,
    token: Address,
    bridge: Address,
    amount: u128,
    msg_fee: u128,
    domain: u32,
    payer_is_user: bool,
) -> Vec<u8> {
    let func =
        Function::parse("step(uint8,address,address,address,uint256,uint256,uint32,bool)").unwrap();
    let values = vec![
        DynSolValue::Uint(U256::from(bridge_type), 8),
        DynSolValue::Address(alloy_primitives::Address::from(address_bytes(
            &recipient_addr,
        ))),
        DynSolValue::Address(alloy_primitives::Address::from(address_bytes(&token))),
        DynSolValue::Address(alloy_primitives::Address::from(address_bytes(&bridge))),
        DynSolValue::Uint(U256::from(amount), 256),
        DynSolValue::Uint(U256::from(msg_fee), 256),
        DynSolValue::Uint(U256::from(domain), 32),
        DynSolValue::Bool(payer_is_user),
    ];
    let raw = func.abi_encode_input(&values).unwrap();
    raw[4..].to_vec()
}

// ───────────────────────────────────────────────────────────────────────────
// Packed-path builders.
// ───────────────────────────────────────────────────────────────────────────

/// Slipstream / V3 packed path — `token(20) ++ tickSpacing(3) ++ token(20)
/// [++ tickSpacing(3) ++ token(20) …]`. `tokens.len() == tick_spacings.len()
/// + 1`. Each tickSpacing is a 3-byte big-endian `int24`.
fn build_slipstream_path(tokens: &[Address], tick_spacings: &[i32]) -> Vec<u8> {
    assert_eq!(
        tokens.len(),
        tick_spacings.len() + 1,
        "Slipstream path: tokens.len() must equal tick_spacings.len() + 1"
    );
    let mut out = Vec::with_capacity(20 + tick_spacings.len() * 23);
    for (i, ts) in tick_spacings.iter().enumerate() {
        out.extend_from_slice(&address_bytes(&tokens[i]));
        let be = ts.to_be_bytes();
        out.push(be[1]);
        out.push(be[2]);
        out.push(be[3]);
    }
    out.extend_from_slice(&address_bytes(tokens.last().unwrap()));
    out
}

/// UniV2 packed path — bare 20-byte address sequence `token ++ token ++ …`
/// (the `isUni = true` layout, `unfold_velo_v2_path` AERODROME_UR_RESEARCH
/// §3.1).
fn build_univ2_path(tokens: &[Address]) -> Vec<u8> {
    assert!(tokens.len() >= 2, "V2 path needs >= 2 tokens");
    let mut out = Vec::with_capacity(tokens.len() * 20);
    for t in tokens {
        out.extend_from_slice(&address_bytes(t));
    }
    out
}

/// VeloV2 packed path — `token(20) ++ stable(1) ++ token(20) ++ stable(1) ++
/// … ++ token(20)` (the `isUni = false` layout). `stables.len() ==
/// tokens.len() - 1`.
fn build_velov2_path(tokens: &[Address], stables: &[bool]) -> Vec<u8> {
    assert_eq!(
        tokens.len(),
        stables.len() + 1,
        "VeloV2 path: tokens.len() must equal stables.len() + 1"
    );
    let mut out = Vec::with_capacity(20 + stables.len() * 21);
    for (i, stable) in stables.iter().enumerate() {
        out.extend_from_slice(&address_bytes(&tokens[i]));
        out.push(u8::from(*stable));
    }
    out.extend_from_slice(&address_bytes(tokens.last().unwrap()));
    out
}

fn load_mapper(bundle_json: &str) -> DeclarativeMapper {
    let bundle: AdapterFunctionBundle =
        serde_json::from_str(bundle_json).expect("Aerodrome UR bundle parses");
    DeclarativeMapper::new(bundle)
}

fn unwrap_swap(envelope: &ActionEnvelope) -> &policy_engine::action::dex::SwapAction {
    match &envelope.action {
        Action::Swap(s) => s,
        other => panic!("expected SwapAction, got {other:?}"),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// (a) Single V3_SWAP_EXACT_IN — packed Slipstream path → swap envelope with
//     correct input / output token endpoints.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn aero_ur_single_v3_swap_exact_in_resolves_slipstream_endpoints() {
    let mapper = load_mapper(AERO_UR_EXECUTE_BUNDLE);
    let ctx = Ctx::new();

    // 2-hop Slipstream path `(A --200--> B --500--> C)`. The `0x00` bundle
    // entry binds `inputToken = unfold_slipstream_path(path, first_token)`
    // and `outputToken = unfold_slipstream_path(path, last_token)`.
    let path = build_slipstream_path(&[token_a(), token_b(), token_c()], &[200, 500]);
    let input = encode_swap_6tuple_input(
        recipient(),
        1_000_000, // amountIn
        900_000,   // amountOutMin
        path,
        true,  // payerIsUser
        false, // isUni — Velodrome CL pool
    );

    let decoded = ur_execute_decoded(mapper.declarative_decoder_id(), vec![0x00], vec![input]);

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("V3_SWAP_EXACT_IN maps");
    assert_eq!(envelopes.len(), 1, "single opcode → single envelope");

    let action = unwrap_swap(&envelopes[0]);
    assert_eq!(envelopes[0].category, Category::Dex);
    assert_eq!(action.swap_mode, SwapMode::ExactIn);
    assert_eq!(action.input_token.asset.kind, AssetKind::Erc20);
    assert_eq!(
        action.input_token.asset.address,
        Some(token_a()),
        "first_token of the Slipstream path must reach inputToken"
    );
    assert_eq!(action.input_token.amount.kind, AmountKind::Exact);
    assert_eq!(
        action
            .input_token
            .amount
            .value
            .as_ref()
            .map(ToString::to_string),
        Some("1000000".to_owned())
    );
    assert_eq!(
        action.output_token.asset.address,
        Some(token_c()),
        "last_token of the Slipstream path must reach outputToken"
    );
    assert_eq!(action.output_token.amount.kind, AmountKind::Min);
    assert_eq!(
        action
            .output_token
            .amount
            .value
            .as_ref()
            .map(ToString::to_string),
        Some("900000".to_owned())
    );
    assert_eq!(action.recipient, recipient());
    // Intermediate token B must never surface.
    assert_ne!(action.input_token.asset.address, Some(token_b()));
    assert_ne!(action.output_token.asset.address, Some(token_b()));
}

// ───────────────────────────────────────────────────────────────────────────
// (b) Single V2_SWAP_EXACT_IN via `unfold_velo_v2_path` — covers BOTH the
//     UniV2 (`token ++ token`) and VeloV2 (`token ++ stable ++ token`)
//     packed layouts. `unfold_velo_v2_path` reads endpoints at fixed
//     `path[0..20]` / `path[len-20..]` offsets, so both layouts resolve.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn aero_ur_v2_swap_exact_in_univ2_path_resolves_endpoints() {
    let mapper = load_mapper(AERO_UR_EXECUTE_BUNDLE);
    let ctx = Ctx::new();

    // UniV2 layout (`isUni = true`): bare `A ++ B ++ C` address sequence.
    let path = build_univ2_path(&[token_a(), token_b(), token_c()]);
    let input = encode_swap_6tuple_input(
        recipient(),
        2_000_000,
        1_950_000,
        path,
        true,
        true, // isUni — UniV2 pools
    );

    let decoded = ur_execute_decoded(mapper.declarative_decoder_id(), vec![0x08], vec![input]);

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("V2_SWAP_EXACT_IN (UniV2 path) maps");
    let action = unwrap_swap(&envelopes[0]);
    assert_eq!(action.swap_mode, SwapMode::ExactIn);
    assert_eq!(
        action.input_token.asset.address,
        Some(token_a()),
        "UniV2 path first 20-byte token → inputToken"
    );
    assert_eq!(
        action.output_token.asset.address,
        Some(token_c()),
        "UniV2 path last 20-byte token → outputToken"
    );
}

#[test]
fn aero_ur_v2_swap_exact_in_velov2_path_resolves_endpoints() {
    let mapper = load_mapper(AERO_UR_EXECUTE_BUNDLE);
    let ctx = Ctx::new();

    // VeloV2 layout (`isUni = false`): `A ++ stable(1) ++ B ++ stable(1) ++ C`.
    // The interleaved stable bytes must NOT shift the resolved endpoints —
    // `unfold_velo_v2_path` slices fixed `[0..20]` / `[len-20..]`.
    let path = build_velov2_path(&[token_a(), token_b(), token_c()], &[true, false]);
    let input = encode_swap_6tuple_input(
        recipient(),
        3_000_000,
        2_900_000,
        path,
        true,
        false, // isUni — Velodrome AMM pools
    );

    let decoded = ur_execute_decoded(mapper.declarative_decoder_id(), vec![0x08], vec![input]);

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("V2_SWAP_EXACT_IN (VeloV2 path) maps");
    let action = unwrap_swap(&envelopes[0]);
    assert_eq!(
        action.input_token.asset.address,
        Some(token_a()),
        "VeloV2 path first 20-byte token → inputToken (stable byte not counted)"
    );
    assert_eq!(
        action.output_token.asset.address,
        Some(token_c()),
        "VeloV2 path last 20-byte token → outputToken"
    );
    // The interleaved stable byte must not corrupt the intermediate either —
    // token B (a real address) must not leak onto an endpoint.
    assert_ne!(action.input_token.asset.address, Some(token_b()));
    assert_ne!(action.output_token.asset.address, Some(token_b()));
}

// ───────────────────────────────────────────────────────────────────────────
// (c) WRAP_ETH + UNWRAP_WETH — Base WETH literal, `amount` vs `amountMin`
//     arg-name divergence pinned against `AERODROME_UR_MAIN_TABLE`.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn aero_ur_wrap_eth_emits_wrap_with_base_weth() {
    let mapper = load_mapper(AERO_UR_EXECUTE_BUNDLE);
    let ctx = Ctx::new();

    // WRAP_ETH arg name is `amount` (Tier B `AERODROME_UR_MAIN_TABLE`).
    let input = encode_wrap_eth_input(recipient(), 1_000_000_000_000_000_000); // 1e18
    let decoded = ur_execute_decoded(mapper.declarative_decoder_id(), vec![0x0b], vec![input]);

    let envelopes = mapper.map(&ctx.map_ctx(), &decoded).expect("WRAP_ETH maps");
    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].category, Category::Misc);
    let Action::Wrap(action) = &envelopes[0].action else {
        panic!("expected Action::Wrap, got {:?}", envelopes[0].action);
    };
    assert_eq!(action.native_asset.asset.kind, AssetKind::Native);
    assert_eq!(action.wrapped_asset.asset.kind, AssetKind::Erc20);
    assert_eq!(
        action.wrapped_asset.asset.address,
        Some(base_weth()),
        "WRAP_ETH wrappedAsset must be Base WETH 0x4200…0006, not mainnet WETH"
    );
    assert_eq!(
        action
            .native_asset
            .amount
            .value
            .as_ref()
            .map(ToString::to_string),
        Some("1000000000000000000".to_owned()),
        "WRAP_ETH amount arg ($.args.amount) must flow to nativeAsset amount"
    );
    assert_eq!(action.recipient, recipient());
}

#[test]
fn aero_ur_unwrap_weth_emits_unwrap_with_base_weth() {
    let mapper = load_mapper(AERO_UR_EXECUTE_BUNDLE);
    let ctx = Ctx::new();

    // UNWRAP_WETH arg name is `amountMin` (distinct from WRAP_ETH's `amount`).
    let input = encode_unwrap_weth_input(recipient(), 500_000_000_000_000_000); // 0.5e18
    let decoded = ur_execute_decoded(mapper.declarative_decoder_id(), vec![0x0c], vec![input]);

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("UNWRAP_WETH maps");
    assert_eq!(envelopes.len(), 1);
    let Action::Unwrap(action) = &envelopes[0].action else {
        panic!("expected Action::Unwrap, got {:?}", envelopes[0].action);
    };
    assert_eq!(action.wrapped_asset.asset.kind, AssetKind::Erc20);
    assert_eq!(
        action.wrapped_asset.asset.address,
        Some(base_weth()),
        "UNWRAP_WETH wrappedAsset must be Base WETH 0x4200…0006"
    );
    assert_eq!(action.native_asset.asset.kind, AssetKind::Native);
    assert_eq!(
        action
            .native_asset
            .amount
            .value
            .as_ref()
            .map(ToString::to_string),
        Some("500000000000000000".to_owned()),
        "UNWRAP_WETH amountMin arg ($.args.amountMin) must flow to nativeAsset amount"
    );
    assert_eq!(action.recipient, recipient());
}

// ───────────────────────────────────────────────────────────────────────────
// (d) PERMIT2_PERMIT — nested Permit2 `PermitSingle` struct decode.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn aero_ur_permit2_permit_emits_permit_envelope() {
    let mapper = load_mapper(AERO_UR_EXECUTE_BUNDLE);
    let ctx = Ctx::new();

    let input = encode_permit2_permit_input(
        token_a(),            // permitSingle.details.token
        u128::from(u64::MAX), // permitSingle.details.amount (uint160-fitting)
        1_700_009_999,        // permitSingle.details.expiration
        7,                    // permitSingle.details.nonce
        token_b(),            // permitSingle.spender
        1_700_005_000,        // permitSingle.sigDeadline
        vec![0xab, 0xcd, 0xef],
    );
    let decoded = ur_execute_decoded(mapper.declarative_decoder_id(), vec![0x0a], vec![input]);

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("PERMIT2_PERMIT maps");
    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].category, Category::Misc);
    let Action::Permit(action) = &envelopes[0].action else {
        panic!("expected Action::Permit, got {:?}", envelopes[0].action);
    };
    assert_eq!(action.permit_kind, PermitKind::Permit2Single);
    assert_eq!(action.token.kind, AssetKind::Erc20);
    assert_eq!(
        action.token.address,
        Some(token_a()),
        "permitSingle.details.token must reach the permit token"
    );
    assert_eq!(
        action.spender,
        Some(token_b()),
        "permitSingle.spender must reach the permit spender"
    );
    // owner is bound to $.tx.from — the EOA signer.
    assert_eq!(
        action.owner,
        Address::from_str("0x00000000000000000000000000000000000000aa").unwrap()
    );
}

// ───────────────────────────────────────────────────────────────────────────
// (e) Multi-step stream — WRAP_ETH + V3_SWAP_EXACT_IN. Both opcodes emit,
//     order preserved.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn aero_ur_multi_step_wrap_then_v3_swap_yields_two_envelopes_in_order() {
    let mapper = load_mapper(AERO_UR_EXECUTE_BUNDLE);
    let ctx = Ctx::new();

    // Step 0: WRAP_ETH (0x0b). Step 1: V3_SWAP_EXACT_IN (0x00) — a common
    // "wrap native, then swap the WETH" UR composition.
    let wrap_input = encode_wrap_eth_input(recipient(), 1_000_000_000_000_000_000);
    let path = build_slipstream_path(&[base_weth(), token_b()], &[100]);
    let swap_input = encode_swap_6tuple_input(
        recipient(),
        1_000_000_000_000_000_000,
        2_500_000,
        path,
        false, // payerIsUser — UR holds the freshly-wrapped WETH
        false,
    );

    let decoded = ur_execute_decoded(
        mapper.declarative_decoder_id(),
        vec![0x0b, 0x00],
        vec![wrap_input, swap_input],
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("WRAP_ETH + V3_SWAP stream maps");
    assert_eq!(envelopes.len(), 2, "two opcodes → two envelopes");
    // Order MUST match the command byte order.
    assert!(
        matches!(envelopes[0].action, Action::Wrap(_)),
        "envelope[0] must be the WRAP_ETH step"
    );
    assert!(
        matches!(envelopes[1].action, Action::Swap(_)),
        "envelope[1] must be the V3_SWAP step"
    );
    let swap = unwrap_swap(&envelopes[1]);
    assert_eq!(swap.input_token.asset.address, Some(base_weth()));
    assert_eq!(swap.output_token.asset.address, Some(token_b()));
}

// ───────────────────────────────────────────────────────────────────────────
// (f) Unmodeled opcodes — `0x10 V4_SWAP` and `0x12 BRIDGE_TOKEN` are NOT in
//     the bundle's `per_opcode_emit`. `unknown_opcode_policy = deny` must
//     FAULT the whole declarative route (→ the orchestrator falls back to the
//     conservative static path). A `warn`-skip here would let a declarative
//     verdict be driven from an envelope set with the unmodeled opcode's
//     intent silently omitted — false-PASS risk (AUDIT_AERODROME_UR A-01/A-02).
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn aero_ur_v4_swap_opcode_denied_faults_route() {
    let mapper = load_mapper(AERO_UR_EXECUTE_BUNDLE);
    let ctx = Ctx::new();

    // V4_SWAP (0x10) — a minimal `(bytes actions, bytes[] params)` blob. The
    // bundle omits 0x10, so `deny` policy must fault rather than skip.
    let v4_input = encode_v4_swap_input(vec![0x00], vec![vec![0xde, 0xad]]);
    let decoded = ur_execute_decoded(mapper.declarative_decoder_id(), vec![0x10], vec![v4_input]);

    let result = mapper.map(&ctx.map_ctx(), &decoded);
    assert!(
        result.is_err(),
        "unmodeled opcode 0x10 under deny policy must fault the route, got {result:?}"
    );
}

#[test]
fn aero_ur_bridge_token_opcode_denied_faults_route() {
    let mapper = load_mapper(AERO_UR_EXECUTE_BUNDLE);
    let ctx = Ctx::new();

    // BRIDGE_TOKEN (0x12) — the bundle omits it; `deny` policy must fault.
    let bridge_input = encode_bridge_token_input(
        0x02, // XVELO
        recipient(),
        token_a(),
        token_b(),
        1_000_000,
        5_000,
        8453,
        true,
    );
    let decoded = ur_execute_decoded(
        mapper.declarative_decoder_id(),
        vec![0x12],
        vec![bridge_input],
    );

    let result = mapper.map(&ctx.map_ctx(), &decoded);
    assert!(
        result.is_err(),
        "unmodeled opcode 0x12 (BRIDGE_TOKEN) under deny policy must fault, got {result:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// (f-bis) Mixed stream — a modeled opcode followed by an unmodeled one. Under
//     `deny`, the unmodeled opcode faults the WHOLE route: a partial
//     (V3_SWAP-only) envelope set must never reach the verdict engine, or the
//     orchestrator would PASS the tx blind to the second opcode
//     (AUDIT_AERODROME_UR A-01).
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn aero_ur_v3_swap_then_unmodeled_opcode_faults_route() {
    let mapper = load_mapper(AERO_UR_EXECUTE_BUNDLE);
    let ctx = Ctx::new();

    let path = build_slipstream_path(&[token_a(), token_b()], &[200]);
    let swap_input = encode_swap_6tuple_input(recipient(), 1_000_000, 900_000, path, true, false);
    let v4_input = encode_v4_swap_input(vec![0x00], vec![vec![0xbe, 0xef]]);

    let decoded = ur_execute_decoded(
        mapper.declarative_decoder_id(),
        vec![0x00, 0x10], // V3_SWAP_EXACT_IN (modeled), then V4_SWAP (unmodeled)
        vec![swap_input, v4_input],
    );

    // The lone V3_SWAP envelope must NOT be returned — the unmodeled 0x10
    // faults the whole route so the orchestrator falls back to the
    // conservative static path rather than driving a verdict from a partial
    // envelope set.
    let result = mapper.map(&ctx.map_ctx(), &decoded);
    assert!(
        result.is_err(),
        "a modeled+unmodeled stream under deny policy must fault, got {result:?}"
    );
}
