//! `$fn` call-form executors for the V3 `emit.body` DSL.
//!
//! The V3 placeholder walker
//! ([`substitute_placeholders`](super::action_builder::substitute_placeholders))
//! resolves a `{ "$fn": "<name>", "$args": [<arg templates>] }` object by
//! substituting each arg, then dispatching here **by name**. These are the
//! WhitelistedFn backends (§5.3.2 / [`super::types::BuiltinFn`]) that a single
//! `$args.x` reference or a `$match` value-map cannot express — notably Curve
//! Router NG's variable-hop output token, which depends on a per-hop swap-type.
//!
//! Each executor is JSON-in / JSON-out: args arrive **already substituted**
//! (addresses as lowercase `"0x"` hex strings, uints as decimal strings or JSON
//! numbers, per [`super::args_json`]), and the returned JSON is spliced back
//! into the `emit.body` template by the caller.

use std::str::FromStr as _;

use abi_resolver::decode::decode_with_signature;
use alloy_dyn_abi::DynSolValue;
use alloy_primitives::{keccak256, Address, U256};
use serde_json::Value as JsonValue;

/// The whitelist of accepted `$fn` names — the **sole** gate on `$fn` names, and
/// enforced at **decode time only**: [`dispatch`] rejects any name not listed here.
/// The author-time validator (`registryV2/scripts/build-index.ts`
/// `validateEmitShape`) checks only `emit.strategy`, NOT `$fn` names, so adding a
/// new `$fn` is a Rust-only change (a manifest referencing an unknown `$fn` builds
/// fine but fails to decode).
pub const WHITELIST: &[&str] = &[
    "curve_route_last_token",
    "route_hash",
    "keccak256",
    "address_from_uint256",
    "uniswap_v3_pool_swap_field",
    "uniswapx_reactor_order_field",
];

/// Dispatch a `$fn` call by name against its already-substituted JSON args.
///
/// Returns `Err(reason)` for an unknown function, a wrong arg count, or a
/// malformed/invalid argument; the caller wraps it in
/// [`V3BuildError::FnCall`](super::action_builder::V3BuildError::FnCall).
pub fn dispatch(name: &str, args: &[JsonValue]) -> Result<JsonValue, String> {
    match name {
        "curve_route_last_token" => curve_route_last_token(args),
        "route_hash" => route_hash(args),
        "keccak256" => keccak256_hex(args),
        "address_from_uint256" => address_from_uint256(args),
        "uniswap_v3_pool_swap_field" => uniswap_v3_pool_swap_field(args),
        "uniswapx_reactor_order_field" => uniswapx_reactor_order_field(args),
        _ => Err(format!(
            "unknown $fn '{name}' (whitelist: {})",
            WHITELIST.join(", ")
        )),
    }
}

/// `curve_route_last_token(route: address[11], swap_params: uint256[N][5]) -> address`.
///
/// Mirrors Curve `Router.vy::exchange` per-hop semantics: hop `i` executes while
/// the pool slot `route[2i+1]` is non-zero. The output token of the **last**
/// executed hop is `route[2i+2]` for coin-producing swap types (1/2/3/6/7) or
/// `route[2i+1]` for pool/LP/vault-producing swap types (4 `LP_ADD`,
/// 5 `LENDING_TO_LP`, 8 `WRAPPED_ASSET_CONVERT`, 9 `ERC4626_ASSET_SHARE`).
/// `swap_type` is read from `swap_params[i][2]` (the inner index is `[2]` for
/// both the `uint256[5][5]` and `uint256[4][5]` Router NG variants).
fn curve_route_last_token(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "curve_route_last_token expects 2 args (route, swap_params), got {}",
            args.len()
        ));
    }
    let route = args[0]
        .as_array()
        .ok_or("curve_route_last_token: route arg is not an array")?;
    let swap_params = args[1]
        .as_array()
        .ok_or("curve_route_last_token: swap_params arg is not an array")?;

    // Last executed hop = highest `i` whose pool slot route[2i+1] is non-zero.
    // Router NG packs at most 5 hops into the 11-element route array.
    let mut last_hop: Option<usize> = None;
    for i in 0..5usize {
        let pool_idx = 2 * i + 1; // 1, 3, 5, 7, 9
        match route.get(pool_idx) {
            Some(v) if !is_zero_address(v) => last_hop = Some(i),
            _ => break,
        }
    }
    let i = last_hop.ok_or("curve_route_last_token: empty route (no non-zero pool slot)")?;

    let swap_type_val = swap_params
        .get(i)
        .and_then(JsonValue::as_array)
        .and_then(|inner| inner.get(2))
        .ok_or_else(|| format!("curve_route_last_token: missing swap_params[{i}][2]"))?;
    // A real Router NG swap_type is a small integer in 1..=9. A value that does
    // not fit u64 (a fuzzed uint256) or is out of that enum is unreachable on a
    // real route — fold both into one "unknown swap_type" (soft under fuzzing,
    // hard-asserted against by the corpus/golden).
    let out_idx = match json_to_u64(swap_type_val) {
        Some(1 | 2 | 3 | 6 | 7) => 2 * i + 2, // coin-producing → next coin slot
        Some(4 | 5 | 8 | 9) => 2 * i + 1,     // pool/LP/vault is itself the output
        _ => {
            return Err(format!(
                "curve_route_last_token: unknown swap_type {swap_type_val} at hop {i}"
            ))
        }
    };
    route.get(out_idx).cloned().ok_or_else(|| {
        format!(
            "curve_route_last_token: route[{out_idx}] out of bounds (len {})",
            route.len()
        )
    })
}

