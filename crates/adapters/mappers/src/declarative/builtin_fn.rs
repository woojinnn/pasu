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
use abi_resolver::subdecode::enum_tagged::{
    dispatch as enum_dispatch, try_dispatch as enum_try_dispatch, DecodedEnum,
};
use abi_resolver::subdecode::protocols::balancer_v2::{
    BALANCER_V2_EXIT_KIND_STABLE, BALANCER_V2_EXIT_KIND_WEIGHTED, BALANCER_V2_JOIN_KIND,
};
use alloy_dyn_abi::DynSolValue;
use alloy_primitives::{keccak256, Address, U256};
use serde_json::Value as JsonValue;

/// The whitelist of accepted `$fn` names. The **single source of truth** is the
/// sibling [`fn_whitelist.json`](./fn_whitelist.json) — `whitelist_matches_shared_json`
/// asserts this const equals it, and `registryV2/scripts/build-index.ts`
/// `validateEmitShape` reads that same JSON and **rejects any manifest `$fn` not in
/// it**, so an unknown / typo'd `$fn` now fails at build-index time, not only at
/// decode time in the user's wallet. Keep this const and the JSON in lockstep.
///
/// Executors are framework-generic by default. The few that are inherently
/// protocol-specific — they encode one venue's fixed calldata / bit layout:
/// `curve_route_last_token`, `route_hash` (Curve), `maker_traits_expiry`
/// (1inch LOP), `uniswap_v3_pool_swap_field` (Uniswap V3),
/// `uniswapx_reactor_order_field` (UniswapX) — say so in their own doc comment
/// below. The rest (`keccak256`, `address_from_uint256`, `coalesce_address`,
/// `token_key_or_native`) are venue-agnostic byte/address utilities.
pub const WHITELIST: &[&str] = &[
    "curve_route_last_token",
    "route_hash",
    "unoswap_route_hash",
    "keccak256",
    "address_from_uint256",
    "maker_traits_expiry",
    "coalesce_address",
    "token_key_or_native",
    "uniswap_v3_pool_swap_field",
    "uniswapx_reactor_order_field",
    "balancer_zip_token_amounts",
    "balancer_pool_id_to_address",
    "balancer_v2_userdata_field",
    "balancer_v2_batch_swap_field",
    "balancer_v3_zip_pool_tokens",
    "balancer_v3_swap_path_field",
    "tuple_array_field",
    "array_len",
    "u64_saturating",
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
        "unoswap_route_hash" => unoswap_route_hash(args),
        "keccak256" => keccak256_hex(args),
        "address_from_uint256" => address_from_uint256(args),
        "maker_traits_expiry" => maker_traits_expiry(args),
        "coalesce_address" => coalesce_address(args),
        "token_key_or_native" => token_key_or_native(args),
        "uniswap_v3_pool_swap_field" => uniswap_v3_pool_swap_field(args),
        "uniswapx_reactor_order_field" => uniswapx_reactor_order_field(args),
        "balancer_zip_token_amounts" => balancer_zip_token_amounts(args),
        "balancer_pool_id_to_address" => balancer_pool_id_to_address(args),
        "balancer_v2_userdata_field" => balancer_v2_userdata_field(args),
        "balancer_v2_batch_swap_field" => balancer_v2_batch_swap_field(args),
        "balancer_v3_zip_pool_tokens" => balancer_v3_zip_pool_tokens(args),
        "balancer_v3_swap_path_field" => balancer_v3_swap_path_field(args),
        "tuple_array_field" => tuple_array_field(args),
        "array_len" => array_len(args),
        "u64_saturating" => u64_saturating(args),
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

/// `maker_traits_expiry(maker_traits: uint256) -> uint` — extract the 1inch LOP v4
/// `MakerTraits` order expiration: bits `[80, 120)`, a `uint40` ABSOLUTE
/// UNIX-seconds timestamp (`(makerTraits >> 80) & ((1<<40)-1)`). On-chain `0`
/// means "never expires"; this fn remaps `0` → max `uint40` (`0xFF_FFFF_FFFF`,
/// ~year 36812) so a never-expiring order surfaces as a far-future `valid_until`
/// (a policy treating `valid_until` as an upper bound flags it) rather than a
/// long-past epoch-0. Returns a JSON number (the `uint40` always fits `u64`),
/// matching `Time`'s `#[serde(transparent)] u64` shape.
fn maker_traits_expiry(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 1 {
        return Err(format!(
            "maker_traits_expiry expects 1 arg (makerTraits), got {}",
            args.len()
        ));
    }
    let traits = json_u256(&args[0], "maker_traits_expiry: makerTraits")?;
    let mask = U256::from(0xFF_FFFF_FFFFu64); // (1 << 40) - 1, a uint40
    let expiry = (traits >> 80usize) & mask;
    let expiry = if expiry.is_zero() { mask } else { expiry };
    let secs = u64::try_from(expiry)
        .map_err(|_| "maker_traits_expiry: expiry does not fit u64".to_owned())?;
    Ok(JsonValue::Number(serde_json::Number::from(secs)))
}

/// `coalesce_address(addr: address, fallback: address) -> address` — return `addr`
/// if it is a non-zero address, else `fallback`. Resolves an "empty" recipient slot
/// to a default — 1inch LOP `Order.receiver == 0` means the maker is the recipient,
/// so `coalesce_address($args.order.receiver, $args.order.maker)` yields the
/// effective recipient. Returns a lowercase `0x` hex address.
fn coalesce_address(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "coalesce_address expects 2 args (addr, fallback), got {}",
            args.len()
        ));
    }
    let addr = json_address(&args[0], "coalesce_address: addr")?;
    let chosen = if addr.is_zero() {
        json_address(&args[1], "coalesce_address: fallback")?
    } else {
        addr
    };
    Ok(JsonValue::String(format!("{chosen:#x}")))
}

