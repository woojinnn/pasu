//! M1 — `declarative_action_builder`
//!
//! Convert a v3 registry manifest's `emit.body` (nested-twice DSL with `$`
//! placeholders) into a typed [`ActionBody`] from the `policy-transition`
//! crate. The translation has four stages:
//!
//! 1. **placeholder substitution** — recursively rewrite every string in the
//!    template starting with `$` into a concrete JSON value drawn from the
//!    [`V3MapContext`].
//! 2. **flatten** — collapse the manifest's nested encoding
//!    `{ domain: "<d>", "<d>": { action: "<a>", "<a>": { ...payload } } }`
//!    into the flat double-tagged form
//!    `{ domain: "<d>", action: "<a>", ...payload }` which serde can decode
//!    straight into [`ActionBody`].
//! 3. **live_inputs injection** — for each `live_inputs.<field>` of the
//!    target variant, substitute placeholders inside the `source` descriptor
//!    and wrap it with a default `value` plus the host-provided `synced_at`,
//!    matching the [`policy_state::LiveField`] serde shape. Default values
//!    are looked up from a small per-`(domain, action, field)` catalog.
//! 4. **deserialize** — `serde_json::from_value::<ActionBody>(_)` produces the
//!    fully typed value.
//!
//! Per the narrow M1 scope the catalog covers only the eight live-input
//! locations exercised by the 11 PDF FSM unit tests (amm swap / add liquidity
//! / remove liquidity, token erc20_permit, permit2_sign_allowance, ur multicall
//! wrap of `Amm(Swap)`). Adding new fields is a one-line update to
//! [`live_input_default`].
//!
//! No on-chain calls, no RPC, no async. The module is pure
//! `(template, ctx) -> ActionBody` and runs unmodified on wasm32.

use std::collections::BTreeMap;

use policy_state::primitives::{Address, ChainId, Time, U256};
use policy_transition::action::ActionBody;
use serde_json::{Map as JsonMap, Value as JsonValue};

/// Errors surfaced by the v3 builder.
#[derive(thiserror::Error, Debug)]
pub enum V3BuildError {
    /// A `$<root>.<field>` reference could not be resolved against the context.
    #[error("unresolved placeholder: {0}")]
    UnresolvedPlaceholder(String),
    /// A `$args.<path>` walk failed (missing field, out-of-range index, bad syntax).
    #[error("invalid arg path '{path}' on args {args}")]
    InvalidArgPath {
        /// JSONPath suffix that failed (without the `$args.` prefix).
        path: String,
        /// JSON-encoded args value the walker was inspecting.
        args: String,
    },
    /// Final `from_value::<ActionBody>` did not match the target enum shape.
    #[error("serde from_value: {0}")]
    SerdeFromValue(#[from] serde_json::Error),
    /// `emit.strategy` is something other than `single_emit` /
    /// `opcode_stream_dispatch`. Future strategies can plug in here.
    #[error("unsupported emit.strategy: {0}")]
    UnsupportedStrategy(String),
    /// `opcode_stream_dispatch` saw a byte not listed in `per_opcode_body` and
    /// the active [`UnknownOpcodePolicy`] is [`UnknownOpcodePolicy::Deny`].
    #[error("unknown opcode 0x{opcode:02x} (policy={policy:?})")]
    UnknownOpcode {
        /// Raw opcode byte (after `mask` is applied).
        opcode: u8,
        /// Policy that triggered the error.
        policy: UnknownOpcodePolicy,
    },
    /// A `$resolved.<k>` / `$derived.<k>` placeholder name is not in the
    /// fallback type catalog. Fail-loud so manifest authors notice when they
    /// introduce a new placeholder that hasn't been wired into
    /// [`placeholder_type_lookup`].
    #[error("unknown placeholder (no fallback type registered): {0}")]
    UnknownPlaceholder(String),
    /// `array_emit`'s `array_source` placeholder resolved to a non-array JSON
    /// value (object / string / number / bool / null). The strategy can only
    /// iterate a homogeneous array, so this is fail-loud — the manifest's
    /// `emit.array_source` points at the wrong field.
    #[error("array_emit array_source did not resolve to an array: {0}")]
    ArraySourceNotArray(String),
    /// `array_emit.parallel_sources.<name>` resolved to a non-array JSON value.
    #[error("array_emit parallel_sources.{name} did not resolve to an array: {placeholder}")]
    ParallelSourceNotArray {
        /// Parallel source key.
        name: String,
        /// Placeholder that was resolved.
        placeholder: String,
    },
    /// A parallel source length differed from `array_source` length.
    #[error(
        "array_emit parallel_sources.{name} length mismatch: array_source has {array_len}, parallel source has {parallel_len}"
    )]
    ParallelSourceLengthMismatch {
        /// Parallel source key.
        name: String,
        /// Primary array length.
        array_len: usize,
        /// Parallel array length.
        parallel_len: usize,
    },
    /// A discriminant value-map (`{ $match, $cases, $default? }`) resolved its
    /// `$match` to a key that is absent from `$cases` and the map declares no
    /// `$default`. Fail-loud — the manifest author missed an on-chain enum
    /// value (e.g. an `InterestRateMode` the `$cases` table doesn't list).
    #[error("value-map: no case for matched key '{matched}' and no $default")]
    ValueMapNoMatch {
        /// The lookup key the `$match` value resolved to.
        matched: String,
    },
    /// A discriminant value-map is structurally invalid: `$cases` missing or
    /// not a JSON object, or the `$match` value resolved to a type that has no
    /// canonical key form (anything other than String / Number / Bool).
    #[error("value-map malformed: {0}")]
    ValueMapMalformed(String),
    /// A `$fn` call object (`{ $fn, $args }`) referenced an unknown function,
    /// had a malformed shape (non-string `$fn`, non-array `$args`, stray key),
    /// or its executor rejected the substituted arguments. Fail-loud — a
    /// manifest author used a non-whitelisted `$fn` or wired bad args.
    #[error("$fn '{function}' failed: {reason}")]
    FnCall {
        /// The `$fn` name (or a sentinel when the name itself was malformed).
        function: String,
        /// Human-readable cause from the shape check / executor.
        reason: String,
    },
}

/// Fallback type for unresolved `$resolved.<k>` / `$derived.<k>` placeholders.
///
/// Plan §M9 — Sync orchestrator (별 plan) 가 실체화 전까지 narrow scope
/// ("value 비어있는 상태") 를 유지하려면 placeholder 의 expected type 에 맞는
/// zero value 를 채워야 함. ABI Address 자리에는 `0x0...0` (20 byte), u32 자리
/// 에는 0, U256 자리에는 `"0"`, bytes32 자리에는 `0x0...0` (32 byte).
#[derive(Copy, Clone, Debug)]
enum FallbackType {
    Address,
    U32,
    U256,
    Bytes32,
}

/// Plan §M9 — 25 manifest 의 placeholder 16종 → FallbackType 매핑.
///
/// match-based const evaluation. 새 placeholder 추가 시 본 함수에 arm 추가
/// (안 하면 [`V3BuildError::UnknownPlaceholder`] fail-loud).
fn placeholder_type_lookup(rest: &str) -> Option<FallbackType> {
    use FallbackType::*;
    match rest {
        "weth"
        | "factory"
        | "pool"
        | "pool_manager"
        | "v3_path_first_token"
        | "v3_path_last_token"
        | "v4_token_in"
        | "v4_token_out"
        | "v4_hooks"
        | "v4_recipient"
        // GeneralAdapter1 erc4626* leg underlying — injected by
        // `maybe_inject_metamorpho_underlying` for a KNOWN listed vault; a synthetic
        // fuzz arg (random vault) is value-gated out, so it falls back to a zero
        // address. (Unlike `morpho_market_id`, which is always keccak-computable.)
        | "metamorpho_underlying" => Some(Address),
        "fee_tier_bp" | "slippage_bp" => Some(U32),
        "v4_amount_in" | "v4_amount_out_min" | "min_lp_out" => Some(U256),
        "v4_pool_id" => Some(Bytes32),
        _ => None,
    }
}

/// Type-aware zero value for [`FallbackType`].
fn zero_value_for(t: FallbackType) -> JsonValue {
    match t {
        FallbackType::Address => {
            JsonValue::String("0x0000000000000000000000000000000000000000".to_owned())
        }
        FallbackType::U32 => JsonValue::Number(serde_json::Number::from(0u32)),
        FallbackType::U256 => JsonValue::String("0".to_owned()),
        FallbackType::Bytes32 => JsonValue::String(
            "0x0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
        ),
    }
}

/// How [`build_multicall_from_opcode_stream`] reacts to an unknown opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownOpcodePolicy {
    /// Skip the opcode but log a warning via stderr (`eprintln!`).
    Warn,
    /// Abort the conversion with [`V3BuildError::UnknownOpcode`].
    Deny,
    /// Skip the opcode silently.
    Skip,
}

/// Inputs to placeholder substitution + live_input injection.
///
/// All fields are borrows from caller-owned data so the builder can run inside
/// a wasm RPC handler without copying.
#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct V3MapContext<'a> {
    /// CAIP-2 chain id of the parent tx (e.g. `"eip155:1"`).
    pub chain: ChainId,
    /// `tx.to` — the on-chain target contract.
    pub tx_to: Address,
    /// `tx.from` — the EOA / SCW initiating the call.
    pub tx_from: Address,
    /// `tx.value` — native value attached to the call.
    pub value: U256,
    /// Host-supplied wall-clock at submission time (drives `synced_at` of
    /// every freshly built [`LiveField`](policy_state::LiveField)).
    pub submitted_at: Time,
    /// Decoded calldata args as a JSON object keyed by ABI argument name.
    pub args_json: &'a JsonValue,
    /// Raw transaction calldata as a `"0x"`-prefixed hex string. Referenced by
    /// the bare `$calldata` placeholder so an [`ActionBody::Unknown`] body can
    /// PRESERVE the full calldata (its whole purpose for a scope analyzer)
    /// instead of emitting a `"0x"` sentinel. Empty (`""`) on the off-chain
    /// typed-data route, which has no calldata.
    pub raw_calldata: &'a str,
    /// `$resolved.<k>` lookups (filled by upstream resolvers — pool address,
    /// fee tier, etc.). May be empty in M1.
    pub resolved: BTreeMap<String, JsonValue>,
    /// `$derived.<k>` lookups (filled by upstream derivers — unfolded V3 path
    /// tokens, slippage_bp, etc.). May be empty in M1.
    pub derived: BTreeMap<String, JsonValue>,
    /// Per-opcode decoded inputs (only set inside the
    /// [`build_multicall_from_opcode_stream`] recursion). `$inputs.<path>`
    /// references fail with [`V3BuildError::UnresolvedPlaceholder`] when this
    /// is `None`.
    pub inputs: Option<&'a JsonValue>,
}

// ===========================================================================
// substitute_placeholders — recursive `$<root>.<path>` walker
// ===========================================================================

