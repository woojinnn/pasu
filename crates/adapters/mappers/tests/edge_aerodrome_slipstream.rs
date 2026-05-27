//! Aerodrome Slipstream packed-path edge case tests (T-TEST-AERO-SLIPSTREAM).
//!
//! Exercises the Phase 8 Round 2 `unfold_slipstream_path` builtin
//! (`src/declarative/builtin_fn.rs`) plus the Round 4 Slipstream
//! `exactInput@1.0.0.json` bundle end-to-end through the `DeclarativeMapper`.
//!
//! Slipstream's packed path mirrors Uniswap V3's layout —
//! `[token (20)][tickSpacing (3)][token (20)][tickSpacing (3)] ... [token]` —
//! but the middle 3 bytes are a **signed** `int24 tickSpacing` instead of a
//! `uint24 fee`. Sign-extension is required on decode: the high bit of byte 0
//! is the int24 sign bit, so `0xFFFFFF` decodes to `-1` (not 16_777_215).
//!
//! Coverage focus (Phase 8 Round 6 — T-TEST-AERO-SLIPSTREAM):
//!
//!   1. Single-hop packed path → `first_token`, `last_token`,
//!      `first_tick_spacing` extraction.
//!   2. 3-hop packed path → `first_token`, `last_token`, `last_tick_spacing`
//!      reach the final hop.
//!   3. 8-hop max-length packed path → endpoints survive deep paths
//!      (length = `20 + 23*8 = 204` bytes).
//!   4. Negative `tickSpacing = -1` (`0xFFFFFF`) → sign-extension to `-1_i32`.
//!   5. Max positive `tickSpacing = 8_388_607` (`0x7FFFFF`) → int24 ceiling.
//!   6. Min negative `tickSpacing = -8_388_608` (`0x800000`) → int24 floor.
//!   7. Malformed length `22 bytes` (< `MIN_LEN=43`) → `FnError::SlipstreamPathDecode`.
//!   8. Malformed length `50 bytes` (`(50-20) % 23 != 0`) → `FnError::SlipstreamPathDecode`.
//!   9. `tick_spacing_at_hop` positive idx → middle hop's tickSpacing.
//!  10. `tick_spacing_at_hop` negative idx → last hop (Python-style `-1`).
//!  11. DSL integration via Aerodrome `exactInput@1.0.0` bundle: real
//!      `DeclarativeMapper::map` pipeline emits the expected `SwapAction`
//!      with first/last tokens resolved through `unfold_slipstream_path`.
//!
//! All tests are read-only on production code. The Slipstream `exactInput`
//! bundle JSON is loaded via `include_str!` from the registry's manifest
//! tree (see `registry/manifests/aerodrome/slipstream-swap-router/exactInput@1.0.0.json`).

use std::str::FromStr as _;

use abi_resolver::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::U256;
use mappers::declarative::builtin_fn::{unfold_slipstream_path, FnError};
use mappers::declarative::{types::AdapterFunctionBundle, DeclarativeMapper};
use mappers::mapper::{MapContext, Mapper};
use mappers::EmptyTokenRegistry;
use policy_engine::action::dex::SwapMode;
use policy_engine::action::{Action, ActionEnvelope, Address, DecimalString};
use serde_json::json;

// ───────────────────────────────────────────────────────────────────────────
// Bundle fixture — the Slipstream exactInput bundle from the registry.
// Loaded directly via `include_str!` (relative to this test file's location).
// ───────────────────────────────────────────────────────────────────────────

const SLIPSTREAM_EXACT_INPUT_BUNDLE: &str = include_str!(
    "../../../../registry/manifests/aerodrome/slipstream-swap-router/exactInput@1.0.0.json"
);

// ───────────────────────────────────────────────────────────────────────────
// Address fixtures. Slipstream addresses are dummy unique ones for unit-test
// purposes — only the `unfold_slipstream_path` decoder cares about positions.
// ───────────────────────────────────────────────────────────────────────────

fn token_a() -> Address {
    Address::from_str("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01").unwrap()
}

