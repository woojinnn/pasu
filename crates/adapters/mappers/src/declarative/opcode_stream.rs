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
//! 2. Dispatch through Tier B `subdecode::opcode_stream::dispatch` against
//!    `subdecode::protocols::universal_router::UNISWAP_UR_TABLE` → one
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
use abi_resolver::subdecode::protocols::aerodrome_ur::AERODROME_UR_MAIN_TABLE;
use abi_resolver::subdecode::protocols::pancake_infinity::PANCAKE_INFI_TABLE;
use abi_resolver::subdecode::protocols::pancake_ur::PANCAKE_UR_TABLE;
use abi_resolver::subdecode::protocols::universal_router::{
    extract_commands_and_inputs, v3_position_manager_address, v4_position_manager_address,
    UNISWAP_UR_TABLE,
};
use abi_resolver::subdecode::protocols::v4_router::{
    extract_actions_and_params, extract_modify_liquidities_actions_and_params, V4_ROUTER_MASK,
    V4_ROUTER_TABLE,
};
use abi_resolver::{CallMatchKey, DecodedCall, DecodedValue, DecoderId};
use alloy_dyn_abi::DynSolValue;
use alloy_primitives::U256;
use policy_engine::action::common::{
    AmountConstraint, AmountKind, AssetRef, AssetRefWithAmountConstraint, DecimalString,
};
use policy_engine::action::{Action, Address};
use policy_engine::ActionEnvelope;
use std::collections::BTreeMap;

use crate::mapper::{MapContext, MapperError};
use crate::protocols::pancake_universal_router::build_pancake_infi_swap_envelopes;
use crate::protocols::universal_router::build_v4_swap_envelopes;
use crate::protocols::universal_router::common as ur_common;

use super::single_emit;
use super::types::{EmitRule, PerOpcodeEmit, UnknownOpcodePolicy, ValueExpr};

/// Dispatcher id for the Uniswap Universal Router `execute(commands, inputs)`
/// opcode stream. Matches the value bundles declare under
/// `emit.dispatcher_id`.
pub const DISPATCHER_ID_UNIVERSAL_ROUTER: &str = "universal_router";

/// Dispatcher id for the Aerodrome / Velodrome `main`-lineage Universal
/// Router. Same `execute(bytes,bytes[],uint256)` entrypoint as Uniswap UR but
/// a distinct opcode table (`mask 0x3f`) — see
/// [`AERODROME_UR_MAIN_TABLE`].
pub const DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER: &str = "aerodrome_universal_router";

/// Dispatcher id for the Uniswap V4 PositionManager `modifyLiquidities` /
/// `modifyLiquiditiesWithoutUnlock` entrypoints (Phase 7B, TB-3). Their
/// payload is a `(bytes actions, bytes[] params)` pair dispatched against the
/// V4 `Actions` table ([`V4_ROUTER_TABLE`]) — the *same* opcode set the UR
/// `V4_SWAP` inner stream uses, but reached via a standalone selector rather
/// than nested inside UR's `execute`. A bundle targeting `modifyLiquidities`
/// declares this dispatcher id under `emit.dispatcher_id` together with
/// `mask = "0xff"` / `allow_revert_bit = "0x00"` (the V4 `Actions` byte has no
/// allow-revert flag, unlike UR command bytes).
pub const DISPATCHER_ID_V4_POSITION_MANAGER: &str = "v4_position_manager";

/// Dispatcher id for the PancakeSwap Infinity Universal Router
/// `execute(bytes,bytes[],uint256)` / `execute(bytes,bytes[])` entrypoints.
/// Outer shape is identical to Uniswap UR (selectors `0x3593564c` /
/// `0x24856bc3`), but the opcode table is [`PANCAKE_UR_TABLE`] (`mask 0x3f` —
/// 6-bit opcode + high-bit `allowRevert`). Inner `INFI_SWAP` (0x10)
/// cross-table dispatches against [`PANCAKE_INFI_TABLE`].
///
/// Unlike `universal_router`, Pancake UR's 0x11/0x12 are placeholders (the
/// Pancake `Dispatcher.sol` reverts), and 0x13/0x14 are
/// `INFI_CL/BIN_INITIALIZE_POOL` whose inner target is a *self-stored*
/// immutable pool manager — no cross-target callkey extraction.
pub const DISPATCHER_ID_PANCAKE_UNIVERSAL_ROUTER: &str = "pancake_universal_router";

/// Dispatcher id for the PancakeSwap Infinity CL / Bin PositionManager
/// `modifyLiquidities(bytes,uint256)` /
/// `modifyLiquiditiesWithoutLock(bytes,bytes[])` entrypoints. Outer payload
/// resolves to a `(bytes actions, bytes[] params)` pair dispatched against
/// the Pancake Infinity Actions table ([`PANCAKE_INFI_TABLE`] — 6-field
/// PoolKey + 6-field PathKey, different from Uniswap V4's 5-field variants).
/// Flat opcode set: no `EXECUTE_SUB_PLAN` / cross-table recursion exists in
/// the Infinity Actions space.
pub const DISPATCHER_ID_PANCAKE_INFINITY_POSITION_MANAGER: &str =
    "pancake_infinity_position_manager";

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

/// Tier B opcode for Pancake UR `INFI_SWAP` after `PANCAKE_UR_MASK` is
/// applied. Same byte value as Uniswap's `V4_SWAP` (`0x10`) — and the outer
/// payload `(bytes actions, bytes[] params)` shape is identical — but the
/// inner action stream dispatches against [`PANCAKE_INFI_TABLE`] rather than
/// the Uniswap [`V4_ROUTER_TABLE`]. The PoolKey/PathKey layouts diverge
/// (Pancake = 6 fields, V4 = 5 fields — D010), so the envelope builder is
/// separate.
const OPCODE_INFI_SWAP: u8 = 0x10;

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

/// The opcodes a dispatcher treats as recursion entrypoints (sub-plan / V4
/// cross-table / Pancake cross-table / cross-target PositionManager). Carried
/// per-dispatcher because each router lays its recursion opcodes out at
/// different byte values, and the *destination tables* differ even when the
/// byte value coincides (Uniswap UR `0x10` → V4_ROUTER_TABLE; Pancake UR
/// `0x10` → PANCAKE_INFI_TABLE).
///
/// All six fields name a masked opcode in the dispatcher's own
/// [`tier_b_opcode_stream::OpcodeTable`]. A dispatcher without recursion
/// special-casing (a flat opcode set) uses `DispatcherConfig::recursion =
/// None` instead, which routes every opcode through the plain
/// `per_opcode_emit` path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RecursionOpcodes {
    /// `EXECUTE_SUB_PLAN` — re-enters the same table (self-recursion).
    execute_sub_plan: u8,
    /// `V4_SWAP` — dispatches the inner stream against the Uniswap V4 router
    /// table (cross-table recursion).
    v4_swap: u8,
    /// `INFI_SWAP` — dispatches the inner stream against
    /// [`PANCAKE_INFI_TABLE`] (Pancake Infinity action set; 6-field PoolKey /
    /// PathKey). Set to [`OPCODE_NONE`] for dispatchers that do not embed a
    /// Pancake Infinity action stream.
    infi_swap: u8,
    /// `V3_POSITION_MANAGER_PERMIT` — inner calldata targets the per-chain V3
    /// NonfungiblePositionManager (cross-target recursion).
    v3_position_manager_permit: u8,
    /// `V3_POSITION_MANAGER_CALL` — same shape as the permit opcode.
    v3_position_manager_call: u8,
    /// `V4_POSITION_MANAGER_CALL` — inner calldata targets the per-chain V4
    /// PositionManager.
    v4_position_manager_call: u8,
}

/// One opcode-stream dispatcher: the bundle-declared `dispatcher_id`, the Tier
/// B [`tier_b_opcode_stream::OpcodeTable`] its command bytes decode against,
/// and (optionally) the recursion opcode layout for routers that nest.
///
/// Adding a dispatcher is a data change — append a [`DispatcherConfig`] to
/// [`DISPATCHERS`] — not a control-flow change. `execute` /
/// `dispatch_steps` thread the resolved `&DispatcherConfig` through and never
/// branch on `id`.
#[derive(Debug, Clone, Copy)]
struct DispatcherConfig {
    /// Value bundles declare under `emit.dispatcher_id`.
    id: &'static str,
    /// Tier B opcode table the command bytes dispatch against. Its `mask` /
    /// `allow_revert_bit` are the single source of truth checked against the
    /// bundle's declared values in `execute`.
    table: &'static tier_b_opcode_stream::OpcodeTable,
    /// Recursion opcode layout, or `None` for a flat opcode set with no
    /// recursion special-casing — every opcode then routes through
    /// `per_opcode_emit`.
    recursion: Option<RecursionOpcodes>,
}

/// Every opcode-stream dispatcher the declarative interpreter supports.
///
/// `universal_router` (Uniswap UR) carries the full recursion layout;
/// `aerodrome_universal_router` (Aerodrome / Velodrome `main`-lineage UR) is a
/// flat opcode set — its bundle maps each opcode directly through
/// `per_opcode_emit`, and opcodes Tier B's table knows but the bundle omits
/// (e.g. `0x10` V4_SWAP, `0x21` EXECUTE_SUB_PLAN) follow the bundle's
/// `unknown_opcode_policy` (a deliberate graceful degrade — Aerodrome
/// recursion handlers are a follow-up, not wired here).
/// Sentinel value for [`RecursionOpcodes`] fields that don't apply to a
/// dispatcher. Every Tier B `OpcodeTable` in this module masks to at most
/// `0x7f` (Uniswap) — so a masked `step.opcode` is always in `[0..=0x7f]`,
/// and `step.opcode == OPCODE_NONE` can never fire. Adding a dispatcher
/// whose table mask is `0xff` would require migrating the struct to
/// `Option<u8>`; the current set (Uniswap `0x7f`, Aerodrome `0x3f`, Pancake
/// `0x3f`) is safe.
const OPCODE_NONE: u8 = 0xff;