/// `token_key_or_native(address, chain) -> TokenKey` — build a token `key` object,
/// mapping the 1inch native-asset sentinel (`0xEeeeeEee...EEeEeEEe`) to a
/// `TokenKey::Native { chain }` and any other address to
/// `TokenKey::Erc20 { chain, address }`. Lets a swap whose token_in/token_out is
/// native ETH decode to the native key (which a "limit native spend" policy keys
/// on) instead of the opaque sentinel address. Returns the `key` object verbatim
/// (the only `$fn` that returns a JSON object, not a scalar).
fn token_key_or_native(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "token_key_or_native expects 2 args (address, chain), got {}",
            args.len()
        ));
    }
    let address = json_address(&args[0], "token_key_or_native: address")?;
    let chain = args[1]
        .as_str()
        .ok_or("token_key_or_native: chain arg is not a string")?;
    // 1inch (and most aggregators) use this sentinel for the native gas asset.
    const NATIVE_SENTINEL: &str = "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    let mut key = serde_json::Map::new();
    if format!("{address:#x}") == NATIVE_SENTINEL {
        key.insert(
            "standard".to_owned(),
            JsonValue::String("native".to_owned()),
        );
        key.insert("chain".to_owned(), JsonValue::String(chain.to_owned()));
    } else {
        key.insert("standard".to_owned(), JsonValue::String("erc20".to_owned()));
        key.insert("chain".to_owned(), JsonValue::String(chain.to_owned()));
        key.insert(
            "address".to_owned(),
            JsonValue::String(format!("{address:#x}")),
        );
    }
    Ok(JsonValue::Object(key))
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

/// `balancer_zip_token_amounts(chain, assets: address[], amounts: uint256[]) -> [[TokenRef, U256]]`.
///
/// Balancer V2 `joinPool`/`exitPool` carry the pooled token set as two parallel
/// calldata arrays — `request.assets[]` (token addresses) and
/// `request.maxAmountsIn[]` / `request.minAmountsOut[]` (per-token amounts). The
/// `AddLiquidity::Pooled.tokens` / `RemoveLiquidity::PooledBurn.minOut` ActionBody
/// field is `Vec<(TokenRef, U256)>`, which the flat `$args.*` placeholder grammar
/// cannot build from two arrays. This zips them index-aligned into the
/// `[[{"key":{"standard":"erc20","chain":c,"address":a}}, "<amount>"], …]` shape
/// `lower_token_amount_set` consumes. Returns the array (the second `$fn` after
/// `token_key_or_native` to return composite JSON).
fn balancer_zip_token_amounts(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 3 {
        return Err(format!(
            "balancer_zip_token_amounts expects 3 args (chain, assets, amounts), got {}",
            args.len()
        ));
    }
    let chain = args[0]
        .as_str()
        .ok_or("balancer_zip_token_amounts: chain arg is not a string")?;
    let assets = args[1]
        .as_array()
        .ok_or("balancer_zip_token_amounts: assets arg is not an array")?;
    let amounts = args[2]
        .as_array()
        .ok_or("balancer_zip_token_amounts: amounts arg is not an array")?;
    // Real Balancer calldata guarantees assets.len() == amounts.len(); the
    // validate's type-valid synthetic fuzz can emit unequal lengths (a
    // would-revert tx), so zip the common prefix instead of erroring — real
    // correctness is pinned by the corpus expect_body, not by this length.
    let mut pairs = Vec::with_capacity(assets.len().min(amounts.len()));
    for (i, (asset, amount)) in assets.iter().zip(amounts).enumerate() {
        let addr = json_address(asset, &format!("balancer_zip_token_amounts: assets[{i}]"))?;
        // Normalise the amount to a decimal string (U256 is serde-de'd from a
        // string), matching how args_json renders width-256 uints.
        let amount_u256 = json_u256(amount, &format!("balancer_zip_token_amounts: amounts[{i}]"))?;
        let mut key = serde_json::Map::new();
        key.insert("standard".to_owned(), JsonValue::String("erc20".to_owned()));
        key.insert("chain".to_owned(), JsonValue::String(chain.to_owned()));
        key.insert(
            "address".to_owned(),
            JsonValue::String(format!("{addr:#x}")),
        );
        let mut token_ref = serde_json::Map::new();
        token_ref.insert("key".to_owned(), JsonValue::Object(key));
        pairs.push(JsonValue::Array(vec![
            JsonValue::Object(token_ref),
            JsonValue::String(amount_u256.to_string()),
        ]));
    }
    Ok(JsonValue::Array(pairs))
}

