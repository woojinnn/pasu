//! Built-in functions invoked by `ValueExpr::Transform`.
//!
//! Phase 1A: only `select_address` is implemented. Spec §5.3.1 ("WhitelistedFn").
//! Phase 3 adds `unfold_v3_path` (TierBBackedFn — backend wraps
//! [`abi_resolver::subdecode::protocols::uniswap_v3::decode_v3_path`]).
//!
//! All built-ins operate over `serde_json::Value` — the interpreter normalises
//! `DecodedValue` to JSON in [`super::eval`] and then walks JSON paths /
//! invokes built-ins on the JSON view. This keeps the interpreter generic
//! across argument types at the cost of one normalisation pass per call.

use std::str::FromStr as _;

use abi_resolver::subdecode::protocols::uniswap_v3::{decode_v3_path, PathDecodeError};
use policy_engine::action::Address;
use thiserror::Error;

/// Error variants for built-in evaluation.
#[derive(Debug, Error)]
pub enum FnError {
    #[error("select_address: argument is not an array (got {0})")]
    NotArray(&'static str),
    #[error("select_address: index out of bounds (idx={idx}, len={len})")]
    IndexOutOfBounds { idx: i64, len: usize },
    #[error("select_address: element {idx} is not an address (value={value})")]
    NotAddress { idx: i64, value: serde_json::Value },
    #[error("select_address: invalid address {value}: {message}")]
    InvalidAddress { value: String, message: String },

    // ── unfold_v3_path ────────────────────────────────────────────────────
    /// `bytes` argument was neither a `"0x.."` hex string nor an array of
    /// numeric octets.
    #[error("unfold_v3_path: bytes argument has unsupported shape: {message}")]
    BytesShape { message: String },
    /// V3 packed path failed structural validation (length not `20 + 23*N`).
    #[error("unfold_v3_path: {error}")]
    PathDecode { error: PathDecodeError },
    /// `select` literal was not one of the four supported modes
    /// (`first_token`, `last_token`, `first_fee`, `last_fee`).
    #[error(
        "unfold_v3_path: unknown select {0:?} \
         (allowed: first_token, last_token, first_fee, last_fee)"
    )]
    UnknownSelect(String),
    /// `select` argument was not a JSON string.
    #[error("unfold_v3_path: select must be a string literal, got {0}")]
    SelectNotString(serde_json::Value),

    // ── curve_route_last_token (Phase 12.3) ──────────────────────────────
    /// Argument shape was not what the built-in expected (e.g. expected
    /// `address[]`, got scalar). Used by `curve_route_last_token`.
    #[error("type mismatch: expected {expected}, got {got}")]
    TypeMismatch {
        expected: &'static str,
        got: serde_json::Value,
    },
    /// Fixed-size array length disagreement. Used by `curve_route_last_token`,
    /// which requires exactly 11 slots.
    #[error("length mismatch: expected {expected}, got {got}")]
    LengthMismatch { expected: usize, got: usize },
    /// `curve_route_last_token` found an all-zero route (every even-index
    /// slot was `address(0)`), so no output token could be resolved.
    #[error("curve_route_last_token: route is empty (all token slots zero)")]
    EmptyRoute,
    /// `curve_route_last_token` resolved a per-hop `swap_type` outside the
    /// Router NG documented range `1..=9` (Router.vy v1.2.0). Router.vy itself
    /// fails closed on unknown swap types; the resolver mirrors that rather
    /// than silently picking a wrong output slot.
    #[error("curve_route_last_token: unknown swap_type {swap_type} (allowed: 1..=9)")]
    UnknownSwapType { swap_type: i64 },
    /// `curve_route_last_token` could not read `swap_params[i][2]`. Either the
    /// argument was not a 2-D JSON array, an inner row was missing index `[2]`,
    /// or the slot was not coercible to integer.
    #[error("curve_route_last_token: swap_params shape error — {reason}")]
    SwapParamsShape {
        /// Static reason string ('outer must be array' / 'inner row missing
        /// slot `[2]`' / 'inner slot not coercible to integer' / …).
        reason: &'static str,
    },

    // ── unfold_slipstream_path (Phase 8 — Aerodrome) ─────────────────────
    /// Slipstream packed path failed structural validation
    /// (length not `20 + 23 * N` for any `N >= 1`).
    #[error("unfold_slipstream_path: {message}")]
    SlipstreamPathDecode { message: String },
    /// `tick_spacing_at_hop` was called without a numeric `hop_index` arg.
    #[error("unfold_slipstream_path: tick_spacing_at_hop requires i64 hop_index arg")]
    SlipstreamHopIndexMissing,

    // ── unfold_velo_v2_path (Phase 2 — Aerodrome UR V2_SWAP) ─────────────
    /// Velo / Uni V2 packed path failed the minimum-length check. The
    /// path must hold at least two 20-byte tokens (`len >= 40`); anything
    /// shorter cannot yield both a first and a last token.
    #[error("unfold_velo_v2_path: {message}")]
    VeloV2PathDecode { message: String },
    /// `select` literal was not one of the two supported modes
    /// (`first_token`, `last_token`).
    #[error(
        "unfold_velo_v2_path: unknown select {0:?} \
         (allowed: first_token, last_token)"
    )]
    VeloV2UnknownSelect(String),

    // ── path-decoder contract violation (AUDIT_PHASE8 #13) ───────────────
    /// A packed-path decoder reported success yet returned an empty
    /// `tokens` / `fees` / `tick_spacings` collection — i.e. the decoder's
    /// "≥ 2 tokens, ≥ 1 fee/tick on success" contract was not upheld.
    ///
    /// In practice the decoders' length validation makes this unreachable,
    /// but the endpoint selectors (`first_token` / `last_fee` / …) must not
    /// `.expect()` on that invariant: a contract regression would otherwise
    /// panic the WASM module instead of surfacing as an `Err` verdict.
    #[error("{builtin}: decoder returned empty {collection} despite reporting success")]
    PathDecoderContract {
        /// Built-in that observed the violation
        /// (`unfold_v3_path` / `unfold_slipstream_path`).
        builtin: &'static str,
        /// Which collection was unexpectedly empty
        /// (`tokens` / `fees` / `tick_spacings`).
        collection: &'static str,
    },
}

/// `select_address(arr: address[], idx: i64) -> AddressRef` (spec §5.3.1).
///
/// `idx` semantics:
///   * `idx >= 0` — pick `arr[idx]`.
///   * `idx <  0` — pick `arr[arr.len() + idx]` (e.g. `-1` = last element).
///
/// Out-of-bounds yields [`FnError::IndexOutOfBounds`].
pub fn select_address(arr: &serde_json::Value, idx: i64) -> Result<Address, FnError> {
    let elements = arr.as_array().ok_or(FnError::NotArray(
        "select_address expects address[] (json array)",
    ))?;

    let len = elements.len();
    let resolved = resolve_index(idx, len)?;
    let element = &elements[resolved];

    let raw = element.as_str().ok_or_else(|| FnError::NotAddress {
        idx,
        value: element.clone(),
    })?;

    Address::from_str(raw).map_err(|message| FnError::InvalidAddress {
        value: raw.to_owned(),
        message,
    })
}

/// `unfold_v3_path(bytes: Bytes, select) -> AddressRef | u32`
/// (spec §5.3.2 — TierBBackedFn).
///
/// Backend wraps [`abi_resolver::subdecode::protocols::uniswap_v3::decode_v3_path`].
/// The packed-path format is `[token0(20B)][fee0(3B)][token1(20B)][fee1(3B)] ...`,
/// so token-endpoint and fee-endpoint selectors map directly onto the decoded
/// `(Vec<Address>, Vec<u32>)`.
///
/// Supported `select` modes:
///   * `"first_token"` / `"last_token"` — return JSON string containing the
///     lowercase `0x..` address (Phase 3).
///   * `"first_fee"` / `"last_fee"` — return JSON number with the uint24 fee
///     (Phase 7B / T-B3, e.g. `500` for the 0.05% tier).
///
/// `bytes_value` accepts either:
///   * JSON string `"0x.."` (the canonical encoding produced by
///     [`super::eval::decoded_value_to_json`] for `DecodedValue::Bytes`).
///   * JSON array of integers — each element must be in `0..=255`.
///
/// Anything else yields [`FnError::BytesShape`]. Decoding errors propagate as
/// [`FnError::PathDecode`].
pub fn unfold_v3_path(
    bytes_value: &serde_json::Value,
    select: &str,
) -> Result<serde_json::Value, FnError> {
    let bytes = json_value_to_bytes(bytes_value)?;
    let (tokens, fees) = decode_v3_path(&bytes).map_err(|error| FnError::PathDecode { error })?;

    // `decode_v3_path` contracts `tokens.len() >= 2` / `fees.len() >= 1` on
    // success; if a regression breaks that, surface an `Err` verdict rather
    // than panicking the WASM module (AUDIT_PHASE8 #13).
    match select {
        "first_token" => {
            let alloy_addr = *tokens.first().ok_or(FnError::PathDecoderContract {
                builtin: "unfold_v3_path",
                collection: "tokens",
            })?;
            Ok(serde_json::Value::String(address_to_json(alloy_addr)?))
        }
        "last_token" => {
            let alloy_addr = *tokens.last().ok_or(FnError::PathDecoderContract {
                builtin: "unfold_v3_path",
                collection: "tokens",
            })?;
            Ok(serde_json::Value::String(address_to_json(alloy_addr)?))
        }
        "first_fee" => {
            let fee = *fees.first().ok_or(FnError::PathDecoderContract {
                builtin: "unfold_v3_path",
                collection: "fees",
            })?;
            Ok(serde_json::Value::Number(serde_json::Number::from(fee)))
        }
        "last_fee" => {
            let fee = *fees.last().ok_or(FnError::PathDecoderContract {
                builtin: "unfold_v3_path",
                collection: "fees",
            })?;
            Ok(serde_json::Value::Number(serde_json::Number::from(fee)))
        }
        other => Err(FnError::UnknownSelect(other.to_owned())),
    }
}