/// `route_hash(route: address[11]) -> bytes32` — a deterministic ScopeBall
/// identity for an aggregator route (NOT an on-chain value). Defined as
/// `keccak256(route[0] ++ route[1] ++ … )` over the packed 20-byte addresses,
/// so the same structural route hashes identically regardless of amounts. Feeds
/// `AmmVenue::AggregatorRoute.route_hash` ("32-byte hex hash of the route").
fn route_hash(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 1 {
        return Err(format!(
            "route_hash expects 1 arg (route), got {}",
            args.len()
        ));
    }
    let route = args[0]
        .as_array()
        .ok_or("route_hash: route arg is not an array")?;
    let mut bytes = Vec::with_capacity(route.len() * 20);
    for (idx, v) in route.iter().enumerate() {
        let s = v
            .as_str()
            .ok_or_else(|| format!("route_hash: route[{idx}] is not a string address"))?;
        let addr = Address::from_str(s)
            .map_err(|e| format!("route_hash: route[{idx}] is not an address ({s}): {e}"))?;
        bytes.extend_from_slice(addr.as_slice());
    }
    Ok(JsonValue::String(format!("{:#x}", keccak256(&bytes))))
}

/// `keccak256(data: bytes) -> bytes32` — keccak256 of an opaque byte string,
/// returned as `0x`-prefixed 32-byte hex. General-purpose route/calldata identity
/// for aggregator venues whose route lives in an opaque `bytes` arg (e.g. 1inch v6
/// `swap(executor, desc, data)` — `data` carries the executor route). Feeds
/// `AmmVenue::AggregatorRoute.route_hash` ("32-byte hex hash of the route"). Unlike
/// [`route_hash`] (which packs an `address[11]` route), this hashes raw bytes.
fn keccak256_hex(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 1 {
        return Err(format!(
            "keccak256 expects 1 arg (data), got {}",
            args.len()
        ));
    }
    let bytes = json_hex_bytes(&args[0], "keccak256: data")?;
    Ok(JsonValue::String(format!("{:#x}", keccak256(&bytes))))
}

/// `address_from_uint256(packed: uint256) -> address` — unmask the low 160 bits
/// of a `uint256` into an address. 1inch's `AddressLib` packs an address into the
/// low 160 bits of a `uint256` (the high 96 bits carry flags), so a calldata or
/// struct field declared `uint256` that is really an address (Clipper `srcToken`,
/// LOP `Order` maker/makerAsset/takerAsset) is decoded to its address here.
/// Returns a lowercase `0x` hex address.
fn address_from_uint256(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 1 {
        return Err(format!(
            "address_from_uint256 expects 1 arg (packed uint256), got {}",
            args.len()
        ));
    }
    let packed = json_u256(&args[0], "address_from_uint256: packed")?;
    // Big-endian word; the low 160 bits = the last 20 bytes = the address.
    let bytes = packed.to_be_bytes::<32>();
    let addr = Address::from_slice(&bytes[12..]);
    Ok(JsonValue::String(format!("{addr:#x}")))
}

/// `uniswap_v3_pool_swap_field(amountSpecified, zeroForOne, token0, token1, field)`.
///
/// Direct `UniswapV3Pool.swap` exposes a signed `amountSpecified` instead of
/// router-style `amountIn` / `amountOutMinimum` fields:
/// - positive means exact input;
/// - negative means exact output;
/// - `zeroForOne` controls token direction.
///
/// The pool calldata has no explicit exact-output max input, so that field is
/// modeled as `U256::MAX` for a conservative spend cap until live price-limit
/// math can tighten it.
fn uniswap_v3_pool_swap_field(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 5 {
        return Err(format!(
            "uniswap_v3_pool_swap_field expects 5 args (amountSpecified, zeroForOne, token0, token1, field), got {}",
            args.len()
        ));
    }

    let amount = json_signed_int(&args[0], "amountSpecified")?;
    let zero_for_one = json_bool(&args[1], "zeroForOne")?;
    let token0 = json_address(&args[2], "token0")?.to_string();
    let token1 = json_address(&args[3], "token1")?.to_string();
    let field = args[4]
        .as_str()
        .ok_or("uniswap_v3_pool_swap_field: field arg is not a string")?;

    let (token_in, token_out) = if zero_for_one {
        (token0, token1)
    } else {
        (token1, token0)
    };

    let exact_output = amount.negative;
    let max_u256 = U256::MAX.to_string();
    let zero = "0".to_owned();
    let value = match field {
        "token_in" => JsonValue::String(token_in),
        "token_out" => JsonValue::String(token_out),
        "direction_kind" => JsonValue::String(
            if exact_output {
                "exact_output"
            } else {
                "exact_input"
            }
            .to_owned(),
        ),
        "amount_in" => JsonValue::String(if exact_output {
            zero.clone()
        } else {
            amount.abs_decimal.clone()
        }),
        "min_amount_out" => JsonValue::String(zero.clone()),
        "max_amount_in" => JsonValue::String(if exact_output { max_u256 } else { zero.clone() }),
        "amount_out" => JsonValue::String(if exact_output {
            amount.abs_decimal
        } else {
            zero
        }),
        _ => {
            return Err(format!(
                "uniswap_v3_pool_swap_field: unsupported field '{field}'"
            ))
        }
    };
    Ok(value)
}