/// `balancer_v3_zip_pool_tokens(chain, pool: address, amounts: uint256[], pool_token_map: {pool->[token]}) -> [[TokenRef, U256]]`.
///
/// Balancer V3 `addLiquidityProportional` / `addLiquidityUnbalanced` /
/// `removeLiquidityProportional` carry the per-token `amounts[]` array but **not**
/// the token addresses — `amounts[i]` is indexed by the pool's registered token
/// list, which lives on-chain (V3 Vault `getPoolTokens`) and is absent from
/// calldata. Since ScopeBall is static (no sim), that list is supplied as a
/// build-time-baked `pool -> [token]` map (registry `_pool_universe.json`,
/// inlined into the manifest via `$source.pool_tokens`). This looks up the pool's
/// token list and zips it with `amounts[]` index-aligned into the
/// `[[{"key":{…}}, "<amount>"], …]` shape that `AddLiquidity::Pooled.tokens` /
/// `RemoveLiquidity::PooledBurn.min_out` consume.
///
/// Pool absent from the baked map (a deferred long-tail pool, or one created
/// after the snapshot) → **error**, so the route misses and the user is
/// warned — never a silent pass of an unresolved deposit/withdrawal.
fn balancer_v3_zip_pool_tokens(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 4 {
        return Err(format!(
            "balancer_v3_zip_pool_tokens expects 4 args (chain, pool, amounts, pool_token_map), got {}",
            args.len()
        ));
    }
    let chain = args[0]
        .as_str()
        .ok_or("balancer_v3_zip_pool_tokens: chain arg is not a string")?;
    let pool = json_address(&args[1], "balancer_v3_zip_pool_tokens: pool")?;
    // Map keys are lowercase 0x hex (resolver convention); `{:#x}` lowercases.
    let pool_key = format!("{pool:#x}");
    let amounts = args[2]
        .as_array()
        .ok_or("balancer_v3_zip_pool_tokens: amounts arg is not an array")?;
    let map = args[3]
        .as_object()
        .ok_or("balancer_v3_zip_pool_tokens: pool_token_map arg is not an object")?;
    let tokens = map
        .get(&pool_key)
        .and_then(JsonValue::as_array)
        .ok_or_else(|| {
            format!(
                "balancer_v3_zip_pool_tokens: pool {pool_key} not in baked pool_token_map \
             (deferred long-tail or post-snapshot pool → fail-closed)"
            )
        })?;
    // Real calldata guarantees tokens.len() == amounts.len() (the pool's token
    // count); type-valid synthetic fuzz can emit a mismatched `amounts` length,
    // so zip the common prefix (min length) instead of erroring — real
    // correctness is pinned by the corpus expect_body, not by this length.
    let mut pairs = Vec::with_capacity(tokens.len().min(amounts.len()));
    for (i, (token, amount)) in tokens.iter().zip(amounts).enumerate() {
        let addr = json_address(
            token,
            &format!("balancer_v3_zip_pool_tokens: pool_token_map[{pool_key}][{i}]"),
        )?;
        let amount_u256 = json_u256(
            amount,
            &format!("balancer_v3_zip_pool_tokens: amounts[{i}]"),
        )?;
        let mut key = serde_json::Map::new();
        key.insert("standard".to_owned(), JsonValue::String("erc20".to_owned()));
        key.insert("chain".to_owned(), JsonValue::String(chain.to_owned()));
        key.insert(
            "address".to_owned(),
            JsonValue::String(format!("{addr:#x}")),
        );
        let mut token_ref = serde_json::Map::new();
        token_ref.insert("key".to_owned(), JsonValue::Object(key));
        pairs.push(JsonValue::Array(vec![
            JsonValue::Object(token_ref),
            JsonValue::String(amount_u256.to_string()),
        ]));
    }
    Ok(JsonValue::Array(pairs))
}

/// `balancer_pool_id_to_address(pool_id: bytes32) -> address` — the 20-byte pool
/// (BPT) address packed into the high bytes of a Balancer V2 `poolId`. Balancer
/// V2 encodes `poolId = poolAddress(20) ++ specialization(2) ++ nonce(10)`, so
/// the pool/BPT contract address is the **first** 20 bytes (unlike
/// `address_from_uint256`, which unmasks the low 20). Used for `exitPool`'s
/// `lp_token` and any V2 venue pool-address need. Returns lowercase `0x` hex.
fn balancer_pool_id_to_address(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 1 {
        return Err(format!(
            "balancer_pool_id_to_address expects 1 arg (pool_id bytes32), got {}",
            args.len()
        ));
    }
    let s = args[0]
        .as_str()
        .ok_or("balancer_pool_id_to_address: pool_id is not a hex string")?;
    let body = s.strip_prefix("0x").unwrap_or(s);
    if body.len() != 64 {
        return Err(format!(
            "balancer_pool_id_to_address: pool_id must be 32 bytes (64 hex chars), got {}",
            body.len()
        ));
    }
    let addr = Address::from_str(&format!("0x{}", &body[..40])).map_err(|e| {
        format!("balancer_pool_id_to_address: leading 20 bytes not an address: {e}")
    })?;
    Ok(JsonValue::String(format!("{addr:#x}")))
}

/// The lone BPT-amount scalar in a decoded Balancer V2 join/exit `userData`.
///
/// Every JoinKind/ExitKind payload is `(kind, …)` where the BPT amount
/// (`minBPTAmountOut` / `bptAmountOut` / `bptAmountIn` / `maxBPTAmountIn`) is the
/// single `uint256` among arg slots 1 and 2 — the other slot, when present, is the
/// `uint256[] amountsIn/amountsOut` array. So the BPT scalar is `arg[1]` if it is a
/// uint, else `arg[2]` if it is a uint, else `0` (e.g. `INIT` join, which carries
/// no BPT bound). This rule is table-agnostic so it covers Weighted and Stable.
fn balancer_bpt_scalar(decoded: &DecodedEnum) -> U256 {
    let as_u256 = |idx: usize| -> Option<U256> {
        match decoded.args.get(idx).map(|a| &a.value) {
            Some(DynSolValue::Uint(v, _)) => Some(*v),
            _ => None,
        }
    };
    as_u256(1).or_else(|| as_u256(2)).unwrap_or(U256::ZERO)
}