/// `unfold_slipstream_path(bytes, select [, hop_index]) -> AddressRef | i32`
/// (Phase 8 — Aerodrome CL).
///
/// Slipstream path encoding mirrors the Uniswap V3 shape but the middle 3
/// bytes are a **signed** `int24 tickSpacing` instead of a `uint24 fee`:
///
/// ```text
/// [token0 (20)][tickSpacing0 (3)][token1 (20)][tickSpacing1 (3)] … [tokenN (20)]
/// ```
///
/// Sign-extension is required on decode (high bit of byte 0 → `0xFF` padding).
///
/// Supported `select` modes:
/// * `"first_token"` / `"last_token"` — JSON string of lowercase address
/// * `"first_tick_spacing"` / `"last_tick_spacing"` — JSON number (i64, sign-preserved)
/// * `"tick_spacing_at_hop"` — JSON number, requires `extra_arg` = i64 hop index
///   (negative idx counts from the end, like `select_address`)
///
/// `bytes_value` accepts: hex string `"0x.."` OR JSON array of u8 (same as
/// [`unfold_v3_path`]).
pub fn unfold_slipstream_path(
    bytes_value: &serde_json::Value,
    select: &str,
    extra_arg: Option<&serde_json::Value>,
) -> Result<serde_json::Value, FnError> {
    let bytes = json_value_to_bytes(bytes_value)?;
    let (tokens, tick_spacings) = decode_slipstream_path(&bytes)?;
    // `decode_slipstream_path` contracts `tokens.len() >= 2` /
    // `tick_spacings.len() >= 1` on success; on a contract regression surface
    // an `Err` verdict rather than panicking the WASM module — the new
    // Aerodrome UR CL-swap path reaches this code (AUDIT_PHASE8 #13).
    match select {
        "first_token" => {
            let alloy_addr = *tokens.first().ok_or(FnError::PathDecoderContract {
                builtin: "unfold_slipstream_path",
                collection: "tokens",
            })?;
            Ok(serde_json::Value::String(address_to_json(alloy_addr)?))
        }
        "last_token" => {
            let alloy_addr = *tokens.last().ok_or(FnError::PathDecoderContract {
                builtin: "unfold_slipstream_path",
                collection: "tokens",
            })?;
            Ok(serde_json::Value::String(address_to_json(alloy_addr)?))
        }
        "first_tick_spacing" => {
            let ts = *tick_spacings.first().ok_or(FnError::PathDecoderContract {
                builtin: "unfold_slipstream_path",
                collection: "tick_spacings",
            })?;
            Ok(serde_json::Value::Number(serde_json::Number::from(
                i64::from(ts),
            )))
        }
        "last_tick_spacing" => {
            let ts = *tick_spacings.last().ok_or(FnError::PathDecoderContract {
                builtin: "unfold_slipstream_path",
                collection: "tick_spacings",
            })?;
            Ok(serde_json::Value::Number(serde_json::Number::from(
                i64::from(ts),
            )))
        }
        "tick_spacing_at_hop" => {
            let idx = extra_arg
                .and_then(serde_json::Value::as_i64)
                .ok_or(FnError::SlipstreamHopIndexMissing)?;
            let resolved = resolve_index(idx, tick_spacings.len())?;
            let ts = tick_spacings[resolved];
            Ok(serde_json::Value::Number(serde_json::Number::from(
                i64::from(ts),
            )))
        }
        other => Err(FnError::UnknownSelect(other.to_owned())),
    }
}

/// Decode a Slipstream packed path into `(tokens, tick_spacings)`.
///
/// Validates: `path.len() == 20 + 23 * N` for some `N >= 1`. Each
/// `tickSpacing` is a 3-byte big-endian `int24`; sign is extended into an
/// `i32` (high bit of byte 0 → `0xFF` padding).
///
/// Returns `(tokens, tick_spacings)` with `tokens.len() == hops + 1` and
/// `tick_spacings.len() == hops`.
fn decode_slipstream_path(
    bytes: &[u8],
) -> Result<(Vec<alloy_primitives::Address>, Vec<i32>), FnError> {
    const ADDR_SIZE: usize = 20;
    const TICK_SIZE: usize = 3;
    const NEXT_OFFSET: usize = ADDR_SIZE + TICK_SIZE; // 23
    const MIN_LEN: usize = NEXT_OFFSET + ADDR_SIZE; // 43 (one full pool)

    if bytes.len() < MIN_LEN {
        return Err(FnError::SlipstreamPathDecode {
            message: format!(
                "path too short: {} bytes (must be >= {} for one hop)",
                bytes.len(),
                MIN_LEN
            ),
        });
    }
    if !(bytes.len() - ADDR_SIZE).is_multiple_of(NEXT_OFFSET) {
        return Err(FnError::SlipstreamPathDecode {
            message: format!(
                "malformed length {}: must be 20 + 23*N for some N >= 1",
                bytes.len()
            ),
        });
    }

    let pool_count = (bytes.len() - ADDR_SIZE) / NEXT_OFFSET;
    let mut tokens = Vec::with_capacity(pool_count + 1);
    let mut tick_spacings = Vec::with_capacity(pool_count);

    tokens.push(alloy_primitives::Address::from_slice(&bytes[0..ADDR_SIZE]));
    for hop in 0..pool_count {
        let off = ADDR_SIZE + hop * NEXT_OFFSET;
        // Sign-extend int24 (3 bytes big-endian) → i32. High bit of byte[off]
        // is the int24 sign bit; if set, prepend 0xFF to make a 4-byte i32.
        let hi = bytes[off];
        let mid = bytes[off + 1];
        let lo = bytes[off + 2];
        let ts_signed = if hi & 0x80 != 0 {
            i32::from_be_bytes([0xFF, hi, mid, lo])
        } else {
            i32::from_be_bytes([0x00, hi, mid, lo])
        };
        tick_spacings.push(ts_signed);
        tokens.push(alloy_primitives::Address::from_slice(
            &bytes[off + TICK_SIZE..off + TICK_SIZE + ADDR_SIZE],
        ));
    }

    Ok((tokens, tick_spacings))
}

/// `unfold_velo_v2_path(bytes: Bytes, select) -> AddressRef`
/// (Phase 2 — Aerodrome Universal Router `V2_SWAP`, opcodes `0x08`/`0x09`).
///
/// The Aerodrome UR `main` build encodes the V2 swap path as a packed
/// `bytes` blob of 20-byte token addresses. The stride between tokens
/// depends on the per-command `isUni` flag (research
/// `docs/AERODROME_UR_RESEARCH.md` §3.1):
///
/// ```text
/// UniV2  (isUni = true):  token(20) ++ token(20) ++ …          len = 20*N,    N >= 2
/// VeloV2 (isUni = false): token(20) ++ stable(1) ++ token(20) ++ stable(1) ++ … ++ token(20)
///                                                              len = 20 + 21*N
/// ```
///
/// Both layouts share an invariant: the path always **starts and ends
/// on a 20-byte token**. The first token is therefore `path[0..20]` and
/// the last token is `path[len-20..len]` regardless of `isUni` or the
/// stride — this built-in never has to parse the `stable` byte.
///
/// `bytes_value` accepts either:
///   * JSON string `"0x.."` (the canonical encoding produced by
///     [`super::eval::decoded_value_to_json`] for `DecodedValue::Bytes`).
///   * JSON array of integers — each element must be in `0..=255`.
///
/// Supported `select` modes:
///   * `"first_token"` — JSON string of the lowercase `0x..` address at
///     `path[0..20]`.
///   * `"last_token"` — JSON string of the lowercase `0x..` address at
///     `path[len-20..len]`.
///
/// Errors:
///   * [`FnError::BytesShape`] — `bytes_value` is neither a hex string
///     nor a `u8` array.
///   * [`FnError::VeloV2PathDecode`] — `bytes.len() < 40`, i.e. the path
///     cannot hold both a first and a last token.
///   * [`FnError::VeloV2UnknownSelect`] — `select` is not `first_token`
///     or `last_token`.
///
/// Never panics — every failure path is an `Err`.
pub fn unfold_velo_v2_path(
    bytes_value: &serde_json::Value,
    select: &str,
) -> Result<serde_json::Value, FnError> {
    const ADDR_SIZE: usize = 20;
    /// Two 20-byte tokens — the shortest path that has both endpoints.
    const MIN_LEN: usize = ADDR_SIZE * 2;

    let bytes = json_value_to_bytes(bytes_value)?;
    if bytes.len() < MIN_LEN {
        return Err(FnError::VeloV2PathDecode {
            message: format!(
                "path too short: {} bytes \
                 (must be >= {MIN_LEN} to hold a first and last token)",
                bytes.len()
            ),
        });
    }

    // `bytes.len() >= 40` makes both slices exactly 20 bytes wide and
    // in-bounds, so neither the index nor `Address::from_slice` panics.
    let token_slice = match select {
        "first_token" => &bytes[0..ADDR_SIZE],
        "last_token" => &bytes[bytes.len() - ADDR_SIZE..],
        other => return Err(FnError::VeloV2UnknownSelect(other.to_owned())),
    };
    let alloy_addr = alloy_primitives::Address::from_slice(token_slice);
    Ok(serde_json::Value::String(address_to_json(alloy_addr)?))
}