/// `uniswapx_reactor_order_field(order_bytes, reactor, field) -> scalar`.
///
/// UniswapX reactors take a generic `SignedOrder { bytes order; bytes sig }`,
/// then the concrete reactor decodes `order` into its family-specific struct.
/// This helper mirrors that second decode for declarative manifests and exposes
/// the policy-relevant scalar fields used by `Amm::SettleIntentOrder`.
fn uniswapx_reactor_order_field(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 3 {
        return Err(format!(
            "uniswapx_reactor_order_field expects 3 args (order_bytes, reactor, field), got {}",
            args.len()
        ));
    }

    let order_bytes = json_hex_bytes(&args[0], "order_bytes")?;
    let reactor = json_address(&args[1], "reactor")?;
    let field = args[2]
        .as_str()
        .ok_or("uniswapx_reactor_order_field: field arg is not a string")?;
    let family = UniswapXOrderFamily::for_reactor(&reactor)?;
    let order = decode_uniswapx_order(family, &order_bytes).or_else(|primary_error| {
        decode_any_uniswapx_order(family, &order_bytes).map_err(|fallback_error| {
            format!(
                "target-family decode failed: {primary_error}; fallback family decode failed: {fallback_error}"
            )
        })
    })?;

    order.field(field).ok_or_else(|| {
        format!(
            "uniswapx_reactor_order_field: unsupported field '{field}' for {:?}",
            family
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UniswapXOrderFamily {
    ExclusiveDutch,
    V2Dutch,
    V3Dutch,
    Priority,
}

impl UniswapXOrderFamily {
    const ALL: [Self; 4] = [
        Self::ExclusiveDutch,
        Self::V2Dutch,
        Self::V3Dutch,
        Self::Priority,
    ];

    fn for_reactor(reactor: &Address) -> Result<Self, String> {
        let lower = reactor.to_string().to_ascii_lowercase();
        match lower.as_str() {
            // Mainnet ExclusiveDutchOrderReactor.
            "0x6000da47483062a0d734ba3dc7576ce6a0b645c4" => Ok(Self::ExclusiveDutch),
            // Mainnet V2DutchOrderReactor and Arbitrum V2DutchOrderReactor.
            "0x00000011f84b9aa48e5f8aa8b9897600006289be"
            | "0x1bd1aadc8a99fe9c48cffd9a5718b67f83cd4c08" => Ok(Self::V2Dutch),
            // V3DutchOrderReactor deployments currently used by UniswapX.
            "0x0000000015757c461808ea25eb309638b62681cf"
            | "0x000000008a8330b5e401f8d6b6f4d82e9e6fef4a"
            | "0x000000000923439a00000cfd2e0c5e60fef971c4"
            | "0xb274d5f4b833b61b340b654d600a864fb604a87c" => Ok(Self::V3Dutch),
            // Base PriorityOrderReactor.
            "0x000000001ec5656dcdb24d90dfa42742738de729" => Ok(Self::Priority),
            _ => Err(format!(
                "uniswapx_reactor_order_field: unsupported reactor {lower}"
            )),
        }
    }

    const fn synthetic_decode_signature(self) -> &'static str {
        match self {
            Self::ExclusiveDutch => "decode(((address,address,uint256,uint256,address,bytes),uint256,uint256,address,uint256,(address,uint256,uint256),(address,uint256,uint256,address)[]))",
            Self::V2Dutch => "decode(((address,address,uint256,uint256,address,bytes),address,(address,uint256,uint256),(address,uint256,uint256,address)[],(uint256,uint256,address,uint256,uint256,uint256[]),bytes))",
            Self::V3Dutch => "decode(((address,address,uint256,uint256,address,bytes),address,uint256,(address,uint256,(uint256,int256[]),uint256,uint256),(address,uint256,(uint256,int256[]),address,uint256,uint256)[],(uint256,address,uint256,uint256,uint256[]),bytes))",
            Self::Priority => "decode(((address,address,uint256,uint256,address,bytes),address,uint256,uint256,(address,uint256,uint256),(address,uint256,uint256,address)[],(uint256),bytes))",
        }
    }
}

fn decode_any_uniswapx_order(
    preferred: UniswapXOrderFamily,
    order_bytes: &[u8],
) -> Result<DecodedUniswapXOrder, String> {
    let mut errors = Vec::new();
    for family in UniswapXOrderFamily::ALL {
        if family == preferred {
            continue;
        }
        match decode_uniswapx_order(family, order_bytes) {
            Ok(order) => return Ok(order),
            Err(error) => errors.push(format!("{family:?}: {error}")),
        }
    }
    Err(errors.join("; "))
}

#[derive(Debug)]
struct DecodedUniswapXOrder {
    swapper: JsonValue,
    sell_token: JsonValue,
    buy_token: JsonValue,
    sell_amount: JsonValue,
    buy_min: JsonValue,
    order_kind: JsonValue,
    recipient: JsonValue,
    deadline: JsonValue,
    nonce: JsonValue,
}

impl DecodedUniswapXOrder {
    fn field(&self, field: &str) -> Option<JsonValue> {
        match field {
            "swapper" => Some(self.swapper.clone()),
            "sell_token" => Some(self.sell_token.clone()),
            "buy_token" => Some(self.buy_token.clone()),
            "sell_amount" => Some(self.sell_amount.clone()),
            "buy_min" => Some(self.buy_min.clone()),
            "order_kind" => Some(self.order_kind.clone()),
            "recipient" => Some(self.recipient.clone()),
            "deadline" | "valid_until" => Some(self.deadline.clone()),
            "nonce" | "order_nonce" => Some(self.nonce.clone()),
            _ => None,
        }
    }
}

fn decode_uniswapx_order(
    family: UniswapXOrderFamily,
    order_bytes: &[u8],
) -> Result<DecodedUniswapXOrder, String> {
    let signature = family.synthetic_decode_signature();
    let mut calldata = Vec::with_capacity(4 + order_bytes.len());
    calldata.extend_from_slice(&synthetic_selector(signature));
    calldata.extend_from_slice(order_bytes);
    let decoded = decode_with_signature(signature, &calldata)
        .map_err(|e| format!("uniswapx order ABI decode failed: {e}"))?;
    let order = decoded
        .args
        .first()
        .ok_or("uniswapx order ABI decode returned no args")?;
    let order = tuple_items(&order.value, "order")?;
    let info = tuple_at(order, 0, "order.info")?;
    let swapper = address_json(tuple_get(info, 1, "order.info.swapper")?)?;
    let nonce = uint_string_json(tuple_get(info, 2, "order.info.nonce")?)?;
    let deadline = uint_u64_json(tuple_get(info, 3, "order.info.deadline")?)?;

    match family {
        UniswapXOrderFamily::ExclusiveDutch => {
            let input = tuple_at(order, 5, "order.input")?;
            let output = first_tuple_array_item(order, 6, "order.outputs")?;
            Ok(DecodedUniswapXOrder {
                swapper,
                sell_token: address_json(tuple_get(input, 0, "order.input.token")?)?,
                buy_token: address_json(tuple_get(output, 0, "order.outputs[0].token")?)?,
                sell_amount: uint_string_json(tuple_get(input, 1, "order.input.startAmount")?)?,
                buy_min: uint_string_json(tuple_get(output, 2, "order.outputs[0].endAmount")?)?,
                order_kind: JsonValue::String("dutch".to_owned()),
                recipient: address_json(tuple_get(output, 3, "order.outputs[0].recipient")?)?,
                deadline,
                nonce,
            })
        }
        UniswapXOrderFamily::V2Dutch => {
            let input = tuple_at(order, 2, "order.baseInput")?;
            let output = first_tuple_array_item(order, 3, "order.baseOutputs")?;
            let cosigner_data = tuple_at(order, 4, "order.cosignerData")?;
            let cosigner_input_amount = uint_from_value(tuple_get(
                cosigner_data,
                4,
                "order.cosignerData.inputAmount",
            )?)?;
            let sell_amount_value = if cosigner_input_amount.is_zero() {
                tuple_get(input, 1, "order.baseInput.startAmount")?
            } else {
                tuple_get(cosigner_data, 4, "order.cosignerData.inputAmount")?
            };
            Ok(DecodedUniswapXOrder {
                swapper,
                sell_token: address_json(tuple_get(input, 0, "order.baseInput.token")?)?,
                buy_token: address_json(tuple_get(output, 0, "order.baseOutputs[0].token")?)?,
                sell_amount: uint_string_json(sell_amount_value)?,
                buy_min: uint_string_json(tuple_get(output, 2, "order.baseOutputs[0].endAmount")?)?,
                order_kind: JsonValue::String("dutch".to_owned()),
                recipient: address_json(tuple_get(output, 3, "order.baseOutputs[0].recipient")?)?,
                deadline,
                nonce,
            })
        }
        UniswapXOrderFamily::V3Dutch => {
            let input = tuple_at(order, 3, "order.baseInput")?;
            let output = first_tuple_array_item(order, 4, "order.baseOutputs")?;
            let cosigner_data = tuple_at(order, 5, "order.cosignerData")?;
            let cosigner_input_amount = uint_from_value(tuple_get(
                cosigner_data,
                3,
                "order.cosignerData.inputAmount",
            )?)?;
            let sell_amount_value = if cosigner_input_amount.is_zero() {
                tuple_get(input, 1, "order.baseInput.startAmount")?
            } else {
                tuple_get(cosigner_data, 3, "order.cosignerData.inputAmount")?
            };
            Ok(DecodedUniswapXOrder {
                swapper,
                sell_token: address_json(tuple_get(input, 0, "order.baseInput.token")?)?,
                buy_token: address_json(tuple_get(output, 0, "order.baseOutputs[0].token")?)?,
                sell_amount: uint_string_json(sell_amount_value)?,
                buy_min: uint_string_json(tuple_get(output, 4, "order.baseOutputs[0].minAmount")?)?,
                order_kind: JsonValue::String("dutch".to_owned()),
                recipient: address_json(tuple_get(output, 3, "order.baseOutputs[0].recipient")?)?,
                deadline,
                nonce,
            })
        }
        UniswapXOrderFamily::Priority => {
            let input = tuple_at(order, 4, "order.input")?;
            let output = first_tuple_array_item(order, 5, "order.outputs")?;
            Ok(DecodedUniswapXOrder {
                swapper,
                sell_token: address_json(tuple_get(input, 0, "order.input.token")?)?,
                buy_token: address_json(tuple_get(output, 0, "order.outputs[0].token")?)?,
                sell_amount: uint_string_json(tuple_get(input, 1, "order.input.amount")?)?,
                buy_min: uint_string_json(tuple_get(output, 1, "order.outputs[0].amount")?)?,
                order_kind: JsonValue::String("limit".to_owned()),
                recipient: address_json(tuple_get(output, 3, "order.outputs[0].recipient")?)?,
                deadline,
                nonce,
            })
        }
    }
}

fn synthetic_selector(signature: &str) -> [u8; 4] {
    let hash = keccak256(signature.as_bytes());
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&hash.as_slice()[..4]);
    selector
}

fn tuple_items<'a>(value: &'a DynSolValue, path: &str) -> Result<&'a [DynSolValue], String> {
    match value {
        DynSolValue::Tuple(items) => Ok(items.as_slice()),
        other => Err(format!(
            "uniswapx order decode: {path} expected tuple, got {}",
            dyn_value_kind(other)
        )),
    }
}