/// Recursively walk `template`, replacing every JSON string starting with `$`
/// by the value it resolves to against `ctx`.
///
/// Strings without a `$` prefix and all non-string values pass through
/// unchanged. Containers are walked depth-first.
///
/// ## Discriminant value-map (`$match` / `$cases` / `$default`)
///
/// A JSON **object** that carries the reserved key `"$match"` is NOT walked
/// field-by-field; it is resolved as a *value-map* by [`resolve_value_map`].
/// This maps a discriminant `uint` / `bool` arg onto an enum-variant value or a
/// whole sub-action object (something a bare `$args.x` substitution cannot do,
/// because it would inject the raw number):
///
/// ```jsonc
/// { "$match": "<placeholder|literal>",
///   "$cases": { "<key>": <value>, ... },
///   "$default": <value>           // optional
/// }
/// ```
///
/// `$match`, `$cases`, and `$default` are RESERVED object keys — an object that
/// happens to contain `$match` is always interpreted as a value-map, and any
/// OTHER key present is rejected fail-loud (a typo'd `$default` must not
/// silently degrade to a no-match). The matched-against value and the selected
/// case value are both run back through `substitute_placeholders`, so cases may
/// themselves embed placeholders or nested structures (field-level AND
/// action-tag-level switches compose).
///
/// # Errors
///
/// Returns [`V3BuildError::UnresolvedPlaceholder`] when a `$resolved.x` /
/// `$derived.x` / `$inputs.*` lookup is empty, [`V3BuildError::InvalidArgPath`]
/// when a `$args.*` JSONPath walk fails, and
/// [`V3BuildError::ValueMapNoMatch`] / [`V3BuildError::ValueMapMalformed`] for
/// value-map resolution failures.
pub fn substitute_placeholders(
    ctx: &V3MapContext<'_>,
    template: &JsonValue,
) -> Result<JsonValue, V3BuildError> {
    match template {
        JsonValue::String(s) if s.starts_with('$') => resolve_placeholder(ctx, s),
        JsonValue::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(substitute_placeholders(ctx, item)?);
            }
            Ok(JsonValue::Array(out))
        }
        JsonValue::Object(map) => {
            // A `$match` key turns this object into a discriminant value-map
            // (resolved instead of walked field-by-field).
            if map.contains_key("$match") {
                return resolve_value_map(ctx, map);
            }
            // A `$fn` key turns this object into a WhitelistedFn call (a value
            // that a single `$args.x` / value-map cannot express, e.g. Curve
            // Router NG's variable-hop output token). Additive reserved key —
            // existing single_emit manifests carry no `$fn`, so unaffected.
            if map.contains_key("$fn") {
                return resolve_fn_call(ctx, map);
            }
            let mut out = JsonMap::with_capacity(map.len());
            for (k, v) in map {
                out.insert(k.clone(), substitute_placeholders(ctx, v)?);
            }
            Ok(JsonValue::Object(out))
        }
        // Strings without `$`, numbers, bools, nulls pass through.
        other => Ok(other.clone()),
    }
}

/// Resolve a discriminant value-map object `{ $match, $cases, $default? }`.
///
/// 1. `$match` is run through [`substitute_placeholders`] (so it may itself be
///    `$args.rateMode`, a literal, or any placeholder).
/// 2. The resolved value is collapsed to a lookup-key string: JSON String →
///    as-is; JSON Number → `to_string()` (integer `1` → `"1"`); JSON Bool →
///    `"true"` / `"false"`. Any other type → [`V3BuildError::ValueMapMalformed`].
///    Numbers are assumed integer-valued (on-chain `uint` discriminants); a
///    fractional JSON number (e.g. `1.0`) stringifies to `"1.0"` and would NOT
///    match an integer case key `"1"`.
/// 3. The key is looked up in `$cases` (which must be a JSON object). On a hit
///    the case value is returned, recursively substituted. On a miss `$default`
///    is used (also recursively substituted) if present, else
///    [`V3BuildError::ValueMapNoMatch`].
fn resolve_value_map(
    ctx: &V3MapContext<'_>,
    map: &JsonMap<String, JsonValue>,
) -> Result<JsonValue, V3BuildError> {
    // 0. Reject any stray / typo'd key. Without this a misspelled reserved key
    //    (e.g. `$dafault`) would silently degrade to a ValueMapNoMatch instead
    //    of a clear "you typo'd a reserved key" error. Fail-loud, naming the
    //    offending key (mirrors how other V3BuildError paths name their input).
    for k in map.keys() {
        if !matches!(k.as_str(), "$match" | "$cases" | "$default") {
            return Err(V3BuildError::ValueMapMalformed(format!(
                "unexpected key '{k}' in value-map (allowed: $match, $cases, $default)"
            )));
        }
    }

    // 1. Resolve the matched-against value.
    let match_template = map
        .get("$match")
        .ok_or_else(|| V3BuildError::ValueMapMalformed("$match key missing".into()))?;
    let matched = substitute_placeholders(ctx, match_template)?;

    // 2. Collapse to a canonical lookup key. uint256 args arrive as decimal
    //    strings (Fix B: width > 64 → string), smaller uints as numbers, bools
    //    as bools — handle all three.
    let key = match &matched {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        other => {
            return Err(V3BuildError::ValueMapMalformed(format!(
                "$match resolved to a non-keyable type: {other}"
            )));
        }
    };

    // 3. Look up the case (then $default) and recursively substitute.
    let cases = map
        .get("$cases")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| {
            V3BuildError::ValueMapMalformed("$cases missing or not a JSON object".into())
        })?;
    if let Some(case_value) = cases.get(&key) {
        substitute_placeholders(ctx, case_value)
    } else if let Some(default_value) = map.get("$default") {
        substitute_placeholders(ctx, default_value)
    } else {
        Err(V3BuildError::ValueMapNoMatch { matched: key })
    }
}

/// Resolve a `$fn` call object `{ "$fn": "<name>", "$args": [<arg templates>] }`.
///
/// Each entry of `$args` is run back through [`substitute_placeholders`] (so an
/// arg may itself be `$args._route`, a literal, or a nested value-map), then the
/// resolved JSON args are dispatched to the named WhitelistedFn executor
/// ([`super::builtin_fn::dispatch`]). `$fn` and `$args` are the ONLY allowed
/// keys; any other is rejected fail-loud (mirrors the value-map key guard). A
/// missing `$args` is treated as an empty arg list.
fn resolve_fn_call(
    ctx: &V3MapContext<'_>,
    map: &JsonMap<String, JsonValue>,
) -> Result<JsonValue, V3BuildError> {
    for k in map.keys() {
        if !matches!(k.as_str(), "$fn" | "$args") {
            return Err(V3BuildError::FnCall {
                function: "<malformed>".to_owned(),
                reason: format!("unexpected key '{k}' in $fn call (allowed: $fn, $args)"),
            });
        }
    }
    let name = map
        .get("$fn")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| V3BuildError::FnCall {
            function: "<malformed>".to_owned(),
            reason: "$fn must be a string function name".to_owned(),
        })?;
    let arg_templates: &[JsonValue] = match map.get("$args") {
        Some(JsonValue::Array(a)) => a,
        None => &[],
        Some(_) => {
            return Err(V3BuildError::FnCall {
                function: name.to_owned(),
                reason: "$args must be a JSON array".to_owned(),
            });
        }
    };
    let mut resolved_args = Vec::with_capacity(arg_templates.len());
    for tpl in arg_templates {
        resolved_args.push(substitute_placeholders(ctx, tpl)?);
    }
    super::builtin_fn::dispatch(name, &resolved_args).map_err(|reason| V3BuildError::FnCall {
        function: name.to_owned(),
        reason,
    })
}

/// Resolve a single `$<root>` / `$<root>.<rest>` placeholder string.
fn resolve_placeholder(ctx: &V3MapContext<'_>, raw: &str) -> Result<JsonValue, V3BuildError> {
    // Strip the leading `$`. The root token ends at the first `.` OR `[`.
    // A `.` separator is dropped from `rest` (`$args.x` → root `args`, rest
    // `x`); a `[` is KEPT (`$inputs[0]` → root `inputs`, rest `[0]`) so the
    // index segment survives into `walk_json_path`. This lets a tuple/tuple[]
    // element be indexed POSITIONALLY straight off a root — the calldata
    // convention for nested tuples (Permit2 `PermitDetails`, V4
    // `modifyLiquidities` params) where decoded tuples are positional arrays.
    let body = &raw[1..];
    let dot = body.find('.');
    let bracket = body.find('[');
    let (root, rest) = match (dot, bracket) {
        // `[` present and not after a `.` → keep the bracket in `rest`.
        (None, Some(b)) => (&body[..b], &body[b..]),
        (Some(d), Some(b)) if b < d => (&body[..b], &body[b..]),
        // `.` separator → drop it.
        (Some(d), _) => (&body[..d], &body[d + 1..]),
        (None, None) => (body, ""),
    };

    match root {
        "chain" => Ok(JsonValue::String(ctx.chain.as_str().to_owned())),
        "to" => Ok(JsonValue::String(format!("{:#x}", ctx.tx_to))),
        // Bare `$calldata` → the raw tx calldata hex. It is a ROOT-ONLY token
        // (no `.<path>` / `[idx]` suffix — calldata is opaque bytes); any suffix
        // is a manifest authoring error, surfaced fail-loud.
        "calldata" if rest.is_empty() => Ok(JsonValue::String(ctx.raw_calldata.to_owned())),
        "calldata" => Err(V3BuildError::UnresolvedPlaceholder(format!(
            "$calldata takes no path suffix (got {raw:?})"
        ))),
        "tx" => match rest {
            "from" => Ok(JsonValue::String(format!("{:#x}", ctx.tx_from))),
            "to" => Ok(JsonValue::String(format!("{:#x}", ctx.tx_to))),
            "value" => Ok(JsonValue::String(ctx.value.to_string())),
            "chain" => Ok(JsonValue::String(ctx.chain.as_str().to_owned())),
            other => Err(V3BuildError::UnresolvedPlaceholder(format!("$tx.{other}"))),
        },
        "args" => {
            if rest.is_empty() {
                return Err(V3BuildError::UnresolvedPlaceholder(raw.to_owned()));
            }
            walk_json_path(ctx.args_json, rest).map_err(|e| V3BuildError::InvalidArgPath {
                path: rest.to_owned(),
                args: format!("{e}: {}", ctx.args_json),
            })
        }
        // Plan §M9 — `$resolved.<k>` / `$derived.<k>` 는 Sync orchestrator
        // (별 plan) 가 채우는 영역. 본 plan narrow scope 안에서 ctx.resolved /
        // ctx.derived 는 비어있을 수 있으므로, placeholder name 에 따라
        // type-aware zero value 로 fallback (Address / U32 / U256 / Bytes32).
        // 카탈로그에 없는 placeholder 는 UnknownPlaceholder 로 fail-loud —
        // manifest 작성자가 새 placeholder 도입 시 placeholder_type_lookup
        // 갱신 강제. 75b05d1 commit 의 Address zero hex 일괄 fallback 이
        // u32/bytes32 자리에서 serde mismatch 를 일으킨 부작용 fix.
        "resolved" | "derived" => {
            let map = if root == "resolved" {
                &ctx.resolved
            } else {
                &ctx.derived
            };
            if let Some(v) = map.get(rest).cloned() {
                Ok(v)
            } else if let Some(ty) = placeholder_type_lookup(rest) {
                Ok(zero_value_for(ty))
            } else {
                Err(V3BuildError::UnknownPlaceholder(format!("{root}.{rest}")))
            }
        }
        "inputs" => {
            let inputs = ctx
                .inputs
                .ok_or_else(|| V3BuildError::UnresolvedPlaceholder(raw.to_owned()))?;
            if rest.is_empty() {
                return Ok(inputs.clone());
            }
            walk_json_path(inputs, rest).map_err(|e| V3BuildError::InvalidArgPath {
                path: rest.to_owned(),
                args: format!("{e}: {inputs}"),
            })
        }
        other => Err(V3BuildError::UnresolvedPlaceholder(format!(
            "${other} (rest={rest:?})"
        ))),
    }
}