const DISPATCHERS: &[DispatcherConfig] = &[
    DispatcherConfig {
        id: DISPATCHER_ID_UNIVERSAL_ROUTER,
        table: &UNISWAP_UR_TABLE,
        recursion: Some(RecursionOpcodes {
            execute_sub_plan: OPCODE_EXECUTE_SUB_PLAN,
            v4_swap: OPCODE_V4_SWAP,
            // Uniswap UR has no Pancake Infinity action stream — its `0x10`
            // is V4_SWAP, not INFI_SWAP.
            infi_swap: OPCODE_NONE,
            v3_position_manager_permit: OPCODE_V3_POSITION_MANAGER_PERMIT,
            v3_position_manager_call: OPCODE_V3_POSITION_MANAGER_CALL,
            v4_position_manager_call: OPCODE_V4_POSITION_MANAGER_CALL,
        }),
    },
    DispatcherConfig {
        id: DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER,
        table: &AERODROME_UR_MAIN_TABLE,
        recursion: None,
    },
    DispatcherConfig {
        // PancakeSwap Infinity UR. Outer entrypoint same as Uniswap UR
        // (`execute(bytes,bytes[],...)`), but with a Pancake-specific opcode
        // table (`PANCAKE_UR_TABLE`, mask 0x3f).
        //
        // Recursion layout — P1 mini-round B.3:
        // - `execute_sub_plan: 0x21` — identical to Uniswap UR; re-enters
        //   `PANCAKE_UR_TABLE` for the inner `(commands, inputs)` pair.
        // - `v4_swap: OPCODE_NONE` — Pancake's 0x10 IS NOT V4_SWAP; routing
        //   it through `build_v4_swap_envelopes` would silently mis-decode
        //   the 6-field PoolKey (D010).
        // - `infi_swap: OPCODE_INFI_SWAP (0x10)` — D008 fix. Same outer
        //   payload `(bytes actions, bytes[] params)` shape as Uniswap V4_SWAP
        //   but the inner stream dispatches against `PANCAKE_INFI_TABLE` via
        //   the Pancake-specific [`build_pancake_infi_swap_envelopes`]
        //   builder (6-field PoolKey + 6-field PathKey).
        // - V3/V4 PM slots all `OPCODE_NONE` because Pancake's 0x11/0x12 are
        //   Commands.sol placeholders (the dispatcher reverts), and Pancake
        //   does NOT use 0x14 for V4 PM (it's `INFI_BIN_INITIALIZE_POOL`).
        id: DISPATCHER_ID_PANCAKE_UNIVERSAL_ROUTER,
        table: &PANCAKE_UR_TABLE,
        recursion: Some(RecursionOpcodes {
            execute_sub_plan: OPCODE_EXECUTE_SUB_PLAN, // 0x21 — same as Uniswap
            v4_swap: OPCODE_NONE,
            infi_swap: OPCODE_INFI_SWAP, // 0x10 — Pancake INFI_SWAP cross-table
            v3_position_manager_permit: OPCODE_NONE,
            v3_position_manager_call: OPCODE_NONE,
            v4_position_manager_call: OPCODE_NONE,
        }),
    },
    DispatcherConfig {
        // PancakeSwap Infinity CL/Bin PositionManager `modifyLiquidities` /
        // `modifyLiquiditiesWithoutLock`. Flat opcode set — no recursive /
        // cross-table actions in the Pancake Infinity Actions space (the
        // Infinity equivalents of EXECUTE_SUB_PLAN / V4_SWAP do not exist in
        // periphery's `Actions.sol`).
        id: DISPATCHER_ID_PANCAKE_INFINITY_POSITION_MANAGER,
        table: &PANCAKE_INFI_TABLE,
        recursion: None,
    },
];

/// Resolve a bundle-declared `dispatcher_id` to its [`DispatcherConfig`].
/// Linear scan — [`DISPATCHERS`] holds a handful of entries. Returns `None`
/// for an unrecognised id; `execute` maps that to `MapperError::Unsupported`.
fn resolve_dispatcher(id: &str) -> Option<&'static DispatcherConfig> {
    DISPATCHERS.iter().find(|d| d.id == id)
}

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

    // The Uniswap V4 PositionManager dispatcher is structurally distinct from
    // the UR-family opcode streams — its payload is a `modifyLiquidities`
    // `(actions, params)` pair, not `execute`'s `(commands, inputs)` stream —
    // so it routes through its own self-contained `execute_v4_position_manager`
    // rather than the `DispatcherConfig` table path below.
    if dispatcher_id == DISPATCHER_ID_V4_POSITION_MANAGER {
        let legacy_decoded = to_legacy_decoded(decoded)?;
        return execute_v4_position_manager(
            ctx,
            &legacy_decoded,
            mask,
            allow_revert_bit,
            per_opcode_emit,
            unknown_opcode_policy,
        );
    }

    // Resolve the UR-family dispatcher by id. An unrecognised id surfaces
    // `MapperError::Unsupported` with the same `opcode_stream_dispatch/<id>`
    // shape the pre-generalisation hard-coded check produced.
    let cfg = resolve_dispatcher(dispatcher_id).ok_or_else(|| {
        MapperError::Unsupported(format!("opcode_stream_dispatch/{dispatcher_id}"))
    })?;

    // Bundle's declared mask / allow_revert_bit must agree with the
    // dispatcher's Tier B table — otherwise the per-opcode keys we're about to
    // look up are computed against a different bit layout than Tier B
    // dispatched against. Detecting this here points authors at a bundle bug
    // rather than surfacing as silent unknown-opcode misses.
    let bundle_mask = parse_hex_byte(mask, "mask")?;
    let bundle_allow_revert_bit = parse_hex_byte(allow_revert_bit, "allow_revert_bit")?;
    if bundle_mask != cfg.table.mask {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "bundle mask {bundle_mask:#04x} disagrees with Tier B {} table mask {:#04x}",
            cfg.id,
            cfg.table.mask
        )));
    }
    if bundle_allow_revert_bit != cfg.table.allow_revert_bit {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "bundle allow_revert_bit {bundle_allow_revert_bit:#04x} disagrees with Tier B \
             {} table allow_revert_bit {:#04x}",
            cfg.id,
            cfg.table.allow_revert_bit
        )));
    }

    // Bridge from the new-pipeline `DecodedCall` back to the legacy form Tier B
    // exposes — `extract_commands_and_inputs` and the `OpcodeTable` schemas
    // were defined against `crate::decode::DecodedCall`.
    let legacy_decoded = to_legacy_decoded(decoded)?;
    let (commands, inputs) = extract_commands_and_inputs(&legacy_decoded).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "opcode_stream_dispatch: outer args do not match (bytes commands, bytes[] inputs) \
             — got function_signature {:?}",
            decoded.function_signature
        ))
    })?;

    let steps = tier_b_opcode_stream::dispatch(&commands, &inputs, cfg.table);
    dispatch_steps(ctx, cfg, &steps, per_opcode_emit, unknown_opcode_policy)
}

/// Uniswap V4 PositionManager dispatch path — `modifyLiquidities(bytes,
/// uint256)` / `modifyLiquiditiesWithoutUnlock(bytes,bytes[])` against the V4
/// `Actions` table ([`V4_ROUTER_TABLE`]).
///
/// Unlike the UR path this is *not* nested inside an `execute(...)` opcode
/// stream — `modifyLiquidities` is a standalone selector whose payload
/// resolves (across both overloads) to a `(bytes actions, bytes[] params)`
/// pair via Tier B's [`extract_modify_liquidities_actions_and_params`]. The
/// inner action stream is then dispatched against `V4_ROUTER_TABLE` and each
/// step is emitted via the bundle's `per_opcode_emit` map.
///
/// The V4 `Actions` byte carries no allow-revert flag (`mask = 0xff`,
/// `allow_revert_bit = 0`), so the bundle's declared values are checked
/// against `V4_ROUTER_TABLE` rather than the UR table. The V4 PM action set
/// (0x00–0x18) contains no self-recursive / cross-table opcode, so this path
/// does not reuse `dispatch_steps`' UR-specific 0x10/0x11/0x12/0x14/0x21
/// branches — those opcode values mean entirely different V4 actions
/// (`TAKE_PORTION`, `TAKE_PAIR`, `CLOSE_CURRENCY`, `SWEEP`) and must not be
/// re-dispatched. `dispatch_v4_pm_steps` walks the steps directly.
fn execute_v4_position_manager(
    ctx: &MapContext<'_>,
    legacy_decoded: &abi_resolver::decode::DecodedCall,
    mask: &str,
    allow_revert_bit: &str,
    per_opcode_emit: &BTreeMap<String, PerOpcodeEmit>,
    unknown_opcode_policy: UnknownOpcodePolicy,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    // Bundle's declared mask / allow_revert_bit must agree with Tier B's
    // V4_ROUTER_TABLE (mask 0xff, allow_revert_bit 0) — a mismatch means the
    // per-opcode keys would be computed against the wrong bit layout.
    let bundle_mask = parse_hex_byte(mask, "mask")?;
    let bundle_allow_revert_bit = parse_hex_byte(allow_revert_bit, "allow_revert_bit")?;
    if bundle_mask != V4_ROUTER_MASK {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "bundle mask {bundle_mask:#04x} disagrees with Tier B V4_ROUTER_TABLE mask {V4_ROUTER_MASK:#04x}"
        )));
    }
    if bundle_allow_revert_bit != V4_ROUTER_TABLE.allow_revert_bit {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "bundle allow_revert_bit {bundle_allow_revert_bit:#04x} disagrees with Tier B \
             V4_ROUTER_TABLE allow_revert_bit {:#04x}",
            V4_ROUTER_TABLE.allow_revert_bit
        )));
    }

    let (actions, params) = extract_modify_liquidities_actions_and_params(legacy_decoded)
        .ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "opcode_stream_dispatch/v4_position_manager: outer args do not match \
                 modifyLiquidities(bytes,uint256) or modifyLiquiditiesWithoutUnlock(bytes,bytes[]) \
                 — got function_signature {:?}",
                legacy_decoded.signature
            ))
        })?;

    let steps = tier_b_opcode_stream::dispatch(&actions, &params, &V4_ROUTER_TABLE);
    dispatch_v4_pm_steps(ctx, &steps, per_opcode_emit, unknown_opcode_policy)
}