fn tuple_at<'a>(
    items: &'a [DynSolValue],
    idx: usize,
    path: &str,
) -> Result<&'a [DynSolValue], String> {
    let value = tuple_get(items, idx, path)?;
    tuple_items(value, path)
}

fn tuple_get<'a>(
    items: &'a [DynSolValue],
    idx: usize,
    path: &str,
) -> Result<&'a DynSolValue, String> {
    items
        .get(idx)
        .ok_or_else(|| format!("uniswapx order decode: missing {path} at tuple index {idx}"))
}

fn first_tuple_array_item<'a>(
    order: &'a [DynSolValue],
    idx: usize,
    path: &str,
) -> Result<&'a [DynSolValue], String> {
    let value = tuple_get(order, idx, path)?;
    let items = match value {
        DynSolValue::Array(items) | DynSolValue::FixedArray(items) => items,
        other => {
            return Err(format!(
                "uniswapx order decode: {path} expected array, got {}",
                dyn_value_kind(other)
            ))
        }
    };
    let first = items
        .first()
        .ok_or_else(|| format!("uniswapx order decode: {path} is empty"))?;
    tuple_items(first, &format!("{path}[0]"))
}

fn address_json(value: &DynSolValue) -> Result<JsonValue, String> {
    match value {
        DynSolValue::Address(address) => Ok(JsonValue::String(address.to_string())),
        other => Err(format!(
            "uniswapx order decode: expected address, got {}",
            dyn_value_kind(other)
        )),
    }
}