/// JSONPath walker for `args.<a>.<b>[idx][idx2]` style suffixes.
///
/// Supports dot-separated field traversal and bracketed integer indexing
/// (`[N]` or `[-N]` for tail-relative). Errors are returned as `String` so the
/// caller can wrap them in a builder-specific variant with full path context.
fn walk_json_path(root: &JsonValue, path: &str) -> Result<JsonValue, String> {
    // We clone at every traversal step. The JSON values we walk are at most a
    // few hundred bytes (Permit2 PermitSingle nested tuple is the widest
    // shape) — well below any threshold where zero-copy would be worth the
    // borrow-checker gymnastics.
    let mut cursor: JsonValue = root.clone();

    for raw_segment in path.split('.') {
        if raw_segment.is_empty() {
            return Err(format!("empty path segment in {path:?}"));
        }
        let (name, indices) = parse_segment(raw_segment, path)?;
        if !name.is_empty() {
            cursor = cursor
                .get(name)
                .cloned()
                .ok_or_else(|| format!("missing field {name:?} in {path:?}"))?;
        }
        for idx in indices {
            let arr = cursor.as_array().ok_or_else(|| {
                format!("indexed access on non-array at {name:?}[{idx}] in {path:?}")
            })?;
            let resolved: Option<usize> = if idx >= 0 {
                usize::try_from(idx).ok().filter(|i| *i < arr.len())
            } else {
                idx.checked_neg()
                    .and_then(|neg| usize::try_from(neg).ok())
                    .filter(|abs| *abs <= arr.len() && *abs > 0)
                    .map(|abs| arr.len() - abs)
            };
            let resolved =
                resolved.ok_or_else(|| format!("index {idx} out of bounds (len={})", arr.len()))?;
            cursor = arr[resolved].clone();
        }
    }

    Ok(cursor)
}

/// Parse one dot-segment into its name plus a chain of bracketed indices.
///
/// Returns the bareword name (may be empty if the segment is purely `[..]`)
/// and a Vec of signed indices. Negative indices are tail-relative.
fn parse_segment<'a>(segment: &'a str, full_path: &str) -> Result<(&'a str, Vec<i64>), String> {
    let bracket_start = segment.find('[');
    let name = match bracket_start {
        Some(idx) => &segment[..idx],
        None => segment,
    };
    let mut indices = Vec::new();
    if let Some(start) = bracket_start {
        let mut remainder = &segment[start..];
        while !remainder.is_empty() {
            if !remainder.starts_with('[') {
                return Err(format!(
                    "expected '[' after previous bracket in {full_path:?}, got {remainder:?}"
                ));
            }
            let close = remainder
                .find(']')
                .ok_or_else(|| format!("unterminated '[' in {full_path:?}"))?;
            let idx_str = &remainder[1..close];
            let idx: i64 = idx_str
                .parse()
                .map_err(|_| format!("invalid index {idx_str:?} in {full_path:?}"))?;
            indices.push(idx);
            remainder = &remainder[close + 1..];
        }
    }
    Ok((name, indices))
}

// ===========================================================================
// build_action_body — manifest emit.body → typed ActionBody
// ===========================================================================

/// Convert one manifest `emit.body` template (plus optional `live_inputs`)
/// into a typed [`ActionBody`].
///
/// The body template may also embed `live_inputs` directly inside its
/// `<domain>.<action>` payload (this is the v3 opcode-stream convention).
/// In that case the inline block is extracted before flattening and merged
/// with `live_inputs_template`.
///
/// # Errors
///
/// Forwards every variant of [`V3BuildError`]: placeholder lookups,
/// JSONPath misses, and the final serde decoding.
pub fn build_action_body(
    ctx: &V3MapContext<'_>,
    body_template: &JsonValue,
    live_inputs_template: Option<&JsonValue>,
) -> Result<ActionBody, V3BuildError> {
    // Stage 1 — substitute placeholders. Strip body-internal `live_inputs`
    // (the v3 opcode-stream convention) before substitution so it does not
    // round-trip through the same code path as the structural body.
    let (stripped_body, inline_live) = strip_inline_live_inputs(body_template);
    let substituted = substitute_placeholders(ctx, &stripped_body)?;
    let mut flat = flatten_body(&substituted)?;

    // Stage 2 — pick the live_inputs source. The explicit argument wins, then
    // the inline block from the body, then nothing.
    let live_source = live_inputs_template.cloned().or(inline_live);

    // Stage 3 — inject live_inputs. The injection shape depends on the
    // destination action's serde schema: some variants nest the fields under
    // `live_inputs` (Swap / AddLiquidity / RemoveLiquidity / CollectFees /
    // SignIntentOrder), others expose them inline (Erc20Permit nonce,
    // Permit2SignAllowance nonce). The catalog [`live_input_layout`] picks
    // the right shape per (domain, action).
    if let Some(live_template) = live_source {
        let (domain, action) = extract_tags(&flat);
        let live_substituted = substitute_placeholders(ctx, &live_template)?;
        if let Some(map) = live_substituted.as_object() {
            let layout = live_input_layout(domain.as_deref(), action.as_deref());
            match layout {
                LiveInputLayout::Nested => {
                    let mut live_obj = JsonMap::with_capacity(map.len());
                    for (field_name, src_payload) in map {
                        let wrapped = wrap_live_field(
                            ctx,
                            domain.as_deref(),
                            action.as_deref(),
                            field_name,
                            src_payload,
                            None,
                        );
                        live_obj.insert(field_name.clone(), wrapped);
                    }
                    flat.insert("live_inputs".into(), JsonValue::Object(live_obj));
                }
                LiveInputLayout::Inline => {
                    for (field_name, src_payload) in map {
                        let default_override = flat.get(field_name);
                        let wrapped = wrap_live_field(
                            ctx,
                            domain.as_deref(),
                            action.as_deref(),
                            field_name,
                            src_payload,
                            default_override,
                        );
                        flat.insert(field_name.clone(), wrapped);
                    }
                }
            }
        }
    }

    coerce_time_like_fields(&mut flat);

    // Stage 4 — typed decode.
    Ok(serde_json::from_value::<ActionBody>(JsonValue::Object(
        flat,
    ))?)
}

fn coerce_time_like_fields(flat: &mut JsonMap<String, JsonValue>) {
    for field in ["deadline", "expires_at", "sig_deadline", "valid_until"] {
        if let Some(value) = flat.get_mut(field) {
            coerce_decimal_string_to_u64(value);
        }
    }
}

fn coerce_decimal_string_to_u64(value: &mut JsonValue) {
    let JsonValue::String(s) = value else {
        return;
    };
    let trimmed = s.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return;
    }
    let parsed = trimmed.parse::<u64>().unwrap_or(u64::MAX);
    *value = JsonValue::Number(parsed.into());
}

/// Strip the `<domain>.<action>.live_inputs` sub-object (if any) from a body
/// template, returning the cleaned body plus the extracted live_inputs.
///
/// The v3 opcode-stream convention embeds `live_inputs` inside the payload
/// — we lift it out so the structural body flow doesn't try to flatten it as
/// part of the payload, and the live-input injection stage gets a uniform
/// `Option<live_inputs_template>` to consume.
fn strip_inline_live_inputs(body: &JsonValue) -> (JsonValue, Option<JsonValue>) {
    let JsonValue::Object(obj) = body else {
        return (body.clone(), None);
    };
    let Some(domain) = obj.get("domain").and_then(JsonValue::as_str) else {
        return (body.clone(), None);
    };
    let mut cloned_obj = obj.clone();
    let inline_live = cloned_obj
        .get_mut(domain)
        .and_then(JsonValue::as_object_mut)
        .and_then(|d| {
            let action_name = d.get("action").and_then(JsonValue::as_str)?.to_owned();
            d.get_mut(&action_name)
                .and_then(JsonValue::as_object_mut)
                .and_then(|payload| payload.remove("live_inputs"))
        });
    (JsonValue::Object(cloned_obj), inline_live)
}

/// Where do the `live_inputs.<field>` entries end up after deserialization?
///
/// * [`LiveInputLayout::Nested`] — the destination action has a
///   `live_inputs: SomeLiveInputs` sub-struct that owns every live field
///   (the AMM family).
/// * [`LiveInputLayout::Inline`] — each live field lives next to the
///   deterministic fields directly on the action struct (the token-permit
///   family).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveInputLayout {
    Nested,
    Inline,
}

/// Pick the live-input layout for a given (domain, action) pair.
///
/// Unknown / cross-cutting variants default to [`LiveInputLayout::Nested`] —
/// this is the safer choice because a missing `live_inputs` field on an
/// action that expects one yields a clear serde error, whereas an extra
/// `live_inputs` key on an action that doesn't is silently ignored by the
/// `deny_unknown_fields`-free variants.
fn live_input_layout(domain: Option<&str>, action: Option<&str>) -> LiveInputLayout {
    match (domain, action) {
        (Some("token"), Some("erc20_permit"))
        | (
            Some("token"),
            Some("permit2_sign_allowance" | "permit2_sign_transfer" | "permit2_transfer_from"),
        ) => LiveInputLayout::Inline,
        _ => LiveInputLayout::Nested,
    }
}

/// Flatten the nested manifest encoding `{ domain, "<d>": { action, "<a>":
/// payload } }` into `{ domain, action, ...payload }` serde can match.
///
/// The function only handles the two-level nesting actually emitted by v3
/// manifests; anything else falls through with the input cloned (which then
/// fails at the final `from_value` with a descriptive serde error).
fn flatten_body(body: &JsonValue) -> Result<JsonMap<String, JsonValue>, V3BuildError> {
    let obj = body
        .as_object()
        .ok_or_else(|| V3BuildError::UnresolvedPlaceholder("body must be a JSON object".into()))?;

    let domain_val = obj
        .get("domain")
        .ok_or_else(|| V3BuildError::UnresolvedPlaceholder("body.domain missing".into()))?;
    let domain = domain_val
        .as_str()
        .ok_or_else(|| V3BuildError::UnresolvedPlaceholder("body.domain not a string".into()))?
        .to_owned();

    // Domain == `multicall` is a struct variant on `ActionBody`, not a
    // newtype, so the flatten skips the second level and exposes `actions`
    // directly. Cross-cutting `unknown` is also a struct variant.
    if domain == "multicall" || domain == "unknown" {
        let mut out = JsonMap::new();
        out.insert("domain".into(), JsonValue::String(domain.clone()));
        // Copy every key under `<domain>` (or, if the manifest writes them
        // flat, copy them directly from `obj`).
        if let Some(nested) = obj.get(&domain).and_then(JsonValue::as_object) {
            for (k, v) in nested {
                out.insert(k.clone(), v.clone());
            }
        } else {
            for (k, v) in obj {
                if k == "domain" {
                    continue;
                }
                out.insert(k.clone(), v.clone());
            }
        }
        return Ok(out);
    }

    // Per-domain newtype: descend into `obj.<domain>` to find `action` + the
    // payload object keyed by `<action>`.
    let inner = obj
        .get(&domain)
        .and_then(JsonValue::as_object)
        .ok_or_else(|| {
            V3BuildError::UnresolvedPlaceholder(format!("body.{domain} missing or not an object"))
        })?;
    let action_val = inner.get("action").ok_or_else(|| {
        V3BuildError::UnresolvedPlaceholder(format!("body.{domain}.action missing"))
    })?;
    let action = action_val
        .as_str()
        .ok_or_else(|| {
            V3BuildError::UnresolvedPlaceholder(format!("body.{domain}.action not a string"))
        })?
        .to_owned();
    let payload = inner
        .get(&action)
        .and_then(JsonValue::as_object)
        .ok_or_else(|| {
            V3BuildError::UnresolvedPlaceholder(format!(
                "body.{domain}.{action} payload missing or not an object"
            ))
        })?;

    let mut out = JsonMap::new();
    out.insert("domain".into(), JsonValue::String(domain));
    out.insert("action".into(), JsonValue::String(action));
    for (k, v) in payload {
        out.insert(k.clone(), v.clone());
    }
    Ok(out)
}