/// `balancer_v2_userdata_field(userData: bytes, class, field) -> uint (decimal string)`.
///
/// Decodes a Balancer V2 `joinPool`/`exitPool` `userData` blob — a `(kind, …)`
/// enum-tagged payload — via the protocol's [`BALANCER_V2_JOIN_KIND`] /
/// [`BALANCER_V2_EXIT_KIND_WEIGHTED`]/[`_STABLE`] subdecode tables and returns the
/// BPT-amount scalar (`min_lp_out` for joins, `lp_amount` burned for exits). The
/// per-token amounts and recipient live in calldata; only this LP bound is inside
/// `userData`. `class ∈ {join, exit}` (selects the table; exits try Weighted then
/// Stable to resolve the kind-reuse ambiguity). `field ∈ {min_bpt, bpt_in}` is
/// validated but extraction is uniform ([`balancer_bpt_scalar`]). Undecodable /
/// kindless payloads fall back to `"0"` (conservative: an unknown LP bound).
fn balancer_v2_userdata_field(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 3 {
        return Err(format!(
            "balancer_v2_userdata_field expects 3 args (userData, class, field), got {}",
            args.len()
        ));
    }
    let user_data = json_hex_bytes(&args[0], "balancer_v2_userdata_field: userData")?;
    let class = args[1]
        .as_str()
        .ok_or("balancer_v2_userdata_field: class arg is not a string")?;
    let field = args[2]
        .as_str()
        .ok_or("balancer_v2_userdata_field: field arg is not a string")?;
    if !matches!(field, "min_bpt" | "bpt_in") {
        return Err(format!(
            "balancer_v2_userdata_field: unsupported field '{field}' (min_bpt | bpt_in)"
        ));
    }
    let decoded = match class {
        "join" => enum_dispatch(&user_data, &BALANCER_V2_JOIN_KIND),
        "exit" => enum_try_dispatch(
            &user_data,
            &[
                &BALANCER_V2_EXIT_KIND_WEIGHTED,
                &BALANCER_V2_EXIT_KIND_STABLE,
            ],
        ),
        other => {
            return Err(format!(
                "balancer_v2_userdata_field: unknown class '{other}' (join | exit)"
            ))
        }
    };
    let value = decoded.map_or(U256::ZERO, |d| balancer_bpt_scalar(&d));
    Ok(JsonValue::String(value.to_string()))
}

/// `balancer_v2_batch_swap_field(kind, swaps, assets, field) -> scalar`.
///
/// `Vault.batchSwap(kind, BatchSwapStep[] swaps, address[] assets, …)` references
/// its tokens INDIRECTLY: step `i`'s in/out token is `assets[swaps[i].assetInIndex]`
/// / `assets[swaps[i].assetOutIndex]` — an indirection the `$args.*` grammar cannot
/// express. This resolves it into a single aggregate `Amm::Swap`:
/// `token_in = assets[swaps[0].assetInIndex]`, `token_out = assets[swaps[last].assetOutIndex]`.
/// `swaps[i]` arrives as a positional array `[poolId, assetInIndex, assetOutIndex, amount, userData]`.
/// `field ∈ {token_in, token_out, pool_id, amount, direction_kind}`. Coarse for true
/// multi-path batches (picks first-in/last-out + first step's amount/pool) — correct
/// for the dominant single-route multi-hop; documented in the manifest `_note`.
/// `kind`: `0 = GIVEN_IN` (exact input), `1 = GIVEN_OUT` (exact output).
fn balancer_v2_batch_swap_field(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 4 {
        return Err(format!(
            "balancer_v2_batch_swap_field expects 4 args (kind, swaps, assets, field), got {}",
            args.len()
        ));
    }
    // kind is uint8 (0=GIVEN_IN/1=GIVEN_OUT); default GIVEN_IN if a fuzz value
    // does not fit u64. swaps/assets/field must be the right JSON kinds (structural).
    let kind = json_to_u64(&args[0]).unwrap_or(0);
    let swaps = args[1]
        .as_array()
        .ok_or("balancer_v2_batch_swap_field: swaps arg is not an array")?;
    let assets = args[2]
        .as_array()
        .ok_or("balancer_v2_batch_swap_field: assets arg is not an array")?;
    let field = args[3]
        .as_str()
        .ok_or("balancer_v2_batch_swap_field: field arg is not a string")?;
    const ZERO_ADDR: &str = "0x0000000000000000000000000000000000000000";
    let first = swaps.first().and_then(JsonValue::as_array);
    let last = swaps.last().and_then(JsonValue::as_array);
    // Resolve assets[idx] for a (possibly huge / out-of-range) index by wrapping
    // modulo assets.len() — identity on real in-bounds indices; on the validate's
    // type-valid synthetic fuzz (huge uint / oob index = would-revert tx) it lands
    // in-bounds so the decode stays shape-valid (real correctness pinned by corpus
    // expect_body). Empty assets → zero-address sentinel.
    let asset_at = |idx_val: Option<&JsonValue>| -> JsonValue {
        if assets.is_empty() {
            return JsonValue::String(ZERO_ADDR.to_owned());
        }
        let idx = idx_val
            .and_then(|v| json_u256(v, "idx").ok())
            .map(|u| usize::try_from(u % U256::from(assets.len())).unwrap_or(0))
            .unwrap_or(0);
        assets
            .get(idx)
            .cloned()
            .unwrap_or_else(|| JsonValue::String(ZERO_ADDR.to_owned()))
    };
    match field {
        // BatchSwapStep = [poolId(0), assetInIndex(1), assetOutIndex(2), amount(3), userData(4)].
        "token_in" => Ok(asset_at(first.and_then(|s| s.get(1)))),
        "token_out" => Ok(asset_at(last.and_then(|s| s.get(2)))),
        "pool_id" => Ok(first
            .and_then(|s| s.first())
            .cloned()
            .unwrap_or_else(|| JsonValue::String(format!("0x{}", "0".repeat(64))))),
        "amount" => Ok(first
            .and_then(|s| s.get(3))
            .cloned()
            .unwrap_or_else(|| JsonValue::String("0".to_owned()))),
        "direction_kind" => Ok(JsonValue::String(
            if kind == 0 {
                "exact_input"
            } else {
                "exact_output"
            }
            .to_owned(),
        )),
        other => Err(format!(
            "balancer_v2_batch_swap_field: unsupported field '{other}'"
        )),
    }
}