fn uint_string_json(value: &DynSolValue) -> Result<JsonValue, String> {
    Ok(JsonValue::String(uint_from_value(value)?.to_string()))
}

fn uint_u64_json(value: &DynSolValue) -> Result<JsonValue, String> {
    let value = uint_from_value(value)?;
    let n = u64::try_from(value)
        .map_err(|_| format!("uniswapx order decode: uint {value} does not fit u64"))?;
    Ok(JsonValue::Number(serde_json::Number::from(n)))
}

fn uint_from_value(value: &DynSolValue) -> Result<U256, String> {
    match value {
        DynSolValue::Uint(value, _) => Ok(*value),
        other => Err(format!(
            "uniswapx order decode: expected uint, got {}",
            dyn_value_kind(other)
        )),
    }
}

fn dyn_value_kind(value: &DynSolValue) -> &'static str {
    match value {
        DynSolValue::Address(_) => "address",
        DynSolValue::Bool(_) => "bool",
        DynSolValue::Bytes(_) => "bytes",
        DynSolValue::FixedBytes(_, _) => "fixed_bytes",
        DynSolValue::Int(_, _) => "int",
        DynSolValue::Uint(_, _) => "uint",
        DynSolValue::String(_) => "string",
        DynSolValue::Array(_) => "array",
        DynSolValue::FixedArray(_) => "fixed_array",
        DynSolValue::Tuple(_) => "tuple",
        DynSolValue::Function(_) => "function",
    }
}