/// Pull the `domain` + `action` discriminator strings from a flat body —
/// used to look up the live_input default catalog. Either may be absent for
/// cross-cutting variants (`multicall`, `unknown`).
fn extract_tags(flat: &JsonMap<String, JsonValue>) -> (Option<String>, Option<String>) {
    let domain = flat
        .get("domain")
        .and_then(JsonValue::as_str)
        .map(str::to_owned);
    let action = flat
        .get("action")
        .and_then(JsonValue::as_str)
        .map(str::to_owned);
    (domain, action)
}

/// Wrap a placeholder-substituted `source` descriptor into the full
/// [`LiveField`](policy_state::LiveField) JSON shape with a default value
/// drawn from [`live_input_default`] and `synced_at = ctx.submitted_at`.
///
/// If the manifest entry carries `ttl_s` (the v3 convention) the value is
/// converted into a `ttl` JSON number — [`policy_state::primitives::Duration`]
/// is `#[serde(transparent)]` over a `u64`.
fn wrap_live_field(
    ctx: &V3MapContext<'_>,
    domain: Option<&str>,
    action: Option<&str>,
    field_name: &str,
    src_payload: &JsonValue,
    default_override: Option<&JsonValue>,
) -> JsonValue {
    let mut out = JsonMap::new();
    out.insert(
        "value".into(),
        live_input_default_with_override(domain, action, field_name, default_override),
    );
    if let Some(obj) = src_payload.as_object() {
        if let Some(src) = obj.get("source") {
            out.insert("source".into(), src.clone());
        }
        if let Some(ttl_s) = obj.get("ttl_s") {
            out.insert("ttl".into(), ttl_s.clone());
        }
    } else {
        // Manifest declared the live_input as a bare value — interpret it as
        // the source itself for backwards compatibility.
        out.insert("source".into(), src_payload.clone());
    }
    out.insert(
        "synced_at".into(),
        JsonValue::Number(ctx.submitted_at.as_unix().into()),
    );
    JsonValue::Object(out)
}

fn live_input_default_with_override(
    domain: Option<&str>,
    action: Option<&str>,
    field_name: &str,
    default_override: Option<&JsonValue>,
) -> JsonValue {
    let Some(value) = default_override else {
        return live_input_default(domain, action, field_name);
    };
    match (domain, action, field_name) {
        (
            Some("token"),
            Some("permit2_sign_allowance" | "permit2_sign_transfer" | "permit2_transfer_from"),
            "nonce",
        ) => permit2_nonce_tuple_default(value)
            .unwrap_or_else(|| live_input_default(domain, action, field_name)),
        _ => value.clone(),
    }
}

fn permit2_nonce_tuple_default(value: &JsonValue) -> Option<JsonValue> {
    let nonce = u256_from_json(value)?;
    let word = nonce / U256::from(256u64);
    let bit = nonce.to_be_bytes::<32>()[31];
    Some(serde_json::json!([word.to_string(), bit]))
}

fn u256_from_json(value: &JsonValue) -> Option<U256> {
    match value {
        JsonValue::String(s) => U256::from_str_radix(s, 10).ok(),
        JsonValue::Number(n) => U256::from_str_radix(&n.to_string(), 10).ok(),
        _ => None,
    }
}

/// Deserializable zero skeleton for `policy_transition::action::lending::ReserveState`.
///
/// `U256` fields (`total_supply` / `total_borrow`) are decimal strings (alloy
/// serde), the `*_bp` / `utilization_bp` / `reserve_factor_bp` fields are
/// `u32` numbers, and the optional `supply_cap` / `borrow_cap` are omitted
/// (their `#[serde(default, skip_serializing_if = "Option::is_none")]` makes
/// absence == `None`).
fn lending_reserve_state_skeleton() -> JsonValue {
    serde_json::json!({
        "total_supply": "0",
        "total_borrow": "0",
        "utilization_bp": 0,
        "ltv_bp": 0,
        "liquidation_threshold_bp": 0,
        "liquidation_bonus_bp": 0,
        "reserve_factor_bp": 0,
        "is_frozen": false,
        "is_paused": false,
    })
}

/// Deserializable zero skeleton for `lending::UserLendingState`.
///
/// `health_factor` is a `Decimal` (`#[serde(transparent)]` over `String`) so it
/// is the string `"0"`; the three USD aggregates (`total_collat_usd` /
/// `total_debt_usd` / `available_borrow_usd`) are `U256` decimal strings.
fn lending_user_state_skeleton() -> JsonValue {
    serde_json::json!({
        "health_factor": "0",
        "total_collat_usd": "0",
        "total_debt_usd": "0",
        "available_borrow_usd": "0",
    })
}

/// Deserializable zero skeleton for `lending::set_emode::EModeConfig`.
///
/// The three `*_bp` fields are `u32` numbers; `price_source` (`Option<Address>`)
/// and `category` (`Option<EModeCategory>`) are omitted (both skip-if-none);
/// `assets_in_category` is an empty `Vec<TokenRef>`.
fn lending_emode_config_skeleton() -> JsonValue {
    serde_json::json!({
        "ltv_bp": 0,
        "liquidation_threshold_bp": 0,
        "liquidation_bonus_bp": 0,
        "assets_in_category": [],
    })
}

/// Per-`(domain, action, field)` default `value` for the `LiveField` wrap.
///
/// Each entry encodes the minimal `serde_json` shape needed for
/// `policy_state::LiveField<T>` to deserialize successfully: typically `"0"`
/// for `U256` / `Decimal` / `Price`, `0` for `u32`, `false` for `bool`,
/// `["0","0"]` for 2-tuples, and object skeletons for richer state types
/// (`SwapRoute`, `PoolState`, `Vec<(TokenRef, U256)>`, `ReserveState`,
/// `UserLendingState`, `EModeConfig`).
///
/// Extending the catalog when a new `live_inputs.<field>` lands in registry
/// V2 is a one-line edit — the test suite covers what's currently emitted by
/// the v3 manifests. (Filling these with REAL fetched values is a separate
/// orchestrator task — here we only need a deserializable zero so the typed
/// `ActionBody` decode succeeds.)
fn live_input_default(domain: Option<&str>, action: Option<&str>, field: &str) -> JsonValue {
    match (domain, action, field) {
        // -------- AMM --------
        (Some("amm"), Some("swap"), "route") => {
            serde_json::json!({ "paths": [] })
        }
        (Some("amm"), Some("swap"), "expected_amount_out") => JsonValue::String("0".into()),
        (Some("amm"), Some("swap"), "price_impact_bp") => JsonValue::Number(0.into()),
        (Some("amm"), Some("swap"), "gas_estimate") => JsonValue::String("0".into()),
        (Some("amm"), Some("add_liquidity"), "pool_state")
        | (Some("amm"), Some("remove_liquidity"), "pool_state") => {
            // `PoolState::XyConstant { reserve_in, reserve_out, fee_bp }` —
            // the simplest variant, matches the V2 manifests' uniswap_v2 pool.
            serde_json::json!({
                "kind": "xy_constant",
                "reserve_in": "0",
                "reserve_out": "0",
                "fee_bp": 0,
            })
        }
        (Some("amm"), Some("add_liquidity"), "current_price") => JsonValue::String("0".into()),
        (Some("amm"), Some("remove_liquidity"), "fees_owed") => JsonValue::Array(vec![]),
        // -------- AMM CollectFees --------
        (Some("amm"), Some("collect_fees"), "fees_owed") => JsonValue::Array(vec![]),
        // -------- AMM Intent --------
        (Some("amm"), Some("sign_intent_order"), "expected_fill_price") => {
            JsonValue::String("0".into())
        }
        (Some("amm"), Some("sign_intent_order"), "competing_orders") => JsonValue::Number(0.into()),
        // -------- Airdrop (Claim) --------
        //
        // `ClaimAirdropLiveInputs` (action/airdrop/claim.rs): `is_still_claimable`
        // (`LiveField<bool>`), `actual_amount` (`LiveField<U256>`), `claim_token`
        // (`LiveField<TokenRef>`), `claim_window` (`LiveField<Option<(Time,Time)>>`).
        // Without a default `value` each `LiveField` serialises `null`; the three
        // non-Option fields reject `null` (bool / U256 / TokenRef) →
        // `build_action_body_failed`. `claim_window` is an `Option` so it takes the
        // `_ => Null` fallback below (= `None`). `claim_token` needs a concrete
        // `TokenRef { key }` skeleton — a zero ERC20 stands in until the Sync
        // orchestrator fills the real ZRO token (the real claim token is always
        // ZRO). The chain in the skeleton is a neutral placeholder.
        (Some("airdrop"), Some("claim"), "is_still_claimable") => JsonValue::Bool(false),
        (Some("airdrop"), Some("claim"), "actual_amount") => JsonValue::String("0".into()),
        (Some("airdrop"), Some("claim"), "claim_token") => serde_json::json!({
            "key": {
                "standard": "erc20",
                "chain": "eip155:1",
                "address": "0x0000000000000000000000000000000000000000"
            }
        }),
        // -------- Token --------
        (Some("token"), Some("erc20_permit"), "nonce")
        | (Some("token"), Some("permit2_approve"), "nonce") => JsonValue::String("0".into()),
        (
            Some("token"),
            Some("permit2_sign_allowance" | "permit2_sign_transfer" | "permit2_transfer_from"),
            "nonce",
        ) => {
            // `LiveField<(U256, u8)>` — JSON encodes a 2-tuple as a 2-element
            // array. Default: bitmap word 0, bit 0.
            serde_json::json!(["0", 0])
        }
        // -------- Lending (Aave V3 family) --------
        //
        // Every lending action wraps its on-chain / derived reads in
        // `LiveField<T>`; without a default `value` here each `LiveField`
        // serialises `null`, and the richer `T`s (`ReserveState`,
        // `UserLendingState`, `EModeConfig`, the 2-tuples) reject `null` →
        // `build_action_body_failed`. The `action` keys below are the serde
        // `LendingAction` tags (snake_case), NOT the source file names — note
        // `set_e_mode` (SetEMode), `enable_collateral` / `disable_collateral`
        // (both SetCollateralAction).
        //
        // Supply (SupplyLiveInputs).
        (Some("lending"), Some("supply"), "reserve_state") => lending_reserve_state_skeleton(),
        (Some("lending"), Some("supply"), "supply_apy") => JsonValue::String("0".into()),
        (Some("lending"), Some("supply"), "a_token_price_usd") => JsonValue::String("0".into()),
        (Some("lending"), Some("supply"), "eligible_as_collat") => JsonValue::Bool(false),
        (Some("lending"), Some("supply"), "user_state_before") => lending_user_state_skeleton(),
        // Borrow (BorrowLiveInputs).
        (Some("lending"), Some("borrow"), "reserve_state") => lending_reserve_state_skeleton(),
        (Some("lending"), Some("borrow"), "user_state_before") => lending_user_state_skeleton(),
        (Some("lending"), Some("borrow"), "asset_price_usd") => JsonValue::String("0".into()),
        (Some("lending"), Some("borrow"), "current_borrow_rate") => JsonValue::String("0".into()),
        (Some("lending"), Some("borrow"), "available_liquidity") => JsonValue::String("0".into()),
        // Withdraw (WithdrawLiveInputs).
        (Some("lending"), Some("withdraw"), "reserve_state") => lending_reserve_state_skeleton(),
        (Some("lending"), Some("withdraw"), "available_to_withdraw") => {
            JsonValue::String("0".into())
        }
        (Some("lending"), Some("withdraw"), "user_state_before") => lending_user_state_skeleton(),
        // Repay (RepayLiveInputs).
        (Some("lending"), Some("repay"), "reserve_state") => lending_reserve_state_skeleton(),
        (Some("lending"), Some("repay"), "current_debt") => JsonValue::String("0".into()),
        (Some("lending"), Some("repay"), "user_state_before") => lending_user_state_skeleton(),
        // Liquidate (LiquidateLiveInputs). `liquidation_bonus` is a `u32`.
        (Some("lending"), Some("liquidate"), "victim_state") => lending_user_state_skeleton(),
        (Some("lending"), Some("liquidate"), "liquidation_bonus") => JsonValue::Number(0.into()),
        (Some("lending"), Some("liquidate"), "debt_asset_price") => JsonValue::String("0".into()),
        (Some("lending"), Some("liquidate"), "collat_asset_price") => JsonValue::String("0".into()),
        // SetEMode (SetEModeLiveInputs) — serde tag `set_e_mode`.
        (Some("lending"), Some("set_e_mode"), "category_config") => lending_emode_config_skeleton(),
        (Some("lending"), Some("set_e_mode"), "user_state_before") => lending_user_state_skeleton(),
        // SwapRateMode (SwapRateModeLiveInputs) — `(U256,U256)` / `(Decimal,Decimal)`
        // both encode as 2-element JSON arrays of decimal strings.
        (Some("lending"), Some("swap_rate_mode"), "current_debts") => serde_json::json!(["0", "0"]),
        (Some("lending"), Some("swap_rate_mode"), "rates") => serde_json::json!(["0", "0"]),
        // SetCollateral (SetCollateralLiveInputs) — used by BOTH the
        // `enable_collateral` and `disable_collateral` tags.
        (Some("lending"), Some("enable_collateral"), "reserve_state")
        | (Some("lending"), Some("disable_collateral"), "reserve_state") => {
            lending_reserve_state_skeleton()
        }
        (Some("lending"), Some("enable_collateral"), "user_state_before")
        | (Some("lending"), Some("disable_collateral"), "user_state_before") => {
            lending_user_state_skeleton()
        }
        // -------- Liquid Staking (Lido) --------
        //
        // Single-`uint256` exchange-rate views. Each `LiveField<U256>` rejects
        // `null` (the `U256` deserialiser); the host fills the real value at
        // sync time, so the skeleton is a `"0"` placeholder.
        (Some("liquid_staking"), Some("wrap"), "expected_wsteth") => JsonValue::String("0".into()),
        (Some("liquid_staking"), Some("unwrap"), "expected_steth") => JsonValue::String("0".into()),
        (Some("liquid_staking"), Some("transfer_shares"), "pooled_eth") => {
            JsonValue::String("0".into())
        }
        // -------- Yield (Pendle market enrichment) --------
        //
        // The four market-based actions carry `MarketTokensLiveInputs`: SY/PT/YT
        // from `IPMarket.readTokens()` and maturity from `IPMarket.expiry()`,
        // sourced from the `$args.market` address. Each `LiveField<Address>` /
        // `LiveField<U256>` rejects a `null` value, so the skeleton is a
        // zero-address / `"0"` until the host fills the real instruments at sync.
        (
            Some("yield"),
            Some("pt_swap" | "yt_swap" | "add_market_liquidity" | "remove_market_liquidity"),
            "sy" | "pt" | "yt",
        ) => JsonValue::String("0x0000000000000000000000000000000000000000".into()),
        (
            Some("yield"),
            Some("pt_swap" | "yt_swap" | "add_market_liquidity" | "remove_market_liquidity"),
            "maturity",
        ) => JsonValue::String("0".into()),
        // Fallback — null lets the per-field type's `Option<T>` (if any) take
        // over; for stricter types serde reports a clear error pointing at the
        // missing catalog entry.
        _ => JsonValue::Null,
    }
}