/// Walk a V4 PositionManager `Actions` step list and emit envelopes via the
/// bundle's `per_opcode_emit` map.
///
/// This is the V4-PM counterpart of [`dispatch_steps`]. It deliberately omits
/// every UR-specific recursion branch (`EXECUTE_SUB_PLAN`, `V4_SWAP`, V3/V4
/// `POSITION_MANAGER_*`) because the V4 `Actions` opcode space (0x00–0x18)
/// has no recursive action — the byte values UR uses for those (0x10/0x11/
/// 0x12/0x14/0x21) are plain V4 actions here. Each step is bridged to a
/// synthetic `DecodedCall` and run through `single_emit`, identical to the
/// per-opcode tail of `dispatch_steps`.
fn dispatch_v4_pm_steps(
    ctx: &MapContext<'_>,
    steps: &[DecodedStep],
    per_opcode_emit: &BTreeMap<String, PerOpcodeEmit>,
    unknown_opcode_policy: UnknownOpcodePolicy,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    let mut envelopes = Vec::new();
    // F5 — TAKE-family steps (TAKE / TAKE_ALL / TAKE_PORTION / TAKE_PAIR /
    // SWEEP) carry the *output* side of the action stream. Collect them here
    // and attach to the decrease_liquidity envelope(s) after the walk, rather
    // than emitting them as standalone envelopes.
    let mut take_outputs: Vec<V4TakeOutput> = Vec::new();
    for step in steps {
        // F5 — intercept TAKE-family opcodes before the per_opcode_emit
        // lookup. A malformed TAKE step is dropped (lenient) so the primary
        // liquidity intent envelope still emits.
        if is_v4_take_opcode(step.opcode) {
            take_outputs.extend(decode_v4_take_outputs(ctx, step));
            continue;
        }
        let key = format!("0x{:02x}", step.opcode);
        let Some(rule) = per_opcode_emit.get(&key) else {
            match unknown_opcode_policy {
                UnknownOpcodePolicy::Deny => {
                    return Err(MapperError::Internal(anyhow::anyhow!(
                        "opcode_stream_dispatch/v4_position_manager: opcode {key} (step index {}, \
                         Tier B name {:?}) has no per_opcode_emit entry and \
                         unknown_opcode_policy=deny",
                        step.index,
                        step.name
                    )));
                }
                UnknownOpcodePolicy::Warn => {
                    eprintln!(
                        "[opcode_stream_dispatch/v4_position_manager] warn: opcode {key} (step \
                         index {}, Tier B name {:?}) has no per_opcode_emit entry — skipping \
                         (policy=warn)",
                        step.index, step.name
                    );
                    continue;
                }
                UnknownOpcodePolicy::IgnoreStep => continue,
            }
        };

        // Skip steps Tier B couldn't ABI-decode — surface as an error rather
        // than silently dropping so authors notice the schema mismatch.
        let step_args = step.args.clone().ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "opcode_stream_dispatch/v4_position_manager: opcode {key} (step index {}, Tier B \
                 name {:?}) has no decoded args — Tier B error: {:?}",
                step.index,
                step.name,
                step.error
            ))
        })?;

        let inner_args = step_args
            .into_iter()
            .map(convert_arg)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                MapperError::Internal(anyhow::anyhow!(
                    "opcode_stream_dispatch/v4_position_manager: opcode {key} step args bridge \
                     failed: {error}"
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
                "opcode_stream_dispatch/v4_position_manager: opcode {key} (step index {}, Tier B \
                 name {:?}) emit failed: {error}",
                step.index,
                step.name
            ))
        })?;
        envelopes.push(envelope);
    }

    // F5 — attach the collected TAKE outputs to every decrease_liquidity
    // envelope this stream produced (`outputTokens` + `recipient`).
    attach_take_outputs_to_decrease(&mut envelopes, take_outputs);

    Ok(envelopes)
}

/// V4 PositionManager "settle the open delta outward" opcodes — the action
/// values that carry a withdrawn currency and a destination: `TAKE` (0x0e),
/// `TAKE_ALL` (0x0f), `TAKE_PORTION` (0x10), `TAKE_PAIR` (0x11), `SWEEP`
/// (0x14). Signatures live in `abi_resolver::subdecode::protocols::v4_router`.
fn is_v4_take_opcode(opcode: u8) -> bool {
    matches!(opcode, 0x0e | 0x0f | 0x10 | 0x11 | 0x14)
}

/// One "currency leaves the V4 PositionManager" effect extracted from a
/// TAKE-family action — the output side of a V4 PM action stream
/// (`VERIFICATION_UNISWAP_REALTX` finding F5).
struct V4TakeOutput {
    asset: AssetRef,
    amount: AmountConstraint,
    recipient: Address,
}

/// Build a synthetic per-step `DecodedCall` from a V4 PM `DecodedStep` — the
/// TAKE-family counterpart of the per-opcode-emit synthesis in
/// `dispatch_v4_pm_steps`. `None` when Tier B could not ABI-decode the step or
/// an arg bridge fails: lenient, a malformed TAKE is dropped (never fatal) so
/// the primary liquidity envelope still emits.
fn v4_step_decoded_call(step: &DecodedStep) -> Option<DecodedCall> {
    let inner_args = step
        .args
        .clone()?
        .into_iter()
        .map(convert_arg)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some(DecodedCall {
        decoder_id: DecoderId::new(format!("opcode_stream::{}", step.name)),
        function_signature: format!("{}({})", step.name, inner_args_signature(&inner_args)),
        args: inner_args,
        nested: Vec::new(),
    })
}

