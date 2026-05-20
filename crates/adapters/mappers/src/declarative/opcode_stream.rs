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

use std::str::FromStr as _;

use abi_resolver::bridge::convert_arg;
use abi_resolver::subdecode::opcode_stream as tier_b_opcode_stream;
use abi_resolver::subdecode::opcode_stream::DecodedStep;
use abi_resolver::subdecode::protocols::universal_router::{
    extract_commands_and_inputs, v3_position_manager_address, v4_position_manager_address,
    UNISWAP_UR_ALLOW_REVERT, UNISWAP_UR_MASK, UNISWAP_UR_TABLE,
};
use abi_resolver::subdecode::protocols::v4_router::{
    extract_actions_and_params, V4_ROUTER_TABLE,
};
use abi_resolver::{CallMatchKey, DecodedCall, DecoderId};
use alloy_dyn_abi::DynSolValue;
use policy_engine::action::Address;
use policy_engine::ActionEnvelope;
use std::collections::BTreeMap;

use crate::mapper::{MapContext, MapperError};

use super::single_emit;
use super::types::{EmitRule, PerOpcodeEmit, UnknownOpcodePolicy, ValueExpr};

/// Dispatcher id supported by the Phase 5 PoC. Matches the value bundles
/// declare under `emit.dispatcher_id`.
pub const DISPATCHER_ID_UNIVERSAL_ROUTER: &str = "universal_router";

/// Maximum nesting depth allowed for `EXECUTE_SUB_PLAN` (0x21) recursion and
/// `V4_SWAP` (0x10) cross-table recursion.
///
/// Production Uniswap UR txs nest `EXECUTE_SUB_PLAN` at most 1-2 levels deep
/// (typically a single sub-plan wrapping a multi-step swap); 3 leaves a one-
/// level safety margin and bounds CPU / stack on adversarial bundles. Reached
/// via `ctx.depth >= MAX_SUB_PLAN_DEPTH` against `MapContext::depth` which the
/// recursive call increments by one each entry via `MapContext::child`. The
/// same cap applies to `V4_SWAP` cross-table dispatch — a V4_SWAP encountered
/// inside a deeply nested sub-plan still spends one depth slot.
const MAX_SUB_PLAN_DEPTH: u8 = 3;

/// Tier B opcode for `EXECUTE_SUB_PLAN` after `UNISWAP_UR_MASK` is applied.
const OPCODE_EXECUTE_SUB_PLAN: u8 = 0x21;

/// Tier B opcode for `V4_SWAP` after `UNISWAP_UR_MASK` is applied. Triggers
/// cross-table dispatch through `V4_ROUTER_TABLE` (Uniswap V4 action set).
const OPCODE_V4_SWAP: u8 = 0x10;

/// Tier B opcode for `V3_POSITION_MANAGER_PERMIT` after `UNISWAP_UR_MASK` is
/// applied. The opcode's single `(bytes data)` arg carries a complete V3
/// NonfungiblePositionManager calldata which we dispatch back through
/// `ctx.resolver` (cross-target recursion — the inner call goes to the per-
/// chain V3 NPM rather than the parent UR address).
const OPCODE_V3_POSITION_MANAGER_PERMIT: u8 = 0x11;

/// Tier B opcode for `V3_POSITION_MANAGER_CALL` — same shape as 0x11.
const OPCODE_V3_POSITION_MANAGER_CALL: u8 = 0x12;

/// Tier B opcode for `V4_POSITION_MANAGER_CALL` — same shape as 0x11/0x12 but
/// the inner calldata targets the per-chain V4 PositionManager.
const OPCODE_V4_POSITION_MANAGER_CALL: u8 = 0x14;

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
        return Err(MapperError::Unsupported(format!(
            "opcode_stream_dispatch/{dispatcher_id}"
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
    dispatch_steps(ctx, &steps, per_opcode_emit, unknown_opcode_policy)
}

/// Walk a [`DecodedStep`] slice (top-level or sub-plan) and emit envelopes.
///
/// Splitting this out lets `EXECUTE_SUB_PLAN` (0x21) re-enter recursively
/// against the same `per_opcode_emit` map without duplicating the unknown-
/// opcode-policy branching or the `step → DecodedCall → single_emit` bridge.
/// Behaviour for every other opcode is byte-identical to the pre-refactor
/// inline loop.
fn dispatch_steps(
    ctx: &MapContext<'_>,
    steps: &[DecodedStep],
    per_opcode_emit: &BTreeMap<String, PerOpcodeEmit>,
    unknown_opcode_policy: UnknownOpcodePolicy,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    let mut envelopes = Vec::new();
    for step in steps {
        // `EXECUTE_SUB_PLAN` (0x21) carries `(bytes commands, bytes[] inputs)`
        // with the same shape as the outer entrypoint — re-dispatch the inner
        // pair through the same opcode table so nested swap / wrap / sweep
        // steps reach their per-opcode rules. Depth-bounded via `MapContext`.
        if step.opcode == OPCODE_EXECUTE_SUB_PLAN {
            let sub_envelopes =
                execute_sub_plan_step(ctx, step, per_opcode_emit, unknown_opcode_policy)?;
            envelopes.extend(sub_envelopes);
            continue;
        }

        // `V4_SWAP` (0x10) carries `(bytes actions, bytes[] params)` — the
        // inner stream is dispatched through V4_ROUTER_TABLE (a different
        // table than UR's), so this is cross-table recursion rather than the
        // self-recursion EXECUTE_SUB_PLAN performs. Depth-bounded by the same
        // `MAX_SUB_PLAN_DEPTH` cap. Per the PoC scope (option D), the V4
        // inner step list is decoded but no envelopes are emitted — the V4
        // action → envelope mapping is a follow-up (T-B6, V4 PM builders).
        if step.opcode == OPCODE_V4_SWAP {
            let v4_envelopes = execute_v4_swap_step(ctx, step)?;
            envelopes.extend(v4_envelopes);
            continue;
        }

        // `V3_POSITION_MANAGER_PERMIT` (0x11), `V3_POSITION_MANAGER_CALL`
        // (0x12), and `V4_POSITION_MANAGER_CALL` (0x14) each carry a single
        // `(bytes data)` arg — the complete calldata for the per-chain NPM /
        // V4 PM. Dispatch this calldata back through `ctx.resolver` (cross-
        // target recursion: the inner call goes to a different contract than
        // the parent UR). Depth-bounded by the same `MAX_SUB_PLAN_DEPTH` cap;
        // the per-chain address lookup uses Tier B's
        // `v3_position_manager_address` / `v4_position_manager_address`.
        if matches!(
            step.opcode,
            OPCODE_V3_POSITION_MANAGER_PERMIT
                | OPCODE_V3_POSITION_MANAGER_CALL
                | OPCODE_V4_POSITION_MANAGER_CALL
        ) {
            let pm_envelopes = execute_position_manager_step(ctx, step)?;
            envelopes.extend(pm_envelopes);
            continue;
        }

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
        let step_args = step.args.clone().ok_or_else(|| {
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

/// Handle a single `EXECUTE_SUB_PLAN` (0x21) step.
///
/// The step's Tier B args carry the inner `(bytes commands, bytes[] inputs)`
/// pair (see `UNISWAP_UR_TABLE` entry for 0x21). We pull both fields out by
/// position, re-dispatch through Tier B's `opcode_stream::dispatch` against
/// `UNISWAP_UR_TABLE`, and then recurse into [`dispatch_steps`] under a child
/// `MapContext` whose `depth` is incremented by one (via
/// [`MapContext::child`]). Recursion is bounded by [`MAX_SUB_PLAN_DEPTH`] —
/// the spec allows arbitrary nesting in principle but production txs nest at
/// most once or twice, so a tight cap caps adversarial CPU / stack.
///
/// `unknown_opcode_policy` is inherited from the outer rule (the bundle has a
/// single map — a sub-plan that contains opcodes not in `per_opcode_emit`
/// follows the same warn / deny / ignore semantics as the outer steps).
fn execute_sub_plan_step(
    ctx: &MapContext<'_>,
    step: &DecodedStep,
    per_opcode_emit: &BTreeMap<String, PerOpcodeEmit>,
    unknown_opcode_policy: UnknownOpcodePolicy,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    // Depth bound: the outer call enters at depth 0, the first sub-plan at
    // depth 1, etc. Reject when entering this sub-plan would exceed the cap.
    if ctx.depth >= MAX_SUB_PLAN_DEPTH {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "EXECUTE_SUB_PLAN exceeded MAX_SUB_PLAN_DEPTH={MAX_SUB_PLAN_DEPTH} \
             at step index {} (current depth {})",
            step.index,
            ctx.depth
        )));
    }

    let step_args = step.args.as_ref().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "EXECUTE_SUB_PLAN step index {} has no decoded args — Tier B error: {:?}",
            step.index,
            step.error
        ))
    })?;

    let inner_commands = extract_bytes_at_field(step_args, "commands").map_err(|e| {
        MapperError::Internal(anyhow::anyhow!(
            "EXECUTE_SUB_PLAN step index {} commands extract failed: {e}",
            step.index
        ))
    })?;
    let inner_inputs = extract_array_bytes_at_field(step_args, "inputs").map_err(|e| {
        MapperError::Internal(anyhow::anyhow!(
            "EXECUTE_SUB_PLAN step index {} inputs extract failed: {e}",
            step.index
        ))
    })?;

    let inner_steps =
        tier_b_opcode_stream::dispatch(&inner_commands, &inner_inputs, &UNISWAP_UR_TABLE);

    // `MapContext::child` requires `parent_calldata: &[u8]` borrowed for the
    // child context's lifetime. The inner commands bytes serve that role —
    // they're the calldata-equivalent for the recursive level. Keeping them
    // bound in this stack frame ensures the borrow outlives `child_ctx`.
    let child_ctx = ctx.child(ctx.to, &inner_commands);
    dispatch_steps(&child_ctx, &inner_steps, per_opcode_emit, unknown_opcode_policy)
}