/// `alloy_primitives::Address` → lowercase `0x..` string, validated against
/// the project's [`Address`] hex regex.
fn address_to_json(alloy_addr: alloy_primitives::Address) -> Result<String, FnError> {
    let hex_repr = format!("0x{}", hex::encode(alloy_addr.0));
    let address = Address::from_str(&hex_repr).map_err(|message| FnError::InvalidAddress {
        value: hex_repr.clone(),
        message,
    })?;
    Ok(address.to_string())
}

/// Coerce a JSON value into raw bytes — accepting either the `"0x.."` hex
/// form (default for `DecodedValue::Bytes` via [`super::eval`]) or a JSON
/// array of u8.
fn json_value_to_bytes(value: &serde_json::Value) -> Result<Vec<u8>, FnError> {
    match value {
        serde_json::Value::String(s) => {
            let stripped = s.strip_prefix("0x").unwrap_or(s.as_str());
            hex::decode(stripped).map_err(|e| FnError::BytesShape {
                message: format!("hex decode of {s:?} failed: {e}"),
            })
        }
        serde_json::Value::Array(elements) => {
            let mut out = Vec::with_capacity(elements.len());
            for (i, element) in elements.iter().enumerate() {
                let byte = element.as_u64().ok_or_else(|| FnError::BytesShape {
                    message: format!("array element {i} is not a u64: {element}"),
                })?;
                if byte > 255 {
                    return Err(FnError::BytesShape {
                        message: format!("array element {i} = {byte} > 255"),
                    });
                }
                out.push(byte as u8);
            }
            Ok(out)
        }
        other => Err(FnError::BytesShape {
            message: format!("expected hex string or array, got {other}"),
        }),
    }
}

/// `curve_route_last_token(route: address[11], swap_params: uint256[N][5]) -> AddressRef`
/// — Phase 12.3, P1-5 (Phase 13), F3 + F-route1.B (Phase C, V3 round).
///
/// Curve Router NG `exchange(...)` encodes a 1-to-5-hop swap path as a fixed-
/// size `address[11]` array — interleaved token / pool slots:
///   * `route[2i]`   (i = 0..=5) — token slot (0 = input, 2..10 = hop outputs)
///   * `route[2i+1]` (i = 0..=4) — pool / helper / vault address of hop i
///   * unused trailing slots = `address(0)`
///
/// `Router.vy::exchange` runs hop `i` and then **breaks** as soon as the next
/// hop's pool slot is `address(0)` (`if _route[i*2+3] == empty(address): break`).
/// Equivalently, hop `i` executes only while its own pool `route[2i+1]` is
/// non-zero.
///
/// Per-hop output-token resolution depends on `swap_params[i][2] = swap_type`
/// (Router.vy v1.2.0 docstring; mirrored in
/// `abi_resolver::subdecode::protocols::curve::CURVE_ROUTER_NG_SWAP_TYPES`):
///
/// * `1` `STABLESWAP_EXCHANGE`               → output = `route[2i+2]` (coin)
/// * `2` `EXCHANGE_UNDERLYING`               → output = `route[2i+2]` (coin)
/// * `3` `ZAP_UNDERLYING_EXCHANGE`           → output = `route[2i+2]` (coin)
/// * `4` `COIN_TO_LP_ADD_LIQUIDITY`          → output = `route[2i+1]`
///   (pool acts as LP token)
/// * `5` `LENDING_UNDERLYING_TO_LP`          → output = `route[2i+1]`
///   (pool acts as LP token)
/// * `6` `LP_TO_COIN_REMOVE_LIQUIDITY_ONE_COIN`
///   → output = `route[2i+2]` (coin)
/// * `7` `LP_TO_LENDING_UNDERLYING`          → output = `route[2i+2]` (coin)
/// * `8` `WRAPPED_ASSET_CONVERT`             → output = `route[2i+1]`
///   (wrap-helper contract is the wrapped-asset token, e.g. wstETH wrapper *is* wstETH)
/// * `9` `ERC4626_ASSET_SHARE`               → output = `route[2i+1]`
///   (ERC-4626 vault is the share token)
///
/// Pre-fix the resolver always returned `route[2k+2]` regardless of swap_type.
/// For swap_type=4/5/8/9 that yielded the wrong address: a sentinel (e.g.
/// `0xeee…` for ETH in stETH→wstETH wraps) or whatever caller-supplied padding
/// happened to sit in the trailing token slot, producing envelopes that asserted
/// outputs the swap never actually produced. That let token-allowlist policies
/// be silently bypassed when the user wrapped or LP-added through Router NG
/// (BACKWARD_CURVE_V2.md §3 F3, F-route1.B).
///
/// `swap_params` is the same 2-D array the bundle passes via
/// `{ "from": "$.args._swap_params" }`. Both `uint256[5][5]` (Router NG v1.1+
/// mainnet / chain 1, 10, 56, 100, 137, 250, 8453, 42161, 43114, 2222) and
/// `uint256[4][5]` (Router NG v1.0 — Fraxtal 252, zkSync 324, Mantle 5000,
/// X-Layer 196) variants encode swap_type at inner index `[2]`. The function
/// only reads `swap_params[i][2]` and ignores the rest, so it is variant-agnostic.
///
/// Errors:
///   * [`FnError::TypeMismatch`] — argument is not a JSON array (or a read
///     slot is not a JSON string / integer).
///   * [`FnError::LengthMismatch`] — `route` length is not exactly 11.
///   * [`FnError::EmptyRoute`] — no executable hop (`route[1]` is zero) or the
///     resolved output token slot is itself `address(0)` (degenerate route).
///   * [`FnError::UnknownSwapType`] — swap_type ∉ 1..=9 for an executed hop
///     (Router.vy fails closed; the resolver must too).
///   * [`FnError::SwapParamsShape`] — `swap_params` is not a 2-D array, an inner
///     hop row is missing index `[2]`, or it cannot be coerced to integer.
///
/// Source: `curvefi/curve-router-ng` @ `1014d369` / `contracts/Router.vy::exchange`
/// (`for i in range(5)` + break on `_route[i*2+3] == empty(address)` +
/// per-hop `swap_type` docstring lines 32-46 of `Router.vy`).
pub fn curve_route_last_token(
    route_value: &serde_json::Value,
    swap_params_value: &serde_json::Value,
) -> Result<serde_json::Value, FnError> {
    let arr = route_value
        .as_array()
        .ok_or_else(|| FnError::TypeMismatch {
            expected: "array",
            got: route_value.clone(),
        })?;

    if arr.len() != 11 {
        return Err(FnError::LengthMismatch {
            expected: 11,
            got: arr.len(),
        });
    }

    let params_outer = swap_params_value
        .as_array()
        .ok_or_else(|| FnError::SwapParamsShape {
            reason: "swap_params must be a JSON array (outer)",
        })?;

    // Read slot `idx` as an address string, surfacing non-strings as TypeMismatch.
    fn addr_str(arr: &[serde_json::Value], idx: usize) -> Result<&str, FnError> {
        arr[idx].as_str().ok_or_else(|| FnError::TypeMismatch {
            expected: "address string",
            got: arr[idx].clone(),
        })
    }

    // Read `swap_params[i][2]` as a swap_type integer. Decoders may surface
    // `uint256` slots as a JSON Number (small values) or JSON String (large /
    // hex-encoded values); both must be accepted, matching the convention in
    // `select_from_literal_array` for `i`/`j` indices.
    fn swap_type_for_hop(outer: &[serde_json::Value], i: usize) -> Result<u8, FnError> {
        let inner = outer
            .get(i)
            .ok_or(FnError::SwapParamsShape {
                reason: "swap_params is shorter than the executed hop count",
            })?
            .as_array()
            .ok_or(FnError::SwapParamsShape {
                reason: "swap_params inner row must be a JSON array",
            })?;
        let raw = inner.get(2).ok_or(FnError::SwapParamsShape {
            reason: "swap_params[i] is missing the swap_type slot at index [2]",
        })?;
        let st = coerce_to_i64(raw).ok_or(FnError::SwapParamsShape {
            reason: "swap_params[i][2] cannot be coerced to an integer",
        })?;
        // Router.vy v1.2.0 docstring documents 1..9 inclusive. Out-of-range
        // values would cause Router.vy itself to revert at the per-hop
        // dispatcher; mirror that here as a fail-closed error rather than
        // silently emitting a wrong output token.
        if !(1..=9).contains(&st) {
            return Err(FnError::UnknownSwapType { swap_type: st });
        }
        Ok(st as u8)
    }

    // hop i: input = route[2i], pool = route[2i+1], swap_type = params[i][2].
    // Mirror Router.vy's early-break — hop i runs only while its pool slot is
    // non-zero. For each executed hop, pick its output token slot per swap_type:
    //   * 1/2/3/6/7 → `route[2i+2]` (coin output, default swap convention)
    //   * 4/5/8/9   → `route[2i+1]` (pool/helper/vault address acts as the
    //                                emitted asset)
    let mut last_output_idx = 0usize;
    for i in 0..5 {
        if is_zero_address(addr_str(arr, 2 * i + 1)?) {
            break;
        }
        let st = swap_type_for_hop(params_outer, i)?;
        last_output_idx = match st {
            1 | 2 | 3 | 6 | 7 => 2 * i + 2,
            4 | 5 | 8 | 9 => 2 * i + 1,
            // Unreachable — `swap_type_for_hop` already validates 1..=9.
            _ => unreachable!("swap_type validated to 1..=9"),
        };
    }

    // `last_output_idx == 0` → route[1] (pool of hop 0) is zero → no executable
    // hop. A zero output token slot is an equally degenerate route. Fail closed.
    if last_output_idx == 0 || is_zero_address(addr_str(arr, last_output_idx)?) {
        return Err(FnError::EmptyRoute);
    }

    Ok(arr[last_output_idx].clone())
}