/// Decode a TAKE-family V4 PM step into its [`V4TakeOutput`](s). Positional
/// arg layout per `V4_ROUTER_TABLE`:
/// * `0x0e TAKE`         — `(currency, recipient, amount)`
/// * `0x0f TAKE_ALL`     — `(currency, minAmount)`; recipient is the unlock
///   caller, mapped to `ctx.from`
/// * `0x10 TAKE_PORTION` — `(currency, recipient, bips)`
/// * `0x11 TAKE_PAIR`    — `(currency0, currency1, recipient)` — two outputs
/// * `0x14 SWEEP`        — `(currency, to)`
///
/// `currency` runs through `token_asset_ref` (the `0x0` native sentinel
/// becomes `native`, consistent with F2); `recipient` runs through
/// `map_recipient` (UR/V4 `0x..01`/`0x..02` sentinels, consistent with F3).
fn decode_v4_take_outputs(ctx: &MapContext<'_>, step: &DecodedStep) -> Vec<V4TakeOutput> {
    let Some(decoded) = v4_step_decoded_call(step) else {
        return Vec::new();
    };
    let addr = |i: usize| -> Option<Address> {
        match decoded.args.get(i).map(|a| &a.value) {
            Some(DecodedValue::Address(a)) => Some(a.clone()),
            _ => None,
        }
    };
    let uint = |i: usize| -> Option<U256> {
        match decoded.args.get(i).map(|a| &a.value) {
            Some(DecodedValue::Uint(u)) => Some(*u),
            _ => None,
        }
    };
    let constrained = |kind: AmountKind, value: U256| AmountConstraint {
        kind,
        value: Some(
            DecimalString::from_str(&value.to_string())
                .expect("U256 decimal string is always a valid DecimalString"),
        ),
    };
    // TAKE_PAIR / SWEEP drain the whole open delta — the amount is not in
    // calldata, so the constraint carries no value.
    let unbounded = AmountConstraint {
        kind: AmountKind::Unknown,
        value: None,
    };

    match step.opcode {
        0x0e => match (addr(0), addr(1), uint(2)) {
            (Some(currency), Some(recipient), Some(amount)) => vec![V4TakeOutput {
                asset: ur_common::token_asset_ref(ctx, &currency),
                amount: constrained(AmountKind::Exact, amount),
                recipient: ur_common::map_recipient(ctx, recipient),
            }],
            _ => Vec::new(),
        },
        0x0f => match (addr(0), uint(1)) {
            (Some(currency), Some(min_amount)) => vec![V4TakeOutput {
                asset: ur_common::token_asset_ref(ctx, &currency),
                amount: constrained(AmountKind::Min, min_amount),
                recipient: ctx.from.clone(),
            }],
            _ => Vec::new(),
        },
        0x10 => match (addr(0), addr(1), uint(2)) {
            (Some(currency), Some(recipient), Some(bips)) => vec![V4TakeOutput {
                asset: ur_common::token_asset_ref(ctx, &currency),
                amount: constrained(AmountKind::Portion, bips),
                recipient: ur_common::map_recipient(ctx, recipient),
            }],
            _ => Vec::new(),
        },
        0x11 => match (addr(0), addr(1), addr(2)) {
            (Some(currency0), Some(currency1), Some(recipient)) => {
                let recipient = ur_common::map_recipient(ctx, recipient);
                vec![
                    V4TakeOutput {
                        asset: ur_common::token_asset_ref(ctx, &currency0),
                        amount: unbounded.clone(),
                        recipient: recipient.clone(),
                    },
                    V4TakeOutput {
                        asset: ur_common::token_asset_ref(ctx, &currency1),
                        amount: unbounded,
                        recipient,
                    },
                ]
            }
            _ => Vec::new(),
        },
        0x14 => match (addr(0), addr(1)) {
            (Some(currency), Some(to)) => vec![V4TakeOutput {
                asset: ur_common::token_asset_ref(ctx, &currency),
                amount: unbounded,
                recipient: ur_common::map_recipient(ctx, to),
            }],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Attach collected V4 TAKE outputs to every `decrease_liquidity` envelope in
/// the stream — `VERIFICATION_UNISWAP_REALTX` finding F5 (the bundle hardcodes
/// `outputTokens: []`).
///
/// V4 PM flash-accounting nets every action's delta across the whole unlock,
/// so a TAKE cannot be statically attributed to one specific DECREASE — the
/// stream-level output set is attached to each decrease_liquidity envelope
/// (the recipient set is exact; per-position amounts are not statically
/// knowable). A stream with TAKE steps but no decrease envelope (e.g.
/// burn-only) drops the collected outputs — `BurnLiquidityNftAction` has no
/// `outputs` field, a separate follow-up out of F5 scope.
fn attach_take_outputs_to_decrease(envelopes: &mut [ActionEnvelope], outputs: Vec<V4TakeOutput>) {
    if outputs.is_empty() {
        return;
    }
    // The common case — a single trailing TAKE_PAIR — has one recipient;
    // surface it on `DecreaseLiquidityAction.recipient`. A mixed set (multiple
    // distinct recipients) leaves it `None`.
    let recipient = {
        let first = &outputs[0].recipient;
        outputs
            .iter()
            .all(|o| &o.recipient == first)
            .then(|| first.clone())
    };
    let output_assets: Vec<AssetRefWithAmountConstraint> = outputs
        .into_iter()
        .map(|o| AssetRefWithAmountConstraint {
            asset: o.asset,
            amount: o.amount,
        })
        .collect();
    for envelope in envelopes.iter_mut() {
        if let Action::DecreaseLiquidity(decrease) = &mut envelope.action {
            decrease.outputs = output_assets.clone();
            decrease.recipient = recipient.clone();
        }
    }
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
    cfg: &'static DispatcherConfig,
    steps: &[DecodedStep],
    per_opcode_emit: &BTreeMap<String, PerOpcodeEmit>,
    unknown_opcode_policy: UnknownOpcodePolicy,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    let mut envelopes = Vec::new();
    for step in steps {
        // Recursion special-casing only applies to dispatchers that declare a
        // recursion opcode layout. A flat dispatcher (`cfg.recursion == None`,
        // e.g. `aerodrome_universal_router`) skips this entire block — every
        // opcode then falls through to the `per_opcode_emit` path below, and
        // recursion-shaped opcodes the bundle omits follow
        // `unknown_opcode_policy`. This is the anti-misfire guard: a non-UR
        // dispatcher must never reach a UR-specific recursion handler.
        if let Some(rec) = cfg.recursion.as_ref() {
            // `EXECUTE_SUB_PLAN` carries `(bytes commands, bytes[] inputs)`
            // with the same shape as the outer entrypoint — re-dispatch the
            // inner pair through the same opcode table so nested swap / wrap /
            // sweep steps reach their per-opcode rules. Depth-bounded via
            // `MapContext`.
            if step.opcode == rec.execute_sub_plan {
                let sub_envelopes =
                    execute_sub_plan_step(ctx, cfg, step, per_opcode_emit, unknown_opcode_policy)?;
                envelopes.extend(sub_envelopes);
                continue;
            }

            // `V4_SWAP` carries `(bytes actions, bytes[] params)` — the inner
            // stream is dispatched through V4_ROUTER_TABLE (a different table
            // than UR's), so this is cross-table recursion rather than the
            // self-recursion EXECUTE_SUB_PLAN performs. Depth-bounded by the
            // same `MAX_SUB_PLAN_DEPTH` cap. Per the PoC scope (option D), the
            // V4 inner step list is decoded but no envelopes are emitted — the
            // V4 action → envelope mapping is a follow-up (T-B6, V4 PM
            // builders).
            if step.opcode == rec.v4_swap {
                let v4_envelopes = execute_v4_swap_step(ctx, step)?;
                envelopes.extend(v4_envelopes);
                continue;
            }

            // `INFI_SWAP` (Pancake UR 0x10) carries the same outer payload
            // shape as Uniswap V4_SWAP (`(bytes actions, bytes[] params)`),
            // but the inner stream dispatches against PANCAKE_INFI_TABLE
            // (6-field PoolKey / PathKey, D010) via
            // `build_pancake_infi_swap_envelopes` — a Pancake-specific
            // builder kept separate from `build_v4_swap_envelopes` so the
            // V4-hardcoded 5-field assumptions cannot silently mis-decode a
            // Pancake swap. D008 root-cause fix (P1 mini-round B.3).
            // Depth-bounded by the same `MAX_SUB_PLAN_DEPTH` cap.
            if step.opcode == rec.infi_swap {
                let infi_envelopes = execute_pancake_infi_swap_step(ctx, step)?;
                envelopes.extend(infi_envelopes);
                continue;
            }

            // `V3_POSITION_MANAGER_PERMIT`, `V3_POSITION_MANAGER_CALL`, and
            // `V4_POSITION_MANAGER_CALL` each carry a single `(bytes data)`
            // arg — the complete calldata for the per-chain NPM / V4 PM.
            // Dispatch this calldata back through `ctx.resolver` (cross-target
            // recursion: the inner call goes to a different contract than the
            // parent UR). Depth-bounded by the same `MAX_SUB_PLAN_DEPTH` cap;
            // the per-chain address lookup uses Tier B's
            // `v3_position_manager_address` / `v4_position_manager_address`.
            if step.opcode == rec.v3_position_manager_permit
                || step.opcode == rec.v3_position_manager_call
                || step.opcode == rec.v4_position_manager_call
            {
                let pm_envelopes = execute_position_manager_step(ctx, step)?;
                envelopes.extend(pm_envelopes);
                continue;
            }
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
    cfg: &'static DispatcherConfig,
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

    let inner_steps = tier_b_opcode_stream::dispatch(&inner_commands, &inner_inputs, cfg.table);

    // `MapContext::child` requires `parent_calldata: &[u8]` borrowed for the
    // child context's lifetime. The inner commands bytes serve that role —
    // they're the calldata-equivalent for the recursive level. Keeping them
    // bound in this stack frame ensures the borrow outlives `child_ctx`.
    let child_ctx = ctx.child(ctx.to, &inner_commands);
    dispatch_steps(
        &child_ctx,
        cfg,
        &inner_steps,
        per_opcode_emit,
        unknown_opcode_policy,
    )
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
/// Phase 7B (TB-2): the dispatched V4 action step list is handed to the
/// shared [`build_v4_swap_envelopes`] two-pass builder — the same builder the
/// imperative UR V4_SWAP mapper (`protocols::universal_router::v4_swap`) uses.
/// It emits one `Action::Swap` envelope per V4 swap action and patches each
/// recipient from the trailing `TAKE` action (V4 swap params carry no
/// recipient — output is staged as a flash-accounting delta and `TAKE` drains
/// it). A V4_SWAP block with no swap actions (e.g. a settle/take-only stream)
/// emits zero envelopes — a clean result, not a fault. The recursion depth is
/// still bounded so a maliciously deep `EXECUTE_SUB_PLAN` chain ending in a
/// V4_SWAP cannot bypass `MAX_SUB_PLAN_DEPTH`.
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

    // Dispatch the inner action stream against the V4 router table, then run
    // the shared two-pass swap-envelope builder. A malformed swap action's
    // params surface as a `MapperError` so the orchestrator falls back to the
    // static path rather than silently dropping a swap.
    let inner_steps = tier_b_opcode_stream::dispatch(&actions, &params, &V4_ROUTER_TABLE);
    build_v4_swap_envelopes(ctx, &inner_steps)
}

/// Handle a single `INFI_SWAP` (Pancake UR 0x10) step — Pancake-specific
/// cross-table recursive dispatch.
///
/// The step's Tier B args carry `(bytes actions, bytes[] params)` (see
/// `PANCAKE_UR_TABLE` entry for 0x10). We pull the inner pair out via
/// Pancake-side `extract_actions_and_params`, then re-dispatch through Tier
/// B's `opcode_stream::dispatch` against **`PANCAKE_INFI_TABLE`** (Pancake
/// Infinity Actions: CL_SWAP_EXACT_IN_SINGLE, SETTLE, TAKE, BIN_SWAP_*, …).
///
/// The dispatched step list is handed to
/// [`build_pancake_infi_swap_envelopes`] — the Pancake-specific two-pass
/// builder accounts for two protocol divergences vs Uniswap V4 (catalogued
/// under D010):
///   * `PoolKey` is **6 fields** (`fee` at index 4), not V4's 5 (`fee` at
///     index 2).
///   * `PathKey` is **6 fields** (adds `poolManager` + `parameters`).
///
/// Running a Pancake `INFI_SWAP` step through the V4 builder would silently
/// mis-bind the fee slot and discard half the path-key metadata; the two
/// builders therefore stay separate even though the outer dispatch logic is
/// structurally identical.
///
/// Settle/take-only streams (no swap action) emit zero envelopes — a clean
/// result, not a fault. Recursion depth is bounded by `MAX_SUB_PLAN_DEPTH`
/// so a maliciously deep `EXECUTE_SUB_PLAN` chain ending in an `INFI_SWAP`
/// cannot bypass the cap.
fn execute_pancake_infi_swap_step(
    ctx: &MapContext<'_>,
    step: &DecodedStep,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    // Depth bound shared with EXECUTE_SUB_PLAN / V4_SWAP / cross-target PM.
    if ctx.depth >= MAX_SUB_PLAN_DEPTH {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "INFI_SWAP exceeded MAX_SUB_PLAN_DEPTH={MAX_SUB_PLAN_DEPTH} \
             at step index {} (current depth {})",
            step.index,
            ctx.depth
        )));
    }

    // Pull `(bytes actions, bytes[] params)` out of the step's decoded args.
    // PANCAKE_UR_TABLE declares INFI_SWAP with the same `(bytes actions,
    // bytes[] params)` signature as Uniswap V4_SWAP, so the structural
    // extractor is the Pancake-side mirror of V4's helper. `None` here
    // surfaces as an internal error so authors notice the schema mismatch
    // instead of silently dropping an INFI_SWAP block.
    let (actions, params) =
        abi_resolver::subdecode::protocols::pancake_infinity::extract_actions_and_params(step)
            .ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "INFI_SWAP step index {} args do not match (bytes actions, bytes[] params) \
                     — Tier B name {:?}, error {:?}",
                    step.index,
                    step.name,
                    step.error
                ))
            })?;

    // Dispatch the inner action stream against the Pancake Infinity table,
    // then run the Pancake-specific two-pass swap-envelope builder.
    let inner_steps = tier_b_opcode_stream::dispatch(&actions, &params, &PANCAKE_INFI_TABLE);
    build_pancake_infi_swap_envelopes(ctx, &inner_steps)
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

    // Universal Router Dispatcher.sol forwards `inputs[i]` raw to
    // `address(V3_POSITION_MANAGER).call(inputs)` /
    // `address(V4_POSITION_MANAGER).call(inputs)` for opcodes 0x11/0x12/0x14
    // — there is no `abi.encode((bytes,))` wrapper. The Tier B `(bytes data)`
    // signature in `UNISWAP_UR_TABLE` is therefore a misleading abstraction:
    // for well-formed inputs `step.args` decodes only when the inner blob
    // happens to look like an ABI-encoded bytes tuple, but real on-chain
    // calldata is just `selector || args`. Always read the raw bytes off
    // `step.raw_input` so the planner / dispatch agree with Dispatcher.sol.
    let pm_calldata: Vec<u8> = step.raw_input.clone();

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
    let pm_addr = Address::from_str(&format!("0x{}", hex::encode(pm_alloy_addr))).map_err(|e| {
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
        other => Err(format!("field `{field}` expected Bytes, got {other:?}")),
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
            return Err(format!("field `{field}[{i}]` expected Bytes, got {item:?}"));
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

/// Bridge a new-pipeline `decoder::DecodedCall` to its Tier B legacy form
/// and pull `(commands, inputs)` via [`extract_commands_and_inputs`].
///
/// `None` when the outer args don't structurally match UR `execute` —
/// caller can treat that as "this is not a UR outer call".
///
/// Pub so `policy-engine-wasm` (`declarative_plan_children_json`) can reuse
/// the same legacy-bridge path Tier B's opcode-stream dispatcher uses for
/// cross-target child planning — both must agree on the (commands, inputs)
/// extraction or the planner and the runtime will disagree on which child
/// callkeys exist.
#[allow(clippy::type_complexity)]
pub fn extract_ur_commands_and_inputs(
    decoded: &DecodedCall,
) -> Result<Option<(Vec<u8>, Vec<Vec<u8>>)>, MapperError> {
    let legacy = to_legacy_decoded(decoded)?;
    Ok(extract_commands_and_inputs(&legacy))
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
        "../../../../../registry/manifests/uniswap/universal-router/execute-v2@1.0.0.json"
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
        alloy_primitives::Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap()
    }

    fn token_out() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap()
    }

    fn recipient_addr() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0x4444444444444444444444444444444444444444").unwrap()
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
        let func = Function::parse("step(address,uint256,uint256,bytes,bool)").unwrap();
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
        let suffix = format!("{label:02x}");
        Address::from_str(&format!("0x{}{}", "0".repeat(38), suffix)).unwrap()
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
            action
                .input_token
                .asset
                .address
                .as_ref()
                .map(|a| a.to_string()),
            Some(format!("0x{}", hex::encode(token_in()))),
        );
        assert_eq!(action.input_token.amount.kind, AmountKind::Exact);
        assert_eq!(
            action
                .input_token
                .amount
                .value
                .as_ref()
                .map(|v| v.to_string()),
            Some("1000000".to_owned())
        );
        assert_eq!(
            action
                .output_token
                .asset
                .address
                .as_ref()
                .map(|a| a.to_string()),
            Some(format!("0x{}", hex::encode(token_out()))),
        );
        assert_eq!(action.output_token.amount.kind, AmountKind::Min);
        assert_eq!(
            action
                .output_token
                .amount
                .value
                .as_ref()
                .map(|v| v.to_string()),
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
        assert_eq!(
            envelopes.len(),
            3,
            "expected 3 envelopes, got {envelopes:?}"
        );

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
        let balance_check = encode_balance_check_erc20_input(recipient_addr(), token_out(), 1);
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
        let input =
            encode_v3_swap_exact_in_input(recipient_addr(), 1, 1, v3_packed_path_usdc_weth(), true);
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
    fn encode_wrap_eth_input(recipient: alloy_primitives::Address, amount_min: u128) -> Vec<u8> {
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
            DynSolValue::Array(inner_inputs.into_iter().map(DynSolValue::Bytes).collect()),
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
        assert_eq!(
            envelopes.len(),
            1,
            "expected 1 wrap envelope, got {envelopes:?}"
        );
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
        assert_eq!(
            envelopes.len(),
            1,
            "expected 1 wrap envelope at depth 2, got {envelopes:?}"
        );
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
        let func = Function::parse("step(((address,uint160,uint48,uint48),address,uint256),bytes)")
            .unwrap();
        let values = vec![permit_single, DynSolValue::Bytes(signature)];
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    /// Encode UR `V4_SWAP` (0x10) input `(bytes actions, bytes[] params)`.
    /// Mirrors `encode_execute_sub_plan_input` — the outer shape is identical
    /// to a sub-plan, but the inner `actions` byte stream is dispatched
    /// against `V4_ROUTER_TABLE` instead of `UNISWAP_UR_TABLE`.
    fn encode_v4_swap_input(inner_actions: Vec<u8>, inner_params: Vec<Vec<u8>>) -> Vec<u8> {
        let func = Function::parse("step(bytes,bytes[])").unwrap();
        let values = vec![
            DynSolValue::Bytes(inner_actions),
            DynSolValue::Array(inner_params.into_iter().map(DynSolValue::Bytes).collect()),
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
    #[allow(clippy::too_many_arguments)]
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
        let steps =
            tier_b_opcode_stream::dispatch(&[OPCODE_V4_SWAP], &[v4_swap], &UNISWAP_UR_TABLE);
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
    /// raising and — since Phase 7B (TB-2) — emit exactly one `Action::Swap`
    /// envelope via the shared two-pass V4 swap builder.
    #[test]
    fn v4_swap_inner_dispatch_emits_swap_envelope() {
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
        // TB-2: the single V4 SWAP_EXACT_IN_SINGLE action emits one Swap
        // envelope. No TAKE in the stream → recipient defaults to ctx.from.
        assert_eq!(envelopes.len(), 1, "V4_SWAP must emit one Swap envelope");
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap, got {:?}", envelopes[0].action);
        };
        assert_eq!(envelopes[0].category, Category::Dex);
        assert_eq!(s.swap_mode, SwapMode::ExactIn);
        assert_eq!(
            s.recipient, from,
            "no TAKE → recipient defaults to ctx.from"
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
        let v4_steps = tier_b_opcode_stream::dispatch(&[0x06], &[v4_inner_check], &V4_ROUTER_TABLE);
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

    /// UR position-manager step input — raw inner PM calldata (selector + args).
    ///
    /// Universal Router's Dispatcher.sol forwards `inputs[i]` raw to
    /// `address(V3_POSITION_MANAGER).call(inputs)` /
    /// `address(V4_POSITION_MANAGER).call(inputs)` for opcodes 0x11/0x12/0x14.
    /// There is no `abi.encode((bytes,))` wrapper — `inputs[i]` IS the inner
    /// calldata. This helper used to wrap in `step(bytes)` to mirror the
    /// (misleading) `(bytes data)` signature in `UNISWAP_UR_TABLE`; the
    /// post-cross-target-fix runtime reads `step.raw_input` directly, so the
    /// wrapper is removed to match Dispatcher.sol's on-chain behaviour.
    fn encode_position_manager_step_input(inner_calldata: Vec<u8>) -> Vec<u8> {
        inner_calldata
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
        assert!(
            envelopes.is_empty(),
            "expected empty envelopes from stub, got {envelopes:?}"
        );

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
        assert!(
            envelopes.is_empty(),
            "empty commands MUST yield no envelopes"
        );
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
    ///   * `0x10` → one Swap envelope via the V4 swap builder (TB-2 — the
    ///     V4 inner stream is a single SWAP_EXACT_IN_SINGLE)
    ///   * `0x12` → resolver-stub return; we wire a `CapturingResolver` that
    ///     returns one transfer-shaped envelope so the outer flatten produces
    ///     3 envelopes total.
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
        // without ambiguity vs the swap-emitted `Action::Swap`.
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
            3,
            "expected 3 envelopes (1 V3 swap + 1 V4 swap + 1 V3 PM resolver), got {envelopes:?}"
        );
        // Order MUST follow command stream — V3 swap, then V4 swap, then
        // V3 PM resolver.
        assert!(
            matches!(envelopes[0].action, Action::Swap(_)),
            "envelopes[0] MUST be Swap from V3_SWAP_EXACT_IN"
        );
        assert!(
            matches!(envelopes[1].action, Action::Swap(_)),
            "envelopes[1] MUST be Swap from V4_SWAP"
        );
        assert!(
            matches!(envelopes[2].action, Action::Transfer(_)),
            "envelopes[2] MUST be Transfer from PM resolver"
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
        assert_eq!(
            envelopes.len(),
            1,
            "depth-1 sub-plan with WRAP_ETH MUST yield 1 envelope"
        );
        assert!(matches!(envelopes[0].action, Action::Wrap(_)));
    }

    /// T-TEST-UR #9: `EXECUTE_SUB_PLAN` at the cap (depth 3). Three nested
    /// sub-plans + inner `WRAP_ETH` — innermost level executes at depth 3,
    /// which is exactly `MAX_SUB_PLAN_DEPTH`. The guard rejects on
    /// `ctx.depth >= MAX_SUB_PLAN_DEPTH` *entering* a sub-plan, so an entry
    /// that lands at depth 3 succeeds (the guard fires at depth 4 entry).
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
                            Address::from_str("0x1111111111111111111111111111111111111111")
                                .unwrap(),
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

    // ─────────────────────────────────────────────────────────────────────
    // Phase 2 B1 — multi-dispatcher generalisation (Aerodrome UR)
    // ─────────────────────────────────────────────────────────────────────

    /// Override the bundle's `dispatcher_id` and `mask` in-place. The UR
    /// fixture declares the Uniswap dispatcher (`universal_router`, mask
    /// `0x7f`); the Aerodrome-dispatcher tests reuse the same fixture with
    /// these two fields swapped.
    fn override_dispatcher(bundle: &mut AdapterFunctionBundle, id: &str, mask: &str) {
        if let EmitRule::OpcodeStreamDispatch {
            dispatcher_id,
            mask: mask_field,
            ..
        } = &mut bundle.emit
        {
            *dispatcher_id = id.to_owned();
            *mask_field = mask.to_owned();
        } else {
            panic!("bundle.emit must be OpcodeStreamDispatch for UR fixture");
        }
    }

    /// Retarget the UR fixture's `0x0b` WRAP_ETH rule from the Uniswap UR
    /// table's arg name (`amountMin`) to the Aerodrome UR table's
    /// (`amount`). The two routers expose WRAP_ETH with the same arity but
    /// different field names — `(address recipient, uint256 amountMin)` vs
    /// `(address recipient, uint256 amount)` — so a faithful Aerodrome rule
    /// references `$.args.amount`. Applied only by the dispatch-routing test;
    /// the real Aerodrome bundle (Phase 3 A1) will ship this directly.
    fn retarget_wrap_eth_amount_field(bundle: &mut AdapterFunctionBundle) {
        let EmitRule::OpcodeStreamDispatch {
            per_opcode_emit, ..
        } = &mut bundle.emit
        else {
            panic!("bundle.emit must be OpcodeStreamDispatch for UR fixture");
        };
        let wrap = per_opcode_emit
            .get_mut("0x0b")
            .expect("UR fixture must declare a 0x0b WRAP_ETH rule");
        for field in ["nativeAsset.amount.value", "wrappedAsset.amount.value"] {
            if let Some(ValueExpr::FromArg { from, .. }) = wrap.fields.get_mut(field) {
                *from = "$.args.amount".to_owned();
            } else {
                panic!("0x0b rule field `{field}` must be a FromArg expr");
            }
        }
    }

    /// `resolve_dispatcher` returns the Uniswap UR config for the
    /// `universal_router` id, and that config carries a recursion layout
    /// (UR nests via EXECUTE_SUB_PLAN / V4_SWAP / PM opcodes).
    #[test]
    fn resolve_dispatcher_returns_universal_router_config() {
        let cfg = resolve_dispatcher(DISPATCHER_ID_UNIVERSAL_ROUTER)
            .expect("universal_router must resolve");
        assert_eq!(cfg.id, DISPATCHER_ID_UNIVERSAL_ROUTER);
        assert!(
            cfg.recursion.is_some(),
            "Uniswap UR config MUST carry a recursion opcode layout"
        );
        // Sanity: the recursion opcodes match the module constants.
        let rec = cfg.recursion.as_ref().unwrap();
        assert_eq!(rec.execute_sub_plan, OPCODE_EXECUTE_SUB_PLAN);
        assert_eq!(rec.v4_swap, OPCODE_V4_SWAP);
        assert_eq!(
            rec.v3_position_manager_permit,
            OPCODE_V3_POSITION_MANAGER_PERMIT
        );
        assert_eq!(
            rec.v3_position_manager_call,
            OPCODE_V3_POSITION_MANAGER_CALL
        );
        assert_eq!(
            rec.v4_position_manager_call,
            OPCODE_V4_POSITION_MANAGER_CALL
        );
    }

    /// `resolve_dispatcher` returns the Aerodrome UR config for the
    /// `aerodrome_universal_router` id; that config is a flat opcode set
    /// (`recursion == None`) and points at `AERODROME_UR_MAIN_TABLE`.
    #[test]
    fn resolve_dispatcher_returns_aerodrome_config() {
        let cfg = resolve_dispatcher(DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER)
            .expect("aerodrome_universal_router must resolve");
        assert_eq!(cfg.id, DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER);
        assert!(
            cfg.recursion.is_none(),
            "Aerodrome UR config MUST be a flat opcode set (no recursion special-casing)"
        );
        // The config's table MUST be the exact `AERODROME_UR_MAIN_TABLE`
        // static — not a copy — so Tier B's mask / opcode set is the single
        // source of truth.
        assert!(
            std::ptr::eq(cfg.table, &AERODROME_UR_MAIN_TABLE),
            "Aerodrome config table MUST be the AERODROME_UR_MAIN_TABLE static"
        );
    }

    /// An unrecognised `dispatcher_id` resolves to `None` — `execute` maps
    /// that to `MapperError::Unsupported`.
    #[test]
    fn resolve_dispatcher_unknown_id_returns_none() {
        assert!(
            resolve_dispatcher("nonexistent_router").is_none(),
            "an unknown dispatcher_id MUST resolve to None"
        );
    }

    /// An `aerodrome_universal_router` bundle (mask `0x3f`) carrying a single
    /// `WRAP_ETH` (0x0b) opcode dispatches through `AERODROME_UR_MAIN_TABLE`
    /// and emits one wrap envelope. The UR fixture is reused with the
    /// dispatcher / mask swapped and the `0x0b` rule retargeted to the
    /// Aerodrome WRAP_ETH arg name (`amount` vs Uniswap's `amountMin`) — what
    /// this test pins is that the Aerodrome dispatcher routes opcode decoding
    /// through `AERODROME_UR_MAIN_TABLE` (mask `0x3f`), not Uniswap's table.
    #[test]
    fn aerodrome_dispatch_routes_to_aerodrome_table() {
        let mut bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();
        override_dispatcher(
            &mut bundle,
            DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER,
            "0x3f",
        );
        retarget_wrap_eth_amount_field(&mut bundle);

        let wrap_input = encode_wrap_eth_input(recipient_addr(), 1_000_000);
        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.aerodrome/universal-router/execute"),
            vec![0x0b],
            vec![wrap_input],
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
            "Aerodrome WRAP_ETH MUST yield exactly one envelope, got {envelopes:?}"
        );
        assert_eq!(envelopes[0].category, Category::Misc);
        assert!(
            matches!(envelopes[0].action, Action::Wrap(_)),
            "envelope MUST be Wrap, got {:?}",
            envelopes[0].action
        );
    }

    /// An `aerodrome_universal_router` bundle that declares the wrong mask
    /// (`0x7f` — Uniswap's value, not Aerodrome's `0x3f`) MUST surface
    /// `MapperError::Internal`: the bundle's declared mask is checked against
    /// the resolved dispatcher's Tier B table, and `AERODROME_UR_MAIN_TABLE`
    /// uses `0x3f`.
    #[test]
    fn aerodrome_mask_mismatch_errors() {
        let mut bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();
        // Aerodrome dispatcher but Uniswap's 0x7f mask — a bundle-author bug.
        override_dispatcher(
            &mut bundle,
            DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER,
            "0x7f",
        );

        let wrap_input = encode_wrap_eth_input(recipient_addr(), 1_000_000);
        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.aerodrome/universal-router/execute"),
            vec![0x0b],
            vec![wrap_input],
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
            msg.contains("disagrees"),
            "expected mask-disagreement error, got: {msg}"
        );
        // The dispatcher id MUST appear so the author sees which table the
        // bundle's mask was checked against.
        assert!(
            msg.contains(DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER),
            "expected aerodrome dispatcher id in error, got: {msg}"
        );
    }

    /// Anti-misfire guard: on the Aerodrome dispatcher (which has no recursion
    /// layout), opcode `0x21` MUST NOT reach the EXECUTE_SUB_PLAN recursion
    /// handler — it falls through to the plain `per_opcode_emit` path. The UR
    /// fixture has no `0x21` per-opcode rule, so with
    /// `unknown_opcode_policy=deny` the dispatch errors out with the
    /// unknown-opcode message naming `0x21`. Had `0x21` been routed to the
    /// recursion handler instead, the error would name EXECUTE_SUB_PLAN's
    /// arg-extraction failure — never `unknown_opcode_policy=deny`.
    #[test]
    fn aerodrome_no_recursion_special_casing() {
        let mut bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();
        override_dispatcher(
            &mut bundle,
            DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER,
            "0x3f",
        );
        override_unknown_opcode_policy(&mut bundle, UnknownOpcodePolicy::Deny);

        // 0x21 is EXECUTE_SUB_PLAN in both tables; empty inputs[0] is fine —
        // the unknown-opcode lookup fires before any arg decoding.
        let decoded = ur_execute_decoded(
            DecoderId::new("declarative.aerodrome/universal-router/execute"),
            vec![0x21],
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
        // Decisive: the deny-policy message proves 0x21 took the
        // per_opcode_emit path, NOT the recursion handler.
        assert!(
            msg.contains("unknown_opcode_policy=deny"),
            "expected unknown-opcode deny error (0x21 via per_opcode_emit path), got: {msg}"
        );
        assert!(
            msg.contains("0x21"),
            "expected masked opcode 0x21 in error, got: {msg}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // TB-2 — UR V4_SWAP envelope emission (declarative path).
    // The legacy imperative V4_SWAP mapper's two-pass builder is now shared;
    // `execute_v4_swap_step` emits envelopes instead of `Ok(Vec::new())`.
    // ─────────────────────────────────────────────────────────────────────

    /// Encode a V4 `TAKE` (0x0e) params blob —
    /// `(address currency, address recipient, uint256 amount)`.
    fn encode_v4_take_input(
        currency: alloy_primitives::Address,
        recipient: alloy_primitives::Address,
        amount: u128,
    ) -> Vec<u8> {
        let func = Function::parse("step(address,address,uint256)").unwrap();
        let raw = func
            .abi_encode_input(&[
                DynSolValue::Address(currency),
                DynSolValue::Address(recipient),
                DynSolValue::Uint(U256::from(amount), 256),
            ])
            .unwrap();
        raw[4..].to_vec()
    }

    /// Encode a V4 `SETTLE` (0x0b) params blob —
    /// `(address currency, uint256 amount, bool payerIsUser)`.
    fn encode_v4_settle_input(
        currency: alloy_primitives::Address,
        amount: u128,
        payer_is_user: bool,
    ) -> Vec<u8> {
        let func = Function::parse("step(address,uint256,bool)").unwrap();
        let raw = func
            .abi_encode_input(&[
                DynSolValue::Address(currency),
                DynSolValue::Uint(U256::from(amount), 256),
                DynSolValue::Bool(payer_is_user),
            ])
            .unwrap();
        raw[4..].to_vec()
    }

    /// TB-2: a `V4_SWAP` whose inner stream is `[SWAP_EXACT_IN_SINGLE, TAKE]`
    /// emits one Swap envelope whose recipient is patched from the TAKE step
    /// (V4 swap params carry no recipient).
    #[test]
    fn v4_swap_with_take_patches_swap_recipient() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let take_dest =
            alloy_primitives::Address::from_str("0x5555555555555555555555555555555555555555")
                .unwrap();
        let v4_swap = encode_v4_swap_input(
            vec![0x06, 0x0e],
            vec![
                encode_v4_swap_exact_in_single_input(
                    token_in(),
                    token_out(),
                    3_000,
                    60,
                    alloy_primitives::Address::ZERO,
                    true,
                    1_000_000,
                    900_000,
                    vec![],
                ),
                encode_v4_take_input(token_out(), take_dest, 900_000),
            ],
        );

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
        assert_eq!(envelopes.len(), 1, "expected one V4 Swap envelope");
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap, got {:?}", envelopes[0].action);
        };
        assert_eq!(s.swap_mode, SwapMode::ExactIn);
        assert_eq!(
            s.recipient.to_string(),
            format!("0x{}", hex::encode(take_dest)),
            "recipient must be patched from the TAKE step"
        );
    }

    /// TB-2: a `V4_SWAP` carrying only settle/take actions (no swap action)
    /// emits zero envelopes — a clean "no swap intent" result, not a fault.
    #[test]
    fn v4_swap_settle_take_only_yields_zero_envelopes() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let dest =
            alloy_primitives::Address::from_str("0x6666666666666666666666666666666666666666")
                .unwrap();
        let v4_swap = encode_v4_swap_input(
            vec![0x0b, 0x0e],
            vec![
                encode_v4_settle_input(token_in(), 1_000_000, true),
                encode_v4_take_input(token_out(), dest, 900_000),
            ],
        );

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
        assert!(
            envelopes.is_empty(),
            "settle/take-only V4_SWAP must emit no envelopes, got {envelopes:?}"
        );
    }

    /// TB-2: a `V4_SWAP` with a `SWAP_EXACT_OUT_SINGLE` action emits an
    /// ExactOut Swap envelope (mode plumbed through the shared builder).
    #[test]
    fn v4_swap_exact_out_single_emits_exact_out() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        let v4_swap = encode_v4_swap_input(
            vec![0x08],
            vec![encode_v4_swap_exact_in_single_input(
                token_in(),
                token_out(),
                3_000,
                60,
                alloy_primitives::Address::ZERO,
                false,
                500_000,
                600_000,
                vec![],
            )],
        );

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
        assert_eq!(envelopes.len(), 1);
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap");
        };
        assert_eq!(s.swap_mode, SwapMode::ExactOut);
        assert_eq!(s.input_token.amount.kind, AmountKind::Max);
        assert_eq!(s.output_token.amount.kind, AmountKind::Exact);
    }

    /// TB-2: a `V4_SWAP` whose swap action's params blob is malformed (here
    /// a 1-byte input that cannot ABI-decode) surfaces a `MapperError` so the
    /// orchestrator falls back to the static path — it is NOT silently
    /// dropped as a 0-envelope hit.
    #[test]
    fn v4_swap_malformed_swap_action_faults() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();

        // Inner action 0x06 SWAP_EXACT_IN_SINGLE but a 1-byte params blob.
        let v4_swap = encode_v4_swap_input(vec![0x06], vec![vec![0x00]]);

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

        let err = super::execute(&ctx, &decoded, &bundle.emit).unwrap_err();
        // The V4 swap builder raises ArgumentMismatch for a swap action it
        // cannot decode; `dispatch_steps` propagates it as-is.
        assert!(
            matches!(err, MapperError::ArgumentMismatch { .. }),
            "expected ArgumentMismatch from malformed V4 swap action, got {err:?}"
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // TB-3 — `v4_position_manager` dispatcher_id branch.
    // `opcode_stream::execute` now recognises a second dispatcher_id for V4
    // PositionManager `modifyLiquidities`, dispatching its `(actions,
    // params)` payload against `V4_ROUTER_TABLE`.
    // ─────────────────────────────────────────────────────────────────────

    /// Synthetic V4 PositionManager bundle for the `v4_position_manager`
    /// dispatcher. Maps V4 action `0x01 DECREASE_LIQUIDITY` to a
    /// `("dex","decrease_liquidity")` single_emit rule. Lives inline (not in
    /// `registry/`) so the test does not depend on the Tier A manifest work
    /// (TB-A). `mask`/`allow_revert_bit` match `V4_ROUTER_TABLE` (0xff / 0x00).
    const V4_PM_BUNDLE_JSON: &str = r#"{
      "type": "adapter_function",
      "id": "test/v4-position-manager/modifyLiquidities@1.0.0",
      "publisher": "test.eth",
      "match": {
        "chain_ids": [1],
        "to": ["0xbd216513d74c8cf14cf4747e6aaa6420ff64ee9e"],
        "selector": "0xdd46508f"
      },
      "abi_fragment": {
        "function_name": "modifyLiquidities",
        "abi": { "name": "modifyLiquidities", "type": "function", "inputs": [
          { "name": "unlockData", "type": "bytes" },
          { "name": "deadline", "type": "uint256" }
        ]}
      },
      "emit": {
        "strategy": "opcode_stream_dispatch",
        "dispatcher_id": "v4_position_manager",
        "mask": "0xff",
        "allow_revert_bit": "0x00",
        "unknown_opcode_policy": "ignore_step",
        "per_opcode_emit": {
          "0x01": {
            "name": "DECREASE_LIQUIDITY",
            "category": "dex",
            "action": "decrease_liquidity",
            "fields": {
              "nft.kind": { "literal": "erc721" },
              "nft.address": { "literal": "0xbd216513d74c8cf14cf4747e6aaa6420ff64ee9e" },
              "nft.tokenId": { "from": "$.args.tokenId" },
              "liquidityDelta.kind": { "literal": "exact" },
              "liquidityDelta.value": { "from": "$.args.liquidity" },
              "outputTokens": { "literal": [] }
            }
          }
        }
      },
      "requires": { "imperative": [], "adapter_capabilities": [],
                    "host_capabilities": [], "extension": ">=0.1.0" }
    }"#;

    /// Encode a V4 `DECREASE_LIQUIDITY` (0x01) params blob — flat
    /// `(uint256 tokenId, uint256 liquidity, uint128 amount0Min,
    /// uint128 amount1Min, bytes hookData)`.
    fn encode_v4_decrease_liquidity_input(token_id: u128, liquidity: u128) -> Vec<u8> {
        let func = Function::parse("step(uint256,uint256,uint128,uint128,bytes)").unwrap();
        let raw = func
            .abi_encode_input(&[
                DynSolValue::Uint(U256::from(token_id), 256),
                DynSolValue::Uint(U256::from(liquidity), 256),
                DynSolValue::Uint(U256::from(0u8), 128),
                DynSolValue::Uint(U256::from(0u8), 128),
                DynSolValue::Bytes(vec![]),
            ])
            .unwrap();
        raw[4..].to_vec()
    }

    /// Build a `modifyLiquidities(bytes unlockData, uint256 deadline)`
    /// `DecodedCall`. `unlockData = abi.encode(bytes actions, bytes[] params)`.
    fn v4_pm_modify_liquidities_decoded(actions: Vec<u8>, params: Vec<Vec<u8>>) -> DecodedCall {
        let unlock = Function::parse("step(bytes,bytes[])")
            .unwrap()
            .abi_encode_input(&[
                DynSolValue::Bytes(actions),
                DynSolValue::Array(params.into_iter().map(DynSolValue::Bytes).collect()),
            ])
            .unwrap();
        DecodedCall {
            decoder_id: DecoderId::new("declarative.test/v4-position-manager/modifyLiquidities"),
            function_signature: "modifyLiquidities(bytes,uint256)".into(),
            args: vec![
                DecodedArg {
                    name: "unlockData".into(),
                    abi_type: "bytes".into(),
                    value: DecodedValue::Bytes(unlock[4..].to_vec()),
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

    /// TB-3: the `v4_position_manager` dispatcher decodes a `modifyLiquidities`
    /// call's `(actions, params)` payload against `V4_ROUTER_TABLE` and emits
    /// one envelope per recognised action.
    #[test]
    fn v4_pm_dispatcher_emits_envelope_per_action() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(V4_PM_BUNDLE_JSON).unwrap();

        let decoded = v4_pm_modify_liquidities_decoded(
            vec![0x01],
            vec![encode_v4_decrease_liquidity_input(42, 1_000_000)],
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
            "one DECREASE_LIQUIDITY action → one envelope"
        );
        assert!(
            matches!(envelopes[0].action, Action::DecreaseLiquidity(_)),
            "expected DecreaseLiquidity, got {:?}",
            envelopes[0].action
        );
    }

    /// TB-3: an empty V4 PM action stream yields zero envelopes.
    #[test]
    fn v4_pm_dispatcher_empty_action_stream_yields_no_envelopes() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(V4_PM_BUNDLE_JSON).unwrap();

        let decoded = v4_pm_modify_liquidities_decoded(vec![], vec![]);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert!(envelopes.is_empty(), "empty action stream → no envelopes");
    }

    /// TB-3: a V4 PM action not in `per_opcode_emit` is skipped under
    /// `ignore_step` policy (here `0x0b SETTLE` against a bundle that only
    /// maps `0x01`).
    #[test]
    fn v4_pm_dispatcher_unknown_action_ignored() {
        let bundle: AdapterFunctionBundle = serde_json::from_str(V4_PM_BUNDLE_JSON).unwrap();

        // 0x0b SETTLE has no per_opcode_emit entry in V4_PM_BUNDLE_JSON; the
        // 0x01 DECREASE_LIQUIDITY does. The ignore_step policy skips 0x0b.
        let decoded = v4_pm_modify_liquidities_decoded(
            vec![0x0b, 0x01],
            vec![
                encode_v4_settle_input(token_in(), 1_000, true),
                encode_v4_decrease_liquidity_input(7, 500),
            ],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = super::execute(&ctx, &decoded, &bundle.emit).unwrap();
        assert_eq!(envelopes.len(), 1, "only the mapped 0x01 action emits");
        assert!(matches!(envelopes[0].action, Action::DecreaseLiquidity(_)));
    }

    /// TB-3: a V4 PM bundle declaring the wrong `mask` (UR's 0x7f instead of
    /// V4's 0xff) is rejected — the per-opcode keys would otherwise be
    /// computed against the wrong bit layout.
    #[test]
    fn v4_pm_dispatcher_wrong_mask_errors() {
        let mut bundle: AdapterFunctionBundle = serde_json::from_str(V4_PM_BUNDLE_JSON).unwrap();
        if let EmitRule::OpcodeStreamDispatch { mask, .. } = &mut bundle.emit {
            *mask = "0x7f".to_owned();
        } else {
            panic!("V4_PM_BUNDLE_JSON emit must be OpcodeStreamDispatch");
        }

        let decoded = v4_pm_modify_liquidities_decoded(
            vec![0x01],
            vec![encode_v4_decrease_liquidity_input(1, 1)],
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
        assert!(
            inner.to_string().contains("V4_ROUTER_TABLE mask"),
            "expected V4 mask-mismatch error, got: {inner}"
        );
    }

    /// TB-3: an unrecognised `dispatcher_id` still surfaces
    /// `MapperError::Unsupported` (the third match arm) — neither the UR nor
    /// the V4 PM path swallows it.
    #[test]
    fn unknown_dispatcher_id_is_unsupported() {
        let mut bundle: AdapterFunctionBundle = serde_json::from_str(V4_PM_BUNDLE_JSON).unwrap();
        if let EmitRule::OpcodeStreamDispatch { dispatcher_id, .. } = &mut bundle.emit {
            *dispatcher_id = "pancake_infinity".to_owned();
        } else {
            panic!("V4_PM_BUNDLE_JSON emit must be OpcodeStreamDispatch");
        }

        let decoded = v4_pm_modify_liquidities_decoded(
            vec![0x01],
            vec![encode_v4_decrease_liquidity_input(1, 1)],
        );

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let err = super::execute(&ctx, &decoded, &bundle.emit).unwrap_err();
        let MapperError::Unsupported(detail) = &err else {
            panic!("expected MapperError::Unsupported, got {err:?}");
        };
        assert!(
            detail.contains("pancake_infinity"),
            "expected dispatcher id in detail, got: {detail}"
        );
    }

    /// Phase 4: the new Pancake dispatcher ids must resolve through
    /// `resolve_dispatcher` (i.e. they're wired into the `DISPATCHERS` array).
    /// A miss here surfaces as `MapperError::Unsupported`, so the positive
    /// assertion is that the resolver does not return `None`.
    #[test]
    fn pancake_dispatcher_ids_resolve() {
        // `resolve_dispatcher` is crate-private; use the public dispatcher_id
        // constants and assert that constructing a bundle with each id is
        // accepted by `execute` (it would early-error on resolve miss).
        assert!(
            super::resolve_dispatcher(DISPATCHER_ID_PANCAKE_UNIVERSAL_ROUTER).is_some(),
            "DISPATCHER_ID_PANCAKE_UNIVERSAL_ROUTER must be wired into DISPATCHERS"
        );
        assert!(
            super::resolve_dispatcher(DISPATCHER_ID_PANCAKE_INFINITY_POSITION_MANAGER).is_some(),
            "DISPATCHER_ID_PANCAKE_INFINITY_POSITION_MANAGER must be wired into DISPATCHERS"
        );
        // Pre-existing wiring sanity (regression guard for the array order).
        assert!(super::resolve_dispatcher(DISPATCHER_ID_UNIVERSAL_ROUTER).is_some());
        assert!(super::resolve_dispatcher(DISPATCHER_ID_AERODROME_UNIVERSAL_ROUTER).is_some());
        // The V4 PM dispatcher does NOT go through the `DISPATCHERS` table
        // (it routes via the `DISPATCHER_ID_V4_POSITION_MANAGER` shortcut in
        // `execute`), so it must miss the resolver — this anchors the
        // routing branch's expectation.
        assert!(super::resolve_dispatcher(DISPATCHER_ID_V4_POSITION_MANAGER).is_none());
    }

    /// Phase 4: a Pancake UR bundle's mask must agree with `PANCAKE_UR_TABLE`
    /// (`0x3f`). A bundle author writing `mask = "0x7f"` (Uniswap value) must
    /// surface `MapperError::Internal` with a "Tier B pancake_universal_router
    /// table mask" diagnostic rather than silently mis-dispatching.
    #[test]
    fn pancake_ur_mask_mismatch_surfaces_error() {
        // Reuse the UR bundle as scaffolding — swap dispatcher_id + mask only.
        let mut bundle: AdapterFunctionBundle = serde_json::from_str(UR_BUNDLE_JSON).unwrap();
        if let EmitRule::OpcodeStreamDispatch {
            dispatcher_id,
            mask,
            allow_revert_bit,
            ..
        } = &mut bundle.emit
        {
            *dispatcher_id = DISPATCHER_ID_PANCAKE_UNIVERSAL_ROUTER.to_owned();
            *mask = "0x7f".to_owned(); // Uniswap mask — wrong for Pancake (must be 0x3f)
            *allow_revert_bit = "0x80".to_owned();
        } else {
            panic!("UR_BUNDLE_JSON emit must be OpcodeStreamDispatch");
        }

        // Minimal `execute(commands, inputs)` payload — empty stream is
        // sufficient because the mask check runs before dispatch.
        let decoded = DecodedCall {
            decoder_id: DecoderId::new("test"),
            function_signature: "execute(bytes,bytes[])".to_owned(),
            args: vec![
                DecodedArg {
                    name: "commands".to_owned(),
                    abi_type: "bytes".to_owned(),
                    value: DecodedValue::Bytes(Vec::new()),
                },
                DecodedArg {
                    name: "inputs".to_owned(),
                    abi_type: "bytes[]".to_owned(),
                    value: DecodedValue::Array(Vec::new()),
                },
            ],
            nested: Vec::new(),
        };

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
            msg.contains("mask") && msg.contains("0x3f"),
            "expected Pancake UR mask-mismatch error mentioning 0x3f, got: {msg}"
        );
    }
}