/// Handle a single `V4_SWAP` (0x10) step — cross-table recursive dispatch.
///
/// The step's Tier B args carry `(bytes actions, bytes[] params)` (see
/// `UNISWAP_UR_TABLE` entry for 0x10). We pull the inner pair out via
/// `extract_actions_and_params`, then re-dispatch through Tier B's
/// `opcode_stream::dispatch` against **`V4_ROUTER_TABLE`** (the V4 action
/// set — SWAP_EXACT_IN_SINGLE, SETTLE, TAKE, etc.) rather than
/// `UNISWAP_UR_TABLE`. This is the key difference vs `execute_sub_plan_step`:
/// 0x21 is *self-recursion* (same table re-entered), 0x10 is *cross-table*
/// (UR → V4 routers).
///
/// PoC scope (option D): the V4 inner step list is decoded so the dispatch
/// wire-up is exercised end-to-end, but no envelopes are emitted from V4
/// inner actions in this phase. Per-action envelope construction (e.g.
/// SWAP_EXACT_IN_SINGLE → `Swap` action with hook context) is deferred to
/// T-B6, where the V4 PositionManager builders are added. As a result this
/// function always returns an empty `Vec<ActionEnvelope>` on success and
/// the bundle's `per_opcode_emit` is not consulted here. We still bound the
/// recursion depth so a maliciously deep `EXECUTE_SUB_PLAN` chain ending in
/// a V4_SWAP cannot bypass `MAX_SUB_PLAN_DEPTH`.
fn execute_v4_swap_step(
    ctx: &MapContext<'_>,
    step: &DecodedStep,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    // Depth bound: the outer call enters at depth 0; a V4_SWAP encountered
    // inside a sub-plan / nested context spends one depth slot just like a
    // sub-plan would. Mirrors `execute_sub_plan_step`'s guard so the cap
    // applies uniformly to both recursion shapes.
    if ctx.depth >= MAX_SUB_PLAN_DEPTH {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "V4_SWAP exceeded MAX_SUB_PLAN_DEPTH={MAX_SUB_PLAN_DEPTH} \
             at step index {} (current depth {})",
            step.index,
            ctx.depth
        )));
    }

    // Pull `(bytes actions, bytes[] params)` out of the step's decoded args.
    // `extract_actions_and_params` returns `None` when the step's Tier B
    // decode dropped to a fallback or the structural shape doesn't match —
    // surface as an internal error so authors notice the schema mismatch
    // instead of silently dropping a V4_SWAP block.
    let (actions, params) = extract_actions_and_params(step).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "V4_SWAP step index {} args do not match (bytes actions, bytes[] params) \
             — Tier B name {:?}, error {:?}",
            step.index,
            step.name,
            step.error
        ))
    })?;

    // Dispatch the inner action stream against the V4 router table. We
    // collect the resulting step list to drive future V4 envelope builders
    // (T-B6); for now we just verify the dispatch doesn't fault and surface
    // the count via the discard below.
    let _inner_steps = tier_b_opcode_stream::dispatch(&actions, &params, &V4_ROUTER_TABLE);

    // Option D: V4 inner actions do not emit envelopes in this phase. The
    // V4 action → envelope builders (Swap with hook context, position
    // mutations, etc.) are scoped to T-B6. Returning an empty vec keeps the
    // outer UR step iteration moving without spurious side-effects, and
    // future activation only needs to swap this return for a per-V4-opcode
    // emit loop (with a child `MapContext` carrying depth+1 if any V4 action
    // ever recurses further).
    Ok(Vec::new())
}