fn json_hex_bytes(value: &JsonValue, label: &str) -> Result<Vec<u8>, String> {
    let s = value
        .as_str()
        .ok_or_else(|| format!("{label} is not a hex string"))?;
    let body = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(body).map_err(|e| format!("{label} is not valid hex: {e}"))
}

/// Parse a JSON uint — decimal string (width > 64), `0x` hex string, or JSON
/// number (width ≤ 64) — into `U256`, matching how [`super::args_json`] renders
/// decoded uints.
fn json_u256(value: &JsonValue, label: &str) -> Result<U256, String> {
    match value {
        JsonValue::Number(n) => n
            .as_u64()
            .map(U256::from)
            .ok_or_else(|| format!("{label} is not a non-negative integer: {n}")),
        JsonValue::String(s) => {
            let trimmed = s.trim();
            if let Some(hex) = trimmed.strip_prefix("0x") {
                U256::from_str_radix(hex, 16)
                    .map_err(|e| format!("{label} is not a valid hex uint ({s}): {e}"))
            } else {
                U256::from_str_radix(trimmed, 10)
                    .map_err(|e| format!("{label} is not a valid decimal uint ({s}): {e}"))
            }
        }
        other => Err(format!("{label} is not a uint: {other}")),
    }
}

fn json_address(value: &JsonValue, label: &str) -> Result<Address, String> {
    let s = value
        .as_str()
        .ok_or_else(|| format!("{label} is not an address string"))?;
    Address::from_str(s).map_err(|e| format!("{label} is not an address ({s}): {e}"))
}

fn json_bool(value: &JsonValue, label: &str) -> Result<bool, String> {
    match value {
        JsonValue::Bool(value) => Ok(*value),
        JsonValue::String(value) if value.eq_ignore_ascii_case("true") => Ok(true),
        JsonValue::String(value) if value.eq_ignore_ascii_case("false") => Ok(false),
        other => Err(format!("{label} is not a bool: {other}")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct JsonSignedInt {
    negative: bool,
    abs_decimal: String,
}

fn json_signed_int(value: &JsonValue, label: &str) -> Result<JsonSignedInt, String> {
    match value {
        JsonValue::Number(number) => {
            if let Some(value) = number.as_i64() {
                let negative = value < 0;
                let abs_decimal = if negative {
                    (-(value as i128)).to_string()
                } else {
                    value.to_string()
                };
                return Ok(JsonSignedInt {
                    negative: negative && abs_decimal != "0",
                    abs_decimal,
                });
            }
            if let Some(value) = number.as_u64() {
                return Ok(JsonSignedInt {
                    negative: false,
                    abs_decimal: value.to_string(),
                });
            }
            Err(format!("{label} must be an integer, got {number}"))
        }
        JsonValue::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err(format!("{label} is empty"));
            }
            let (negative, digits) = if let Some(rest) = trimmed.strip_prefix('-') {
                (true, rest)
            } else if let Some(rest) = trimmed.strip_prefix('+') {
                (false, rest)
            } else {
                (false, trimmed)
            };
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(format!("{label} is not a decimal integer: {raw}"));
            }
            let abs = digits.trim_start_matches('0');
            let abs_decimal = if abs.is_empty() { "0" } else { abs }.to_owned();
            Ok(JsonSignedInt {
                negative: negative && abs_decimal != "0",
                abs_decimal,
            })
        }
        other => Err(format!("{label} is not an integer: {other}")),
    }
}

/// A route slot is "empty" when it parses to the zero address (Router NG
/// zero-pads unused hops). Falls back to an all-zero-hex string check if the
/// value is not a parseable address.
fn is_zero_address(v: &JsonValue) -> bool {
    match v.as_str() {
        Some(s) => Address::from_str(s).map_or_else(
            |_| s.trim_start_matches("0x").chars().all(|c| c == '0'),
            |a| a.is_zero(),
        ),
        None => false,
    }
}