/// Case-insensitive comparison against the canonical zero address. Bundles
/// MAY emit either lowercase or EIP-55 mixed-case `0x0...0` — both should
/// resolve to "padded slot".
fn is_zero_address(addr: &str) -> bool {
    addr.eq_ignore_ascii_case("0x0000000000000000000000000000000000000000")
}

/// `select_from_literal_array(array, idx) -> Value` — Phase 12.7 P0-2.
///
/// Pick an element from a bundle-embedded literal array (typically a Curve
/// pool `coins[]`) by a caller-supplied integer index. Used by V1 / V2 / NG
/// `exchange` + `remove_liquidity_one_coin` bundles to resolve `coins[i]` /
/// `coins[j]` instead of hardcoding the first/second token of the pool —
/// the old bundles silently mislabelled inputs and outputs whenever the
/// user passed any `(i, j) != (0, 1)` (P0-2 audit finding).
///
/// `idx` semantics mirror [`select_address`]:
///   * `idx >= 0` — pick `array[idx]`.
///   * `idx <  0` — pick `array[array.len() + idx]` (e.g. `-1` = last).
///
/// `idx_value` may be supplied as a JSON integer, JSON string of a signed
/// decimal integer, or a JSON object wrapper (interpreted via `as_i64`).
/// Curve `exchange` accepts `int128` i/j values which serialize as either
/// `Number` (when the decoder produces small values) or `String` (when the
/// value is large or hex-formatted); both paths are accepted.
///
/// Errors:
///   * [`FnError::TypeMismatch`] — `array_value` is not a JSON array.
///   * [`FnError::TypeMismatch`] — `idx_value` cannot be coerced to `i64`.
///   * [`FnError::IndexOutOfBounds`] — resolved index is outside
///     `0..array.len()`.
pub fn select_from_literal_array(
    array_value: &serde_json::Value,
    idx_value: &serde_json::Value,
) -> Result<serde_json::Value, FnError> {
    let arr = array_value
        .as_array()
        .ok_or_else(|| FnError::TypeMismatch {
            expected: "array",
            got: array_value.clone(),
        })?;
    let idx = coerce_to_i64(idx_value).ok_or_else(|| FnError::TypeMismatch {
        expected: "integer index",
        got: idx_value.clone(),
    })?;
    let resolved = resolve_index(idx, arr.len())?;
    Ok(arr[resolved].clone())
}

/// Accept `idx_value` as any of: JSON integer, JSON decimal string
/// (`"3"`, `"-1"`), or anything else `as_i64` recognises. Returns `None`
/// when the value cannot be reduced to a signed 64-bit integer.
fn coerce_to_i64(value: &serde_json::Value) -> Option<i64> {
    if let Some(n) = value.as_i64() {
        return Some(n);
    }
    if let Some(u) = value.as_u64() {
        return i64::try_from(u).ok();
    }
    value.as_str().and_then(|s| s.parse::<i64>().ok())
}