// ===========================================================================
// build_multicall_from_opcode_stream — UR opcode dispatch
// ===========================================================================

/// Translate a parsed UR opcode stream into an
/// `ActionBody::Multicall { actions: [...] }`.
///
/// Each `commands[i]` byte is masked with `mask` to obtain the opcode id
/// (`allow_revert_bit` is recorded as audit metadata only — it does not gate
/// inclusion). The corresponding `per_opcode_body["0x<hh>"]` entry's `body`
/// is fed to [`build_action_body`] with a fresh [`V3MapContext`] whose
/// `inputs` field is set to `decoded_inputs_array[i]`.
///
/// # Errors
///
/// Propagates [`V3BuildError`] from inner [`build_action_body`] calls plus
/// [`V3BuildError::UnknownOpcode`] when [`UnknownOpcodePolicy::Deny`] hits an
/// opcode missing from `per_opcode_body`.
pub fn build_multicall_from_opcode_stream(
    ctx: &V3MapContext<'_>,
    per_opcode_body: &JsonMap<String, JsonValue>,
    decoded_commands: &[u8],
    decoded_inputs_array: &[JsonValue],
    mask: u8,
    allow_revert_bit: u8,
    unknown_policy: UnknownOpcodePolicy,
) -> Result<ActionBody, V3BuildError> {
    let mut actions = Vec::with_capacity(decoded_commands.len());

    for (i, raw_byte) in decoded_commands.iter().enumerate() {
        let opcode = raw_byte & mask;
        let _allow_revert = (raw_byte & allow_revert_bit) != 0;

        let opcode_key = format!("0x{opcode:02x}");
        let Some(opcode_entry) = per_opcode_body.get(&opcode_key) else {
            match unknown_policy {
                UnknownOpcodePolicy::Deny => {
                    return Err(V3BuildError::UnknownOpcode {
                        opcode,
                        policy: unknown_policy,
                    });
                }
                UnknownOpcodePolicy::Warn => {
                    eprintln!("[action_builder] warn: unknown opcode 0x{opcode:02x} at index {i}");
                    continue;
                }
                UnknownOpcodePolicy::Skip => continue,
            }
        };

        let body_template = opcode_entry.get("body").ok_or_else(|| {
            V3BuildError::UnresolvedPlaceholder(format!("{opcode_key}.body missing"))
        })?;

        let inputs_for_this = decoded_inputs_array.get(i);
        let child_ctx = V3MapContext {
            chain: ctx.chain.clone(),
            tx_to: ctx.tx_to,
            tx_from: ctx.tx_from,
            value: ctx.value,
            submitted_at: ctx.submitted_at,
            args_json: ctx.args_json,
            raw_calldata: ctx.raw_calldata,
            resolved: ctx.resolved.clone(),
            derived: ctx.derived.clone(),
            inputs: inputs_for_this,
        };

        let child_action = build_action_body(&child_ctx, body_template, None)?;
        actions.push(child_action);
    }

    Ok(ActionBody::Multicall { actions })
}

// ===========================================================================
// build_array_emit — homogeneous array → Multicall
// ===========================================================================

/// Translate a homogeneous args/message array into `ActionBody::Multicall`.
///
/// `array_source` is a `$args.<path>` / `$inputs.<path>` placeholder that MUST
/// resolve to a JSON array. Each element becomes the `inputs` of a fresh
/// child [`V3MapContext`], and `per_item_body` is built against it — so the
/// per-item template references the element via `$inputs.<field>`.
/// If `parallel_sources` is present, each named placeholder must resolve to an
/// array of the same length. The per-item `$inputs` becomes an object:
/// `{ "element": <array_source[i]>, "<name>": <parallel_sources[name][i]> }`.
/// This models ABI shapes such as Permit2 batch signature transfer where
/// `permit.permitted[]` and `transferDetails[]` advance in lock-step.
///
/// This REUSES the exact `$inputs` mechanism
/// [`build_multicall_from_opcode_stream`] uses: per iteration we clone the
/// parent context and set `inputs: Some(element)`. No new placeholder root.
///
/// An empty array yields `ActionBody::Multicall { actions: [] }` (valid, no
/// error). `array_source` resolving to a non-array surfaces
/// [`V3BuildError::ArraySourceNotArray`].
///
/// # Errors
///
/// * [`V3BuildError::ArraySourceNotArray`] — `array_source` did not resolve to
///   a JSON array.
/// * Any placeholder / JSONPath / serde error propagated from
///   [`resolve_placeholder`] or the inner [`build_action_body`] calls.
pub fn build_array_emit(
    ctx: &V3MapContext<'_>,
    array_source: &str,
    parallel_sources: Option<&JsonValue>,
    per_item_body: &JsonValue,
    per_item_live_inputs: Option<&JsonValue>,
) -> Result<ActionBody, V3BuildError> {
    // `resolve_placeholder` returns an OWNED value; bind it locally so the
    // `&element` borrows below live through the whole loop.
    let array_val = resolve_placeholder(ctx, array_source)?;
    let arr = array_val
        .as_array()
        .ok_or_else(|| V3BuildError::ArraySourceNotArray(array_source.to_owned()))?;

    let mut parallels: Vec<(String, Vec<JsonValue>)> = Vec::new();
    if let Some(parallel_sources) = parallel_sources {
        let sources = parallel_sources.as_object().ok_or_else(|| {
            V3BuildError::UnresolvedPlaceholder(
                "array_emit.parallel_sources must be an object".into(),
            )
        })?;
        for (name, source_value) in sources {
            let source = source_value.as_str().ok_or_else(|| {
                V3BuildError::UnresolvedPlaceholder(format!(
                    "array_emit.parallel_sources.{name} must be a placeholder string"
                ))
            })?;
            let parallel_val = resolve_placeholder(ctx, source)?;
            let parallel_arr =
                parallel_val
                    .as_array()
                    .ok_or_else(|| V3BuildError::ParallelSourceNotArray {
                        name: name.clone(),
                        placeholder: source.to_owned(),
                    })?;
            if parallel_arr.len() != arr.len() {
                return Err(V3BuildError::ParallelSourceLengthMismatch {
                    name: name.clone(),
                    array_len: arr.len(),
                    parallel_len: parallel_arr.len(),
                });
            }
            parallels.push((name.clone(), parallel_arr.clone()));
        }
    }

    let mut actions = Vec::with_capacity(arr.len());
    for (index, element) in arr.iter().enumerate() {
        let child_inputs;
        let inputs = if parallels.is_empty() {
            element
        } else {
            let mut object = JsonMap::new();
            object.insert("element".into(), element.clone());
            for (name, values) in &parallels {
                object.insert(name.clone(), values[index].clone());
            }
            child_inputs = JsonValue::Object(object);
            &child_inputs
        };
        let child_ctx = V3MapContext {
            chain: ctx.chain.clone(),
            tx_to: ctx.tx_to,
            tx_from: ctx.tx_from,
            value: ctx.value,
            submitted_at: ctx.submitted_at,
            args_json: ctx.args_json,
            raw_calldata: ctx.raw_calldata,
            resolved: ctx.resolved.clone(),
            derived: ctx.derived.clone(),
            inputs: Some(inputs),
        };
        actions.push(build_action_body(
            &child_ctx,
            per_item_body,
            per_item_live_inputs,
        )?);
    }

    Ok(ActionBody::Multicall { actions })
}

