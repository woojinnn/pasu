//! M1 — `declarative_action_builder`
//!
//! Convert a v3 registry manifest's `emit.body` (nested-twice DSL with `$`
//! placeholders) into a typed [`ActionBody`] from the `simulation-reducer`
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
//!    matching the [`simulation_state::LiveField`] serde shape. Default values
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

use serde_json::{Map as JsonMap, Value as JsonValue};
use simulation_reducer::action::ActionBody;
use simulation_state::primitives::{Address, ChainId, Time, U256};

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
    /// every freshly built [`LiveField`](simulation_state::LiveField)).
    pub submitted_at: Time,
    /// Decoded calldata args as a JSON object keyed by ABI argument name.
    pub args_json: &'a JsonValue,
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
/// # Errors
///
/// Returns [`V3BuildError::UnresolvedPlaceholder`] when a `$resolved.x` /
/// `$derived.x` / `$inputs.*` lookup is empty, and
/// [`V3BuildError::InvalidArgPath`] when a `$args.*` JSONPath walk fails.
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

/// Resolve a single `$<root>` / `$<root>.<rest>` placeholder string.
fn resolve_placeholder(ctx: &V3MapContext<'_>, raw: &str) -> Result<JsonValue, V3BuildError> {
    // Strip the leading `$`. Split on the first `.` to identify the root.
    let body = &raw[1..];
    let (root, rest) = match body.find('.') {
        Some(idx) => (&body[..idx], &body[idx + 1..]),
        None => (body, ""),
    };

    match root {
        "chain" => Ok(JsonValue::String(ctx.chain.as_str().to_owned())),
        "to" => Ok(JsonValue::String(format!("{:#x}", ctx.tx_to))),
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
        "resolved" => ctx
            .resolved
            .get(rest)
            .cloned()
            .ok_or_else(|| V3BuildError::UnresolvedPlaceholder(raw.to_owned())),
        "derived" => ctx
            .derived
            .get(rest)
            .cloned()
            .ok_or_else(|| V3BuildError::UnresolvedPlaceholder(raw.to_owned())),
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
            let resolved = resolved
                .ok_or_else(|| format!("index {idx} out of bounds (len={})", arr.len()))?;
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
                        );
                        live_obj.insert(field_name.clone(), wrapped);
                    }
                    flat.insert("live_inputs".into(), JsonValue::Object(live_obj));
                }
                LiveInputLayout::Inline => {
                    for (field_name, src_payload) in map {
                        let wrapped = wrap_live_field(
                            ctx,
                            domain.as_deref(),
                            action.as_deref(),
                            field_name,
                            src_payload,
                        );
                        flat.insert(field_name.clone(), wrapped);
                    }
                }
            }
        }
    }

    // Stage 4 — typed decode.
    Ok(serde_json::from_value::<ActionBody>(JsonValue::Object(
        flat,
    ))?)
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
        | (Some("token"), Some("permit2_sign_allowance")) => LiveInputLayout::Inline,
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
    let inner = obj.get(&domain).and_then(JsonValue::as_object).ok_or_else(|| {
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
    let domain = flat.get("domain").and_then(JsonValue::as_str).map(str::to_owned);
    let action = flat.get("action").and_then(JsonValue::as_str).map(str::to_owned);
    (domain, action)
}

/// Wrap a placeholder-substituted `source` descriptor into the full
/// [`LiveField`](simulation_state::LiveField) JSON shape with a default value
/// drawn from [`live_input_default`] and `synced_at = ctx.submitted_at`.
///
/// If the manifest entry carries `ttl_s` (the v3 convention) the value is
/// converted into a `ttl` JSON number — [`simulation_state::primitives::Duration`]
/// is `#[serde(transparent)]` over a `u64`.
fn wrap_live_field(
    ctx: &V3MapContext<'_>,
    domain: Option<&str>,
    action: Option<&str>,
    field_name: &str,
    src_payload: &JsonValue,
) -> JsonValue {
    let mut out = JsonMap::new();
    out.insert(
        "value".into(),
        live_input_default(domain, action, field_name),
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

/// Per-`(domain, action, field)` default `value` for the `LiveField` wrap.
///
/// Each entry encodes the minimal `serde_json` shape needed for
/// `simulation_state::LiveField<T>` to deserialize successfully — typically
/// `"0"` for `U256`, `0` for `u32`, and a hand-rolled object skeleton for the
/// richer state types (`SwapRoute`, `PoolState`, `Vec<(TokenRef, U256)>`).
///
/// Extending the catalog when a new `live_inputs.<field>` lands in registry
/// V2 is a one-line edit — the test suite covers what's currently emitted by
/// the 25 v3 manifests.
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
        // -------- Token --------
        (Some("token"), Some("erc20_permit"), "nonce")
        | (Some("token"), Some("permit2_approve"), "nonce") => JsonValue::String("0".into()),
        (Some("token"), Some("permit2_sign_allowance"), "nonce") => {
            // `LiveField<(U256, u8)>` — JSON encodes a 2-tuple as a 2-element
            // array. Default: bitmap word 0, bit 0.
            serde_json::json!(["0", 0])
        }
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
                    eprintln!(
                        "[action_builder] warn: unknown opcode 0x{opcode:02x} at index {i}"
                    );
                    continue;
                }
                UnknownOpcodePolicy::Skip => continue,
            }
        };

        let body_template = opcode_entry
            .get("body")
            .ok_or_else(|| V3BuildError::UnresolvedPlaceholder(format!("{opcode_key}.body missing")))?;

        let inputs_for_this = decoded_inputs_array.get(i);
        let child_ctx = V3MapContext {
            chain: ctx.chain.clone(),
            tx_to: ctx.tx_to,
            tx_from: ctx.tx_from,
            value: ctx.value,
            submitted_at: ctx.submitted_at,
            args_json: ctx.args_json,
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
// Inline unit tests (11) — see `## 11 inline unit test` in the M1 plan.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use simulation_reducer::action::{amm::AmmAction, token::TokenAction};
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
                assert_eq!(a.spender, addr("0x00000000000000000000000000000000deadbeef"));
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
        assert!(matches!(action, ActionBody::Token(TokenAction::Erc20Transfer(_))));
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
        assert!(matches!(action, ActionBody::Token(TokenAction::NftApprove(_))));
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
    // `expires_at` deserializes into a `Time` (transparent over `u64`), so
    // the test feeds it as a raw JSON number. The on-chain ABI decoder
    // produces decimal strings, but a numeric coercion shim is out of M1
    // scope — when M2 wires the decoder we'll add a numeric-string -> number
    // pass keyed by the destination's serde shape.
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
        assert!(matches!(action, ActionBody::Token(TokenAction::Permit2Approve(_))));
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
        assert!(matches!(action, ActionBody::Amm(AmmAction::AddLiquidity(_))));
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
        assert!(matches!(action, ActionBody::Amm(AmmAction::RemoveLiquidity(_))));
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
}