/// Handle a single V3/V4 PositionManager opcode step — cross-target recursive
/// dispatch.
///
/// The opcode's Tier B args carry a single `(bytes data)` blob — the complete
/// calldata for the per-chain V3 NonfungiblePositionManager (0x11/0x12) or V4
/// PositionManager (0x14). Unlike `EXECUTE_SUB_PLAN` (self-recursion, same
/// `to`) and `V4_SWAP` (cross-table, same `to`), this is *cross-target*
/// recursion: the inner call targets a different contract than the parent UR.
///
/// Flow:
///   1. Pull the inner `bytes data` from `step.args`.
///   2. Verify it's ≥ 4 bytes (a valid selector + ABI arg block).
///   3. Look up the per-chain PM address via Tier B (0x11/0x12 → V3 NPM,
///      0x14 → V4 PM).
///   4. Build a `CallMatchKey { chain_id, to: <PM addr>, selector }` and a
///      child `MapContext` whose `to` is the PM address and whose
///      `parent_calldata` carries the inner blob.
///   5. Dispatch through `ctx.resolver.resolve_child(...)`.
///
/// Depth-bounded by `MAX_SUB_PLAN_DEPTH` (shared with `EXECUTE_SUB_PLAN` and
/// `V4_SWAP`) — a PM call encountered inside a deeply nested sub-plan still
/// spends one depth slot, mirroring the `execute_sub_plan_step` /
/// `execute_v4_swap_step` guards.
///
/// `ctx.resolver` MUST be wired by the host. If it is `None`, the dispatch
/// surfaces `MapperError::Internal` rather than silently dropping the step
/// (mirrors the multicall_recurse error so authors notice the missing host
/// capability).
fn execute_position_manager_step(
    ctx: &MapContext<'_>,
    step: &DecodedStep,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    // Depth bound shared with EXECUTE_SUB_PLAN / V4_SWAP.
    if ctx.depth >= MAX_SUB_PLAN_DEPTH {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "{} exceeded MAX_SUB_PLAN_DEPTH={MAX_SUB_PLAN_DEPTH} \
             at step index {} (current depth {})",
            step.name,
            step.index,
            ctx.depth
        )));
    }

    let step_args = step.args.as_ref().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "{} step index {} has no decoded args — Tier B error: {:?}",
            step.name,
            step.index,
            step.error
        ))
    })?;

    let pm_calldata = extract_bytes_at_field(step_args, "data").map_err(|e| {
        MapperError::Internal(anyhow::anyhow!(
            "{} step index {} data extract failed: {e}",
            step.name,
            step.index
        ))
    })?;

    if pm_calldata.len() < 4 {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "{} step index {} inner calldata too short for selector (len={})",
            step.name,
            step.index,
            pm_calldata.len()
        )));
    }
    let mut selector = [0u8; 4];
    selector.copy_from_slice(&pm_calldata[..4]);

    // Resolve the per-chain PositionManager address from Tier B.
    let pm_alloy_addr = match step.opcode {
        OPCODE_V3_POSITION_MANAGER_PERMIT | OPCODE_V3_POSITION_MANAGER_CALL => {
            v3_position_manager_address(ctx.chain_id).ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "{} step index {}: no V3 NonfungiblePositionManager address \
                     registered for chain_id {}",
                    step.name,
                    step.index,
                    ctx.chain_id
                ))
            })?
        }
        OPCODE_V4_POSITION_MANAGER_CALL => {
            v4_position_manager_address(ctx.chain_id).ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "{} step index {}: no V4 PositionManager address registered \
                     for chain_id {}",
                    step.name,
                    step.index,
                    ctx.chain_id
                ))
            })?
        }
        other => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "execute_position_manager_step called with unexpected opcode {other:#04x} \
                 (step index {})",
                step.index
            )));
        }
    };

    // Convert the alloy 20-byte address to the policy-engine string-wrapper
    // form so `CallMatchKey` and the child `MapContext` can borrow / hold it.
    // The string is lowercase-normalised by `Address::from_str` (see
    // `policy_engine::action::Address`).
    let pm_addr = Address::from_str(&format!("0x{}", hex::encode(pm_alloy_addr)))
        .map_err(|e| {
            MapperError::Internal(anyhow::anyhow!(
                "{} step index {}: PM address conversion failed: {e}",
                step.name,
                step.index
            ))
        })?;

    let resolver = ctx.resolver.ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "{} step index {} requires ctx.resolver (host:resolver), but it is None — \
             host did not wire a ChildResolver",
            step.name,
            step.index
        ))
    })?;

    let child_key = CallMatchKey {
        chain_id: ctx.chain_id,
        to: pm_addr.clone(),
        selector,
    };

    // Build the child context — depth+1, parent_calldata = inner blob, to =
    // PM address (not the outer UR's `ctx.to`).
    let child_ctx = ctx.child(&pm_addr, &pm_calldata);

    resolver.resolve_child(&child_key, &child_ctx, &pm_calldata)
}

/// Pull a `DynSolValue::Bytes` field out of a Tier B step's decoded args by
/// matching on the arg name. Field-name lookup mirrors the JSON ABI for
/// 0x21 (`commands` at index 0, `inputs` at index 1) while staying robust to
/// position changes.
fn extract_bytes_at_field(
    args: &[abi_resolver::decode::DecodedArg],
    field: &str,
) -> Result<Vec<u8>, String> {
    let arg = args
        .iter()
        .find(|a| a.name == field)
        .ok_or_else(|| format!("missing field `{field}`"))?;
    match &arg.value {
        DynSolValue::Bytes(b) => Ok(b.clone()),
        other => Err(format!(
            "field `{field}` expected Bytes, got {other:?}"
        )),
    }
}