fn token_b() -> Address {
    Address::from_str("0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02").unwrap()
}

fn token_c() -> Address {
    Address::from_str("0xcccccccccccccccccccccccccccccccccccccc03").unwrap()
}

fn token_d() -> Address {
    Address::from_str("0xdddddddddddddddddddddddddddddddddddddd04").unwrap()
}

fn recipient() -> Address {
    Address::from_str("0x4444444444444444444444444444444444444444").unwrap()
}

// ───────────────────────────────────────────────────────────────────────────
// Helpers.
// ───────────────────────────────────────────────────────────────────────────

/// Decode the canonical `"0x...".to_string()` form of `Address` to raw bytes.
fn address_bytes(addr: &Address) -> [u8; 20] {
    let s = addr.to_string();
    let stripped = s.strip_prefix("0x").expect("Address is 0x-prefixed");
    let decoded = hex::decode(stripped).expect("Address hex is valid");
    let mut out = [0u8; 20];
    out.copy_from_slice(&decoded);
    out
}

/// Build a Slipstream packed path from interleaved tokens / tick spacings.
///
/// `tokens.len() == tick_spacings.len() + 1` is required (each tick spacing
/// sits between two tokens). Each `tick_spacing` is encoded as a 3-byte
/// big-endian `int24`; negative values use two's-complement (the top byte's
/// high bit acts as the sign).
fn build_slipstream_path(tokens: &[Address], tick_spacings: &[i32]) -> Vec<u8> {
    assert_eq!(
        tokens.len(),
        tick_spacings.len() + 1,
        "Slipstream path: tokens.len() must equal tick_spacings.len() + 1"
    );
    let mut out = Vec::with_capacity(20 + tick_spacings.len() * 23);
    for (i, ts) in tick_spacings.iter().enumerate() {
        out.extend_from_slice(&address_bytes(&tokens[i]));
        // i32 big-endian — take the low 3 bytes (drop the top byte). For
        // negative values, sign-extension lives in the top byte (e.g.
        // `-1_i32 = 0xFFFFFFFF`, low 3 bytes = `0xFFFFFF`).
        let be = ts.to_be_bytes();
        out.push(be[1]);
        out.push(be[2]);
        out.push(be[3]);
    }
    out.extend_from_slice(&address_bytes(tokens.last().unwrap()));
    out
}

/// Hex-encoded JSON string view of a packed path (`"0x..."`). Matches how
/// `decoded_value_to_json` encodes `DecodedValue::Bytes`.
fn path_as_hex_json(path: &[u8]) -> serde_json::Value {
    serde_json::Value::String(format!("0x{}", hex::encode(path)))
}

/// Build a `DecodedCall` matching the Slipstream `exactInput` bundle layout.
/// Phase A B-1 fix — the bundle now resolves flat field names (`$.args.path`,
/// `$.args.recipient`, `$.args.amountIn`, etc.) per Uniswap V3's flatten
/// convention. The synthetic decoder must therefore emit one `DecodedArg` per
/// tuple component, mirroring `decode_with_json_abi`'s single-tuple flatten.
fn slipstream_exact_input_decoded(
    decoder_id: DecoderId,
    path_bytes: Vec<u8>,
    recipient_addr: Address,
    amount_in: U256,
    amount_out_minimum: U256,
) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "exactInput((bytes,address,uint256,uint256,uint256))".into(),
        args: vec![
            DecodedArg {
                name: "path".into(),
                abi_type: "bytes".into(),
                value: DecodedValue::Bytes(path_bytes),
            },
            DecodedArg {
                name: "recipient".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(recipient_addr),
            },
            DecodedArg {
                name: "deadline".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(U256::from(9_999_999_999_u64)),
            },
            DecodedArg {
                name: "amountIn".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount_in),
            },
            DecodedArg {
                name: "amountOutMinimum".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount_out_minimum),
            },
        ],
        nested: vec![],
    }
}