/// `balancer_v3_swap_path_field(paths, field) -> address|uint` — aggregate-`Swap`
/// field from a Balancer V3 BatchRouter `swapExactIn`'s `SwapPathExactAmountIn[]`.
///
/// Unlike V2's index-indirection, V3 carries the tokens directly in the path:
/// `path = [tokenIn, steps[], exactAmountIn, minAmountOut]` and
/// `steps[j] = [pool, tokenOut, isBuffer]` (positional, per `args_json`). The
/// aggregate swap is `path[0]`: `token_in = tokenIn`, `token_out = the LAST step's
/// tokenOut`, `pool = the first step's pool`, `amount_in = exactAmountIn`,
/// `min_out = minAmountOut`. No baked map needed (tokens are in calldata). Coarse
/// for true multi-path batches (picks the first path) — correct for the dominant
/// single-route multi-hop; documented in the manifest `_note`. Fuzz-tolerant:
/// empty paths/steps degrade to zero-address / "0" (real correctness pinned by the
/// corpus expect_body), so no harness shape-artifact entry is needed.
fn balancer_v3_swap_path_field(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "balancer_v3_swap_path_field expects 2 args (paths, field), got {}",
            args.len()
        ));
    }
    let paths = args[0]
        .as_array()
        .ok_or("balancer_v3_swap_path_field: paths arg is not an array")?;
    let field = args[1]
        .as_str()
        .ok_or("balancer_v3_swap_path_field: field arg is not a string")?;
    const ZERO_ADDR: &str = "0x0000000000000000000000000000000000000000";
    let zero_addr = || JsonValue::String(ZERO_ADDR.to_owned());
    let p0 = paths.first().and_then(JsonValue::as_array);
    let steps = p0.and_then(|p| p.get(1)).and_then(JsonValue::as_array);
    match field {
        // path[0] = [tokenIn(0), steps(1), exactAmountIn(2), minAmountOut(3)].
        "token_in" => Ok(p0
            .and_then(|p| p.first())
            .cloned()
            .unwrap_or_else(zero_addr)),
        // step = [pool(0), tokenOut(1), isBuffer(2)]; last step's tokenOut, else tokenIn (no-step fuzz).
        "token_out" => Ok(steps
            .and_then(|s| s.last())
            .and_then(JsonValue::as_array)
            .and_then(|st| st.get(1))
            .cloned()
            .or_else(|| p0.and_then(|p| p.first()).cloned())
            .unwrap_or_else(zero_addr)),
        "pool" => Ok(steps
            .and_then(|s| s.first())
            .and_then(JsonValue::as_array)
            .and_then(|st| st.first())
            .cloned()
            .unwrap_or_else(zero_addr)),
        "amount_in" => Ok(p0
            .and_then(|p| p.get(2))
            .cloned()
            .unwrap_or_else(|| JsonValue::String("0".to_owned()))),
        "min_out" => Ok(p0
            .and_then(|p| p.get(3))
            .cloned()
            .unwrap_or_else(|| JsonValue::String("0".to_owned()))),
        other => Err(format!(
            "balancer_v3_swap_path_field: unsupported field '{other}'"
        )),
    }
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

/// `unoswap_route_hash(dex: uint256, [dex2: uint256], [dex3: uint256]) -> bytes32`
/// — a deterministic ScopeBall identity for a 1inch unoswap route (NOT an
/// on-chain value). 1inch unoswap/unoswap2/unoswap3 pack each pool as a
/// `uint256` `dex` word (low 160 bits = pool address, high bits = direction /
/// protocol flags). This hashes `keccak256(dex ++ [dex2] ++ [dex3])` over the
/// raw 32-byte big-endian words, so the same packed pool sequence (addresses
/// AND flags) hashes identically regardless of amounts. Variadic over 1..=3 dex
/// words (one per hop). Feeds `AmmVenue::AggregatorRoute.route_hash`.
fn unoswap_route_hash(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.is_empty() || args.len() > 3 {
        return Err(format!(
            "unoswap_route_hash expects 1..=3 dex words, got {}",
            args.len()
        ));
    }
    let mut bytes = Vec::with_capacity(args.len() * 32);
    for (idx, v) in args.iter().enumerate() {
        let packed = json_u256(v, &format!("unoswap_route_hash: dex[{idx}]"))?;
        bytes.extend_from_slice(&packed.to_be_bytes::<32>());
    }
    Ok(JsonValue::String(format!("{:#x}", keccak256(&bytes))))
}

/// `u64_saturating(uint) -> u64 number`. Clamps a (possibly >u64) uint-like
/// value to `u64::MAX`. Used by Umbrella batch permit-deadline fields.
fn u64_saturating(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 1 {
        return Err(format!("u64_saturating expects 1 arg, got {}", args.len()));
    }
    let value = match &args[0] {
        JsonValue::Number(n) => n.as_u64().unwrap_or(u64::MAX),
        JsonValue::String(s) => s.parse::<u64>().unwrap_or(u64::MAX),
        other => {
            return Err(format!(
                "u64_saturating: argument is not a uint-like value: {other}"
            ))
        }
    };
    Ok(JsonValue::Number(serde_json::Number::from(value)))
}

/// `array_len(array) -> decimal-string length`. Used for proposal payload counts.
fn array_len(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 1 {
        return Err(format!("array_len expects 1 arg, got {}", args.len()));
    }
    let arr = args[0]
        .as_array()
        .ok_or("array_len: argument is not an array")?;
    Ok(JsonValue::String(arr.len().to_string()))
}