/// Pull a `DynSolValue::Array(Bytes...)` field out of a Tier B step's decoded
/// args. Returns each inner Bytes as a `Vec<u8>`.
fn extract_array_bytes_at_field(
    args: &[abi_resolver::decode::DecodedArg],
    field: &str,
) -> Result<Vec<Vec<u8>>, String> {
    let arg = args
        .iter()
        .find(|a| a.name == field)
        .ok_or_else(|| format!("missing field `{field}`"))?;
    let DynSolValue::Array(items) = &arg.value else {
        return Err(format!(
            "field `{field}` expected Array, got {:?}",
            arg.value
        ));
    };
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let DynSolValue::Bytes(b) = item else {
            return Err(format!(
                "field `{field}[{i}]` expected Bytes, got {item:?}"
            ));
        };
        out.push(b.clone());
    }
    Ok(out)
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

    const UR_BUNDLE_JSON: &str = include_str!(
        "../../../../../registry/manifests/uniswap/universal-router/execute@1.0.0.json"
    );

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

    /// Encode BALANCE_CHECK_ERC20 input — used to exercise the unknown_opcode
    /// path. The Phase 9A2 bundle does NOT include 0x0e in `per_opcode_emit`
    /// (BALANCE_CHECK_ERC20 is view-only and has no `(category, action)` analog
    /// in the action schema).
    fn encode_balance_check_erc20_input(
        owner: alloy_primitives::Address,
        token: alloy_primitives::Address,
        min_balance: u128,
    ) -> Vec<u8> {
        let func = Function::parse("step(address,address,uint256)").unwrap();
        let values = vec![
            DynSolValue::Address(owner),
            DynSolValue::Address(token),
            DynSolValue::Uint(U256::from(min_balance), 256),
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

        // BALANCE_CHECK_ERC20 (0x0e) is NOT in the bundle's per_opcode_emit
        // (view-only opcode with no policy-engine action analog); the bundle
        // declares unknown_opcode_policy=warn so the step is skipped.
        let balance_check =
            encode_balance_check_erc20_input(recipient_addr(), token_out(), 1);
        let swap = encode_v3_swap_exact_in_input(
            recipient_addr(),
            1_000_000,
            900_000,
            v3_packed_path_usdc_weth(),
            true,
        );

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x0e, 0x00],
            vec![balance_check, swap],
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

    /// Encode WRAP_ETH input `(address recipient, uint256 amountMin)`.
    fn encode_wrap_eth_input(
        recipient: alloy_primitives::Address,
        amount_min: u128,
    ) -> Vec<u8> {
        let func = Function::parse("step(address,uint256)").unwrap();
        let values = vec![
            DynSolValue::Address(recipient),
            DynSolValue::Uint(U256::from(amount_min), 256),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    /// Encode `EXECUTE_SUB_PLAN` (0x21) input `(bytes commands, bytes[] inputs)`.
    /// Same shape as the outer `execute(...)` pair — Uniswap UR uses the inner
    /// pair to recurse into another command stream.
    fn encode_execute_sub_plan_input(
        inner_commands: Vec<u8>,
        inner_inputs: Vec<Vec<u8>>,
    ) -> Vec<u8> {
        let func = Function::parse("step(bytes,bytes[])").unwrap();
        let values = vec![
            DynSolValue::Bytes(inner_commands),
            DynSolValue::Array(
                inner_inputs.into_iter().map(DynSolValue::Bytes).collect(),
            ),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    #[test]
    fn execute_sub_plan_inner_wrap_eth_yields_wrap_envelope() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        // Inner sub-plan = [0x0b WRAP_ETH] — one step that should produce a
        // single wrap envelope via the bundle's `0x0b` per_opcode_emit entry.
        let inner_wrap = encode_wrap_eth_input(recipient_addr(), 1_000_000);
        let sub_plan = encode_execute_sub_plan_input(vec![0x0b], vec![inner_wrap]);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x21],
            vec![sub_plan],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert_eq!(envelopes.len(), 1, "expected 1 wrap envelope, got {envelopes:?}");
        // The bundle's 0x0b rule emits category=misc, action=wrap.
        assert_eq!(envelopes[0].category, Category::Misc);
        assert!(matches!(envelopes[0].action, Action::Wrap(_)));
    }

    #[test]
    fn execute_sub_plan_recursive_depth_2_yields_inner_envelopes() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        // Build inner-most level: [0x0b WRAP_ETH] — depth 2.
        let inner_wrap = encode_wrap_eth_input(recipient_addr(), 2_000_000);
        let inner_sub_plan = encode_execute_sub_plan_input(vec![0x0b], vec![inner_wrap]);
        // Middle level: [0x21 EXECUTE_SUB_PLAN] — depth 1 sub-plan that itself
        // contains another sub-plan. Outer level (depth 0) wraps this.
        let outer_sub_plan = encode_execute_sub_plan_input(vec![0x21], vec![inner_sub_plan]);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x21],
            vec![outer_sub_plan],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        // After 2 levels of EXECUTE_SUB_PLAN unwrapping, the innermost
        // WRAP_ETH yields exactly one envelope.
        assert_eq!(envelopes.len(), 1, "expected 1 wrap envelope at depth 2, got {envelopes:?}");
        assert!(matches!(envelopes[0].action, Action::Wrap(_)));
    }

    #[test]
    fn execute_sub_plan_exceeds_max_depth_errors() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        // Build a chain of 4 nested EXECUTE_SUB_PLAN steps. The outer call
        // enters at depth 0; sub-plan 1 enters at depth 1, ..., sub-plan 4 at
        // depth 4. With MAX_SUB_PLAN_DEPTH=3, the 4th entry must be rejected.
        //
        // Innermost payload is a no-op WRAP — the test asserts on the depth
        // guard so the eventual leaf shape is unimportant.
        let leaf_wrap = encode_wrap_eth_input(recipient_addr(), 1);
        // Level 4 (would-be depth 4): wraps the leaf.
        let level4 = encode_execute_sub_plan_input(vec![0x0b], vec![leaf_wrap]);
        // Level 3: wraps level4.
        let level3 = encode_execute_sub_plan_input(vec![0x21], vec![level4]);
        // Level 2: wraps level3.
        let level2 = encode_execute_sub_plan_input(vec![0x21], vec![level3]);
        // Level 1: wraps level2.
        let level1 = encode_execute_sub_plan_input(vec![0x21], vec![level2]);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x21],
            vec![level1],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let err = super::execute(&ctx, &decoded, &bundle.emit).unwrap_err();
        let MapperError::Internal(inner) = &err else {
            panic!("expected MapperError::Internal, got {err:?}");
        };
        let msg = inner.to_string();
        assert!(
            msg.contains("MAX_SUB_PLAN_DEPTH"),
            "expected depth-bound error, got: {msg}"
        );
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

    /// Encode UR `V4_SWAP` (0x10) input `(bytes actions, bytes[] params)`.
    /// Mirrors `encode_execute_sub_plan_input` — the outer shape is identical
    /// to a sub-plan, but the inner `actions` byte stream is dispatched
    /// against `V4_ROUTER_TABLE` instead of `UNISWAP_UR_TABLE`.
    fn encode_v4_swap_input(
        inner_actions: Vec<u8>,
        inner_params: Vec<Vec<u8>>,
    ) -> Vec<u8> {
        let func = Function::parse("step(bytes,bytes[])").unwrap();
        let values = vec![
            DynSolValue::Bytes(inner_actions),
            DynSolValue::Array(
                inner_params.into_iter().map(DynSolValue::Bytes).collect(),
            ),
        ];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    /// Encode a V4_ROUTER_TABLE `SWAP_EXACT_IN_SINGLE` (0x06) action params
    /// in the mainnet-deployed (pre-#497, no `minHopPriceX36`) shape:
    /// `(((address,address,uint24,int24,address), bool, uint128, uint128, bytes) params)`.
    /// The exact field values are irrelevant for the V4_SWAP wire-up test —
    /// what matters is that Tier B can ABI-decode the blob against the V4
    /// table without raising.
    fn encode_v4_swap_exact_in_single_input(
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
            DynSolValue::Int(alloy_primitives::I256::try_from(tick_spacing).unwrap(), 24),
            DynSolValue::Address(hooks),
        ]);
        let params_tuple = DynSolValue::Tuple(vec![
            pool_key,
            DynSolValue::Bool(zero_for_one),
            DynSolValue::Uint(U256::from(amount_in), 128),
            DynSolValue::Uint(U256::from(amount_out_min), 128),
            DynSolValue::Bytes(hook_data),
        ]);
        let func = Function::parse(
            "step(((address,address,uint24,int24,address),bool,uint128,uint128,bytes))",
        )
        .unwrap();
        let values = vec![params_tuple];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    /// V4_SWAP (UR 0x10) wraps `(bytes actions, bytes[] params)`. Outer step's
    /// args MUST decode to a `(Bytes, Array<Bytes>)` pair so
    /// `extract_actions_and_params` can pull both fields back out for the
    /// cross-table dispatch. This is the structural invariant the rest of
    /// the V4_SWAP plumbing relies on.
    #[test]
    fn v4_swap_outer_args_decode_succeeds() {
        // Inner V4 action stream is a single SWAP_EXACT_IN_SINGLE (0x06) —
        // shape doesn't matter for outer decode, but giving it a real V4
        // action params blob exercises the same code path the integration
        // hits.
        let v4_inner = encode_v4_swap_exact_in_single_input(
            token_in(),
            token_out(),
            3_000,
            60,
            alloy_primitives::Address::ZERO,
            true,
            1_000_000,
            900_000,
            vec![],
        );
        let v4_swap = encode_v4_swap_input(vec![0x06], vec![v4_inner.clone()]);

        // Dispatch through Tier B directly so we can inspect the decoded
        // V4_SWAP step the declarative layer would receive.
        let steps = tier_b_opcode_stream::dispatch(
            &[OPCODE_V4_SWAP],
            &[v4_swap],
            &UNISWAP_UR_TABLE,
        );
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].name, "V4_SWAP");
        let args = steps[0]
            .args
            .as_ref()
            .expect("V4_SWAP outer args must decode");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0].name, "actions");
        assert_eq!(args[1].name, "params");

        // `extract_actions_and_params` must round-trip the inner pair.
        let (actions, params) = extract_actions_and_params(&steps[0])
            .expect("extract_actions_and_params must succeed for a clean V4_SWAP");
        assert_eq!(actions, vec![0x06]);
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], v4_inner);
    }

    /// End-to-end: an outer UR call carrying a single `V4_SWAP` whose inner
    /// stream is a single V4 `SWAP_EXACT_IN_SINGLE` MUST execute without
    /// raising and — per the PoC's option D — emit zero envelopes. The
    /// cross-table dispatch is exercised (Tier B can ABI-decode the inner
    /// 0x06 against `V4_ROUTER_TABLE`) but envelope construction for V4
    /// actions is deferred to T-B6.
    #[test]
    fn v4_swap_inner_dispatch_yields_inner_steps_recognized() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let v4_inner = encode_v4_swap_exact_in_single_input(
            token_in(),
            token_out(),
            3_000,
            60,
            alloy_primitives::Address::ZERO,
            true,
            1_000_000,
            900_000,
            vec![],
        );
        let v4_swap = encode_v4_swap_input(vec![0x06], vec![v4_inner]);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![OPCODE_V4_SWAP],
            vec![v4_swap],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        // Option D: V4 inner actions are decoded but no envelopes are
        // emitted in this phase.
        assert!(
            envelopes.is_empty(),
            "V4_SWAP must emit no envelopes under option D, got {envelopes:?}"
        );

        // Sanity: dispatching the same inner stream directly against
        // V4_ROUTER_TABLE must produce one recognized step (i.e. the
        // cross-table wire-up isn't masking a Tier B regression).
        let v4_inner_check = encode_v4_swap_exact_in_single_input(
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
        let v4_steps = tier_b_opcode_stream::dispatch(
            &[0x06],
            &[v4_inner_check],
            &V4_ROUTER_TABLE,
        );
        assert_eq!(v4_steps.len(), 1);
        assert_eq!(v4_steps[0].name, "SWAP_EXACT_IN_SINGLE");
        assert!(
            v4_steps[0].args.is_some(),
            "V4 SWAP_EXACT_IN_SINGLE args must decode against V4_ROUTER_TABLE"
        );
    }

    /// Mixed `EXECUTE_SUB_PLAN` + `V4_SWAP` nesting MUST still observe the
    /// shared `MAX_SUB_PLAN_DEPTH` cap. We chain three sub-plans (depths 1,
    /// 2, 3) and place a V4_SWAP at the leaf — entering the V4_SWAP would
    /// require depth 4, exceeding the cap.
    #[test]
    fn v4_swap_exceeds_max_depth_errors() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        // Leaf: a V4_SWAP with a minimal inner action (the inner action's
        // content is irrelevant because the depth guard fires before
        // dispatch).
        let v4_inner = encode_v4_swap_exact_in_single_input(
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

        // Wrap the V4_SWAP in three levels of EXECUTE_SUB_PLAN. The outer
        // call enters at depth 0; each sub-plan increments depth, putting
        // the V4_SWAP at depth 4 when it tries to dispatch.
        let level3 = encode_execute_sub_plan_input(vec![OPCODE_V4_SWAP], vec![leaf_v4]);
        let level2 = encode_execute_sub_plan_input(vec![0x21], vec![level3]);
        let level1 = encode_execute_sub_plan_input(vec![0x21], vec![level2]);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x21],
            vec![level1],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let err = super::execute(&ctx, &decoded, &bundle.emit).unwrap_err();
        let MapperError::Internal(inner) = &err else {
            panic!("expected MapperError::Internal, got {err:?}");
        };
        let msg = inner.to_string();
        assert!(
            msg.contains("MAX_SUB_PLAN_DEPTH"),
            "expected depth-bound error from V4_SWAP guard, got: {msg}"
        );
        // Disambiguate which guard fired — the V4_SWAP-side message names
        // V4_SWAP explicitly so a regression that bypasses the V4 guard
        // (e.g. by hitting the EXECUTE_SUB_PLAN guard instead) is visible.
        assert!(
            msg.contains("V4_SWAP"),
            "expected V4_SWAP-specific depth error, got: {msg}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // T-B1.3 — V3/V4 PositionManager opcodes (0x11/0x12/0x14)
    // ─────────────────────────────────────────────────────────────────────

    /// Mock `ChildResolver` that captures every child dispatch and returns a
    /// caller-supplied response. Mirrors the `RecordingResolver` pattern from
    /// `multicall::tests` but is local here because the test fixtures live in
    /// a separate file.
    struct CapturingResolver {
        calls: std::sync::Mutex<Vec<CapturedCall>>,
        responses: std::sync::Mutex<Vec<Result<Vec<ActionEnvelope>, MapperError>>>,
    }

    #[derive(Debug)]
    struct CapturedCall {
        key: abi_resolver::CallMatchKey,
        calldata: Vec<u8>,
        depth: u8,
        had_parent: bool,
        to_in_ctx: Address,
    }

    impl CapturingResolver {
        fn new(responses: Vec<Result<Vec<ActionEnvelope>, MapperError>>) -> Self {
            Self {
                calls: std::sync::Mutex::new(Vec::new()),
                responses: std::sync::Mutex::new(responses),
            }
        }

        fn calls(&self) -> std::sync::MutexGuard<'_, Vec<CapturedCall>> {
            self.calls.lock().unwrap()
        }
    }

    impl crate::mapper::ChildResolver for CapturingResolver {
        fn resolve_child(
            &self,
            child: &abi_resolver::CallMatchKey,
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

    /// Build a `MapContext` with a resolver wired in.
    fn ctx_with_resolver<'a>(
        resolver: &'a dyn crate::mapper::ChildResolver,
        registry: &'a EmptyTokenRegistry,
        from: &'a Address,
        to: &'a Address,
        value: &'a DecimalString,
        depth: u8,
    ) -> MapContext<'a> {
        MapContext {
            chain_id: 1,
            from,
            to,
            value_wei: value,
            block_timestamp: Some(1_700_000_000),
            token_registry: registry,
            parent_calldata: None,
            depth,
            resolver: Some(resolver),
        }
    }

    /// Encode a UR position-manager step input — `(bytes data)` carrying the
    /// inner PM calldata. Mirrors the structure Tier B's
    /// `position_manager_opcodes_decode_bytes_data` test exercises against
    /// `UNISWAP_UR_TABLE`.
    fn encode_position_manager_step_input(inner_calldata: Vec<u8>) -> Vec<u8> {
        let func = Function::parse("step(bytes)").unwrap();
        let values = vec![DynSolValue::Bytes(inner_calldata)];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    /// Synthetic V3 NPM `decreaseLiquidity` calldata — 4-byte selector +
    /// 32-byte zero-padded arg block. The selector value is the canonical
    /// `0x0c49ccbe` (`decreaseLiquidity((uint256,uint128,uint256,uint256,uint256))`)
    /// so the assertion below catches accidental selector slicing bugs.
    fn synthetic_v3_npm_decrease_liquidity_calldata() -> Vec<u8> {
        let mut v = vec![0x0c, 0x49, 0xcc, 0xbe];
        v.extend_from_slice(&[0u8; 64]);
        v
    }

    /// Expected V3 NPM address for chain_id=1, matching Tier B's
    /// `V3_NPM_ADDRESSES` entry.
    fn expected_v3_npm_mainnet_addr() -> Address {
        Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap()
    }

    /// Expected V4 PM address for chain_id=1, matching Tier B's
    /// `V4_PM_ADDRESSES` entry.
    fn expected_v4_pm_mainnet_addr() -> Address {
        Address::from_str("0xbd216513d74c8cf14cf4747e6aaa6420ff64ee9e").unwrap()
    }

    /// 0x12 `V3_POSITION_MANAGER_CALL` — outer UR step carrying an inner V3
    /// NPM `decreaseLiquidity` blob must dispatch through `ctx.resolver` with
    /// the per-chain V3 NPM address and the selector pulled from the first
    /// 4 bytes of the inner calldata.
    #[test]
    fn v3_position_manager_call_dispatches_to_resolver() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let inner = synthetic_v3_npm_decrease_liquidity_calldata();
        let step_input = encode_position_manager_step_input(inner.clone());

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x12],
            vec![step_input],
        );

        let resolver = CapturingResolver::new(vec![Ok(Vec::new())]);
        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        // No envelopes from a stub resolver — verifying dispatch shape, not
        // downstream emission.
        assert!(envelopes.is_empty(), "expected empty envelopes from stub, got {envelopes:?}");

        let calls = resolver.calls();
        assert_eq!(calls.len(), 1, "resolver must be invoked exactly once");
        // Selector pulled from first 4 bytes of inner calldata.
        assert_eq!(calls[0].key.selector, [0x0c, 0x49, 0xcc, 0xbe]);
        // chain_id propagates from outer ctx.
        assert_eq!(calls[0].key.chain_id, 1);
        // Cross-target: child.to MUST be the per-chain V3 NPM, NOT the parent UR.
        assert_eq!(calls[0].key.to, expected_v3_npm_mainnet_addr());
        // child_ctx.to mirrors child_key.to.
        assert_eq!(calls[0].to_in_ctx, expected_v3_npm_mainnet_addr());
        // Full inner calldata preserved.
        assert_eq!(calls[0].calldata, inner);
        // Depth incremented and parent_calldata wired through MapContext::child.
        assert_eq!(calls[0].depth, 1, "child depth must be parent depth + 1");
        assert!(calls[0].had_parent, "child must have parent_calldata set");
    }

    /// 0x14 `V4_POSITION_MANAGER_CALL` — same shape as 0x12 but the per-chain
    /// V4 PM is used.
    #[test]
    fn v4_position_manager_call_dispatches_to_resolver() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        // Synthetic V4 PM `modifyLiquidities(bytes,uint256)` selector
        // (0xdd46508f) + 64-byte zero-padded arg block.
        let mut inner = vec![0xdd, 0x46, 0x50, 0x8f];
        inner.extend_from_slice(&[0u8; 64]);
        let step_input = encode_position_manager_step_input(inner.clone());

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x14],
            vec![step_input],
        );

        let resolver = CapturingResolver::new(vec![Ok(Vec::new())]);
        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert!(envelopes.is_empty());

        let calls = resolver.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].key.selector, [0xdd, 0x46, 0x50, 0x8f]);
        // 0x14 → V4 PM address (NOT V3 NPM).
        assert_eq!(calls[0].key.to, expected_v4_pm_mainnet_addr());
        assert_eq!(calls[0].calldata, inner);
        assert_eq!(calls[0].depth, 1);
    }

    /// 0x12 `V3_POSITION_MANAGER_CALL` without a wired resolver MUST surface a
    /// `MapperError::Internal` rather than silently dropping the step.
    #[test]
    fn v3_position_manager_call_without_resolver_errors() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let inner = synthetic_v3_npm_decrease_liquidity_calldata();
        let step_input = encode_position_manager_step_input(inner);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x12],
            vec![step_input],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        // Build ctx WITHOUT a resolver.
        let ctx = build_ctx(&registry, &from, &to, &value);

        let err = super::execute(&ctx, &decoded, &bundle.emit).unwrap_err();
        let MapperError::Internal(inner) = &err else {
            panic!("expected MapperError::Internal, got {err:?}");
        };
        let msg = inner.to_string();
        assert!(
            msg.contains("requires ctx.resolver"),
            "expected resolver-missing error, got: {msg}"
        );
        assert!(
            msg.contains("V3_POSITION_MANAGER_CALL"),
            "expected V3 PM-specific error, got: {msg}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // T-TEST-UR — UR.execute edge cases (T-B1.5 + T-B1.2 + T-B1.3 integration)
    // ─────────────────────────────────────────────────────────────────────

    /// Override the bundle's `unknown_opcode_policy` in-place. The bundle JSON
    /// fixture declares `warn`; deny/ignore_step variants need a programmatic
    /// override so a single fixture suffices for all three policy tests.
    fn override_unknown_opcode_policy(
        bundle: &mut AdapterFunctionBundle,
        policy: UnknownOpcodePolicy,
    ) {
        if let EmitRule::OpcodeStreamDispatch {
            unknown_opcode_policy,
            ..
        } = &mut bundle.emit
        {
            *unknown_opcode_policy = policy;
        } else {
            panic!("bundle.emit must be OpcodeStreamDispatch for UR fixture");
        }
    }

    /// T-TEST-UR #1: explicit empty-commands edge case. The dispatch loop
    /// produces zero `DecodedStep`s, the per_opcode_emit table is never
    /// consulted, and `super::execute` returns `Ok(vec![])`. Complementary to
    /// the existing `empty_commands_yields_no_envelopes` — kept as a separate
    /// case under the T-TEST-UR umbrella so test-suite regressions are
    /// attributed to the right roll-up.
    #[test]
    fn ur_execute_empty_commands_yields_no_envelopes() {
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
        assert!(envelopes.is_empty(), "empty commands MUST yield no envelopes");
    }

    /// T-TEST-UR #2: maximum-length commands. UR's dispatch loop has no
    /// per-call command cap — only the per-sub-plan depth limit
    /// (`MAX_SUB_PLAN_DEPTH`) bounds adversarial recursion. A 256-step stream
    /// where every step is `WRAP_ETH` (0x0b) MUST decode end-to-end and
    /// produce 256 wrap envelopes, exercising both Tier B's lockstep dispatch
    /// and the per_opcode_emit lookup at scale.
    #[test]
    fn ur_execute_max_256_inputs_succeeds() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let commands = vec![0x0b_u8; 256];
        let inputs: Vec<Vec<u8>> = (0..256)
            .map(|i| encode_wrap_eth_input(recipient_addr(), 1_000 + u128::from(i as u64)))
            .collect();

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            commands,
            inputs,
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert_eq!(envelopes.len(), 256, "expected 256 wrap envelopes");
        for env in &envelopes {
            assert_eq!(env.category, Category::Misc);
            assert!(
                matches!(env.action, Action::Wrap(_)),
                "every envelope MUST be Wrap, got {:?}",
                env.action
            );
        }
    }

    /// T-TEST-UR #3: `unknown_opcode_policy=deny` on a command byte Tier B
    /// cannot resolve MUST surface `MapperError::Internal`. We pick
    /// `0xFE` — after `UNISWAP_UR_MASK (0x7f)` it lands on `0x7e`, which is
    /// not in `UNISWAP_UR_TABLE` and not in the bundle's `per_opcode_emit`.
    #[test]
    fn ur_execute_unknown_opcode_with_deny_policy_errors() {
        let mut bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();
        override_unknown_opcode_policy(&mut bundle, UnknownOpcodePolicy::Deny);

        // Empty inputs[0] is fine — Tier B drops the step to NoSchema /
        // UnknownOpcode before any decoding is attempted.
        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0xFE],
            vec![vec![]],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let err = super::execute(&ctx, &decoded, &bundle.emit).unwrap_err();
        let MapperError::Internal(inner) = &err else {
            panic!("expected MapperError::Internal, got {err:?}");
        };
        let msg = inner.to_string();
        assert!(
            msg.contains("unknown_opcode_policy=deny"),
            "expected deny-policy error, got: {msg}"
        );
        // Masked opcode key MUST appear in the diagnostic — confirms the
        // mask is applied before the lookup.
        assert!(
            msg.contains("0x7e"),
            "expected masked opcode 0x7e in error, got: {msg}"
        );
    }

    /// T-TEST-UR #4: `unknown_opcode_policy=warn` on the same `0xFE` byte
    /// MUST skip the step silently from the envelope perspective (a warn
    /// log goes to stderr, but the test only asserts on the envelope side).
    #[test]
    fn ur_execute_unknown_opcode_with_warn_policy_skips() {
        let mut bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();
        override_unknown_opcode_policy(&mut bundle, UnknownOpcodePolicy::Warn);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0xFE],
            vec![vec![]],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert!(
            envelopes.is_empty(),
            "warn policy MUST skip unknown opcode, got {envelopes:?}"
        );
    }

    /// T-TEST-UR #5: `unknown_opcode_policy=ignore_step` on `0xFE` MUST
    /// silently skip (no stderr warn). The envelope-side behaviour is the
    /// same as `warn` — we exercise the variant explicitly so a regression
    /// that collapses one policy into the other surfaces here.
    #[test]
    fn ur_execute_unknown_opcode_with_ignore_step_policy_skips() {
        let mut bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();
        override_unknown_opcode_policy(&mut bundle, UnknownOpcodePolicy::IgnoreStep);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0xFE],
            vec![vec![]],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert!(
            envelopes.is_empty(),
            "ignore_step policy MUST skip unknown opcode, got {envelopes:?}"
        );
    }

    /// T-TEST-UR #6: `0x8b` (= 0x0b WRAP_ETH OR'd with 0x80 allow_revert
    /// bit). Tier B's dispatch masks the byte to 0x0b, sets `allow_revert =
    /// true` on the step, and decodes the WRAP_ETH `(recipient, amountMin)`
    /// schema. The per_opcode_emit lookup uses the *masked* opcode key
    /// (`"0x0b"`) so the wrap envelope is emitted just as it would be for a
    /// bare 0x0b — the allow-revert bit is purely a Tier B flag and never
    /// reaches the bundle key path.
    #[test]
    fn ur_execute_allow_revert_bit_set_succeeds() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let wrap_input = encode_wrap_eth_input(recipient_addr(), 1_000_000);
        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x8b], // 0x80 (allow_revert) | 0x0b (WRAP_ETH)
            vec![wrap_input],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert_eq!(envelopes.len(), 1, "0x8b MUST decode as WRAP_ETH");
        assert_eq!(envelopes[0].category, Category::Misc);
        assert!(matches!(envelopes[0].action, Action::Wrap(_)));
    }

    /// T-TEST-UR #7: mixed-opcode stream covering all three recursion shapes
    /// in a single outer call — `[0x00 V3_SWAP_EXACT_IN, 0x10 V4_SWAP, 0x12
    /// V3_POSITION_MANAGER_CALL]`. Expected envelope tally:
    ///
    ///   * `0x00` → one Swap envelope via per_opcode_emit
    ///   * `0x10` → zero envelopes (PoC option D — V4 inner decoded but no
    ///     emission)
    ///   * `0x12` → resolver-stub return; we wire a `CapturingResolver` that
    ///     returns one transfer-shaped envelope so the outer flatten produces
    ///     2 envelopes total.
    ///
    /// Together this exercises the three handler branches in `dispatch_steps`
    /// (V4_SWAP, V3/V4 PM, per_opcode_emit) in a single end-to-end pass.
    #[test]
    fn ur_execute_mixed_v3_swap_v4_swap_v3_pm() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let swap_input = encode_v3_swap_exact_in_input(
            recipient_addr(),
            1_000_000,
            900_000,
            v3_packed_path_usdc_weth(),
            true,
        );
        let v4_inner = encode_v4_swap_exact_in_single_input(
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
        let v4_swap_input = encode_v4_swap_input(vec![0x06], vec![v4_inner]);
        let pm_inner = synthetic_v3_npm_decrease_liquidity_calldata();
        let pm_step_input = encode_position_manager_step_input(pm_inner.clone());

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x00, 0x10, 0x12],
            vec![swap_input, v4_swap_input, pm_step_input],
        );

        // Pre-stage one synthetic envelope for the V3 PM resolver call. Concrete
        // action content is irrelevant — we only assert the outer flatten
        // preserves the resolver-side count. We use a `Transfer` envelope so
        // the order-of-flatten assertion below can match on `Action::Transfer`
        // without ambiguity vs the V3_SWAP_EXACT_IN-emitted `Action::Swap`.
        let resolver_envelope = synthetic_transfer_envelope();
        let resolver = CapturingResolver::new(vec![Ok(vec![resolver_envelope])]);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert_eq!(
            envelopes.len(),
            2,
            "expected 2 envelopes (1 V3 swap + 1 V3 PM resolver), got {envelopes:?}"
        );
        // Order MUST follow command stream — V3 swap first, V3 PM resolver
        // last (V4_SWAP slot is skipped).
        assert!(
            matches!(envelopes[0].action, Action::Swap(_)),
            "envelopes[0] MUST be Swap from V3_SWAP_EXACT_IN"
        );
        assert!(
            matches!(envelopes[1].action, Action::Transfer(_)),
            "envelopes[1] MUST be Transfer from PM resolver"
        );

        // Resolver invoked exactly once for 0x12.
        let calls = resolver.calls();
        assert_eq!(calls.len(), 1, "resolver MUST be invoked once for 0x12");
        assert_eq!(calls[0].key.selector, [0x0c, 0x49, 0xcc, 0xbe]);
        assert_eq!(calls[0].key.to, expected_v3_npm_mainnet_addr());
        assert_eq!(calls[0].calldata, pm_inner);
    }

    /// T-TEST-UR #8: `EXECUTE_SUB_PLAN` (0x21) at depth 1 with a single
    /// `WRAP_ETH` inner step. Verifies the basic self-recursion entry —
    /// outer depth 0 enters sub-plan, child depth 1 executes WRAP_ETH,
    /// yields exactly one wrap envelope.
    #[test]
    fn ur_execute_sub_plan_at_depth_1_succeeds() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let inner_wrap = encode_wrap_eth_input(recipient_addr(), 1_000_000);
        let sub_plan = encode_execute_sub_plan_input(vec![0x0b], vec![inner_wrap]);
        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x21],
            vec![sub_plan],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert_eq!(envelopes.len(), 1, "depth-1 sub-plan with WRAP_ETH MUST yield 1 envelope");
        assert!(matches!(envelopes[0].action, Action::Wrap(_)));
    }

    /// T-TEST-UR #9: `EXECUTE_SUB_PLAN` at the cap (depth 3). Three nested
    /// sub-plans + inner `WRAP_ETH` — innermost level executes at depth 3,
    /// which is exactly `MAX_SUB_PLAN_DEPTH`. The guard rejects on `ctx.depth
    /// >= MAX_SUB_PLAN_DEPTH` *entering* a sub-plan, so an entry that lands
    /// at depth 3 succeeds (the guard fires at depth 4 entry).
    #[test]
    fn ur_execute_sub_plan_at_max_depth_3_succeeds() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        // Leaf WRAP_ETH input.
        let leaf_wrap = encode_wrap_eth_input(recipient_addr(), 1_000);
        // Depth 3 sub-plan: directly wraps the WRAP_ETH.
        let depth3 = encode_execute_sub_plan_input(vec![0x0b], vec![leaf_wrap]);
        // Depth 2 sub-plan: wraps depth3.
        let depth2 = encode_execute_sub_plan_input(vec![0x21], vec![depth3]);
        // Depth 1 sub-plan: wraps depth2. Outer call at depth 0.
        let depth1 = encode_execute_sub_plan_input(vec![0x21], vec![depth2]);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x21],
            vec![depth1],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert_eq!(
            envelopes.len(),
            1,
            "sub-plan at max depth 3 MUST succeed and yield 1 wrap envelope"
        );
        assert!(matches!(envelopes[0].action, Action::Wrap(_)));
    }

    /// T-TEST-UR #10: four nested sub-plans push the deepest entry to depth
    /// 4 — exceeding `MAX_SUB_PLAN_DEPTH=3`. The fourth entry MUST surface
    /// `MapperError::Internal` carrying `MAX_SUB_PLAN_DEPTH` in the message.
    /// Complement to the existing `execute_sub_plan_exceeds_max_depth_errors`
    /// test (kept here for the T-TEST-UR roll-up — wires the same shape but
    /// names the cap explicitly in the assertion).
    #[test]
    fn ur_execute_sub_plan_at_depth_4_exceeds_cap_errors() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let leaf_wrap = encode_wrap_eth_input(recipient_addr(), 1);
        // depth4 = sub-plan whose entry would land at depth 4 (cap-exceeding).
        let depth4 = encode_execute_sub_plan_input(vec![0x0b], vec![leaf_wrap]);
        let depth3 = encode_execute_sub_plan_input(vec![0x21], vec![depth4]);
        let depth2 = encode_execute_sub_plan_input(vec![0x21], vec![depth3]);
        let depth1 = encode_execute_sub_plan_input(vec![0x21], vec![depth2]);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x21],
            vec![depth1],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let err = super::execute(&ctx, &decoded, &bundle.emit).unwrap_err();
        let MapperError::Internal(inner) = &err else {
            panic!("expected MapperError::Internal, got {err:?}");
        };
        let msg = inner.to_string();
        assert!(
            msg.contains("MAX_SUB_PLAN_DEPTH"),
            "expected depth-cap error, got: {msg}"
        );
        assert!(
            msg.contains("EXECUTE_SUB_PLAN"),
            "expected EXECUTE_SUB_PLAN-specific message, got: {msg}"
        );
    }

    /// Recipient as policy-engine `Address` — mirror of `recipient_addr()`
    /// but returning the wrapper form transfer-shaped envelopes expect.
    fn recipient_addr_as_policy() -> Address {
        Address::from_str(&format!("0x{}", hex::encode(recipient_addr()))).unwrap()
    }

    /// Synthetic transfer-shaped `ActionEnvelope` for resolver stubs. The
    /// concrete field values are irrelevant — the only consumer is the test
    /// that asserts on `Action::Transfer` after the outer flatten.
    fn synthetic_transfer_envelope() -> ActionEnvelope {
        use policy_engine::action::common::{
            AmountConstraint, AssetRef, AssetRefWithAmountConstraint,
        };
        use policy_engine::action::misc::TransferAction;
        ActionEnvelope {
            category: Category::Misc,
            action: Action::Transfer(TransferAction {
                token: AssetRefWithAmountConstraint {
                    asset: AssetRef {
                        kind: AssetKind::Erc20,
                        address: Some(
                            Address::from_str("0x1111111111111111111111111111111111111111").unwrap(),
                        ),
                        token_id: None,
                        symbol: None,
                        decimals: None,
                    },
                    amount: AmountConstraint {
                        kind: AmountKind::Exact,
                        value: Some(DecimalString::from_str("1").unwrap()),
                    },
                },
                from: dummy_addr(0xCC),
                recipient: recipient_addr_as_policy(),
            }),
        }
    }

    /// 0x11 with `pm_calldata.len() < 4` MUST error before dispatch — no
    /// selector can be extracted from a sub-4-byte blob.
    #[test]
    fn v3_position_manager_call_with_short_calldata_errors() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        // Inner calldata = 2 bytes — too short for a selector. The outer
        // `(bytes data)` ABI still decodes cleanly; the short-circuit check
        // fires inside `execute_position_manager_step`.
        let step_input = encode_position_manager_step_input(vec![0x12, 0x34]);

        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.uniswap/universal-router/execute"),
            vec![0x11], // V3_POSITION_MANAGER_PERMIT
            vec![step_input],
        );

        let resolver = CapturingResolver::new(vec![]);
        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let err = super::execute(&ctx, &decoded, &bundle.emit).unwrap_err();
        let MapperError::Internal(inner) = &err else {
            panic!("expected MapperError::Internal, got {err:?}");
        };
        let msg = inner.to_string();
        assert!(
            msg.contains("too short for selector"),
            "expected short-calldata error, got: {msg}"
        );
        // Resolver MUST NOT be invoked — the gate fires before dispatch.
        assert!(
            resolver.calls().is_empty(),
            "resolver invoked despite short-calldata guard"
        );
    }
}