fn unwrap_swap(envelope: &ActionEnvelope) -> &policy_engine::action::dex::SwapAction {
    match &envelope.action {
        Action::Swap(s) => s,
        other => panic!("expected SwapAction, got {other:?}"),
    }
}

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
            to: Address::from_str("0x698cb2b6dd822994581fea6ea4fc755d1363a92f").unwrap(),
            value: DecimalString::from_str("0").unwrap(),
        }
    }

    fn map_ctx(&self) -> MapContext<'_> {
        MapContext::new(
            8453,
            &self.from,
            &self.to,
            &self.value,
            Some(1_700_000_000),
            &self.registry,
        )
    }
}

// ───────────────────────────────────────────────────────────────────────────
// 1. Single-hop packed path (43 bytes) — first_token, last_token, first_tick_spacing.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_single_hop_endpoints_and_tick_spacing() {
    // `(token_a, +200, token_b)` — minimum Slipstream path length (43 B).
    let path = build_slipstream_path(&[token_a(), token_b()], &[200]);
    assert_eq!(
        path.len(),
        43,
        "single-hop Slipstream path = 20 + 23 = 43 bytes"
    );

    let path_json = path_as_hex_json(&path);
    let first = unfold_slipstream_path(&path_json, "first_token", None).unwrap();
    let last = unfold_slipstream_path(&path_json, "last_token", None).unwrap();
    let ts_first = unfold_slipstream_path(&path_json, "first_tick_spacing", None).unwrap();
    let ts_last = unfold_slipstream_path(&path_json, "last_tick_spacing", None).unwrap();

    assert_eq!(first.as_str().unwrap(), token_a().to_string());
    assert_eq!(last.as_str().unwrap(), token_b().to_string());
    // Single hop → first and last tickSpacing coincide.
    assert_eq!(ts_first.as_i64().unwrap(), 200);
    assert_eq!(ts_last.as_i64().unwrap(), 200);
}

// ───────────────────────────────────────────────────────────────────────────
// 2. 3-hop packed path (89 bytes) — first_token, last_token, last_tick_spacing.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_three_hop_first_last_token_and_last_tick_spacing() {
    // `(A, +100, B, +200, C, +500, D)` — 3 hops.
    let path = build_slipstream_path(
        &[token_a(), token_b(), token_c(), token_d()],
        &[100, 200, 500],
    );
    assert_eq!(path.len(), 20 + 23 * 3, "3-hop Slipstream path = 89 bytes");

    let path_json = path_as_hex_json(&path);
    let first = unfold_slipstream_path(&path_json, "first_token", None).unwrap();
    let last = unfold_slipstream_path(&path_json, "last_token", None).unwrap();
    let ts_first = unfold_slipstream_path(&path_json, "first_tick_spacing", None).unwrap();
    let ts_last = unfold_slipstream_path(&path_json, "last_tick_spacing", None).unwrap();

    assert_eq!(first.as_str().unwrap(), token_a().to_string());
    assert_eq!(last.as_str().unwrap(), token_d().to_string());
    assert_eq!(ts_first.as_i64().unwrap(), 100);
    assert_eq!(ts_last.as_i64().unwrap(), 500);
}

// ───────────────────────────────────────────────────────────────────────────
// 3. 8-hop max-length packed path (204 bytes) — endpoints survive depth.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_eight_hop_max_length_endpoints_preserved() {
    // 9 tokens for 8 hops. Intermediate tokens are unique dummy addresses.
    let intermediates: Vec<Address> = (1..=7)
        .map(|i| {
            let suffix = format!("{i:02x}");
            Address::from_str(&format!("0x{}{}", "0".repeat(38), suffix)).unwrap()
        })
        .collect();
    let mut tokens = vec![token_a()];
    tokens.extend(intermediates);
    tokens.push(token_d());
    assert_eq!(tokens.len(), 9, "8 hops requires 9 tokens");

    // Mixed production-valid Slipstream tick spacings.
    // Aerodrome CL pools use {1, 50, 100, 200, 500, 2000} historically; we
    // mix in some others to confirm the decoder is tick-spacing-agnostic.
    let tick_spacings: Vec<i32> = vec![1, 50, 100, 200, 500, 2000, 100, 50];
    let path = build_slipstream_path(&tokens, &tick_spacings);
    assert_eq!(path.len(), 20 + 23 * 8, "8-hop Slipstream path = 204 bytes");

    let path_json = path_as_hex_json(&path);
    let first = unfold_slipstream_path(&path_json, "first_token", None).unwrap();
    let last = unfold_slipstream_path(&path_json, "last_token", None).unwrap();
    assert_eq!(first.as_str().unwrap(), token_a().to_string());
    assert_eq!(last.as_str().unwrap(), token_d().to_string());

    // Sanity-check the first / last tick spacings traversed the full chain.
    let ts_first = unfold_slipstream_path(&path_json, "first_tick_spacing", None).unwrap();
    let ts_last = unfold_slipstream_path(&path_json, "last_tick_spacing", None).unwrap();
    assert_eq!(ts_first.as_i64().unwrap(), 1);
    assert_eq!(ts_last.as_i64().unwrap(), 50);
}