// ===========================================================================
// Inline unit tests (11) — see `## 11 inline unit test` in the M1 plan.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use policy_transition::action::{amm::AmmAction, token::TokenAction};
    use serde_json::json;
    use std::str::FromStr;

    fn addr(hex: &str) -> Address {
        Address::from_str(hex).unwrap()
    }

    fn mk_ctx<'a>(args: &'a JsonValue) -> V3MapContext<'a> {
        V3MapContext {
            chain: ChainId::ethereum_mainnet(),
            tx_to: addr("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
            tx_from: addr("0x000000000000000000000000000000000000a01c"),
            value: U256::ZERO,
            submitted_at: Time::from_unix(1_738_000_000),
            args_json: args,
            raw_calldata: "0xdeadbeef",
            resolved: BTreeMap::new(),
            derived: BTreeMap::new(),
            inputs: None,
        }
    }

    // 1. ERC20 approve → ActionBody::Token(Erc20Approve)
    #[test]
    fn t1_erc20_approve() {
        let args = json!({
            "spender": "0x00000000000000000000000000000000deadbeef",
            "amount": "1000000",
        });
        let ctx = mk_ctx(&args);
        let body = json!({
            "domain": "token",
            "token": {
                "action": "erc20_approve",
                "erc20_approve": {
                    "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$to" } },
                    "spender": "$args.spender",
                    "amount": "$args.amount"
                }
            }
        });
        let action = build_action_body(&ctx, &body, None).unwrap();
        match action {
            ActionBody::Token(TokenAction::Erc20Approve(a)) => {
                assert_eq!(a.amount, U256::from(1_000_000u64));
                assert_eq!(
                    a.spender,
                    addr("0x00000000000000000000000000000000deadbeef")
                );
            }
            other => panic!("expected Erc20Approve, got {other:?}"),
        }
    }

    // 2. ERC20 transfer → ActionBody::Token(Erc20Transfer)
    #[test]
    fn t2_erc20_transfer() {
        let args = json!({
            "to": "0x00000000000000000000000000000000deadbeef",
            "amount": "12345",
        });
        let ctx = mk_ctx(&args);
        let body = json!({
            "domain": "token",
            "token": {
                "action": "erc20_transfer",
                "erc20_transfer": {
                    "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$to" } },
                    "recipient": "$args.to",
                    "amount": "$args.amount"
                }
            }
        });
        let action = build_action_body(&ctx, &body, None).unwrap();
        assert!(matches!(
            action,
            ActionBody::Token(TokenAction::Erc20Transfer(_))
        ));
    }

    // 3. ERC721 approve → ActionBody::Token(NftApprove)
    #[test]
    fn t3_erc721_approve() {
        let args = json!({
            "to": "0x00000000000000000000000000000000deadbeef",
            "tokenId": "42",
        });
        let ctx = mk_ctx(&args);
        let body = json!({
            "domain": "token",
            "token": {
                "action": "nft_approve",
                "nft_approve": {
                    "nft_key": {
                        "standard": "erc721",
                        "chain": "$chain",
                        "contract": "$to",
                        "token_id": "$args.tokenId"
                    },
                    "spender": "$args.to"
                }
            }
        });
        let action = build_action_body(&ctx, &body, None).unwrap();
        assert!(matches!(
            action,
            ActionBody::Token(TokenAction::NftApprove(_))
        ));
    }

    // 4. ERC721 setApprovalForAll → ActionBody::Token(NftSetApprovalForAll)
    #[test]
    fn t4_erc721_set_approval_for_all() {
        let args = json!({
            "operator": "0x00000000000000000000000000000000deadbeef",
            "approved": true,
        });
        let ctx = mk_ctx(&args);
        let body = json!({
            "domain": "token",
            "token": {
                "action": "nft_set_approval_for_all",
                "nft_set_approval_for_all": {
                    "chain": "$chain",
                    "contract": "$to",
                    "spender": "$args.operator",
                    "approved": "$args.approved"
                }
            }
        });
        let action = build_action_body(&ctx, &body, None).unwrap();
        match action {
            ActionBody::Token(TokenAction::NftSetApprovalForAll(a)) => {
                assert!(a.approved);
            }
            other => panic!("expected NftSetApprovalForAll, got {other:?}"),
        }
    }

    // 5. ERC721 safeTransferFrom → ActionBody::Token(NftTransfer)
    #[test]
    fn t5_erc721_safe_transfer_from() {
        let args = json!({
            "from": "0x000000000000000000000000000000000000a01c",
            "to": "0x00000000000000000000000000000000deadbeef",
            "tokenId": "7",
        });
        let ctx = mk_ctx(&args);
        let body = json!({
            "domain": "token",
            "token": {
                "action": "nft_transfer",
                "nft_transfer": {
                    "nft_key": {
                        "standard": "erc721",
                        "chain": "$chain",
                        "contract": "$to",
                        "token_id": "$args.tokenId"
                    },
                    "recipient": "$args.to",
                    "amount": null
                }
            }
        });
        let action = build_action_body(&ctx, &body, None).unwrap();
        match action {
            ActionBody::Token(TokenAction::NftTransfer(a)) => {
                assert_eq!(a.amount, None);
            }
            other => panic!("expected NftTransfer, got {other:?}"),
        }
    }

    // 6. Permit2 approve (onchain) → ActionBody::Token(Permit2Approve)
    //
    // `expires_at` deserializes into a `Time` (transparent over `u64`).
    // The builder accepts ABI-decoded decimal strings for time-like fields and
    // normalizes them before the final typed decode.
    #[test]
    fn t6_permit2_approve() {
        let args = json!({
            "token": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "spender": "0x00000000000000000000000000000000deadbeef",
            "amount": "999",
            "expiration": 1_738_001_800u64,
        });
        let ctx = mk_ctx(&args);
        let body = json!({
            "domain": "token",
            "token": {
                "action": "permit2_approve",
                "permit2_approve": {
                    "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.token" } },
                    "spender": "$args.spender",
                    "amount": "$args.amount",
                    "expires_at": "$args.expiration"
                }
            }
        });
        let action = build_action_body(&ctx, &body, None).unwrap();
        assert!(matches!(
            action,
            ActionBody::Token(TokenAction::Permit2Approve(_))
        ));
    }

    #[test]
    fn t6b_erc20_permit_onchain_deadline_string_and_live_nonce() {
        let args = json!({
            "owner": "0x000000000000000000000000000000000000a01c",
            "spender": "0x00000000000000000000000000000000deadbeef",
            "value": "999",
            "deadline": "115792089237316195423570985008687907853269984665640564039457584007913129639935",
        });
        let ctx = mk_ctx(&args);
        let body = json!({
            "domain": "token",
            "token": {
                "action": "erc20_permit",
                "erc20_permit": {
                    "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$tx.to" } },
                    "spender": "$args.spender",
                    "amount": "$args.value",
                    "deadline": "$args.deadline"
                }
            }
        });
        let live_inputs = json!({
            "nonce": {
                "source": {
                    "kind": "onchain_view",
                    "chain": "$chain",
                    "contract": "$tx.to",
                    "function": "nonces(address)",
                    "decoder_id": "erc20_permit_nonce"
                },
                "ttl_s": 12
            }
        });
        let action = build_action_body(&ctx, &body, Some(&live_inputs)).unwrap();
        match action {
            ActionBody::Token(TokenAction::Erc20Permit(a)) => {
                assert_eq!(a.deadline.as_unix(), u64::MAX);
                assert_eq!(a.nonce.value, U256::ZERO);
            }
            other => panic!("expected Erc20Permit, got {other:?}"),
        }
    }

    // 7. Permit2 PermitSingle → ActionBody::Token(Permit2SignAllowance)
    //
    // Also covers the live_inputs.nonce wrap: the manifest emits
    // `{ "source": {...}, "ttl_s": 12 }`, the builder must wrap it into the
    // `LiveField<(U256, u8)>` shape with `value = ["0", 0]` (default).
    #[test]
    fn t7_permit2_permit_single_with_live_inputs() {
        let args = json!({
            "permitSingle": {
                "details": {
                    "token": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                    "amount": "1000",
                    "expiration": 1_738_001_800u64,
                    "nonce": "0",
                },
                "spender": "0x00000000000000000000000000000000deadbeef",
                "sigDeadline": 1_738_002_000u64,
            }
        });
        let ctx = mk_ctx(&args);
        let body = json!({
            "domain": "token",
            "token": {
                "action": "permit2_sign_allowance",
                "permit2_sign_allowance": {
                    "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.permitSingle.details.token" } },
                    "spender": "$args.permitSingle.spender",
                    "amount": "$args.permitSingle.details.amount",
                    "expires_at": "$args.permitSingle.details.expiration",
                    "sig_deadline": "$args.permitSingle.sigDeadline"
                }
            }
        });
        let live_inputs = json!({
            "nonce": {
                "source": {
                    "kind": "onchain_view",
                    "chain": "$chain",
                    "contract": "0x000000000022d473030f116ddee9f6b43ac78ba3",
                    "function": "nonceBitmap(address,uint256)",
                    "decoder_id": "permit2_nonce_bitmap"
                },
                "ttl_s": 12
            }
        });
        let action = build_action_body(&ctx, &body, Some(&live_inputs)).unwrap();
        match action {
            ActionBody::Token(TokenAction::Permit2SignAllowance(a)) => {
                // Default value applied — narrow scope: bitmap word 0, bit 0.
                assert_eq!(a.nonce.value.0, U256::ZERO);
                assert_eq!(a.nonce.value.1, 0u8);
            }
            other => panic!("expected Permit2SignAllowance, got {other:?}"),
        }
    }

    // 8. V2 swapExactTokensForTokens → ActionBody::Amm(Swap)
    #[test]
    fn t8_v2_swap_exact_tokens_for_tokens() {
        let args = json!({
            "amountIn": "1000000000",
            "amountOutMin": "1",
            "path": [
                "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
            ],
            "to": "0x000000000000000000000000000000000000a01c"
        });
        let mut ctx = mk_ctx(&args);
        ctx.resolved.insert(
            "pool".into(),
            json!("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"),
        );
        ctx.resolved.insert(
            "factory".into(),
            json!("0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f"),
        );
        ctx.derived
            .insert("slippage_bp".into(), JsonValue::Number(50.into()));
        let body = json!({
            "domain": "amm",
            "amm": {
                "action": "swap",
                "swap": {
                    "venue": {
                        "name": "uniswap_v2",
                        "chain": "$chain",
                        "pool": "$resolved.pool",
                        "factory": "$resolved.factory"
                    },
                    "params": {
                        "token_in":  { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.path[0]" } },
                        "token_out": { "key": { "standard": "erc20", "chain": "$chain", "address": "$args.path[-1]" } },
                        "direction": {
                            "kind": "exact_input",
                            "amount_in": "$args.amountIn",
                            "min_amount_out": "$args.amountOutMin"
                        },
                        "recipient": "$args.to",
                        "slippage_bp": "$derived.slippage_bp"
                    }
                }
            }
        });
        let live_inputs = json!({
            "route":               { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getReserves()", "decoder_id": "uniswap_v2_get_reserves" }, "ttl_s": 12 },
            "expected_amount_out": { "source": { "kind": "derived_from", "inputs": [{ "scope": "global", "name": "x" }], "calc_id": "y" }, "ttl_s": 12 },
            "price_impact_bp":     { "source": { "kind": "derived_from", "inputs": [{ "scope": "global", "name": "x" }], "calc_id": "y" }, "ttl_s": 12 },
            "gas_estimate":        { "source": { "kind": "oracle_feed",  "provider": "pyth", "feed_id": "gas/ethereum" }, "ttl_s": 6 }
        });
        let action = build_action_body(&ctx, &body, Some(&live_inputs)).unwrap();
        match action {
            ActionBody::Amm(AmmAction::Swap(s)) => {
                // Defaults applied to live_inputs values.
                assert_eq!(s.live_inputs.expected_amount_out.value, U256::ZERO);
                assert_eq!(s.live_inputs.price_impact_bp.value, 0);
            }
            other => panic!("expected Amm(Swap), got {other:?}"),
        }
    }

    // 9. V2 addLiquidity → ActionBody::Amm(AddLiquidity)
    #[test]
    fn t9_v2_add_liquidity() {
        let args = json!({
            "tokenA": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "tokenB": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
            "amountADesired": "1000",
            "amountBDesired": "2000",
            "amountAMin": "990",
            "amountBMin": "1980",
            "to": "0x000000000000000000000000000000000000a01c",
            "deadline": "1738002000",
        });
        let mut ctx = mk_ctx(&args);
        ctx.resolved.insert(
            "pool".into(),
            json!("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"),
        );
        ctx.resolved.insert(
            "factory".into(),
            json!("0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f"),
        );
        ctx.derived
            .insert("min_lp_out".into(), JsonValue::String("0".into()));
        let body = json!({
            "domain": "amm",
            "amm": {
                "action": "add_liquidity",
                "add_liquidity": {
                    "venue": {
                        "name": "uniswap_v2",
                        "chain": "$chain",
                        "pool": "$resolved.pool",
                        "factory": "$resolved.factory"
                    },
                    "params": {
                        "kind": "pooled",
                        "tokens": [
                            [{ "key": { "standard": "erc20", "chain": "$chain", "address": "$args.tokenA" } }, "$args.amountADesired"],
                            [{ "key": { "standard": "erc20", "chain": "$chain", "address": "$args.tokenB" } }, "$args.amountBDesired"]
                        ],
                        "min_lp_out": "$derived.min_lp_out",
                        "recipient":  "$args.to"
                    }
                }
            }
        });
        let live_inputs = json!({
            "pool_state":    { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getReserves()", "decoder_id": "uniswap_v2_get_reserves" }, "ttl_s": 12 },
            "current_price": { "source": { "kind": "derived_from", "inputs": [{ "scope": "global", "name": "x" }], "calc_id": "y" }, "ttl_s": 12 }
        });
        let action = build_action_body(&ctx, &body, Some(&live_inputs)).unwrap();
        assert!(matches!(
            action,
            ActionBody::Amm(AmmAction::AddLiquidity(_))
        ));
    }

    // 10. V2 removeLiquidity → ActionBody::Amm(RemoveLiquidity)
    #[test]
    fn t10_v2_remove_liquidity() {
        let args = json!({
            "tokenA": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "tokenB": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
            "liquidity": "1234567",
            "amountAMin": "100",
            "amountBMin": "200",
            "to": "0x000000000000000000000000000000000000a01c",
            "deadline": "1738002000",
        });
        let mut ctx = mk_ctx(&args);
        ctx.resolved.insert(
            "pool".into(),
            json!("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"),
        );
        ctx.resolved.insert(
            "factory".into(),
            json!("0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f"),
        );
        let body = json!({
            "domain": "amm",
            "amm": {
                "action": "remove_liquidity",
                "remove_liquidity": {
                    "venue": {
                        "name": "uniswap_v2",
                        "chain": "$chain",
                        "pool": "$resolved.pool",
                        "factory": "$resolved.factory"
                    },
                    "params": {
                        "kind": "pooled_burn",
                        "lp_token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$resolved.pool" } },
                        "lp_amount": "$args.liquidity",
                        "min_out": [
                            [{ "key": { "standard": "erc20", "chain": "$chain", "address": "$args.tokenA" } }, "$args.amountAMin"],
                            [{ "key": { "standard": "erc20", "chain": "$chain", "address": "$args.tokenB" } }, "$args.amountBMin"]
                        ],
                        "recipient": "$args.to"
                    }
                }
            }
        });
        let live_inputs = json!({
            "pool_state": { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "getReserves()", "decoder_id": "uniswap_v2_get_reserves" }, "ttl_s": 12 },
            "fees_owed":  { "source": { "kind": "derived_from", "inputs": [{ "scope": "global", "name": "x" }], "calc_id": "y" }, "ttl_s": 12 }
        });
        let action = build_action_body(&ctx, &body, Some(&live_inputs)).unwrap();
        assert!(matches!(
            action,
            ActionBody::Amm(AmmAction::RemoveLiquidity(_))
        ));
    }

    // 11. UR execute V3_SWAP_EXACT_IN single opcode →
    //     ActionBody::Multicall { actions: [Amm(Swap)] }
    #[test]
    fn t11_ur_execute_v3_swap_exact_in_single() {
        let args = json!({
            "commands": "0x00",
            "inputs": ["0x..."],
            "deadline": "1738002000",
        });
        let mut ctx = mk_ctx(&args);
        ctx.resolved.insert(
            "pool".into(),
            json!("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640"),
        );
        ctx.resolved
            .insert("fee_tier_bp".into(), JsonValue::Number(500.into()));
        ctx.derived.insert(
            "v3_path_first_token".into(),
            json!("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
        );
        ctx.derived.insert(
            "v3_path_last_token".into(),
            json!("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
        );
        ctx.derived
            .insert("slippage_bp".into(), JsonValue::Number(50.into()));

        // Decoded opcode inputs for V3_SWAP_EXACT_IN.
        let opcode_inputs = json!({
            "recipient": "0x000000000000000000000000000000000000a01c",
            "amountIn": "1000000",
            "amountOutMin": "1",
            "path": "0x",
            "payerIsUser": true,
        });
        let decoded_inputs_array = vec![opcode_inputs];

        // The opcode body inlines `live_inputs` inside the `swap` payload —
        // `build_action_body` extracts it via `strip_inline_live_inputs` and
        // injects defaults at the right key per the action's serde shape.
        let per_opcode_obj = json!({
            "0x00": {
                "name": "V3_SWAP_EXACT_IN",
                "inputs_abi": "(address recipient, uint256 amountIn, uint256 amountOutMin, bytes path, bool payerIsUser)",
                "body": {
                    "domain": "amm",
                    "amm": {
                        "action": "swap",
                        "swap": {
                            "venue": {
                                "name": "uniswap_v3",
                                "chain": "$chain",
                                "pool": "$resolved.pool",
                                "fee_tier_bp": "$resolved.fee_tier_bp"
                            },
                            "params": {
                                "token_in":  { "key": { "standard": "erc20", "chain": "$chain", "address": "$derived.v3_path_first_token" } },
                                "token_out": { "key": { "standard": "erc20", "chain": "$chain", "address": "$derived.v3_path_last_token" } },
                                "direction": {
                                    "kind": "exact_input",
                                    "amount_in": "$inputs.amountIn",
                                    "min_amount_out": "$inputs.amountOutMin"
                                },
                                "recipient": "$inputs.recipient",
                                "slippage_bp": "$derived.slippage_bp"
                            },
                            "live_inputs": {
                                "route":               { "source": { "kind": "onchain_view", "chain": "$chain", "contract": "$resolved.pool", "function": "slot0()", "decoder_id": "uniswap_v3_slot0" }, "ttl_s": 12 },
                                "expected_amount_out": { "source": { "kind": "venue_api", "endpoint": "https://api.example", "parser_id": "p" }, "ttl_s": 6 },
                                "price_impact_bp":     { "source": { "kind": "derived_from", "inputs": [{ "scope": "global", "name": "x" }], "calc_id": "y" }, "ttl_s": 12 },
                                "gas_estimate":        { "source": { "kind": "oracle_feed", "provider": "pyth", "feed_id": "gas/ethereum" }, "ttl_s": 6 }
                            }
                        }
                    }
                }
            }
        });
        let per_opcode_map = per_opcode_obj.as_object().unwrap().clone();

        let multicall = build_multicall_from_opcode_stream(
            &ctx,
            &per_opcode_map,
            &[0x00u8],
            &decoded_inputs_array,
            0x7f,
            0x80,
            UnknownOpcodePolicy::Warn,
        )
        .unwrap();

        match &multicall {
            ActionBody::Multicall { actions } => {
                assert_eq!(actions.len(), 1);
                match &actions[0] {
                    ActionBody::Amm(AmmAction::Swap(s)) => {
                        // Default applied during live_input injection.
                        assert_eq!(s.live_inputs.gas_estimate.value, U256::ZERO);
                    }
                    other => panic!("expected Amm(Swap), got {other:?}"),
                }
            }
            other => panic!("expected Multicall, got {other:?}"),
        }
    }

    // ── auxiliary tests (do not count toward the 11) ───────────────────────

    #[test]
    fn aux_unknown_opcode_deny() {
        let args = json!({});
        let ctx = mk_ctx(&args);
        let empty_per_opcode = JsonMap::new();
        let err = build_multicall_from_opcode_stream(
            &ctx,
            &empty_per_opcode,
            &[0x42u8],
            &[],
            0x7f,
            0x80,
            UnknownOpcodePolicy::Deny,
        )
        .unwrap_err();
        match err {
            V3BuildError::UnknownOpcode { opcode, policy } => {
                assert_eq!(opcode, 0x42);
                assert_eq!(policy, UnknownOpcodePolicy::Deny);
            }
            other => panic!("expected UnknownOpcode, got {other:?}"),
        }
    }

    #[test]
    fn aux_args_negative_index() {
        let args = json!({ "path": ["0xaaaa", "0xbbbb", "0xcccc"] });
        let ctx = mk_ctx(&args);
        let v = resolve_placeholder(&ctx, "$args.path[-1]").unwrap();
        assert_eq!(v, json!("0xcccc"));
    }

    // Plan §M9 — type-aware placeholder fallback (Sync orchestrator 별 plan).

    #[test]
    fn m9_resolved_address_fallback() {
        let args = json!({});
        let ctx = mk_ctx(&args);
        let v = resolve_placeholder(&ctx, "$resolved.weth").unwrap();
        assert_eq!(v, json!("0x0000000000000000000000000000000000000000"));
    }

    #[test]
    fn m9_derived_u32_fallback() {
        let args = json!({});
        let ctx = mk_ctx(&args);
        let v = resolve_placeholder(&ctx, "$derived.fee_tier_bp").unwrap();
        assert_eq!(v, json!(0u32));
    }

    #[test]
    fn m9_derived_u256_fallback() {
        let args = json!({});
        let ctx = mk_ctx(&args);
        let v = resolve_placeholder(&ctx, "$derived.v4_amount_in").unwrap();
        assert_eq!(v, json!("0"));
    }

    #[test]
    fn m9_derived_bytes32_fallback() {
        let args = json!({});
        let ctx = mk_ctx(&args);
        let v = resolve_placeholder(&ctx, "$derived.v4_pool_id").unwrap();
        assert_eq!(
            v,
            json!("0x0000000000000000000000000000000000000000000000000000000000000000")
        );
    }

    #[test]
    fn m9_unknown_placeholder_fail_loud() {
        let args = json!({});
        let ctx = mk_ctx(&args);
        let err = resolve_placeholder(&ctx, "$derived.nonexistent_field").unwrap_err();
        match err {
            V3BuildError::UnknownPlaceholder(s) => {
                assert_eq!(s, "derived.nonexistent_field");
            }
            other => panic!("expected UnknownPlaceholder, got {other:?}"),
        }
    }

    // ── Phase A.2 — build_array_emit (homogeneous array → Multicall) ────────

    // array_emit over a 2-element `$args.transfers` array. Each element binds
    // as `$inputs.<field>` for its own erc20_transfer body — element-0 and
    // element-1 differ, proving per-element context binding.
    #[test]
    fn array_emit_calldata_two_transfers_per_element_binding() {
        let args = json!({
            "transfers": [
                {
                    "token": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                    "recipient": "0x00000000000000000000000000000000deadbeef",
                    "amount": "1000"
                },
                {
                    "token": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                    "recipient": "0x00000000000000000000000000000000cafef00d",
                    "amount": "2000"
                }
            ]
        });
        let ctx = mk_ctx(&args);
        let per_item_body = json!({
            "domain": "token",
            "token": {
                "action": "erc20_transfer",
                "erc20_transfer": {
                    "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.token" } },
                    "recipient": "$inputs.recipient",
                    "amount": "$inputs.amount"
                }
            }
        });

        let action = build_array_emit(&ctx, "$args.transfers", None, &per_item_body, None).unwrap();
        match action {
            ActionBody::Multicall { actions } => {
                assert_eq!(actions.len(), 2);
                // element-0
                match &actions[0] {
                    ActionBody::Token(TokenAction::Erc20Transfer(a)) => {
                        assert_eq!(a.amount, U256::from(1000u64));
                        assert_eq!(
                            a.recipient,
                            addr("0x00000000000000000000000000000000deadbeef")
                        );
                        assert_eq!(
                            a.token.key.contract(),
                            Some(&addr("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"))
                        );
                    }
                    other => panic!("expected Erc20Transfer at 0, got {other:?}"),
                }
                // element-1 — DIFFERENT fields prove per-element binding.
                match &actions[1] {
                    ActionBody::Token(TokenAction::Erc20Transfer(a)) => {
                        assert_eq!(a.amount, U256::from(2000u64));
                        assert_eq!(
                            a.recipient,
                            addr("0x00000000000000000000000000000000cafef00d")
                        );
                        assert_eq!(
                            a.token.key.contract(),
                            Some(&addr("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"))
                        );
                    }
                    other => panic!("expected Erc20Transfer at 1, got {other:?}"),
                }
            }
            other => panic!("expected Multicall, got {other:?}"),
        }
    }

    #[test]
    fn array_emit_parallel_sources_bind_same_index() {
        let args = json!({
            "owner": "0x0000000000000000000000000000000000000a01",
            "nonce": "513",
            "deadline": "1738002000",
            "permitted": [
                ["0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "1000"],
                ["0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "2000"]
            ],
            "transferDetails": [
                ["0x00000000000000000000000000000000deadbeef", "900"],
                ["0x00000000000000000000000000000000cafef00d", "1800"]
            ]
        });
        let ctx = mk_ctx(&args);
        let per_item_body = json!({
            "domain": "token",
            "token": {
                "action": "permit2_transfer_from",
                "permit2_transfer_from": {
                    "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.element[0]" } },
                    "owner": "$args.owner",
                    "spender": "$tx.from",
                    "recipient": "$inputs.detail[0]",
                    "amount": "$inputs.detail[1]",
                    "permitted_amount": "$inputs.element[1]",
                    "nonce": "$args.nonce",
                    "sig_deadline": "$args.deadline"
                }
            }
        });
        let parallel_sources = json!({ "detail": "$args.transferDetails" });
        let live_inputs = json!({
            "nonce": {
                "source": { "kind": "user_supplied" },
                "ttl_s": 12
            }
        });
        let action = build_array_emit(
            &ctx,
            "$args.permitted",
            Some(&parallel_sources),
            &per_item_body,
            Some(&live_inputs),
        )
        .unwrap();

        let ActionBody::Multicall { actions } = action else {
            panic!("expected Multicall");
        };
        assert_eq!(actions.len(), 2);
        let ActionBody::Token(TokenAction::Permit2TransferFrom(first)) = &actions[0] else {
            panic!("expected Permit2TransferFrom at 0");
        };
        assert_eq!(first.amount, U256::from(900u64));
        assert_eq!(first.permitted_amount, U256::from(1000u64));
        assert_eq!(
            first.recipient,
            addr("0x00000000000000000000000000000000deadbeef")
        );
        assert_eq!(first.nonce.value, (U256::from(2u64), 1u8));

        let ActionBody::Token(TokenAction::Permit2TransferFrom(second)) = &actions[1] else {
            panic!("expected Permit2TransferFrom at 1");
        };
        assert_eq!(second.amount, U256::from(1800u64));
        assert_eq!(second.permitted_amount, U256::from(2000u64));
        assert_eq!(
            second.recipient,
            addr("0x00000000000000000000000000000000cafef00d")
        );
        assert_eq!(second.nonce.value, (U256::from(2u64), 1u8));
    }

    // Empty array → empty Multicall (valid, no error).
    #[test]
    fn array_emit_empty_array_empty_multicall() {
        let args = json!({ "transfers": [] });
        let ctx = mk_ctx(&args);
        let per_item_body = json!({
            "domain": "token",
            "token": {
                "action": "erc20_transfer",
                "erc20_transfer": {
                    "token": { "key": { "standard": "erc20", "chain": "$chain", "address": "$inputs.token" } },
                    "recipient": "$inputs.recipient",
                    "amount": "$inputs.amount"
                }
            }
        });
        let action = build_array_emit(&ctx, "$args.transfers", None, &per_item_body, None).unwrap();
        match action {
            ActionBody::Multicall { actions } => assert!(actions.is_empty()),
            other => panic!("expected empty Multicall, got {other:?}"),
        }
    }

    // array_source resolving to a non-array → ArraySourceNotArray.
    #[test]
    fn array_emit_non_array_source_errors() {
        let args = json!({ "transfers": "not-an-array" });
        let ctx = mk_ctx(&args);
        let per_item_body = json!({
            "domain": "token",
            "token": { "action": "erc20_transfer", "erc20_transfer": {} }
        });
        let err =
            build_array_emit(&ctx, "$args.transfers", None, &per_item_body, None).unwrap_err();
        match err {
            V3BuildError::ArraySourceNotArray(s) => assert_eq!(s, "$args.transfers"),
            other => panic!("expected ArraySourceNotArray, got {other:?}"),
        }
    }

    // ── B.2-infra — discriminant value-map ($match / $cases / $default) ──────

    // (a) Field-level value-map on a uint256 arg. uint256 args arrive as a
    // decimal STRING (Fix B coercion: width > 64 → string), so the match key
    // is `"2"` and the case lookup yields the RateMode serde value `"variable"`.
    #[test]
    fn value_map_field_number_string_match() {
        let args = json!({ "rateMode": "2" });
        let ctx = mk_ctx(&args);
        let template = json!({
            "$match": "$args.rateMode",
            "$cases": { "1": "stable", "2": "variable" }
        });
        let out = substitute_placeholders(&ctx, &template).unwrap();
        assert_eq!(out, json!("variable"));
    }

    // (b) Bool match selecting a whole object case (action-tag-level switch).
    // `useAsCollateral` is a `bool` arg → JSON `true`, key `"true"`. The chosen
    // case is an object that ITSELF contains a placeholder (`$args.asset`),
    // proving the recursive substitution of the selected case value.
    #[test]
    fn value_map_bool_object_case_recursive() {
        let args = json!({ "useAsCollateral": false, "asset": "0xabc" });
        let ctx = mk_ctx(&args);
        let template = json!({
            "$match": "$args.useAsCollateral",
            "$cases": {
                "true":  { "action": "enable_collateral",  "tag_asset": "$args.asset" },
                "false": { "action": "disable_collateral", "tag_asset": "$args.asset" }
            }
        });
        let out = substitute_placeholders(&ctx, &template).unwrap();
        assert_eq!(
            out,
            json!({ "action": "disable_collateral", "tag_asset": "0xabc" })
        );
    }

    // (c) No matching case → fall back to `$default` (also recursively
    // substituted).
    #[test]
    fn value_map_no_match_uses_default() {
        let args = json!({ "rateMode": "0" });
        let ctx = mk_ctx(&args);
        let template = json!({
            "$match": "$args.rateMode",
            "$cases": { "1": "stable", "2": "variable" },
            "$default": "fixed"
        });
        let out = substitute_placeholders(&ctx, &template).unwrap();
        assert_eq!(out, json!("fixed"));
    }

    // (d) No matching case AND no `$default` → fail-loud ValueMapNoMatch.
    #[test]
    fn value_map_no_match_no_default_errors() {
        let args = json!({ "rateMode": "0" });
        let ctx = mk_ctx(&args);
        let template = json!({
            "$match": "$args.rateMode",
            "$cases": { "1": "stable", "2": "variable" }
        });
        let err = substitute_placeholders(&ctx, &template).unwrap_err();
        match err {
            V3BuildError::ValueMapNoMatch { matched } => assert_eq!(matched, "0"),
            other => panic!("expected ValueMapNoMatch, got {other:?}"),
        }
    }

    // (e) Malformed value-map (`$cases` not an object) → ValueMapMalformed.
    #[test]
    fn value_map_malformed_cases_errors() {
        let args = json!({ "rateMode": "1" });
        let ctx = mk_ctx(&args);
        let template = json!({ "$match": "$args.rateMode", "$cases": "not-an-object" });
        let err = substitute_placeholders(&ctx, &template).unwrap_err();
        assert!(matches!(err, V3BuildError::ValueMapMalformed(_)));
    }

    // (f) `$match` resolving to a non-keyable JSON type (here an object via an
    // `$args.<path>` that points at a nested object) → ValueMapMalformed. Only
    // String / Number / Bool collapse to a lookup key.
    #[test]
    fn value_map_non_keyable_match_errors() {
        let args = json!({ "permit": { "spender": "0xabc", "amount": "1" } });
        let ctx = mk_ctx(&args);
        let template = json!({
            "$match": "$args.permit",
            "$cases": { "1": "stable", "2": "variable" }
        });
        let err = substitute_placeholders(&ctx, &template).unwrap_err();
        assert!(matches!(err, V3BuildError::ValueMapMalformed(_)));
    }

    // (g) A stray / typo'd reserved key (`$dafault` instead of `$default`) →
    // fail-loud ValueMapMalformed naming the offending key, instead of silently
    // degrading to ValueMapNoMatch.
    #[test]
    fn value_map_stray_key_errors() {
        let args = json!({ "rateMode": "0" });
        let ctx = mk_ctx(&args);
        let template = json!({
            "$match": "$args.rateMode",
            "$cases": { "1": "stable", "2": "variable" },
            "$dafault": "fixed"
        });
        let err = substitute_placeholders(&ctx, &template).unwrap_err();
        assert!(matches!(err, V3BuildError::ValueMapMalformed(_)));
    }
}