/// Read a `uint` arg as `u64` — decimal string (width > 64) or JSON number
/// (width ≤ 64), matching how [`super::args_json`] renders uints.
fn json_to_u64(v: &JsonValue) -> Option<u64> {
    match v {
        JsonValue::Number(n) => n.as_u64(),
        JsonValue::String(s) => s.parse::<u64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const A: &str = "0x1111111111111111111111111111111111111111";
    const POOL1: &str = "0x2222222222222222222222222222222222222222";
    const B: &str = "0x3333333333333333333333333333333333333333";
    const POOL2: &str = "0x4444444444444444444444444444444444444444";
    const C: &str = "0x5555555555555555555555555555555555555555";
    const Z: &str = "0x0000000000000000000000000000000000000000";

    fn route(addrs: &[&str]) -> JsonValue {
        let mut v: Vec<JsonValue> = addrs.iter().map(|s| json!(s)).collect();
        while v.len() < 11 {
            v.push(json!(Z));
        }
        JsonValue::Array(v)
    }

    #[test]
    fn single_hop_coin_swap_emits_next_coin() {
        // 1 hop, swap_type 1 (coin-producing) → route[2] = B.
        let r = route(&[A, POOL1, B]);
        let sp = json!([["0", "1", "1", "1", "2"]]); // swap_type at [0][2] = "1"
        let out = curve_route_last_token(&[r, sp]).unwrap();
        assert_eq!(out, json!(B));
    }

    #[test]
    fn two_hop_uses_last_hop_output() {
        // 2 hops, both coin-producing → last output route[4] = C.
        let r = route(&[A, POOL1, B, POOL2, C]);
        let sp = json!([["0", "1", "1", "1", "2"], ["0", "1", "1", "1", "2"]]);
        let out = curve_route_last_token(&[r, sp]).unwrap();
        assert_eq!(out, json!(C));
    }

    #[test]
    fn vault_swap_type_emits_pool_slot() {
        // swap_type 9 (ERC4626 share) → output is the pool/vault slot route[1].
        let r = route(&[A, POOL1]);
        let sp = json!([[0, 0, 9, 0, 0]]); // numeric swap_type accepted too
        let out = curve_route_last_token(&[r, sp]).unwrap();
        assert_eq!(out, json!(POOL1));
    }

    #[test]
    fn empty_route_errors() {
        let r = route(&[A]); // route[1] is zero → no executed hop
        let sp = json!([["0", "0", "1", "0", "0"]]);
        assert!(curve_route_last_token(&[r, sp]).is_err());
    }

    #[test]
    fn route_hash_is_deterministic_and_32_bytes() {
        let r = route(&[A, POOL1, B]);
        let h1 = route_hash(std::slice::from_ref(&r)).unwrap();
        let h2 = route_hash(&[r]).unwrap();
        assert_eq!(h1, h2);
        let s = h1.as_str().unwrap();
        assert!(s.starts_with("0x"));
        assert_eq!(s.len(), 66); // 0x + 64 hex
                                 // different route → different hash
        let other = route_hash(&[route(&[A, POOL1, C])]).unwrap();
        assert_ne!(h1, other);
    }

    #[test]
    fn keccak256_hashes_bytes_deterministically_to_32_bytes() {
        // keccak256("") is the well-known empty-input hash.
        let empty = keccak256_hex(&[json!("0x")]).unwrap();
        assert_eq!(
            empty.as_str().unwrap(),
            "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
        // Deterministic + exactly 32 bytes for arbitrary data.
        let d = json!("0xdeadbeef");
        let h1 = keccak256_hex(std::slice::from_ref(&d)).unwrap();
        let h2 = keccak256_hex(&[d]).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.as_str().unwrap().len(), 66); // 0x + 64 hex
                                                    // different data → different hash
        let other = keccak256_hex(&[json!("0xdeadbeff")]).unwrap();
        assert_ne!(h1, other);
        // wrong arg count + non-hex error out
        assert!(keccak256_hex(&[]).is_err());
        assert!(keccak256_hex(&[json!("not-hex")]).is_err());
    }

    #[test]
    fn address_from_uint256_unmasks_low_160_bits() {
        // High 96 bits (flags) are discarded; low 160 bits = the address.
        // Word = 0xdeadbeef..00 (high 12 bytes) ++ 0xaa..aa (low 20 bytes).
        let packed = json!("0xdeadbeef0000000000000000aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let out = address_from_uint256(&[packed]).unwrap();
        assert_eq!(
            out.as_str().unwrap(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );

        // Decimal-string form (how a width-256 uint is rendered) and JSON number.
        let dec = address_from_uint256(&[json!("1")]).unwrap();
        assert_eq!(
            dec.as_str().unwrap(),
            "0x0000000000000000000000000000000000000001"
        );
        let num = address_from_uint256(&[json!(255)]).unwrap();
        assert_eq!(
            num.as_str().unwrap(),
            "0x00000000000000000000000000000000000000ff"
        );

        // Wrong arg count + non-numeric error out.
        assert!(address_from_uint256(&[]).is_err());
        assert!(address_from_uint256(&[json!("not-a-number")]).is_err());
    }

    #[test]
    fn uniswap_v3_pool_swap_field_positive_amount_is_exact_input() {
        let out = uniswap_v3_pool_swap_field(&[
            json!("1000"),
            json!(true),
            json!(A),
            json!(B),
            json!("direction_kind"),
        ])
        .unwrap();
        assert_eq!(out, json!("exact_input"));

        let token_in = uniswap_v3_pool_swap_field(&[
            json!("1000"),
            json!(true),
            json!(A),
            json!(B),
            json!("token_in"),
        ])
        .unwrap();
        let token_out = uniswap_v3_pool_swap_field(&[
            json!("1000"),
            json!(true),
            json!(A),
            json!(B),
            json!("token_out"),
        ])
        .unwrap();
        let amount_in = uniswap_v3_pool_swap_field(&[
            json!("1000"),
            json!(true),
            json!(A),
            json!(B),
            json!("amount_in"),
        ])
        .unwrap();

        assert_eq!(token_in, json!(A));
        assert_eq!(token_out, json!(B));
        assert_eq!(amount_in, json!("1000"));
    }

    #[test]
    fn uniswap_v3_pool_swap_field_negative_amount_is_exact_output() {
        let kind = uniswap_v3_pool_swap_field(&[
            json!("-2500"),
            json!(false),
            json!(A),
            json!(B),
            json!("direction_kind"),
        ])
        .unwrap();
        let token_in = uniswap_v3_pool_swap_field(&[
            json!("-2500"),
            json!(false),
            json!(A),
            json!(B),
            json!("token_in"),
        ])
        .unwrap();
        let amount_out = uniswap_v3_pool_swap_field(&[
            json!("-2500"),
            json!(false),
            json!(A),
            json!(B),
            json!("amount_out"),
        ])
        .unwrap();
        let max_amount_in = uniswap_v3_pool_swap_field(&[
            json!("-2500"),
            json!(false),
            json!(A),
            json!(B),
            json!("max_amount_in"),
        ])
        .unwrap();

        assert_eq!(kind, json!("exact_output"));
        assert_eq!(token_in, json!(B));
        assert_eq!(amount_out, json!("2500"));
        assert_eq!(max_amount_in, json!(U256::MAX.to_string()));
    }

    #[test]
    fn uniswapx_v2_reactor_order_field_decodes_cosigned_order() {
        let order = v2_dutch_order_bytes();
        let order_hex = json!(format!("0x{}", hex::encode(order)));
        let reactor = json!("0x00000011f84b9aa48e5f8aa8b9897600006289be");

        let sell_amount = uniswapx_reactor_order_field(&[
            order_hex.clone(),
            reactor.clone(),
            json!("sell_amount"),
        ])
        .unwrap();
        let buy_min =
            uniswapx_reactor_order_field(&[order_hex.clone(), reactor.clone(), json!("buy_min")])
                .unwrap();
        let swapper =
            uniswapx_reactor_order_field(&[order_hex.clone(), reactor.clone(), json!("swapper")])
                .unwrap();
        let valid_until =
            uniswapx_reactor_order_field(&[order_hex, reactor, json!("valid_until")]).unwrap();

        assert_eq!(sell_amount, json!("900"));
        assert_eq!(buy_min, json!("1800"));
        assert_eq!(swapper, json!("0x1111111111111111111111111111111111111111"));
        assert_eq!(valid_until, json!(1_900_000_000u64));
    }

    #[test]
    fn dispatch_unknown_fn_errors() {
        assert!(dispatch("nope", &[]).is_err());
    }

    fn v2_dutch_order_bytes() -> Vec<u8> {
        let reactor = addr("0x00000011f84b9aa48e5f8aa8b9897600006289be");
        let swapper = addr("0x1111111111111111111111111111111111111111");
        let sell = addr("0x2222222222222222222222222222222222222222");
        let buy = addr("0x3333333333333333333333333333333333333333");
        let recipient = addr("0x4444444444444444444444444444444444444444");
        let zero = addr("0x0000000000000000000000000000000000000000");

        let info = DynSolValue::Tuple(vec![
            DynSolValue::Address(reactor),
            DynSolValue::Address(swapper),
            uint(42),
            uint(1_900_000_000),
            DynSolValue::Address(zero),
            DynSolValue::Bytes(Vec::new()),
        ]);
        let base_input =
            DynSolValue::Tuple(vec![DynSolValue::Address(sell), uint(1_000), uint(1_100)]);
        let base_output = DynSolValue::Tuple(vec![
            DynSolValue::Address(buy),
            uint(2_000),
            uint(1_800),
            DynSolValue::Address(recipient),
        ]);
        let cosigner_data = DynSolValue::Tuple(vec![
            uint(1_800_000_000),
            uint(1_900_000_000),
            DynSolValue::Address(zero),
            uint(0),
            uint(900),
            DynSolValue::Array(vec![uint(0)]),
        ]);
        let order = DynSolValue::Tuple(vec![
            info,
            DynSolValue::Address(zero),
            base_input,
            DynSolValue::Array(vec![base_output]),
            cosigner_data,
            DynSolValue::Bytes(vec![0xab; 65]),
        ]);

        DynSolValue::Tuple(vec![order]).abi_encode_params()
    }

    fn addr(value: &str) -> Address {
        Address::from_str(value).unwrap()
    }

    fn uint(value: u64) -> DynSolValue {
        DynSolValue::Uint(U256::from(value), 256)
    }
}