// ───────────────────────────────────────────────────────────────────────────
// 4. Negative tickSpacing = -1 (0xFFFFFF) → sign-extension to i32::-1.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_negative_tick_spacing_neg_one() {
    // `(token_a, -1, token_b)` — `-1_i32 = 0xFFFFFFFF`, low 3 bytes = `0xFFFFFF`.
    let path = build_slipstream_path(&[token_a(), token_b()], &[-1]);
    // Sanity-check the on-the-wire encoding really is `FFFFFF` for that slot.
    assert_eq!(&path[20..23], &[0xFF, 0xFF, 0xFF]);

    let path_json = path_as_hex_json(&path);
    let ts = unfold_slipstream_path(&path_json, "first_tick_spacing", None).unwrap();
    assert_eq!(
        ts.as_i64().unwrap(),
        -1,
        "0xFFFFFF must sign-extend to -1, not 16_777_215"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// 5. Max positive tickSpacing = 8_388_607 (0x7FFFFF) — int24 ceiling.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_max_positive_tick_spacing() {
    // int24 max = 2^23 - 1 = 8_388_607. Encoded as `0x7FFFFF` (sign bit clear).
    let path = build_slipstream_path(&[token_a(), token_b()], &[8_388_607]);
    assert_eq!(&path[20..23], &[0x7F, 0xFF, 0xFF]);

    let path_json = path_as_hex_json(&path);
    let ts = unfold_slipstream_path(&path_json, "first_tick_spacing", None).unwrap();
    assert_eq!(ts.as_i64().unwrap(), 8_388_607);
}

// ───────────────────────────────────────────────────────────────────────────
// 6. Min negative tickSpacing = -8_388_608 (0x800000) — int24 floor.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_min_negative_tick_spacing() {
    // int24 min = -2^23 = -8_388_608. Two's complement: `0x800000` (sign bit set,
    // remaining bits zero).
    let path = build_slipstream_path(&[token_a(), token_b()], &[-8_388_608]);
    assert_eq!(&path[20..23], &[0x80, 0x00, 0x00]);

    let path_json = path_as_hex_json(&path);
    let ts = unfold_slipstream_path(&path_json, "first_tick_spacing", None).unwrap();
    assert_eq!(ts.as_i64().unwrap(), -8_388_608);
}