fn resolve_index(idx: i64, len: usize) -> Result<usize, FnError> {
    if idx >= 0 {
        let resolved = usize::try_from(idx).map_err(|_| FnError::IndexOutOfBounds { idx, len })?;
        if resolved >= len {
            return Err(FnError::IndexOutOfBounds { idx, len });
        }
        Ok(resolved)
    } else {
        let abs_negative =
            usize::try_from(-idx).map_err(|_| FnError::IndexOutOfBounds { idx, len })?;
        if abs_negative > len {
            return Err(FnError::IndexOutOfBounds { idx, len });
        }
        Ok(len - abs_negative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn addrs() -> serde_json::Value {
        json!([
            "0x1111111111111111111111111111111111111111",
            "0x2222222222222222222222222222222222222222",
            "0x3333333333333333333333333333333333333333",
        ])
    }

    #[test]
    fn select_address_idx_zero_picks_first() {
        let address = select_address(&addrs(), 0).unwrap();
        assert_eq!(
            address.to_string(),
            "0x1111111111111111111111111111111111111111"
        );
    }

    #[test]
    fn select_address_idx_neg_one_picks_last() {
        let address = select_address(&addrs(), -1).unwrap();
        assert_eq!(
            address.to_string(),
            "0x3333333333333333333333333333333333333333"
        );
    }

    #[test]
    fn select_address_idx_neg_two_picks_second_from_last() {
        let address = select_address(&addrs(), -2).unwrap();
        assert_eq!(
            address.to_string(),
            "0x2222222222222222222222222222222222222222"
        );
    }

    #[test]
    fn select_address_idx_oob_pos_errors() {
        let err = select_address(&addrs(), 3).unwrap_err();
        assert!(matches!(err, FnError::IndexOutOfBounds { .. }));
    }

    #[test]
    fn select_address_idx_oob_neg_errors() {
        let err = select_address(&addrs(), -4).unwrap_err();
        assert!(matches!(err, FnError::IndexOutOfBounds { .. }));
    }

    #[test]
    fn select_address_non_array_errors() {
        let err = select_address(&json!("0x1234"), 0).unwrap_err();
        assert!(matches!(err, FnError::NotArray(_)));
    }

    // ── unfold_v3_path ────────────────────────────────────────────────────

    /// V3 packed path `WETH --0x000bb8--> USDC` (fee 3000 = 0x000bb8).
    /// Total length: 20 + 3 + 20 = 43 bytes — one hop.
    const SINGLE_HOP_PATH_HEX: &str = concat!(
        "0xC02aaa39b223FE8D0A0e5C4F27eAD9083C756Cc2",
        "000bb8",
        "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
    );

    /// Two-hop path: `WETH --3000--> USDC --500--> USDT`.
    /// Length = 20 + 3 + 20 + 3 + 20 = 66 bytes.
    const TWO_HOP_PATH_HEX: &str = concat!(
        "0xC02aaa39b223FE8D0A0e5C4F27eAD9083C756Cc2",
        "000bb8",
        "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        "0001f4",
        "dAC17F958D2ee523a2206206994597C13D831ec7",
    );

    #[test]
    fn unfold_v3_path_first_token_single_hop() {
        let value = unfold_v3_path(&json!(SINGLE_HOP_PATH_HEX), "first_token").unwrap();
        // WETH (lowercased — policy_engine::action::Address normalises).
        assert_eq!(
            value.as_str().unwrap(),
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        );
    }

    #[test]
    fn unfold_v3_path_last_token_single_hop() {
        let value = unfold_v3_path(&json!(SINGLE_HOP_PATH_HEX), "last_token").unwrap();
        // USDC
        assert_eq!(
            value.as_str().unwrap(),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
    }

    #[test]
    fn unfold_v3_path_multi_hop_endpoints() {
        let first = unfold_v3_path(&json!(TWO_HOP_PATH_HEX), "first_token").unwrap();
        let last = unfold_v3_path(&json!(TWO_HOP_PATH_HEX), "last_token").unwrap();
        // First = WETH, last = USDT (third token in the chain).
        assert_eq!(
            first.as_str().unwrap(),
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        );
        assert_eq!(
            last.as_str().unwrap(),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    #[test]
    fn unfold_v3_path_accepts_array_of_u8() {
        // Same single-hop path, but supplied as a JSON array of octets.
        let raw = hex::decode(SINGLE_HOP_PATH_HEX.strip_prefix("0x").unwrap()).unwrap();
        let array_json = serde_json::Value::Array(raw.iter().map(|b| json!(*b)).collect());
        let first = unfold_v3_path(&array_json, "first_token").unwrap();
        assert_eq!(
            first.as_str().unwrap(),
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        );
    }

    // ── unfold_v3_path: fee modes (Phase 7B / T-B3) ─────────────────────

    /// Two-hop path `USDT --0x0001f4(500)--> USDC --0x000bb8(3000)--> WETH`.
    /// 66 bytes (20 + 3 + 20 + 3 + 20).
    const FEE_TWO_HOP_PATH_HEX: &str = concat!(
        "0xdAC17F958D2ee523a2206206994597C13D831ec7",
        "0001f4",
        "A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
        "000bb8",
        "C02aaa39b223FE8D0A0e5C4F27eAD9083C756Cc2",
    );

    #[test]
    fn unfold_v3_path_first_fee_returns_500_for_two_hop() {
        let value = unfold_v3_path(&json!(FEE_TWO_HOP_PATH_HEX), "first_fee").unwrap();
        assert_eq!(value, json!(500));
    }

    #[test]
    fn unfold_v3_path_last_fee_returns_3000() {
        let value = unfold_v3_path(&json!(FEE_TWO_HOP_PATH_HEX), "last_fee").unwrap();
        assert_eq!(value, json!(3000));
    }

    #[test]
    fn unfold_v3_path_single_hop_first_fee_equals_last_fee() {
        let first = unfold_v3_path(&json!(SINGLE_HOP_PATH_HEX), "first_fee").unwrap();
        let last = unfold_v3_path(&json!(SINGLE_HOP_PATH_HEX), "last_fee").unwrap();
        // Both endpoints coincide on a one-hop path — the fee is 3000.
        assert_eq!(first, json!(3000));
        assert_eq!(last, json!(3000));
    }

    #[test]
    fn unfold_v3_path_too_short_errors() {
        // 20 + 22 = 42 bytes — neither single token nor a full hop.
        let bytes_hex = format!("0x{}", "11".repeat(42));
        let err = unfold_v3_path(&json!(bytes_hex), "first_token").unwrap_err();
        assert!(matches!(err, FnError::PathDecode { .. }));
    }

    // ── AUDIT_PHASE8 #13 — malformed path is an `Err`, never a panic ─────
    // `unfold_v3_path` used to `.expect()` on the decoder's "≥ 2 tokens / ≥ 1
    // fee" contract. A panic inside the WASM module would abort the whole
    // evaluation instead of producing a verdict. These tests reach every
    // endpoint selector with structurally malformed input — the `#[test]`
    // harness treats any panic as a failure, so `unwrap_err()` succeeding is
    // itself the proof that no `.expect()` fires.

    #[test]
    fn unfold_v3_path_malformed_does_not_panic_for_any_select() {
        // 19 bytes — shorter than even a single bare token address.
        let malformed = format!("0x{}", "ab".repeat(19));
        for select in ["first_token", "last_token", "first_fee", "last_fee"] {
            let err = unfold_v3_path(&json!(malformed), select).unwrap_err();
            // Length validation rejects this before the endpoint selector —
            // the point is it is an `Err`, not an `.expect()` panic.
            assert!(
                matches!(err, FnError::PathDecode { .. }),
                "select {select:?}: expected PathDecode, got {err:?}"
            );
        }
    }

    #[test]
    fn unfold_v3_path_empty_bytes_does_not_panic() {
        // Zero-length path — the most degenerate malformed input.
        let err = unfold_v3_path(&json!("0x"), "last_fee").unwrap_err();
        assert!(matches!(err, FnError::PathDecode { .. }));
    }

    #[test]
    fn unfold_v3_path_well_formed_still_byte_identical_after_fix() {
        // Regression — the `.expect()` → `.ok_or()?` swap must not change
        // the success path. Endpoints of the canonical two-hop fixture.
        assert_eq!(
            unfold_v3_path(&json!(TWO_HOP_PATH_HEX), "first_token").unwrap(),
            json!("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
        );
        assert_eq!(
            unfold_v3_path(&json!(TWO_HOP_PATH_HEX), "last_token").unwrap(),
            json!("0xdac17f958d2ee523a2206206994597c13d831ec7"),
        );
        assert_eq!(
            unfold_v3_path(&json!(FEE_TWO_HOP_PATH_HEX), "first_fee").unwrap(),
            json!(500),
        );
        assert_eq!(
            unfold_v3_path(&json!(FEE_TWO_HOP_PATH_HEX), "last_fee").unwrap(),
            json!(3000),
        );
    }

    #[test]
    fn unfold_v3_path_unknown_select_errors() {
        let err = unfold_v3_path(&json!(SINGLE_HOP_PATH_HEX), "middle_token").unwrap_err();
        assert!(matches!(err, FnError::UnknownSelect(_)));
    }

    #[test]
    fn unfold_v3_path_non_bytes_errors() {
        let err = unfold_v3_path(&json!(42), "first_token").unwrap_err();
        assert!(matches!(err, FnError::BytesShape { .. }));
    }

    // ── unfold_slipstream_path (Phase 8 — Aerodrome CL) ──────────────────

    /// Single-hop Slipstream path with positive `tickSpacing=100` (0x000064):
    /// `0xaaa…01 --100--> 0xbbb…02`. Length = 20 + 3 + 20 = 43 bytes.
    const SLIP_SINGLE_HOP_HEX: &str = concat!(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
        "000064",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
    );

    #[test]
    fn unfold_slipstream_path_single_hop_first_last_token() {
        let v = serde_json::Value::String(SLIP_SINGLE_HOP_HEX.to_owned());
        let first = unfold_slipstream_path(&v, "first_token", None).unwrap();
        let last = unfold_slipstream_path(&v, "last_token", None).unwrap();
        assert_eq!(
            first.as_str().unwrap(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01"
        );
        assert_eq!(
            last.as_str().unwrap(),
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02"
        );
    }

    #[test]
    fn unfold_slipstream_path_first_tick_spacing_positive() {
        let v = serde_json::Value::String(SLIP_SINGLE_HOP_HEX.to_owned());
        let ts = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        assert_eq!(ts.as_i64().unwrap(), 100);
    }

    #[test]
    fn unfold_slipstream_path_negative_tick_spacing_sign_extension() {
        // tickSpacing = -1 = 0xFFFFFF (int24 — all bits set, sign bit set)
        let bytes_hex = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "ffffff",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
        );
        let v = serde_json::Value::String(bytes_hex.to_owned());
        let ts = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        assert_eq!(ts.as_i64().unwrap(), -1);
    }

    #[test]
    fn unfold_slipstream_path_max_positive_tick_spacing() {
        // tickSpacing = 0x7FFFFF = 8_388_607 (int24 max positive)
        let bytes_hex = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "7fffff",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
        );
        let v = serde_json::Value::String(bytes_hex.to_owned());
        let ts = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        assert_eq!(ts.as_i64().unwrap(), 8_388_607);
    }

    #[test]
    fn unfold_slipstream_path_min_negative_tick_spacing() {
        // tickSpacing = 0x800000 = -8_388_608 (int24 min negative)
        let bytes_hex = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "800000",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
        );
        let v = serde_json::Value::String(bytes_hex.to_owned());
        let ts = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        assert_eq!(ts.as_i64().unwrap(), -8_388_608);
    }

    #[test]
    fn unfold_slipstream_path_two_hop_tick_spacing_at_hop() {
        // tokenA + ts=50 (0x000032) + tokenB + ts=100 (0x000064) + tokenC
        let bytes_hex = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "000032",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
            "000064",
            "cccccccccccccccccccccccccccccccccccccc03",
        );
        let v = serde_json::Value::String(bytes_hex.to_owned());
        let hop0 = unfold_slipstream_path(&v, "tick_spacing_at_hop", Some(&json!(0))).unwrap();
        let hop1 = unfold_slipstream_path(&v, "tick_spacing_at_hop", Some(&json!(1))).unwrap();
        let hop_last = unfold_slipstream_path(&v, "tick_spacing_at_hop", Some(&json!(-1))).unwrap();
        assert_eq!(hop0.as_i64().unwrap(), 50);
        assert_eq!(hop1.as_i64().unwrap(), 100);
        assert_eq!(hop_last.as_i64().unwrap(), 100);
    }

    #[test]
    fn unfold_slipstream_path_two_hop_last_tick_spacing() {
        let bytes_hex = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "000032",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
            "000064",
            "cccccccccccccccccccccccccccccccccccccc03",
        );
        let v = serde_json::Value::String(bytes_hex.to_owned());
        let first = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        let last = unfold_slipstream_path(&v, "last_tick_spacing", None).unwrap();
        assert_eq!(first.as_i64().unwrap(), 50);
        assert_eq!(last.as_i64().unwrap(), 100);
    }

    #[test]
    fn unfold_slipstream_path_malformed_too_short() {
        // length 30 — too short for one full pool (need >= 43)
        let bytes: Vec<u8> = vec![0xAA; 30];
        let v = serde_json::Value::Array(bytes.into_iter().map(|b| json!(b)).collect());
        let err = unfold_slipstream_path(&v, "first_token", None).unwrap_err();
        assert!(matches!(err, FnError::SlipstreamPathDecode { .. }));
    }

    #[test]
    fn unfold_slipstream_path_malformed_misaligned() {
        // length 50 — not 20 + 23*N for any N (20+23=43, 20+46=66)
        let bytes: Vec<u8> = vec![0xAA; 50];
        let v = serde_json::Value::Array(bytes.into_iter().map(|b| json!(b)).collect());
        let err = unfold_slipstream_path(&v, "first_token", None).unwrap_err();
        assert!(matches!(err, FnError::SlipstreamPathDecode { .. }));
    }

    #[test]
    fn unfold_slipstream_path_unknown_select() {
        let v = serde_json::Value::String(SLIP_SINGLE_HOP_HEX.to_owned());
        let err = unfold_slipstream_path(&v, "invalid_select", None).unwrap_err();
        assert!(matches!(err, FnError::UnknownSelect(_)));
    }

    #[test]
    fn unfold_slipstream_path_accepts_array_of_u8() {
        let raw = hex::decode(SLIP_SINGLE_HOP_HEX.strip_prefix("0x").unwrap()).unwrap();
        let v = serde_json::Value::Array(raw.iter().map(|b| json!(*b)).collect());
        let ts = unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap();
        assert_eq!(ts.as_i64().unwrap(), 100);
    }

    #[test]
    fn unfold_slipstream_path_tick_spacing_at_hop_without_arg_errors() {
        let v = serde_json::Value::String(SLIP_SINGLE_HOP_HEX.to_owned());
        let err = unfold_slipstream_path(&v, "tick_spacing_at_hop", None).unwrap_err();
        assert!(matches!(err, FnError::SlipstreamHopIndexMissing));
    }

    // ── AUDIT_PHASE8 #13 — malformed Slipstream path is `Err`, not panic ─
    // The new Aerodrome Universal Router CL-swap rule routes packed paths
    // through `unfold_slipstream_path`, so a malformed path from a UR
    // command stream must surface as a verdict — not an `.expect()` panic
    // that aborts WASM evaluation. As with the V3 case, any panic fails the
    // `#[test]` harness, so a successful `unwrap_err()` is the proof.

    #[test]
    fn unfold_slipstream_path_malformed_does_not_panic_for_any_select() {
        // 30 bytes — too short for one full pool (need >= 43).
        let bytes: Vec<u8> = vec![0xCD; 30];
        let v = serde_json::Value::Array(bytes.iter().map(|b| json!(*b)).collect());
        for select in [
            "first_token",
            "last_token",
            "first_tick_spacing",
            "last_tick_spacing",
        ] {
            let err = unfold_slipstream_path(&v, select, None).unwrap_err();
            assert!(
                matches!(err, FnError::SlipstreamPathDecode { .. }),
                "select {select:?}: expected SlipstreamPathDecode, got {err:?}"
            );
        }
        // `tick_spacing_at_hop` (3-arg form) must also fail closed.
        let err = unfold_slipstream_path(&v, "tick_spacing_at_hop", Some(&json!(0))).unwrap_err();
        assert!(matches!(err, FnError::SlipstreamPathDecode { .. }));
    }

    #[test]
    fn unfold_slipstream_path_empty_bytes_does_not_panic() {
        // Zero-length path — the most degenerate malformed input.
        let v = serde_json::Value::Array(vec![]);
        let err = unfold_slipstream_path(&v, "last_token", None).unwrap_err();
        assert!(matches!(err, FnError::SlipstreamPathDecode { .. }));
    }

    #[test]
    fn unfold_slipstream_path_well_formed_still_byte_identical_after_fix() {
        // Regression — the `.expect()` → `.ok_or()?` swap must leave the
        // success path unchanged. Two-hop fixture, every endpoint selector.
        let two_hop = concat!(
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
            "000032", // ts = 50
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
            "000064", // ts = 100
            "cccccccccccccccccccccccccccccccccccccc03",
        );
        let v = serde_json::Value::String(two_hop.to_owned());
        assert_eq!(
            unfold_slipstream_path(&v, "first_token", None).unwrap(),
            json!("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01"),
        );
        assert_eq!(
            unfold_slipstream_path(&v, "last_token", None).unwrap(),
            json!("0xcccccccccccccccccccccccccccccccccccccc03"),
        );
        assert_eq!(
            unfold_slipstream_path(&v, "first_tick_spacing", None).unwrap(),
            json!(50),
        );
        assert_eq!(
            unfold_slipstream_path(&v, "last_tick_spacing", None).unwrap(),
            json!(100),
        );
    }

    // ── unfold_velo_v2_path (Phase 2 — Aerodrome UR V2_SWAP) ─────────────
    //
    // Aerodrome UR `main` V2_SWAP packs the path two ways depending on the
    // per-command `isUni` flag (docs/AERODROME_UR_RESEARCH.md §3.1):
    //   * UniV2  layout — `token(20) ++ token(20) ++ …`              len = 20*N
    //   * VeloV2 layout — `token(20) ++ stable(1) ++ token(20) ++ …`  len = 20 + 21*N
    // The built-in only ever reads `path[0..20]` / `path[len-20..len]`,
    // which are tokens in both layouts — the stable byte is never parsed.

    /// UniV2 layout, 2 tokens — `0xaa…01 ++ 0xbb…02`. Length 40 (`20*2`).
    const VELO_UNI_2TOKEN_HEX: &str = concat!(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
    );

    /// UniV2 layout, 3 tokens — `0xaa…01 ++ 0xbb…02 ++ 0xcc…03`.
    /// Length 60 (`20*3`).
    const VELO_UNI_3TOKEN_HEX: &str = concat!(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
        "cccccccccccccccccccccccccccccccccccccc03",
    );

    /// VeloV2 layout, N=1 — `token ++ stable ++ token`. The `stable`
    /// byte is `0x01`; length 41 (`20 + 21*1`).
    const VELO_VELO_N1_HEX: &str = concat!(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
        "01", // stable flag — never parsed by unfold_velo_v2_path
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
    );

    /// VeloV2 layout, N=2 — `token ++ stable ++ token ++ stable ++ token`.
    /// Stable bytes `0x00` / `0x01`; length 62 (`20 + 21*2`).
    const VELO_VELO_N2_HEX: &str = concat!(
        "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01",
        "00", // stable flag (hop 1)
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02",
        "01", // stable flag (hop 2)
        "cccccccccccccccccccccccccccccccccccccc03",
    );

    const VELO_TOKEN_A: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01";
    const VELO_TOKEN_B: &str = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb02";
    const VELO_TOKEN_C: &str = "0xcccccccccccccccccccccccccccccccccccccc03";

    #[test]
    fn unfold_velo_v2_path_first_token() {
        // First token = path[0..20] = token A — identical across both the
        // UniV2 (20*N) and VeloV2 (20 + 21*N) strides.
        for hex in [
            VELO_UNI_2TOKEN_HEX,
            VELO_UNI_3TOKEN_HEX,
            VELO_VELO_N1_HEX,
            VELO_VELO_N2_HEX,
        ] {
            let first = unfold_velo_v2_path(&json!(hex), "first_token").unwrap();
            assert_eq!(
                first.as_str().unwrap(),
                VELO_TOKEN_A,
                "first_token mismatch for {hex}"
            );
        }
    }

    #[test]
    fn unfold_velo_v2_path_last_token() {
        // Last token = path[len-20..len]. UniV2 2-token + VeloV2 N=1 end on
        // token B; UniV2 3-token + VeloV2 N=2 end on token C.
        let cases = [
            (VELO_UNI_2TOKEN_HEX, VELO_TOKEN_B),
            (VELO_VELO_N1_HEX, VELO_TOKEN_B),
            (VELO_UNI_3TOKEN_HEX, VELO_TOKEN_C),
            (VELO_VELO_N2_HEX, VELO_TOKEN_C),
        ];
        for (hex, expected) in cases {
            let last = unfold_velo_v2_path(&json!(hex), "last_token").unwrap();
            assert_eq!(
                last.as_str().unwrap(),
                expected,
                "last_token mismatch for {hex}"
            );
        }
    }

    #[test]
    fn unfold_velo_v2_path_accepts_array_of_u8() {
        // The `bytes` argument may also arrive as a JSON array of octets
        // (mirrors `unfold_v3_path` / `unfold_slipstream_path`).
        let raw = hex::decode(VELO_VELO_N1_HEX.strip_prefix("0x").unwrap()).unwrap();
        let array_json = serde_json::Value::Array(raw.iter().map(|b| json!(*b)).collect());
        let first = unfold_velo_v2_path(&array_json, "first_token").unwrap();
        let last = unfold_velo_v2_path(&array_json, "last_token").unwrap();
        assert_eq!(first.as_str().unwrap(), VELO_TOKEN_A);
        assert_eq!(last.as_str().unwrap(), VELO_TOKEN_B);
    }

    #[test]
    fn unfold_velo_v2_path_too_short_errs() {
        // 39 bytes — one byte short of two full tokens. Must be an `Err`,
        // never an `.expect()` panic (the `#[test]` harness fails on any
        // panic, so a successful `unwrap_err()` is itself the proof).
        let short_hex = format!("0x{}", "11".repeat(39));
        for select in ["first_token", "last_token"] {
            let err = unfold_velo_v2_path(&json!(short_hex), select).unwrap_err();
            assert!(
                matches!(err, FnError::VeloV2PathDecode { .. }),
                "select {select:?}: expected VeloV2PathDecode, got {err:?}"
            );
        }
        // Zero-length path — the most degenerate malformed input.
        let err = unfold_velo_v2_path(&json!("0x"), "first_token").unwrap_err();
        assert!(matches!(err, FnError::VeloV2PathDecode { .. }));
        // Empty u8 array — same degenerate case via the array branch.
        let err = unfold_velo_v2_path(&serde_json::Value::Array(vec![]), "last_token").unwrap_err();
        assert!(matches!(err, FnError::VeloV2PathDecode { .. }));
    }

    #[test]
    fn unfold_velo_v2_path_unknown_select_errs() {
        // A well-formed (40-byte) path with an unsupported `select` must
        // fail closed with `VeloV2UnknownSelect` — no fee modes exist here.
        let err = unfold_velo_v2_path(&json!(VELO_UNI_2TOKEN_HEX), "first_fee").unwrap_err();
        assert!(matches!(err, FnError::VeloV2UnknownSelect(_)));
        let err = unfold_velo_v2_path(&json!(VELO_UNI_2TOKEN_HEX), "middle_token").unwrap_err();
        assert!(matches!(err, FnError::VeloV2UnknownSelect(_)));
    }

    #[test]
    fn unfold_velo_v2_path_non_bytes_errs() {
        // A non-string / non-array `bytes` argument is a `BytesShape` error
        // (shared with `unfold_v3_path`), surfaced before any length check.
        let err = unfold_velo_v2_path(&json!(42), "first_token").unwrap_err();
        assert!(matches!(err, FnError::BytesShape { .. }));
    }

    #[test]
    fn unfold_velo_v2_path_exactly_min_len_two_tokens() {
        // Boundary — exactly 40 bytes (the minimum). first == token A,
        // last == token B, and the two endpoints must not coincide.
        let first = unfold_velo_v2_path(&json!(VELO_UNI_2TOKEN_HEX), "first_token").unwrap();
        let last = unfold_velo_v2_path(&json!(VELO_UNI_2TOKEN_HEX), "last_token").unwrap();
        assert_eq!(first.as_str().unwrap(), VELO_TOKEN_A);
        assert_eq!(last.as_str().unwrap(), VELO_TOKEN_B);
        assert_ne!(first, last);
    }
}

#[cfg(test)]
mod tests_curve_route_last_token {
    use super::*;
    use serde_json::json;

    /// Build a `swap_params: uint256[5][5]` fixture where hop 0 has
    /// `swap_type = st` and every remaining hop is fully zero. Matches the
    /// shape the Curve Router NG ABI passes via `$.args._swap_params` for
    /// both the 5-arg `uint256[5][5]` and the 4-arg `uint256[4][5]` variants
    /// (only `[i][2]` is read).
    fn swap_params_first_hop(st: u64) -> serde_json::Value {
        let mut hop0 = vec![json!(0u64); 5];
        hop0[2] = json!(st);
        json!([
            hop0,
            vec![json!(0u64); 5],
            vec![json!(0u64); 5],
            vec![json!(0u64); 5],
            vec![json!(0u64); 5]
        ])
    }

    /// Build a `swap_params` with per-hop `swap_type` values supplied directly.
    /// Each inner row is 5 slots; unspecified hops fill with zeros.
    fn swap_params_per_hop(types: &[u64]) -> serde_json::Value {
        let mut rows: Vec<serde_json::Value> = (0..5)
            .map(|i| {
                let st = types.get(i).copied().unwrap_or(0);
                let mut row = vec![json!(0u64); 5];
                row[2] = json!(st);
                json!(row)
            })
            .collect();
        // Guarantee exactly 5 rows (per-hop max in Router NG).
        rows.truncate(5);
        json!(rows)
    }

    /// 1-hop swap_type=1 (STABLESWAP_EXCHANGE) route: `_route = [USDC, 3pool,
    /// USDT, 0×8]`. Output token = idx 2 (USDT) — coin output convention.
    #[test]
    fn one_hop_swap_type_1_returns_idx_2() {
        let route = json!([
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // [0] USDC
            "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7", // [1] 3pool
            "0xdac17f958d2ee523a2206206994597c13d831ec7", // [2] USDT
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let result = curve_route_last_token(&route, &swap_params_first_hop(1)).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    /// 5-hop swap_type=1 route: every even idx (0/2/4/6/8/10) has a token,
    /// every odd idx has a pool. All hops are STABLESWAP_EXCHANGE → output
    /// = idx 10 (coin convention applied at hop 4).
    #[test]
    fn five_hop_all_swap_type_1_returns_idx_10() {
        let route = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1", // [0] input
            "0x1111111111111111111111111111111111111111", // [1] pool1
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2", // [2] mid1
            "0x2222222222222222222222222222222222222222", // [3] pool2
            "0xcccccccccccccccccccccccccccccccccccccccc", // [4] mid2
            "0x3333333333333333333333333333333333333333", // [5] pool3
            "0xdddddddddddddddddddddddddddddddddddddddd", // [6] mid3
            "0x4444444444444444444444444444444444444444", // [7] pool4
            "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", // [8] mid4
            "0x5555555555555555555555555555555555555555", // [9] pool5
            "0xfffffffffffffffffffffffffffffffffffffff5", // [10] output
        ]);
        let result =
            curve_route_last_token(&route, &swap_params_per_hop(&[1, 1, 1, 1, 1])).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            "0xfffffffffffffffffffffffffffffffffffffff5"
        );
    }

    /// All-zero route → EmptyRoute. Mirrors Router.vy's `for i in range(5)` +
    /// pool break — no hop executes, so no output token can be resolved.
    #[test]
    fn empty_route_returns_error() {
        let route = json!([
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let err = curve_route_last_token(&route, &swap_params_first_hop(1)).unwrap_err();
        assert!(matches!(err, FnError::EmptyRoute));
    }

    /// Length validation — Curve Router NG always passes exactly 11 slots.
    /// Any other length is a calldata corruption (or wrong decoder).
    #[test]
    fn wrong_length_returns_error() {
        let route = json!([
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let err = curve_route_last_token(&route, &swap_params_first_hop(1)).unwrap_err();
        assert!(matches!(
            err,
            FnError::LengthMismatch {
                expected: 11,
                got: 2
            }
        ));
    }

    /// Argument-type validation — non-array `route` surfaces as TypeMismatch
    /// (vs panic-on-cast).
    #[test]
    fn non_array_returns_error() {
        let err = curve_route_last_token(&json!("0xdead"), &swap_params_first_hop(1)).unwrap_err();
        assert!(matches!(
            err,
            FnError::TypeMismatch {
                expected: "array",
                ..
            }
        ));
    }

    /// P1-5 regression — a zero pool slot mid-route terminates the swap.
    /// `_route = [A, P0, B, 0x0(pool1), MID, P2, D, 0x0×4]`: on-chain the swap
    /// runs hop 0 only (A->B) and stops because pool1 is zero. The output is B
    /// (idx 2), NOT D — the pre-fix scan returned the last non-zero even slot
    /// (D) and let calldata assert an unreachable output token. With swap_type=1
    /// hop 0 the coin-output convention picks idx 2.
    #[test]
    fn gap_route_stops_at_first_zero_pool() {
        let route = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1", // [0] input A
            "0x1111111111111111111111111111111111111111", // [1] pool0
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2", // [2] output B
            "0x0000000000000000000000000000000000000000", // [3] pool1 = ZERO -> break
            "0xcccccccccccccccccccccccccccccccccccccccc", // [4] MID (unreachable)
            "0x2222222222222222222222222222222222222222", // [5] pool2
            "0xdddddddddddddddddddddddddddddddddddddddd", // [6] D (unreachable)
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let result = curve_route_last_token(&route, &swap_params_first_hop(1)).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2"
        );
    }

    /// Fail-closed — if the resolved output token slot is itself `address(0)`
    /// (hop 0 has a pool but its output slot is zero), surface EmptyRoute
    /// instead of emitting a zero-address output token into the envelope.
    /// swap_type=1 still picks the coin slot at `route[2]`.
    #[test]
    fn zero_output_token_returns_error() {
        let route = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1", // [0] input
            "0x1111111111111111111111111111111111111111", // [1] pool0
            "0x0000000000000000000000000000000000000000", // [2] output = ZERO
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let err = curve_route_last_token(&route, &swap_params_first_hop(1)).unwrap_err();
        assert!(matches!(err, FnError::EmptyRoute));
    }

    // ── F3 + F-route1.B swap_type branch tests (Phase C V3 round) ─────────

    /// **F3 root-cause regression** — stETH→wstETH wrap (Etherscan tx
    /// `0x29bbfa2d…`, BACKWARD_CURVE_V2.md fixture #14). The on-chain
    /// `_route = [stETH, wstETH-contract, ETH-sentinel, 0×8]` +
    /// `_swap_params[0][2] = 8 (WRAPPED_ASSET_CONVERT)`.
    ///
    /// Pre-fix: the resolver returned `_route[2]` = ETH sentinel
    /// (`0xeee…`) — silent misdecode that emitted "stETH → ETH swap" and let
    /// token-allowlist policies be bypassed.
    ///
    /// Post-fix: with swap_type=8 the wrap-helper convention applies and the
    /// resolver returns `_route[1]` = the wstETH contract itself, which IS the
    /// wstETH token. The envelope now asserts the actual on-chain output.
    #[test]
    fn swap_type_8_wrapped_asset_returns_helper_slot() {
        let stetth = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84"; // stETH
        let wsteth = "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"; // wstETH (token = wrap helper)
        let eth_sentinel = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
        let route = json!([
            stetth,
            wsteth,       // [1] = wrap-helper contract = wstETH token (POST-FIX output)
            eth_sentinel, // [2] = trailing sentinel (PRE-FIX silent-misdecode output)
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let result = curve_route_last_token(&route, &swap_params_first_hop(8)).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            wsteth,
            "swap_type=8 must resolve to route[1] (wrap-helper contract = wrapped asset token), \
             NOT route[2] (which holds the unrelated ETH sentinel in the wrap-helper convention)"
        );
        assert_ne!(
            result.as_str().unwrap(),
            eth_sentinel,
            "regression guard — pre-fix bug returned the ETH sentinel here"
        );
    }

    /// **F-route1.B root-cause regression** — `swap_type=4`
    /// (COIN_TO_LP_ADD_LIQUIDITY). Pool address itself is the LP token in
    /// Curve convention; the trailing route slot (`route[2]`) holds an
    /// unrelated sentinel / padding that has no semantic meaning.
    ///
    /// Pre-fix: the resolver returned `route[2]` regardless of swap_type,
    /// emitting a swap envelope with the wrong output token and letting an
    /// add_liquidity allow-rule be bypassed (`category=dex / action=swap`
    /// while the on-chain effect was adding liquidity to a pool).
    ///
    /// Post-fix: `route[2i+1]` (pool address = LP token) is returned for
    /// swap_type=4.
    #[test]
    fn swap_type_4_lp_add_returns_pool_slot() {
        let coin_a = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"; // USDC
        let pool_lp = "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7"; // 3pool = LP token
        let unrelated_padding = "0xdeaddeaddeaddeaddeaddeaddeaddeaddeaddead";
        let route = json!([
            coin_a,
            pool_lp,           // [1] = pool = LP token (POST-FIX output)
            unrelated_padding, // [2] = sentinel/padding (PRE-FIX wrong output)
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let result = curve_route_last_token(&route, &swap_params_first_hop(4)).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            pool_lp,
            "swap_type=4 must resolve to route[1] (pool = LP token in Curve convention)"
        );
    }

    /// **F-route1.B `swap_type=6` regression** —
    /// LP_TO_COIN_REMOVE_LIQUIDITY_ONE_COIN. Output IS a coin
    /// (the underlying received from the pool), so `route[2i+2]` is correct.
    /// Distinct branch from swap_type=4: confirms the resolver does not lump
    /// "anything LP-related" into the pool-slot branch.
    #[test]
    fn swap_type_6_lp_remove_returns_coin_slot() {
        let lp_in = "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7"; // 3pool LP
        let pool = "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7"; // same pool acts as router target
        let usdt_out = "0xdac17f958d2ee523a2206206994597c13d831ec7"; // USDT (coin out)
        let route = json!([
            lp_in,
            pool,
            usdt_out, // [2] = coin output (correct for swap_type=6)
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let result = curve_route_last_token(&route, &swap_params_first_hop(6)).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            usdt_out,
            "swap_type=6 (remove_liquidity_one_coin) must resolve to route[2] (coin slot)"
        );
    }

    /// **swap_type=9 (ERC4626_ASSET_SHARE)** — vault address IS the share
    /// token. Same convention as swap_type=8 (route[2i+1] is the emitted
    /// asset).
    #[test]
    fn swap_type_9_erc4626_returns_vault_slot() {
        let asset = "0x6b175474e89094c44da98b954eedeac495271d0f"; // DAI
        let vault = "0xfeeeefeeefefeeefeefeeeefeefefeeefefeefef"; // ERC4626 vault = share token
        let route = json!([
            asset,
            vault,
            "0xdeaddeaddeaddeaddeaddeaddeaddeaddeaddead", // unrelated padding
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let result = curve_route_last_token(&route, &swap_params_first_hop(9)).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            vault,
            "swap_type=9 (ERC4626) must resolve to route[1] (vault = share token)"
        );
    }

    /// **swap_type=5 (LENDING_UNDERLYING_TO_LP)** — same convention as
    /// swap_type=4 (pool = LP output).
    #[test]
    fn swap_type_5_lending_to_lp_returns_pool_slot() {
        let route = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2", // pool = LP token
            "0xcccccccccccccccccccccccccccccccccccccccc", // unrelated
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let result = curve_route_last_token(&route, &swap_params_first_hop(5)).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2"
        );
    }

    /// **swap_type=7 (LP_TO_LENDING_UNDERLYING)** — same convention as
    /// swap_type=6 (coin output at route[2i+2]).
    #[test]
    fn swap_type_7_lp_to_lending_returns_coin_slot() {
        let route = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2",
            "0xcccccccccccccccccccccccccccccccccccccccc", // coin out
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let result = curve_route_last_token(&route, &swap_params_first_hop(7)).unwrap();
        assert_eq!(
            result.as_str().unwrap(),
            "0xcccccccccccccccccccccccccccccccccccccccc"
        );
    }

    /// **Multi-hop branch mixing** — hops 0/1/2 = swap_type=1 (coin) →
    /// swap_type=8 (wrap) → swap_type=1 (coin). Confirms the per-hop
    /// dispatcher resolves the last EXECUTED hop's convention, not a
    /// uniform "first hop only" or "last hop only" rule.
    /// Sequence: USDC --(stableswap)--> DAI --(WRAP)--> wDAI --(stableswap)--> USDT.
    /// Final hop is swap_type=1 → output = route[6] (USDT coin).
    #[test]
    fn multi_hop_mixed_swap_types_uses_last_executed_hop_convention() {
        let usdc = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
        let stable_pool = "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7";
        let dai = "0x6b175474e89094c44da98b954eedeac495271d0f";
        let wrapper = "0x7777777777777777777777777777777777777777";
        let wdai = "0x8888888888888888888888888888888888888888";
        let stable_pool_2 = "0x9999999999999999999999999999999999999999";
        let usdt = "0xdac17f958d2ee523a2206206994597c13d831ec7";
        let route = json!([
            usdc,
            stable_pool,
            dai,
            wrapper,
            wdai,
            stable_pool_2,
            usdt,
            "0x0000000000000000000000000000000000000000", // pool3 = 0 → break after hop 2
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        // hop 0 = STABLESWAP_EXCHANGE (1) → route[2]
        // hop 1 = WRAPPED_ASSET_CONVERT (8) → route[3]
        // hop 2 = STABLESWAP_EXCHANGE (1) → route[6] = USDT  ← final
        let result =
            curve_route_last_token(&route, &swap_params_per_hop(&[1, 8, 1, 0, 0])).unwrap();
        assert_eq!(result.as_str().unwrap(), usdt);
    }

    /// **Unknown swap_type** — Router.vy v1.2.0 documents 1..=9. Anything
    /// outside that range would revert on-chain at the per-hop dispatcher.
    /// The resolver fails closed with `UnknownSwapType` rather than silently
    /// picking a wrong output slot.
    #[test]
    fn unknown_swap_type_returns_error() {
        let route = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2",
            "0xcccccccccccccccccccccccccccccccccccccccc",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let err = curve_route_last_token(&route, &swap_params_first_hop(10)).unwrap_err();
        assert!(matches!(err, FnError::UnknownSwapType { swap_type: 10 }));

        let err_zero = curve_route_last_token(&route, &swap_params_first_hop(0)).unwrap_err();
        assert!(matches!(
            err_zero,
            FnError::UnknownSwapType { swap_type: 0 }
        ));
    }

    /// **Malformed swap_params** — non-array outer surfaces as
    /// `SwapParamsShape` rather than `TypeMismatch` (the latter is reserved
    /// for `route`), keeping the error space coherent for downstream
    /// interpreters that may branch on the variant.
    #[test]
    fn swap_params_not_array_returns_shape_error() {
        let route = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2",
            "0xcccccccccccccccccccccccccccccccccccccccc",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        let err = curve_route_last_token(&route, &json!("not-an-array")).unwrap_err();
        assert!(matches!(err, FnError::SwapParamsShape { .. }));
    }

    /// **swap_params row missing slot [2]** — Router NG ABI requires
    /// `uint256[N][5]` with N >= 3 (swap_type is at inner `[2]`). A row
    /// shorter than 3 elements is malformed.
    #[test]
    fn swap_params_inner_too_short_returns_shape_error() {
        let route = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb2",
            "0xcccccccccccccccccccccccccccccccccccccccc",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
            "0x0000000000000000000000000000000000000000",
        ]);
        // hop 0 row has only 2 slots — missing index [2].
        let malformed = json!([[json!(0u64), json!(0u64)]]);
        let err = curve_route_last_token(&route, &malformed).unwrap_err();
        assert!(matches!(err, FnError::SwapParamsShape { .. }));
    }
}

#[cfg(test)]
mod tests_select_from_literal_array {
    use super::*;
    use serde_json::json;

    fn coins() -> serde_json::Value {
        // Curve 3pool coins: DAI / USDC / USDT.
        json!([
            "0x6b175474e89094c44da98b954eedeac495271d0f",
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "0xdac17f958d2ee523a2206206994597c13d831ec7",
        ])
    }

    #[test]
    fn idx_zero_returns_first_coin_dai() {
        let value = select_from_literal_array(&coins(), &json!(0)).unwrap();
        assert_eq!(
            value.as_str().unwrap(),
            "0x6b175474e89094c44da98b954eedeac495271d0f"
        );
    }

    #[test]
    fn idx_two_returns_usdt() {
        // P0-2 anchor — the previous bundles hardcoded coins[0] / coins[1],
        // so a tx with `i = 2` would mislabel the input token as DAI.
        let value = select_from_literal_array(&coins(), &json!(2)).unwrap();
        assert_eq!(
            value.as_str().unwrap(),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    #[test]
    fn idx_neg_one_returns_last() {
        let value = select_from_literal_array(&coins(), &json!(-1)).unwrap();
        assert_eq!(
            value.as_str().unwrap(),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    #[test]
    fn idx_decimal_string_is_accepted() {
        // Curve int128 i/j values can serialize through the decoder as
        // strings — make sure both paths produce the same lookup.
        let value = select_from_literal_array(&coins(), &json!("1")).unwrap();
        assert_eq!(
            value.as_str().unwrap(),
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
        );
    }

    #[test]
    fn idx_negative_decimal_string_is_accepted() {
        let value = select_from_literal_array(&coins(), &json!("-1")).unwrap();
        assert_eq!(
            value.as_str().unwrap(),
            "0xdac17f958d2ee523a2206206994597c13d831ec7"
        );
    }

    #[test]
    fn out_of_bounds_positive_errors() {
        let err = select_from_literal_array(&coins(), &json!(5)).unwrap_err();
        assert!(matches!(err, FnError::IndexOutOfBounds { .. }));
    }

    #[test]
    fn out_of_bounds_negative_errors() {
        let err = select_from_literal_array(&coins(), &json!(-4)).unwrap_err();
        assert!(matches!(err, FnError::IndexOutOfBounds { .. }));
    }

    #[test]
    fn non_array_input_errors() {
        let err = select_from_literal_array(&json!("0xdeadbeef"), &json!(0)).unwrap_err();
        assert!(matches!(
            err,
            FnError::TypeMismatch {
                expected: "array",
                ..
            }
        ));
    }

    #[test]
    fn non_integer_index_errors() {
        let err = select_from_literal_array(&coins(), &json!("not-a-number")).unwrap_err();
        assert!(matches!(
            err,
            FnError::TypeMismatch {
                expected: "integer index",
                ..
            }
        ));
    }
}