/// `tuple_array_field(tuple[], index) -> array`.
///
/// Solidity tuple arrays decode positionally in this registry path. This helper
/// projects one tuple slot out of every row, e.g. Aave Governance V3
/// `Payload[]` slot N into a flat `array`.
fn tuple_array_field(args: &[JsonValue]) -> Result<JsonValue, String> {
    if args.len() != 2 {
        return Err(format!(
            "tuple_array_field expects 2 args (array, index), got {}",
            args.len()
        ));
    }
    let arr = args[0]
        .as_array()
        .ok_or("tuple_array_field: first argument is not an array")?;
    let index = match &args[1] {
        JsonValue::Number(n) => n.as_u64(),
        JsonValue::String(s) => s.parse::<u64>().ok(),
        _ => None,
    }
    .and_then(|n| usize::try_from(n).ok())
    .ok_or("tuple_array_field: index is not a uint")?;
    let mut out = Vec::with_capacity(arr.len());
    for (row_idx, row) in arr.iter().enumerate() {
        let tuple = row
            .as_array()
            .ok_or_else(|| format!("tuple_array_field: row {row_idx} is not a tuple array"))?;
        let value = tuple.get(index).cloned().ok_or_else(|| {
            format!("tuple_array_field: row {row_idx} index {index} out of bounds")
        })?;
        out.push(value);
    }
    Ok(JsonValue::Array(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unoswap_route_hash_is_variadic_deterministic_and_32_bytes() {
        // Accepts decimal-string, hex-string, and numeric dex words.
        let dex = json!("0xc45a81bc23a64ea556ab4cdf08a86b61cdceea8bfb39cfb5");
        let h1 = unoswap_route_hash(std::slice::from_ref(&dex)).unwrap();
        let h2 = unoswap_route_hash(std::slice::from_ref(&dex)).unwrap();
        assert_eq!(h1, h2);
        let s = h1.as_str().unwrap();
        assert!(s.starts_with("0x"));
        assert_eq!(s.len(), 66); // 0x + 64 hex

        // Multi-hop (2 and 3 dex words) is supported and order-sensitive.
        let dex2 = json!("0x1111111111111111111111111111111111111111");
        let dex3 = json!(42u64);
        let two = unoswap_route_hash(&[dex.clone(), dex2.clone()]).unwrap();
        let three = unoswap_route_hash(&[dex.clone(), dex2.clone(), dex3.clone()]).unwrap();
        assert_ne!(h1, two);
        assert_ne!(two, three);
        // Reordering the dex words changes the hash.
        let swapped = unoswap_route_hash(&[dex2, dex]).unwrap();
        assert_ne!(two, swapped);

        // Arg-count bounds (0 and >3) error out.
        assert!(unoswap_route_hash(&[]).is_err());
        assert!(unoswap_route_hash(&[json!(1), json!(2), json!(3), json!(4)]).is_err());
        // Non-uint arg errors out.
        assert!(unoswap_route_hash(&[json!("not-a-number")]).is_err());
    }

    #[test]
    fn whitelist_matches_shared_json() {
        // Single source of truth: registryV2/scripts/build-index.ts validates
        // manifest `$fn` names against this same JSON. Keep WHITELIST and
        // fn_whitelist.json identical so the build-time gate and the decode-time
        // dispatch agree — a drift would let an unknown `$fn` pass one but not the
        // other.
        let shared: Vec<String> = serde_json::from_str(include_str!("fn_whitelist.json"))
            .expect("parse fn_whitelist.json");
        assert_eq!(
            WHITELIST
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            shared,
            "Rust WHITELIST and fn_whitelist.json (build-index source) drifted",
        );
    }

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
    fn maker_traits_expiry_extracts_bits_80_to_120() {
        // expiry 101 at bits [80,120); a high flag bit (255) must be ignored.
        let traits = (U256::from(101u64) << 80usize) | (U256::from(1u64) << 255usize);
        let out = maker_traits_expiry(&[json!(format!("{traits:#x}"))]).unwrap();
        assert_eq!(out, json!(101u64));

        // A realistic uint40 unix timestamp round-trips.
        let traits = U256::from(1_738_003_600u64) << 80usize;
        let out = maker_traits_expiry(&[json!(format!("{traits:#x}"))]).unwrap();
        assert_eq!(out, json!(1_738_003_600u64));

        // expiry region == 0 (never-expires) remaps to the max-uint40 sentinel.
        let no_expiry = (U256::from(1u64) << 255usize) | U256::from(0xdeadu64);
        let out = maker_traits_expiry(&[json!(format!("{no_expiry:#x}"))]).unwrap();
        assert_eq!(out, json!(0xFF_FFFF_FFFFu64)); // 1_099_511_627_775

        assert!(maker_traits_expiry(&[]).is_err());
    }

    #[test]
    fn coalesce_address_prefers_nonzero_then_fallback() {
        let a = "0x1111111111111111111111111111111111111111";
        let b = "0x2222222222222222222222222222222222222222";
        let zero = "0x0000000000000000000000000000000000000000";
        // non-zero addr → returned as-is (lowercase).
        assert_eq!(coalesce_address(&[json!(a), json!(b)]).unwrap(), json!(a));
        // zero addr → fallback.
        assert_eq!(
            coalesce_address(&[json!(zero), json!(b)]).unwrap(),
            json!(b)
        );
        // wrong arity + non-address error out.
        assert!(coalesce_address(&[json!(a)]).is_err());
        assert!(coalesce_address(&[json!("nope"), json!(b)]).is_err());
    }

    #[test]
    fn token_key_or_native_maps_sentinel_to_native_else_erc20() {
        let chain = "eip155:1";
        // Native sentinel -> native key (no address).
        let native = token_key_or_native(&[
            json!("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"),
            json!(chain),
        ])
        .unwrap();
        assert_eq!(native["standard"], json!("native"));
        assert_eq!(native["chain"], json!(chain));
        assert!(native.get("address").is_none());
        // A real ERC-20 -> erc20 key with the lowercase address.
        let erc20 = token_key_or_native(&[
            json!("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"),
            json!(chain),
        ])
        .unwrap();
        assert_eq!(erc20["standard"], json!("erc20"));
        assert_eq!(
            erc20["address"],
            json!("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")
        );
        // arity + bad address error out.
        assert!(token_key_or_native(&[json!("0x0")]).is_err());
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

    // ── Balancer V2/V3 deeper-onboarding builtins ────────────────────────────

    #[test]
    fn balancer_zip_token_amounts_zips_index_aligned() {
        let chain = "eip155:1";
        let out = balancer_zip_token_amounts(&[json!(chain), json!([A, B]), json!(["100", "200"])])
            .unwrap();
        // [[{key:{standard,chain,address:A}}, "100"], [{...B}, "200"]]
        assert_eq!(out.as_array().unwrap().len(), 2);
        assert_eq!(out[0][0]["key"]["standard"], json!("erc20"));
        assert_eq!(out[0][0]["key"]["chain"], json!(chain));
        assert_eq!(out[0][0]["key"]["address"], json!(A));
        assert_eq!(out[0][1], json!("100"));
        assert_eq!(out[1][0]["key"]["address"], json!(B));
        assert_eq!(out[1][1], json!("200"));
        // length mismatch → zips the common prefix (min length), not an error
        // (real Balancer calldata is always equal-length; this tolerates fuzz).
        let trunc =
            balancer_zip_token_amounts(&[json!(chain), json!([A]), json!(["1", "2"])]).unwrap();
        assert_eq!(trunc.as_array().unwrap().len(), 1);
        // wrong arity still errors (structural).
        assert!(balancer_zip_token_amounts(&[json!(chain), json!([A])]).is_err());
    }

    #[test]
    fn balancer_v3_zip_pool_tokens_looks_up_and_zips() {
        let chain = "eip155:1";
        let pool = "0x2222222222222222222222222222222222222222";
        let map = json!({ "0x2222222222222222222222222222222222222222": [A, B] });
        let out = balancer_v3_zip_pool_tokens(&[
            json!(chain),
            json!(pool),
            json!(["100", "200"]),
            map.clone(),
        ])
        .unwrap();
        assert_eq!(out.as_array().unwrap().len(), 2);
        assert_eq!(out[0][0]["key"]["standard"], json!("erc20"));
        assert_eq!(out[0][0]["key"]["chain"], json!(chain));
        assert_eq!(out[0][0]["key"]["address"], json!(A));
        assert_eq!(out[0][1], json!("100"));
        assert_eq!(out[1][0]["key"]["address"], json!(B));
        assert_eq!(out[1][1], json!("200"));
        // pool absent from the baked map → fail-closed error (warns, never silent-passes).
        let missing = balancer_v3_zip_pool_tokens(&[
            json!(chain),
            json!("0x9999999999999999999999999999999999999999"),
            json!(["1", "2"]),
            map.clone(),
        ]);
        assert!(missing.is_err());
        // amounts shorter than the pool token list → zips common prefix (fuzz-tolerant).
        let trunc =
            balancer_v3_zip_pool_tokens(&[json!(chain), json!(pool), json!(["1"]), map]).unwrap();
        assert_eq!(trunc.as_array().unwrap().len(), 1);
        // wrong arity errors (structural).
        assert!(balancer_v3_zip_pool_tokens(&[json!(chain), json!(pool)]).is_err());
    }

    #[test]
    fn balancer_v3_swap_path_field_extracts_endpoints() {
        // path = [tokenIn, steps[[pool,tokenOut,isBuffer]], exactAmountIn, minAmountOut]
        let paths = json!([[A, [[C, B, false]], "1000", "990"]]);
        let f = |field: &str| balancer_v3_swap_path_field(&[paths.clone(), json!(field)]).unwrap();
        assert_eq!(f("token_in"), json!(A));
        assert_eq!(f("token_out"), json!(B)); // last step's tokenOut
        assert_eq!(f("pool"), json!(C));
        assert_eq!(f("amount_in"), json!("1000"));
        assert_eq!(f("min_out"), json!("990"));
        // multi-hop: token_out = the LAST step's tokenOut.
        let multi = json!([[A, [[C, B, false], [C, A, false]], "1", "1"]]);
        assert_eq!(
            balancer_v3_swap_path_field(&[multi, json!("token_out")]).unwrap(),
            json!(A)
        );
        // empty paths (fuzz) → zero/0, not an error.
        assert_eq!(
            balancer_v3_swap_path_field(&[json!([]), json!("token_in")]).unwrap(),
            json!("0x0000000000000000000000000000000000000000")
        );
        // unknown field errors (structural).
        assert!(balancer_v3_swap_path_field(&[paths, json!("nope")]).is_err());
    }

    #[test]
    fn balancer_pool_id_to_address_takes_leading_20_bytes() {
        // poolId = poolAddress(20) ++ specialization(2) ++ nonce(10).
        let pool = "0xabcdef0123456789abcdef0123456789abcdef01";
        let pool_id = format!("{pool}00000000000000000000aaaa"); // 20+2+10 bytes
        let out = balancer_pool_id_to_address(&[json!(pool_id)]).unwrap();
        assert_eq!(out.as_str().unwrap(), pool);
        // wrong length + non-string error out.
        assert!(balancer_pool_id_to_address(&[json!("0xdeadbeef")]).is_err());
        assert!(balancer_pool_id_to_address(&[json!(123)]).is_err());
    }

    /// Encode a Balancer `userData` blob = `abi.encode(kind, …)` (no selector).
    fn encode_userdata(values: Vec<DynSolValue>) -> String {
        let bytes = DynSolValue::Tuple(values).abi_encode_params();
        format!("0x{}", hex::encode(bytes))
    }

    #[test]
    fn balancer_v2_userdata_field_join_min_bpt() {
        // JOIN kind 1 EXACT_TOKENS_IN_FOR_BPT_OUT = (1, amountsIn[], minBPTAmountOut).
        let user_data = encode_userdata(vec![
            uint(1),
            DynSolValue::Array(vec![uint(50), uint(60)]),
            uint(777),
        ]);
        let out = balancer_v2_userdata_field(&[json!(user_data), json!("join"), json!("min_bpt")])
            .unwrap();
        assert_eq!(out, json!("777")); // minBPTAmountOut is arg[2] (arg[1] is the array)

        // JOIN kind 2 TOKEN_IN_FOR_EXACT_BPT_OUT = (2, bptAmountOut, tokenIndex) → arg[1].
        let ud2 = encode_userdata(vec![uint(2), uint(888), uint(0)]);
        let out2 =
            balancer_v2_userdata_field(&[json!(ud2), json!("join"), json!("min_bpt")]).unwrap();
        assert_eq!(out2, json!("888"));
    }

    #[test]
    fn balancer_v2_userdata_field_exit_bpt_in() {
        // EXIT kind 0 EXACT_BPT_IN_FOR_ONE_TOKEN_OUT = (0, bptAmountIn, tokenIndex) → arg[1].
        let ud0 = encode_userdata(vec![uint(0), uint(1234), uint(1)]);
        let out0 =
            balancer_v2_userdata_field(&[json!(ud0), json!("exit"), json!("bpt_in")]).unwrap();
        assert_eq!(out0, json!("1234"));

        // EXIT kind 2 (weighted) BPT_IN_FOR_EXACT_TOKENS_OUT = (2, amountsOut[], maxBPTAmountIn) → arg[2].
        let ud2 = encode_userdata(vec![
            uint(2),
            DynSolValue::Array(vec![uint(10), uint(20)]),
            uint(4321),
        ]);
        let out2 =
            balancer_v2_userdata_field(&[json!(ud2), json!("exit"), json!("bpt_in")]).unwrap();
        assert_eq!(out2, json!("4321"));
    }

    #[test]
    fn balancer_v2_userdata_field_unknown_kind_is_zero() {
        // kind 99 matches no entry → conservative "0".
        let ud = encode_userdata(vec![uint(99)]);
        let out =
            balancer_v2_userdata_field(&[json!(ud), json!("join"), json!("min_bpt")]).unwrap();
        assert_eq!(out, json!("0"));
        // bad field / class / arity error out.
        let ud1 = encode_userdata(vec![uint(0), uint(1), uint(0)]);
        assert!(balancer_v2_userdata_field(&[json!(ud1), json!("exit"), json!("nope")]).is_err());
        assert!(
            balancer_v2_userdata_field(&[json!("0x"), json!("nope"), json!("bpt_in")]).is_err()
        );
    }

    #[test]
    fn balancer_v2_batch_swap_field_resolves_indirect_indices() {
        // 2-hop A→B→C: swaps[0] in=assets[0]=A out=assets[1]=B; swaps[1] in=B out=assets[2]=C.
        // BatchSwapStep = [poolId, assetInIndex, assetOutIndex, amount, userData].
        let swaps = json!([
            [POOL1, "0", "1", "1000", "0x"],
            [POOL2, "1", "2", "0", "0x"]
        ]);
        let assets = json!([A, B, C]);
        let tin = balancer_v2_batch_swap_field(&[
            json!(0),
            swaps.clone(),
            assets.clone(),
            json!("token_in"),
        ])
        .unwrap();
        let tout = balancer_v2_batch_swap_field(&[
            json!(0),
            swaps.clone(),
            assets.clone(),
            json!("token_out"),
        ])
        .unwrap();
        let dir_in = balancer_v2_batch_swap_field(&[
            json!(0),
            swaps.clone(),
            assets.clone(),
            json!("direction_kind"),
        ])
        .unwrap();
        let dir_out = balancer_v2_batch_swap_field(&[
            json!(1),
            swaps.clone(),
            assets.clone(),
            json!("direction_kind"),
        ])
        .unwrap();
        let amount = balancer_v2_batch_swap_field(&[
            json!(0),
            swaps.clone(),
            assets.clone(),
            json!("amount"),
        ])
        .unwrap();
        let pool =
            balancer_v2_batch_swap_field(&[json!(0), swaps.clone(), assets, json!("pool_id")])
                .unwrap();
        assert_eq!(tin, json!(A));
        assert_eq!(tout, json!(C));
        assert_eq!(dir_in, json!("exact_input"));
        assert_eq!(dir_out, json!("exact_output"));
        assert_eq!(amount, json!("1000"));
        assert_eq!(pool, json!(POOL1));
        // out-of-range index wraps modulo assets.len() (9 % 1 = 0 → A); fuzz-tolerant.
        let bad = json!([[POOL1, "0", "9", "1", "0x"]]);
        assert_eq!(
            balancer_v2_batch_swap_field(&[json!(0), bad, json!([A]), json!("token_out")]).unwrap(),
            json!(A)
        );
        // empty assets → zero-address sentinel (shape-valid, fuzz would-revert case).
        let ok_swaps = json!([[POOL1, "0", "1", "1", "0x"]]);
        assert_eq!(
            balancer_v2_batch_swap_field(&[
                json!(0),
                ok_swaps.clone(),
                json!([]),
                json!("token_in")
            ])
            .unwrap(),
            json!("0x0000000000000000000000000000000000000000")
        );
        // unsupported field still errors (structural manifest bug, not data).
        assert!(
            balancer_v2_batch_swap_field(&[json!(0), ok_swaps, json!([A]), json!("nope")]).is_err()
        );
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