// ───────────────────────────────────────────────────────────────────────────
// 7. Malformed length: 22 bytes (< MIN_LEN=43) → SlipstreamPathDecode error.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_malformed_too_short_22_bytes_errors() {
    // 22 bytes is less than `MIN_LEN=43`. The decoder should reject before any
    // pool can be read.
    let bytes: Vec<u8> = vec![0xAA; 22];
    let json_v = path_as_hex_json(&bytes);
    let err = unfold_slipstream_path(&json_v, "first_token", None).unwrap_err();
    assert!(
        matches!(err, FnError::SlipstreamPathDecode { .. }),
        "expected SlipstreamPathDecode, got {err:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// 8. Malformed length: 50 bytes ((50-20) % 23 != 0) → SlipstreamPathDecode.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_malformed_misaligned_50_bytes_errors() {
    // 50 bytes is >= MIN_LEN but `(50 - 20) % 23 = 30 % 23 = 7`, not zero.
    // The decoder catches the alignment failure and reports it as a separate
    // SlipstreamPathDecode variant (different message than the too-short case).
    let bytes: Vec<u8> = vec![0xAA; 50];
    let json_v = path_as_hex_json(&bytes);
    let err = unfold_slipstream_path(&json_v, "first_token", None).unwrap_err();
    let FnError::SlipstreamPathDecode { message } = &err else {
        panic!("expected SlipstreamPathDecode, got {err:?}");
    };
    assert!(
        message.contains("20 + 23*N") || message.contains("malformed"),
        "error message should mention the 20+23*N alignment invariant, got {message:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// 9. tick_spacing_at_hop with positive idx → middle hop's tickSpacing.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_tick_spacing_at_hop_positive_idx() {
    // 3-hop `(A, +100, B, +200, C, +500, D)` — `tick_spacing_at_hop(1)` = 200.
    let path = build_slipstream_path(
        &[token_a(), token_b(), token_c(), token_d()],
        &[100, 200, 500],
    );
    let path_json = path_as_hex_json(&path);

    let hop0 = unfold_slipstream_path(&path_json, "tick_spacing_at_hop", Some(&json!(0))).unwrap();
    let hop1 = unfold_slipstream_path(&path_json, "tick_spacing_at_hop", Some(&json!(1))).unwrap();
    let hop2 = unfold_slipstream_path(&path_json, "tick_spacing_at_hop", Some(&json!(2))).unwrap();
    assert_eq!(hop0.as_i64().unwrap(), 100);
    assert_eq!(hop1.as_i64().unwrap(), 200);
    assert_eq!(hop2.as_i64().unwrap(), 500);
}

// ───────────────────────────────────────────────────────────────────────────
// 10. tick_spacing_at_hop with negative idx → last hop (Python-style).
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_tick_spacing_at_hop_negative_idx_picks_last() {
    // Same 3-hop path. `-1` = last hop (500). `-2` = penultimate (200).
    // `-3` = first (100). `-4` is out-of-bounds.
    let path = build_slipstream_path(
        &[token_a(), token_b(), token_c(), token_d()],
        &[100, 200, 500],
    );
    let path_json = path_as_hex_json(&path);

    let neg1 = unfold_slipstream_path(&path_json, "tick_spacing_at_hop", Some(&json!(-1))).unwrap();
    let neg2 = unfold_slipstream_path(&path_json, "tick_spacing_at_hop", Some(&json!(-2))).unwrap();
    let neg3 = unfold_slipstream_path(&path_json, "tick_spacing_at_hop", Some(&json!(-3))).unwrap();
    assert_eq!(neg1.as_i64().unwrap(), 500);
    assert_eq!(neg2.as_i64().unwrap(), 200);
    assert_eq!(neg3.as_i64().unwrap(), 100);

    // `-4` falls past the start of the array → IndexOutOfBounds.
    let err =
        unfold_slipstream_path(&path_json, "tick_spacing_at_hop", Some(&json!(-4))).unwrap_err();
    assert!(
        matches!(err, FnError::IndexOutOfBounds { .. }),
        "expected IndexOutOfBounds, got {err:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────────
// 11. DSL integration: Aerodrome `exactInput@1.0.0` bundle → real
//     DeclarativeMapper::map pipeline emits a SwapAction with first_token /
//     last_token resolved through `unfold_slipstream_path`.
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn slipstream_dsl_integration_exact_input_emits_swap_with_correct_endpoints() {
    let bundle: AdapterFunctionBundle = serde_json::from_str(SLIPSTREAM_EXACT_INPUT_BUNDLE)
        .expect("Slipstream exactInput bundle parses");
    let mapper = DeclarativeMapper::new(bundle);
    let ctx = Ctx::new();

    // 3-hop packed path `(A --100--> B --200--> C --500--> D)`. The bundle
    // binds `inputToken.asset.address = unfold_slipstream_path(..., first_token)`
    // and `outputToken.asset.address = unfold_slipstream_path(..., last_token)`,
    // so the resulting envelope should anchor to `token_a` / `token_d`.
    let path = build_slipstream_path(
        &[token_a(), token_b(), token_c(), token_d()],
        &[100, 200, 500],
    );

    let decoded = slipstream_exact_input_decoded(
        mapper.declarative_decoder_id(),
        path,
        recipient(),
        U256::from(1_000_000_u64),
        U256::from(900_000_u64),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("Slipstream exactInput maps end-to-end");
    assert_eq!(
        envelopes.len(),
        1,
        "single_emit yields exactly one envelope"
    );

    let action = unwrap_swap(&envelopes[0]);
    assert_eq!(
        action.swap_mode,
        SwapMode::ExactIn,
        "exactInput → ExactIn swap mode"
    );
    assert_eq!(
        action.input_token.asset.address,
        Some(token_a()),
        "first_token must reach the input asset address"
    );
    assert_eq!(
        action.output_token.asset.address,
        Some(token_d()),
        "last_token must reach the output asset address"
    );
    assert_eq!(action.recipient, recipient());
}

// ───────────────────────────────────────────────────────────────────────────
// 12. Slipstream NPM `decreaseLiquidity` — the pool's token0 / token1 are
//     resolved off the position `tokenId`, NOT present in calldata, so the
//     bundle emits `outputTokens[*].asset.kind: "unknown"`. The serialized
//     envelope must survive the evaluate stage's serde round-trip; an
//     `erc20` asset with no address would fail-close it with
//     `__engine::invalid_input_json`. Regression for that false-`fail`.
// ───────────────────────────────────────────────────────────────────────────

const SLIPSTREAM_NPM_DECREASE_LIQUIDITY_BUNDLE: &str = include_str!(
    "../../../../registry/manifests/aerodrome/slipstream-nfpm/decreaseLiquidity@1.0.0.json"
);

/// Build a `DecodedCall` for Slipstream NPM `decreaseLiquidity`. Phase A B-1
/// fix — the bundle now resolves flat field names (`$.args.tokenId`, etc.) per
/// the Uniswap V3 flatten convention. Args are emitted one per tuple
/// component, mirroring `decode_with_json_abi`'s single-tuple flatten.
fn slipstream_npm_decrease_liquidity_decoded(
    decoder_id: DecoderId,
    token_id: U256,
    liquidity: U256,
    amount0_min: U256,
    amount1_min: U256,
) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "decreaseLiquidity((uint256,uint128,uint256,uint256,uint256))".into(),
        args: vec![
            DecodedArg {
                name: "tokenId".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(token_id),
            },
            DecodedArg {
                name: "liquidity".into(),
                abi_type: "uint128".into(),
                value: DecodedValue::Uint(liquidity),
            },
            DecodedArg {
                name: "amount0Min".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount0_min),
            },
            DecodedArg {
                name: "amount1Min".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount1_min),
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

#[test]
fn slipstream_npm_decrease_liquidity_envelope_survives_serde_roundtrip() {
    let bundle: AdapterFunctionBundle =
        serde_json::from_str(SLIPSTREAM_NPM_DECREASE_LIQUIDITY_BUNDLE)
            .expect("Slipstream NPM decreaseLiquidity bundle parses");
    let mapper = DeclarativeMapper::new(bundle);
    let ctx = Ctx::new();

    let decoded = slipstream_npm_decrease_liquidity_decoded(
        mapper.declarative_decoder_id(),
        U256::from(12_345_u64),
        U256::from(1_000_000_000_000_000_u64),
        U256::from(1_u64),
        U256::from(1_u64),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("Slipstream NPM decreaseLiquidity maps");
    assert_eq!(envelopes.len(), 1);

    let json = serde_json::to_string(&envelopes[0]).expect("envelope serialises");
    serde_json::from_str::<ActionEnvelope>(&json).unwrap_or_else(|err| {
        panic!(
            "decreaseLiquidity envelope must deserialize back (evaluate-stage \
             contract); got error: {err}\njson: {json}"
        )
    });
}
