//! `single_emit` strategy execution (spec §5.2.1).
//!
//! Phase 1A only supported `category="dex" / action="swap"`. Phase 5 added the
//! handful of Universal Router opcode mappings that emit non-swap envelopes
//! (`misc/wrap`, `misc/unwrap`, `misc/transfer`, `misc/permit`) so the
//! `opcode_stream_dispatch` per-opcode rules can reuse this builder.
//!
//! The interpreter:
//!
//!  1. Builds the JSON view of `decoded.args` ([`super::eval::args_to_json`]).
//!  2. Evaluates every `(field_path, ValueExpr)` entry into a JSON value.
//!  3. Materialises a nested `serde_json::Value` tree by splitting each
//!     `field_path` on `.` (so `inputToken.asset.address` becomes
//!     `{ inputToken: { asset: { address: <value> } } }`).
//!  4. Maps that tree into the requested action variant (`SwapAction`,
//!     `WrapAction`, `UnwrapAction`, `TransferAction`, `PermitAction`).
//!
//! The intermediate JSON tree is deliberately ignorant of policy-engine types,
//! and conversion happens only at the action boundary. This keeps the
//! interpreter generic for when category/action expand further.
//!
//! `fee_bps` is intentionally `None` — declarative bundles in the PoC do not
//! emit it. The V2 equivalence test asserts this gap explicitly (the static V2
//! mapper returns `Some(30)` while declarative returns `None`).

use std::collections::BTreeMap;
use std::str::FromStr as _;

use abi_resolver::DecodedCall;
use policy_engine::action::dex::{
    AddLiquidityAction, BurnKind, BurnLiquidityNftAction, DecreaseLiquidityAction,
    IncreaseLiquidityAction, InitializePoolAction, MintLiquidityNftAction, PoolRef,
    RemoveLiquidityAction, RemoveLiquidityExitMode, SwapAction, SwapMode, TickRange,
};
use policy_engine::action::lending::{
    AmountMode, BorrowAction, LiquidateAction, LiquidateMode, LiquidationKind, MarketRef,
    RepayAction, RepayKind, SupplyAction,
};
use policy_engine::action::misc::{
    ApprovalKind, ApproveAction, ClaimRewardsAction, GaugeVoteAction, GaugeVoteKind,
    LockCreateAction, LockIncreaseAction, LockIncreaseKind, LockManageAction, LockManageKind,
    LpStakeAction, LpUnstakeAction, PermitAction, PermitKind, SetApprovalForAllAction, SourceRef,
    TransferAction, UnwrapAction, VoteAction, VoteSupport, WrapAction,
};
use policy_engine::action::staking::{ClaimUnstakeAction, StakeAction, TicketRef};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
    AssetRefWithAmountConstraint, Category, DecimalString, Hex, Validity, ValiditySource,
};

use crate::mapper::{MapContext, MapperError};

use super::eval::{args_to_json, evaluate};
use super::types::{EmitRule, ValueExpr};

/// Execute a `single_emit` rule against the given decoded call.
///
/// Supported combinations (PoC):
///   * `("dex", "swap")` — Phase 1A
///   * `("misc", "wrap")` / `("misc", "unwrap")` — Phase 5 (UR WRAP_ETH /
///     UNWRAP_WETH opcodes)
///   * `("misc", "transfer")` — Phase 5 (UR SWEEP opcode)
///   * `("misc", "permit")` — Phase 5 (UR PERMIT2_PERMIT opcode)
///   * `("dex", "add_liquidity")` / `("dex", "remove_liquidity")` — Phase 7 T-B2
///     (Uniswap V2 router liquidity)
///   * `("dex", "mint_liquidity_nft")` / `("dex", "increase_liquidity")` /
///     `("dex", "decrease_liquidity")` / `("dex", "burn_liquidity_nft")` —
///     Phase 7 T-B2 (Uniswap V3 NFPM concentrated-liquidity positions)
///
/// Any other combination yields [`MapperError::Unsupported`].
pub fn execute(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
    rule: &EmitRule,
) -> Result<ActionEnvelope, MapperError> {
    let (category, action, fields) = match rule {
        EmitRule::SingleEmit {
            category,
            action,
            fields,
        } => (category.as_str(), action.as_str(), fields),
        other => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "single_emit::execute called with non-single_emit rule: {other:?}"
            )))
        }
    };

    let args_json = args_to_json(decoded);
    execute_with_args(ctx, &args_json, category, action, fields)
}

/// Execute a `single_emit`-style emission against a pre-built `args_json`
/// (Phase 12.0).
///
/// Phase 12.0 introduces `enum_tagged_dispatch`, where per-variant emit rules
/// reuse the same field-tree logic as `single_emit` but the `args_json` is
/// derived from a sub-decoded enum payload, not the outer [`DecodedCall`].
/// Sharing this helper avoids duplicating the (a) build_field_tree + (b)
/// per-category arm match.
///
/// `args_json` must already encode each named argument as a top-level field
/// (the same shape [`super::eval::args_to_json`] produces).
pub fn execute_with_args(
    ctx: &MapContext<'_>,
    args_json: &serde_json::Value,
    category: &str,
    action: &str,
    fields: &BTreeMap<String, ValueExpr>,
) -> Result<ActionEnvelope, MapperError> {
    let tree = build_field_tree(ctx, args_json, fields)?;

    match (category, action) {
        ("dex", "swap") => Ok(build_swap_envelope(&tree)?),
        ("misc", "wrap") => Ok(build_wrap_envelope(&tree)?),
        ("misc", "unwrap") => Ok(build_unwrap_envelope(&tree)?),
        ("misc", "transfer") => Ok(build_transfer_envelope(&tree)?),
        ("misc", "permit") => Ok(build_permit_envelope(&tree)?),
        // Phase 7B — Permit2 `approve` + ERC-721/NFPM `setApprovalForAll`.
        ("misc", "approve") => Ok(build_approve_envelope(&tree)?),
        ("misc", "set_approval_for_all") => Ok(build_set_approval_for_all_envelope(&tree)?),
        ("dex", "add_liquidity") => Ok(build_add_liquidity_envelope(&tree)?),
        ("dex", "remove_liquidity") => Ok(build_remove_liquidity_envelope(&tree)?),
        ("dex", "mint_liquidity_nft") => Ok(build_mint_liquidity_nft_envelope(&tree)?),
        ("dex", "increase_liquidity") => Ok(build_increase_liquidity_envelope(&tree)?),
        ("dex", "decrease_liquidity") => Ok(build_decrease_liquidity_envelope(&tree)?),
        ("dex", "burn_liquidity_nft") => Ok(build_burn_liquidity_nft_envelope(&tree)?),
        ("dex", "initialize_pool") => Ok(build_initialize_pool_envelope(&tree)?),
        // Phase 12.5 — lending builders for crvUSD Controller (LLAMMA).
        // Phase B / F1 added `supply` for `addCollateral` / `addCollateral-for`
        // — without this arm the 6 `crvusd/{wsteth,sfrxeth,wbtc}/
        // addCollateral{,-for}@1.0.0` manifests faulted at the
        // ("lending","supply") match and fell back to the static path.
        ("lending", "supply") => Ok(build_supply_envelope(&tree)?),
        ("lending", "borrow") => Ok(build_borrow_envelope(&tree)?),
        ("lending", "repay") => Ok(build_repay_envelope(&tree)?),
        ("lending", "liquidate") => Ok(build_liquidate_envelope(&tree)?),
        // Phase 12.6 — staking / claim / vote builders for veCRV + Gauge +
        // GaugeController.
        ("staking", "stake") => Ok(build_stake_envelope(&tree)?),
        ("staking", "claim_unstake") => Ok(build_claim_unstake_envelope(&tree)?),
        ("misc", "claim_rewards") => Ok(build_claim_rewards_envelope(&tree)?),
        ("misc", "vote") => Ok(build_vote_envelope(&tree)?),
        // Phase 8 — Aerodrome ve(3,3) builders (gauge vote / LP stake / locks).
        ("misc", "gauge_vote") => Ok(build_gauge_vote_envelope(&tree)?),
        ("misc", "lp_stake") => Ok(build_lp_stake_envelope(&tree)?),
        ("misc", "lp_unstake") => Ok(build_lp_unstake_envelope(&tree)?),
        ("misc", "lock_create") => Ok(build_lock_create_envelope(&tree)?),
        ("misc", "lock_increase") => Ok(build_lock_increase_envelope(&tree)?),
        ("misc", "lock_manage") => Ok(build_lock_manage_envelope(&tree)?),
        (c, a) => Err(MapperError::Unsupported(format!("single_emit/{c}/{a}"))),
    }
}

/// Evaluate each `ValueExpr`, then merge the dot-paths into a nested JSON tree.
fn build_field_tree(
    ctx: &MapContext<'_>,
    args_json: &serde_json::Value,
    fields: &BTreeMap<String, ValueExpr>,
) -> Result<serde_json::Value, MapperError> {
    let mut root = serde_json::Value::Object(serde_json::Map::new());
    for (path, expr) in fields {
        let value = evaluate(ctx, args_json, expr)?;
        set_nested(&mut root, path, value)?;
    }
    Ok(root)
}

/// Upper bound on a single `[N]` array index in a field path.
///
/// `set_nested` grows the target JSON array on demand, padding intervening
/// slots with `Null` (see [`set_nested`] docs). A bundle that writes a huge
/// index (`field[1000000000]`) would therefore force a multi-gigabyte
/// null-padded array — an OOM / DoS. Registry SHA-256 verification gates
/// *which* bundles run, but this cap is a defense-in-depth limit on what any
/// (even verified) bundle can ask the interpreter to allocate. 64 comfortably
/// exceeds every real intent shape (longest observed: a handful of
/// `rewardTokens[k]` / `inputTokens[k]` slots).
const MAX_FIELD_ARRAY_INDEX: usize = 64;

/// One step in a parsed field-path: an object key, then an optional sequence
/// of numeric array indices.
///
/// For `inputTokens[0].asset.kind` this parses to three steps:
///   * `Step { key: "inputTokens", indices: [0] }`
///   * `Step { key: "asset",       indices: []  }`
///   * `Step { key: "kind",        indices: []  }`
///
/// And for a hypothetical `swap_params[0][1]` two-dimensional index it parses
/// to a single step `Step { key: "swap_params", indices: [0, 1] }`.
#[derive(Debug)]
struct PathStep<'a> {
    key: &'a str,
    indices: Vec<usize>,
}

/// Parse one dot-segment like `"inputTokens[0]"` or `"swap_params[0][1]"`.
///
/// Returns `Err` if the segment has unbalanced / non-numeric brackets, or an
/// empty bareword.
fn parse_path_segment<'a>(segment: &'a str, full_path: &str) -> Result<PathStep<'a>, MapperError> {
    // Fast path — no `[` means the whole segment is the key.
    let Some(bracket_start) = segment.find('[') else {
        if segment.is_empty() {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "field path {full_path:?}: empty segment"
            )));
        }
        return Ok(PathStep {
            key: segment,
            indices: vec![],
        });
    };
    let key = &segment[..bracket_start];
    if key.is_empty() {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "field path {full_path:?}: empty bareword before '['"
        )));
    }
    let mut indices = Vec::new();
    let mut remainder = &segment[bracket_start..];
    while !remainder.is_empty() {
        // Each iteration consumes exactly `[<digits>]`.
        if !remainder.starts_with('[') {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "field path {full_path:?}: unexpected char {remainder:?} after bracket"
            )));
        }
        let close = remainder.find(']').ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "field path {full_path:?}: unterminated '['"
            ))
        })?;
        let idx_str = &remainder[1..close];
        let idx = idx_str.parse::<usize>().map_err(|e| {
            MapperError::Internal(anyhow::anyhow!(
                "field path {full_path:?}: bracket index {idx_str:?}: {e}"
            ))
        })?;
        // Defense-in-depth: a giant index would null-pad the array up to that
        // length (see `MAX_FIELD_ARRAY_INDEX`). Reject before `set_nested`
        // ever allocates.
        if idx > MAX_FIELD_ARRAY_INDEX {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "field path {full_path:?}: bracket index {idx} exceeds \
                 maximum {MAX_FIELD_ARRAY_INDEX}"
            )));
        }
        indices.push(idx);
        remainder = &remainder[close + 1..];
    }
    Ok(PathStep { key, indices })
}

/// `set_nested(root, "a.b.c", v)` mutates `root` so `root.a.b.c == v`.
///
/// Bracket-array indices are supported for any number of dimensions:
///   * `inputTokens[0].asset.kind = "erc20"` → `inputTokens` becomes a
///     JSON array, indices grow on demand and gaps are filled with
///     `serde_json::Value::Null`. Out-of-order writes are fine — index `2`
///     can be assigned before index `1`.
///
/// The function refuses to overwrite a non-object / non-array intermediate
/// (which would indicate two fields disagreeing about the type of a parent).
fn set_nested(
    root: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) -> Result<(), MapperError> {
    if path.is_empty() {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "field path is empty"
        )));
    }
    let raw_segments: Vec<&str> = path.split('.').collect();
    let mut steps = Vec::with_capacity(raw_segments.len());
    for segment in raw_segments {
        steps.push(parse_path_segment(segment, path)?);
    }

    let mut cursor = root;
    let total_steps = steps.len();
    for (step_idx, step) in steps.iter().enumerate() {
        let is_last_step = step_idx + 1 == total_steps;
        let total_indices = step.indices.len();

        // 1) Descend the object key.
        let map = cursor.as_object_mut().ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "field path {path:?}: ancestor at step {step_idx} is not an object"
            ))
        })?;
        let key_target_is_array = total_indices > 0;
        let entry = map.entry(step.key.to_owned()).or_insert_with(|| {
            if key_target_is_array {
                serde_json::Value::Array(Vec::new())
            } else if is_last_step {
                // Will be overwritten below.
                serde_json::Value::Null
            } else {
                serde_json::Value::Object(serde_json::Map::new())
            }
        });

        // No indices → simple object descent (or assignment at last step).
        if !key_target_is_array {
            if is_last_step {
                *entry = value;
                return Ok(());
            }
            cursor = entry;
            continue;
        }

        // 2) For each index in `step.indices`, descend into the array, growing
        //    it as needed. At the last (step, index) pair, assign `value`.
        let mut array_cursor = entry;
        for (idx_pos, &idx) in step.indices.iter().enumerate() {
            let is_last_index = idx_pos + 1 == total_indices;
            let array = array_cursor.as_array_mut().ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "field path {path:?}: expected array at {}{}",
                    step.key,
                    step.indices[..=idx_pos]
                        .iter()
                        .map(|i| format!("[{i}]"))
                        .collect::<String>()
                ))
            })?;
            while array.len() <= idx {
                array.push(serde_json::Value::Null);
            }
            if is_last_index && is_last_step {
                array[idx] = value;
                return Ok(());
            }
            // Need to descend further. If the slot is Null, materialise it as
            // either the next-dimensional array, or an object (for the
            // following segment's dot-key descent).
            if matches!(array[idx], serde_json::Value::Null) {
                array[idx] = if is_last_index {
                    serde_json::Value::Object(serde_json::Map::new())
                } else {
                    serde_json::Value::Array(Vec::new())
                };
            }
            array_cursor = &mut array[idx];
        }
        // After consuming all indices of this step, move to next step.
        cursor = array_cursor;
    }
    unreachable!("loop returns on the last step's last index");
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → SwapAction
// ───────────────────────────────────────────────────────────────────────────

fn build_swap_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let input_token = read_asset_with_amount(tree, "inputToken")?;
    let output_token = read_asset_with_amount(tree, "outputToken")?;
    let recipient = read_address(tree, "recipient")?;
    let validity = read_validity(tree)?;
    let swap_mode = derive_swap_mode(&input_token.amount.kind, &output_token.amount.kind);

    let action = SwapAction {
        swap_mode,
        input_token,
        output_token,
        recipient,
        validity,
        fee_bps: None,
    };
    Ok(ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(action),
    })
}

fn derive_swap_mode(input: &AmountKind, output: &AmountKind) -> SwapMode {
    match (input, output) {
        (AmountKind::Exact, AmountKind::Min) => SwapMode::ExactIn,
        (AmountKind::Max, AmountKind::Exact) => SwapMode::ExactOut,
        _ => SwapMode::Unknown,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → WrapAction / UnwrapAction (Phase 5 — UR WRAP_ETH / UNWRAP_WETH)
// ───────────────────────────────────────────────────────────────────────────

fn build_wrap_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let native_asset = read_asset_with_amount(tree, "nativeAsset")?;
    let wrapped_asset = read_asset_with_amount(tree, "wrappedAsset")?;
    let recipient = read_address(tree, "recipient")?;
    let action = WrapAction {
        native_asset,
        wrapped_asset,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Wrap(action),
    })
}

fn build_unwrap_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let wrapped_asset = read_asset_with_amount(tree, "wrappedAsset")?;
    let native_asset = read_asset_with_amount(tree, "nativeAsset")?;
    let recipient = read_address(tree, "recipient")?;
    let action = UnwrapAction {
        wrapped_asset,
        native_asset,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Unwrap(action),
    })
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → TransferAction (Phase 5 — UR SWEEP)
// ───────────────────────────────────────────────────────────────────────────

fn build_transfer_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let token = read_asset_with_amount(tree, "token")?;
    let from = read_address(tree, "from")?;
    let recipient = read_address(tree, "recipient")?;
    let action = TransferAction {
        token,
        from,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Transfer(action),
    })
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → PermitAction (Phase 5 — UR PERMIT2_PERMIT)
// ───────────────────────────────────────────────────────────────────────────

fn build_permit_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let permit_kind_str =
        required_string(tree, "permitKind").map_err(|_| missing_field("$", "permitKind"))?;
    let permit_kind = parse_permit_kind(permit_kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "permitKind {permit_kind_str:?} not recognised"
        ))
    })?;

    let token = read_asset_inline(tree, "token")?;
    let owner = read_address(tree, "owner")?;
    let spender = match tree.get("spender") {
        Some(serde_json::Value::String(s)) => Some(
            Address::from_str(s)
                .map_err(|m| MapperError::Internal(anyhow::anyhow!("spender {s:?}: {m}")))?,
        ),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "spender: expected string, got {other}"
            )));
        }
    };
    let amount = read_amount_inline(tree, "amount")?;
    // Phase 7B — Permit2 `permitTransferFrom` / `permitWitnessTransferFrom`
    // (ISignatureTransfer) carries a one-shot transfer destination
    // (`recipient`) + requested amount (`requestedAmount`) alongside the
    // permit. Absent for `eip2612` / `permit2_single` / `permit2_batch`
    // manifests (those omit the fields → `None`, preserving prior behaviour).
    let recipient = match tree.get("recipient") {
        Some(serde_json::Value::String(s)) => Some(
            Address::from_str(s)
                .map_err(|m| MapperError::Internal(anyhow::anyhow!("recipient {s:?}: {m}")))?,
        ),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "recipient: expected string, got {other}"
            )));
        }
    };
    let requested_amount = read_amount_inline(tree, "requestedAmount")?;
    let validity =
        read_validity(tree)?.ok_or_else(|| MapperError::MissingArgument("validity".to_owned()))?;
    let signature_validity = read_signature_validity(tree)?;

    let action = PermitAction {
        permit_kind,
        token,
        owner,
        spender,
        recipient,
        amount,
        requested_amount,
        operator: None,
        approved: None,
        validity,
        signature_validity,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Permit(action),
    })
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → ApproveAction / SetApprovalForAllAction (Phase 7B)
//
// `("misc", "approve")` covers Permit2 `approve(token, spender, amount,
// expiration)` (selector 0x87517c45) and any ERC-20 `approve` whose manifest
// emits the `approve` action. `approvalKind` is a literal in the manifest
// (`"erc20"` / `"erc20_increase"` / `"erc20_decrease"` / `"permit2"`).
//
// `("misc", "set_approval_for_all")` covers ERC-721 / Uniswap V3 NFPM
// `setApprovalForAll(operator, approved)`. Both arms exist so that a Cedar
// `forbid` policy on `Action::"approve"` / `Action::"set_approval_for_all"`
// observes a matching PolicyRequest instead of fail-opening to `Pass`.
// ───────────────────────────────────────────────────────────────────────────

/// Build an [`ApproveAction`] envelope from the field tree (Phase 7B).
///
/// Schema reference: `crates/policy-engine/src/action/misc/approve.rs`.
///
/// Required fields:
///   * `token.kind` / `.address` — token whose allowance is granted
///   * `spender` — address receiving the allowance
///   * `amount.kind` / `.value` — approved amount
///   * `approvalKind` — `"erc20"` / `"erc20_increase"` / `"erc20_decrease"` /
///     `"permit2"`
///
/// Optional fields:
///   * `spenderLabel` — human-readable spender name
///   * `currentAllowance` — pre-action allowance (decimal string)
///   * `validity.expiresAt` / `.source` — Permit2 `expiration` window
fn build_approve_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let token = read_asset_inline(tree, "token")?;
    let spender = read_address(tree, "spender")?;
    let spender_label = read_optional_string(tree, "spenderLabel")?;
    let amount = read_amount_inline(tree, "amount")?
        .ok_or_else(|| MapperError::MissingArgument("amount".to_owned()))?;
    let approval_kind_str =
        required_string(tree, "approvalKind").map_err(|_| missing_field("$", "approvalKind"))?;
    let approval_kind = parse_approval_kind(approval_kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "approvalKind {approval_kind_str:?} not recognised"
        ))
    })?;
    let current_allowance = read_optional_decimal(tree, "currentAllowance")?;
    let validity = read_validity(tree)?;

    let action = ApproveAction {
        token,
        spender,
        spender_label,
        amount,
        approval_kind,
        current_allowance,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Approve(action),
    })
}

fn parse_approval_kind(kind: &str) -> Option<ApprovalKind> {
    match kind {
        "erc20" => Some(ApprovalKind::Erc20),
        "erc20_increase" => Some(ApprovalKind::Erc20Increase),
        "erc20_decrease" => Some(ApprovalKind::Erc20Decrease),
        "permit2" => Some(ApprovalKind::Permit2),
        "erc721" => Some(ApprovalKind::Erc721),
        _ => None,
    }
}

/// Build a [`SetApprovalForAllAction`] envelope from the field tree (Phase 7B).
///
/// Schema reference: `crates/policy-engine/src/action/misc/set_approval_for_all.rs`.
///
/// Required fields:
///   * `collection.kind` / `.address` — NFT collection whose operator
///     approval changes (typically `kind = "erc721"`)
///   * `operator` — address gaining or losing collection-wide approval
///   * `approved` — boolean toggle
///
/// Optional fields:
///   * `operatorLabel` — human-readable operator name
///   * `previouslyApproved` — prior approval state
fn build_set_approval_for_all_envelope(
    tree: &serde_json::Value,
) -> Result<ActionEnvelope, MapperError> {
    let collection = read_asset_inline(tree, "collection")?;
    let operator = read_address(tree, "operator")?;
    let operator_label = read_optional_string(tree, "operatorLabel")?;
    let approved = read_bool(tree, "approved")?;
    let previously_approved = read_optional_bool(tree, "previouslyApproved")?;

    let action = SetApprovalForAllAction {
        collection,
        operator,
        operator_label,
        approved,
        previously_approved,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::SetApprovalForAll(action),
    })
}

/// Read a required `bool` from `tree.<field>`.
fn read_bool(tree: &serde_json::Value, field: &str) -> Result<bool, MapperError> {
    match tree.get(field) {
        Some(serde_json::Value::Bool(b)) => Ok(*b),
        Some(other) => Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected bool, got {other}"
        ))),
        None => Err(MapperError::MissingArgument(field.to_owned())),
    }
}

/// Read an `Option<bool>` from `tree.<field>`. Missing or JSON null → `None`.
fn read_optional_bool(tree: &serde_json::Value, field: &str) -> Result<Option<bool>, MapperError> {
    match tree.get(field) {
        Some(serde_json::Value::Bool(b)) => Ok(Some(*b)),
        Some(serde_json::Value::Null) | None => Ok(None),
        Some(other) => Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected bool, got {other}"
        ))),
    }
}

/// Read an `Option<String>` from `tree.<field>`. Missing or JSON null →
/// `None`. Used for the optional `spenderLabel` / `operatorLabel` fields.
fn read_optional_string(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<String>, MapperError> {
    match tree.get(field) {
        Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
        Some(serde_json::Value::Null) | None => Ok(None),
        Some(other) => Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected string, got {other}"
        ))),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → AddLiquidityAction / RemoveLiquidityAction (Phase 7 T-B2 — V2)
// ───────────────────────────────────────────────────────────────────────────

fn build_add_liquidity_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let pool = read_pool(tree, "pool")?;
    let inputs = read_assets_array(tree, "inputTokens")?;
    let output_lp = read_asset_with_amount(tree, "outputLp")?;
    let recipient = read_address(tree, "recipient")?;
    let validity = read_validity(tree)?;

    let action = AddLiquidityAction {
        pool,
        inputs,
        output_lp,
        recipient,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Dex,
        action: Action::AddLiquidity(action),
    })
}

fn build_remove_liquidity_envelope(
    tree: &serde_json::Value,
) -> Result<ActionEnvelope, MapperError> {
    let exit_mode_str =
        required_string(tree, "exitMode").map_err(|_| missing_field("$", "exitMode"))?;
    let exit_mode = parse_remove_liquidity_exit_mode(exit_mode_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("exitMode {exit_mode_str:?} not recognised"))
    })?;
    let pool = read_pool(tree, "pool")?;
    let input_lp = read_asset_with_amount(tree, "inputLp")?;
    let outputs = read_assets_array(tree, "outputTokens")?;
    let recipient = read_address(tree, "recipient")?;
    let validity = read_validity(tree)?;

    let action = RemoveLiquidityAction {
        exit_mode,
        pool,
        input_lp,
        outputs,
        recipient,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Dex,
        action: Action::RemoveLiquidity(action),
    })
}

fn parse_remove_liquidity_exit_mode(mode: &str) -> Option<RemoveLiquidityExitMode> {
    match mode {
        "proportional" => Some(RemoveLiquidityExitMode::Proportional),
        "single_asset" => Some(RemoveLiquidityExitMode::SingleAsset),
        "exact_out" => Some(RemoveLiquidityExitMode::ExactOut),
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → MintLiquidityNftAction / IncreaseLiquidity / DecreaseLiquidity /
// BurnLiquidityNft (Phase 7 T-B2 — V3 NFPM)
// ───────────────────────────────────────────────────────────────────────────

fn build_mint_liquidity_nft_envelope(
    tree: &serde_json::Value,
) -> Result<ActionEnvelope, MapperError> {
    let pool = read_pool(tree, "pool")?;
    let fee_tier_bps = read_u32(tree, "feeBps")?;
    let tick_range = read_tick_range(tree, "tickRange")?;
    let inputs = read_assets_array(tree, "inputTokens")?;
    let recipient = read_address(tree, "recipient")?;
    let validity = read_validity(tree)?;

    let action = MintLiquidityNftAction {
        pool,
        fee_tier_bps,
        tick_range,
        inputs,
        recipient,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Dex,
        action: Action::MintLiquidityNft(action),
    })
}

fn build_increase_liquidity_envelope(
    tree: &serde_json::Value,
) -> Result<ActionEnvelope, MapperError> {
    let nft = read_nft_asset(tree, "nft")?;
    let inputs = read_assets_array(tree, "inputTokens")?;
    let validity = read_validity(tree)?;

    let action = IncreaseLiquidityAction {
        nft,
        inputs,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Dex,
        action: Action::IncreaseLiquidity(action),
    })
}

fn build_decrease_liquidity_envelope(
    tree: &serde_json::Value,
) -> Result<ActionEnvelope, MapperError> {
    let nft = read_nft_asset(tree, "nft")?;
    let liquidity_delta = read_amount_inline(tree, "liquidityDelta")?
        .ok_or_else(|| MapperError::MissingArgument("liquidityDelta".to_owned()))?;
    let outputs = read_assets_array(tree, "outputTokens")?;
    let recipient = read_optional_address(tree, "recipient")?;
    let validity = read_validity(tree)?;

    let action = DecreaseLiquidityAction {
        nft,
        liquidity_delta,
        outputs,
        recipient,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Dex,
        action: Action::DecreaseLiquidity(action),
    })
}

fn build_burn_liquidity_nft_envelope(
    tree: &serde_json::Value,
) -> Result<ActionEnvelope, MapperError> {
    let nft = read_nft_asset(tree, "nft")?;
    let burn_kind_str =
        required_string(tree, "burnKind").map_err(|_| missing_field("$", "burnKind"))?;
    let burn_kind = parse_burn_kind(burn_kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("burnKind {burn_kind_str:?} not recognised"))
    })?;
    let outputs = match tree.get("outputTokens") {
        Some(serde_json::Value::Null) | None => None,
        Some(_) => Some(read_assets_array(tree, "outputTokens")?),
    };
    let recipient = read_optional_address(tree, "recipient")?;
    let validity = read_validity(tree)?;

    let action = BurnLiquidityNftAction {
        nft,
        burn_kind,
        outputs,
        recipient,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Dex,
        action: Action::BurnLiquidityNft(action),
    })
}

fn parse_burn_kind(kind: &str) -> Option<BurnKind> {
    match kind {
        "empty_only" => Some(BurnKind::EmptyOnly),
        "auto_decrease" => Some(BurnKind::AutoDecrease),
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → InitializePoolAction (Phase 7 T-B6 — UR V4_INITIALIZE_POOL 0x13)
// ───────────────────────────────────────────────────────────────────────────

/// Build an [`InitializePoolAction`] envelope from the field tree emitted by
/// the UR `0x13` (V4_INITIALIZE_POOL) opcode rule.
///
/// Field shape (matches [`InitializePoolAction`] schema):
///   * `pool.address` (required) — placeholder string, manifest sets it to
///     `currency0` since V4 pools have no distinct contract address (they
///     live inside the PoolManager keyed by poolId).
///   * `token0.kind` (literal "erc20"), `token0.address` — from `currency0`
///   * `token1.kind` (literal "erc20"), `token1.address` — from `currency1`
///   * `feeBps` — from `poolKey.fee` (u32 — raw V4 fee tier, may include the
///     `0x800000` dynamic-fee flag)
///   * `tickSpacing` — from `poolKey.tickSpacing` (i32, optional)
///   * `hooks` — from `poolKey.hooks` (optional Address)
///   * `sqrtPriceX96` — from outer `sqrtPriceX96` (optional DecimalString)
fn build_initialize_pool_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let pool = read_pool(tree, "pool")?;
    let token0 = read_asset_inline(tree, "token0")?;
    let token1 = read_asset_inline(tree, "token1")?;
    let fee_bps = read_u32(tree, "feeBps")?;
    let tick_spacing = read_optional_i32(tree, "tickSpacing")?;
    let sqrt_price_x96 = read_optional_decimal(tree, "sqrtPriceX96")?;
    let hooks = read_optional_address(tree, "hooks")?;

    let action = InitializePoolAction {
        pool,
        token0,
        token1,
        fee_bps,
        tick_spacing,
        sqrt_price_x96,
        hooks,
        // The remaining fields require host-side derivation (poolId hash,
        // dynamic-fee flag interpretation, hook permission bit decoding) so
        // the static mapper leaves them empty — the policy engine sees
        // `None` and falls back on `feeBps` raw bytes for any masking.
        is_dynamic_fee: None,
        hook_permissions: None,
    };
    Ok(ActionEnvelope {
        category: Category::Dex,
        action: Action::InitializePool(action),
    })
}

// ───────────────────────────────────────────────────────────────────────────
// Phase 12.5 — Lending builders (crvUSD Controller / Aave / Morpho)
// ───────────────────────────────────────────────────────────────────────────

/// Build a [`SupplyAction`] envelope from the field tree (Phase B / F1).
///
/// Schema reference: `crates/policy-engine/src/action/lending/supply.rs`.
/// crvUSD `add_collateral(uint256 collateral)` / `add_collateral(uint256
/// collateral, address _for)` map here — both deposit collateral into a
/// borrower's existing crvUSD Controller position without minting more debt.
/// The PoC bundles hardcode the controller's collateral asset (per-controller)
/// and the `Supply` envelope records the asset + amount + recipient (= debt
/// position owner). Mirrors the [`BorrowAction`] builder shape so the
/// per-Controller manifest pair stays consistent.
///
/// Required fields (`FieldPath`):
/// * `asset.kind` / `.address` — collateral token (per-Controller hardcoded)
/// * `amount.kind` / `.value` — supplied collateral amount
/// * `recipient` — debt position owner (`$.tx.from` for 1-arg variant,
///   `$.args._for` for 2-arg `addCollateral-for`)
///
/// Optional fields:
/// * `market.address` / `.label` / `.id` — `MarketRef` for the Controller
/// * `amountMode` — `"assets"` or `"shares"` (defaults to `None`)
/// * `from` — provider account when distinct from `recipient` (rare)
/// * `validity` — present when the bundle exposes `validity.expiresAt` and
///   `validity.source`; Curve has no native deadline so the bundles emit no
///   validity by default.
fn build_supply_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let market = read_optional_market_ref(tree, "market")?;
    let asset = read_asset_inline(tree, "asset")?;
    let amount = read_amount_inline(tree, "amount")?
        .ok_or_else(|| MapperError::MissingArgument("amount".to_owned()))?;
    let amount_mode = read_optional_amount_mode(tree, "amountMode")?;
    let recipient = read_address(tree, "recipient")?;
    let from = read_optional_address(tree, "from")?;
    let validity = read_validity(tree)?;

    let action = SupplyAction {
        market,
        asset,
        amount,
        amount_mode,
        recipient,
        from,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Lending,
        action: Action::Supply(action),
    })
}

/// Build a [`BorrowAction`] envelope from the field tree (Phase 12.5).
///
/// Schema reference: `crates/policy-engine/src/action/lending/borrow.rs`.
/// crvUSD `create_loan(uint256 collateral, uint256 debt, uint256 N)` /
/// `borrow_more(uint256 collateral, uint256 debt)` map here — both are
/// **collateral deposit + debt mint** atomic intents. The PoC bundle hardcodes
/// the controller's collateral asset (per-controller) but the schema records
/// only the borrowed asset (debt token, `crvUSD`); the collateral side enters
/// the schema as a separate `Supply` (out of scope here) or remains implicit.
///
/// Required fields (FieldPath):
///   * `asset.kind` / `.address` — borrowed asset (debt token, e.g. crvUSD)
///   * `amount.kind` / `.value` — debt amount minted
///   * `recipient` — debtor receiving the borrowed assets (`$.tx.from`)
///   * `onBehalf` — debt position owner (`$.tx.from`)
///
/// Optional fields:
///   * `market.address` / `.label` / `.id` — `MarketRef` for the Controller
///   * `amountMode` — `"assets"` or `"shares"` (defaults to `None`)
///   * `validity` — present when the bundle exposes `validity.expiresAt` +
///     `.source` (Curve has no native deadline; default `None`).
fn build_borrow_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let market = read_optional_market_ref(tree, "market")?;
    let asset = read_asset_inline(tree, "asset")?;
    let amount = read_amount_inline(tree, "amount")?
        .ok_or_else(|| MapperError::MissingArgument("amount".to_owned()))?;
    let amount_mode = read_optional_amount_mode(tree, "amountMode")?;
    let recipient = read_address(tree, "recipient")?;
    let on_behalf = read_address(tree, "onBehalf")?;
    let validity = read_validity(tree)?;
    let collateral_asset = read_optional_asset_inline(tree, "collateralAsset")?;
    let collateral_amount = read_amount_inline(tree, "collateralAmount")?;

    let action = BorrowAction {
        market,
        asset,
        amount,
        amount_mode,
        collateral_asset,
        collateral_amount,
        recipient,
        on_behalf,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Lending,
        action: Action::Borrow(action),
    })
}

/// Build a [`RepayAction`] envelope from the field tree (Phase 12.5).
///
/// Schema reference: `crates/policy-engine/src/action/lending/repay.rs`.
/// crvUSD `repay(uint256 _d_debt, address _for, uint256 max_active_band, bool use_eth)`
/// maps here. The first arg is the repay amount in debt-token units; the
/// second is the position owner (`onBehalf`).
///
/// Required fields:
///   * `asset.kind` / `.address` — repayment asset (e.g. crvUSD)
///   * `amount.kind` / `.value` — repayment amount
///   * `onBehalf` — debt position owner (`$.args._for`)
///   * `repayKind` — `"debt_asset"` (Curve) or `"atoken_direct"` (Aave only)
///
/// Optional fields:
///   * `market` — `MarketRef` for the Controller
///   * `amountMode`
///   * `validity`
fn build_repay_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let market = read_optional_market_ref(tree, "market")?;
    let asset = read_asset_inline(tree, "asset")?;
    let amount = read_amount_inline(tree, "amount")?
        .ok_or_else(|| MapperError::MissingArgument("amount".to_owned()))?;
    let amount_mode = read_optional_amount_mode(tree, "amountMode")?;
    let on_behalf = read_address(tree, "onBehalf")?;
    let repay_kind_str =
        required_string(tree, "repayKind").map_err(|_| missing_field("$", "repayKind"))?;
    let repay_kind = parse_repay_kind(repay_kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "repayKind {repay_kind_str:?} not recognised"
        ))
    })?;
    let validity = read_validity(tree)?;

    let action = RepayAction {
        market,
        asset,
        amount,
        amount_mode,
        on_behalf,
        repay_kind,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Lending,
        action: Action::Repay(action),
    })
}

/// Build a [`LiquidateAction`] envelope from the field tree (Phase 12.5).
///
/// Schema reference: `crates/policy-engine/src/action/lending/liquidate.rs`.
/// crvUSD `liquidate(address user, uint256 min_x)` ends here — the bundle
/// resolves `borrower` to `$.args.user`. `min_x` is the minimum debt asset
/// (crvUSD) the liquidator receives — recorded as `debtToCover` (kind `min`).
/// (Self-liquidation uses the same `liquidate` entrypoint with `user` set to
/// `msg.sender`; Curve has no separate `self_liquidate` function.)
///
/// Required fields:
///   * `borrower` — position being liquidated
///   * `debtAsset.kind` / `.address` — debt asset (e.g. crvUSD)
///   * `liquidationKind` — `"pool_share"` (Curve), `"protocol_absorb"`,
///     `"socializable"`, `"single_asset"`
///
/// Optional fields:
///   * `market` — `MarketRef` for the Controller
///   * `collateralAsset.kind` / `.address`
///   * `debtToCover.kind` / `.value`
///   * `seizedCollateralAmount.kind` / `.value`
///   * `liquidateMode` — `"single_step"` / `"seize"` / `"repay"`
///   * `recipient` — assets seized destination
///   * `receiveAToken` — Aave-specific; ignored on Curve
fn build_liquidate_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let market = read_optional_market_ref(tree, "market")?;
    let borrower = read_address(tree, "borrower")?;
    let collateral_asset = match tree.get("collateralAsset") {
        Some(serde_json::Value::Null) | None => None,
        Some(_) => Some(read_asset_inline(tree, "collateralAsset")?),
    };
    let debt_asset = read_asset_inline(tree, "debtAsset")?;
    let debt_to_cover = read_amount_inline(tree, "debtToCover")?;
    let seized_collateral_amount = read_amount_inline(tree, "seizedCollateralAmount")?;
    let liquidation_kind_str = required_string(tree, "liquidationKind")
        .map_err(|_| missing_field("$", "liquidationKind"))?;
    let liquidation_kind = parse_liquidation_kind(liquidation_kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "liquidationKind {liquidation_kind_str:?} not recognised"
        ))
    })?;
    let liquidate_mode = match tree.get("liquidateMode") {
        Some(serde_json::Value::Null) | None => None,
        Some(serde_json::Value::String(s)) => Some(parse_liquidate_mode(s).ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!("liquidateMode {s:?} not recognised"))
        })?),
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "liquidateMode: expected string, got {other}"
            )));
        }
    };
    let recipient = read_optional_address(tree, "recipient")?;
    let receive_a_token = match tree.get("receiveAToken") {
        Some(serde_json::Value::Bool(b)) => Some(*b),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "receiveAToken: expected bool, got {other}"
            )));
        }
    };

    let action = LiquidateAction {
        market,
        borrower,
        collateral_asset,
        debt_asset,
        debt_to_cover,
        seized_collateral_amount,
        liquidation_kind,
        liquidate_mode,
        recipient,
        receive_a_token,
    };
    Ok(ActionEnvelope {
        category: Category::Lending,
        action: Action::Liquidate(action),
    })
}

// ───────────────────────────────────────────────────────────────────────────
// Phase 12.6 — Staking / Claim / Vote builders
//
// `("staking", "stake")` covers veCRV `create_lock` / `increase_amount` /
// `increase_unlock_time` (CRV → veCRV) and Gauge `deposit(uint256)` (LP →
// gauge receipt).
//
// `("staking", "claim_unstake")` covers veCRV `withdraw()` (post lock expiry)
// and Gauge `withdraw(uint256)`.
//
// `("misc", "claim_rewards")` covers Gauge `claim_rewards()` /
// `claim_rewards(address)`.
//
// `("misc", "vote")` covers GaugeController
// `vote_for_gauge_weights(address _gauge_addr, uint256 _user_weight)`.
// The mapping shoehorns Curve's per-gauge weighting into the protocol-agnostic
// `VoteAction` schema: `governance` = `_gauge_addr` (the target being weighted),
// `votingPower` = `_user_weight` (basis points, 0-10000), and the bundle
// hardcodes `support` = `"for"` since Curve has no support-direction concept
// at the gauge level. `proposalId` is the literal `"0"`.
// ───────────────────────────────────────────────────────────────────────────

/// Build a [`StakeAction`] envelope from the field tree (Phase 12.6).
///
/// Schema reference: `crates/policy-engine/src/action/staking/stake.rs`.
///
/// Required fields:
///   * `tokenIn.kind` / `.address` — token being staked (e.g. CRV for veCRV,
///     LP token for Gauge)
///   * `receiptToken.kind` / `.address` — receipt issued by the stake
///     (`veCRV` / gauge receipt token)
///   * `amountIn.kind` / `.value` — staked amount (`$.args._value` for veCRV,
///     `$.args._value`-style for Gauge)
///   * `recipient` — receipt token recipient (`$.tx.from`)
///
/// Optional fields:
///   * `amountOut.kind` / `.value` — expected receipt amount (almost never
///     known at static-analysis time)
fn build_stake_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let token_in = read_asset_inline(tree, "tokenIn")?;
    let receipt_token = read_asset_inline(tree, "receiptToken")?;
    let amount_in = read_amount_inline(tree, "amountIn")?
        .ok_or_else(|| MapperError::MissingArgument("amountIn".to_owned()))?;
    let amount_out = read_amount_inline(tree, "amountOut")?;
    let recipient = read_address(tree, "recipient")?;

    let action = StakeAction {
        token_in,
        receipt_token,
        amount_in,
        amount_out,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::LiquidStaking,
        action: Action::Stake(action),
    })
}

/// Build a [`ClaimUnstakeAction`] envelope from the field tree (Phase 12.6).
///
/// Schema reference: `crates/policy-engine/src/action/staking/claim_unstake.rs`.
///
/// Required fields:
///   * `tokenOut.kind` / `.address` — token being claimed (the original CRV
///     for veCRV.withdraw, the LP token for Gauge.withdraw)
///   * `recipient` — claim recipient (`$.tx.from`)
///   * `ticket` — `TicketRef { nft?, tokenId?, id? }`; Curve has no ticket
///     concept so the bundle emits the empty object `{}`.
///
/// Optional fields:
///   * `amountOut.kind` / `.value` — claim amount (Gauge: explicit;
///     veCRV.withdraw: full balance, unknown at static time)
fn build_claim_unstake_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let token_out = read_asset_inline(tree, "tokenOut")?;
    let amount_out = read_amount_inline(tree, "amountOut")?;
    let ticket = read_ticket_ref(tree, "ticket")?;
    let recipient = read_address(tree, "recipient")?;

    let action = ClaimUnstakeAction {
        token_out,
        amount_out,
        ticket,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::LiquidStaking,
        action: Action::ClaimUnstake(action),
    })
}

/// Build a [`ClaimRewardsAction`] envelope from the field tree (Phase 12.6).
///
/// Schema reference: `crates/policy-engine/src/action/misc/claim_rewards.rs`.
///
/// Required fields:
///   * `from` — account whose rewards are claimed (`$.tx.from`)
///   * `recipient` — recipient of claimed assets
///
/// Optional fields:
///   * `source.address` / `.label` — `SourceRef` for the Gauge / Voter
///     contract
///   * `nft.kind` / `.address` — position NFT (Aerodrome `claim_*`)
///   * `tokenId` — NFT id
///   * `rewardTokens[i].kind` / `.address` — reward token list
///   * `maxAmounts[i].kind` / `.value` — corresponding max claim amounts
fn build_claim_rewards_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let source = read_optional_source_ref(tree, "source")?;
    let token_id = match tree.get("tokenId") {
        Some(serde_json::Value::String(s)) => Some(
            DecimalString::from_str(s)
                .map_err(|m| MapperError::Internal(anyhow::anyhow!("tokenId {s:?}: {m}")))?,
        ),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "tokenId: expected decimal string, got {other}"
            )));
        }
    };
    let nft = match tree.get("nft") {
        Some(serde_json::Value::Null) | None => None,
        Some(_) => {
            let mut asset = read_asset_inline(tree, "nft")?;
            // AssetRef invariant (kind=erc721/1155 → tokenId required) 가
            // claim_rewards 의 root-level tokenId 로도 충족되도록 후처리.
            // schema 의 dual-tokenId 패턴 (nft AssetRef + root tokenId) 의
            // 정합성 유지 — `read_asset_inline` 가 tokenId 를 읽지 않으므로
            // (line 1872 의 comment 참조: "tokenId 는 read_nft_asset 가 layers on
            // afterwards") root-level tokenId 가 있으면 AssetRef 에 inject.
            if asset.token_id.is_none() {
                if let Some(id) = token_id.clone() {
                    asset.token_id = Some(id);
                }
            }
            Some(asset)
        }
    };
    let from = read_address(tree, "from")?;
    let recipient = read_address(tree, "recipient")?;
    let reward_tokens = read_optional_asset_list(tree, "rewardTokens")?;
    let max_amounts = read_optional_amount_list(tree, "maxAmounts")?;

    let action = ClaimRewardsAction {
        source,
        nft,
        token_id,
        from,
        recipient,
        reward_tokens,
        max_amounts,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::ClaimRewards(action),
    })
}

/// Build a [`VoteAction`] envelope from the field tree (Phase 12.6).
///
/// Schema reference: `crates/policy-engine/src/action/misc/vote.rs`.
/// Curve `vote_for_gauge_weights(address _gauge_addr, uint256 _user_weight)`
/// is shoehorned into the protocol-agnostic VoteAction by mapping `_gauge_addr`
/// to `governance` (the per-gauge target being weighted) and `_user_weight`
/// (basis points 0-10000) to `votingPower`. `support` defaults to `"for"` since
/// Curve has no support-direction at the gauge layer.
///
/// Required fields:
///   * `governance` — target being weighted (`$.args._gauge_addr` for Curve)
///   * `proposalId` — proposal id; Curve has none, bundle hardcodes `"0"`
///   * `support` — `"for"` / `"against"` / `"abstain"`
///
/// Optional fields:
///   * `governanceLabel`
///   * `reason`
///   * `votingPower` — vote weight (`$.args._user_weight` for Curve)
///   * `validity`
fn build_vote_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let governance = read_address(tree, "governance")?;
    let governance_label = match tree.get("governanceLabel") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "governanceLabel: expected string, got {other}"
            )));
        }
    };
    let proposal_id_str =
        required_string(tree, "proposalId").map_err(|_| missing_field("$", "proposalId"))?;
    let proposal_id = DecimalString::from_str(proposal_id_str).map_err(|m| {
        MapperError::Internal(anyhow::anyhow!("proposalId {proposal_id_str:?}: {m}"))
    })?;
    let support_str =
        required_string(tree, "support").map_err(|_| missing_field("$", "support"))?;
    let support = parse_vote_support(support_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("support {support_str:?} not recognised"))
    })?;
    let reason = match tree.get("reason") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "reason: expected string, got {other}"
            )));
        }
    };
    let voting_power = read_optional_decimal(tree, "votingPower")?;
    let validity = read_validity(tree)?;

    let action = VoteAction {
        governance,
        governance_label,
        proposal_id,
        support,
        reason,
        voting_power,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Vote(action),
    })
}

// ───────────────────────────────────────────────────────────────────────────
// Phase 12.5 / 12.6 helpers
// ───────────────────────────────────────────────────────────────────────────

/// Read an optional [`MarketRef`] from `tree.<field>`. Accepts JSON null or a
/// missing key as `None`. The object must have at least one of `address`,
/// `id`, `label`.
fn read_optional_market_ref(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<MarketRef>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let object = raw.as_object().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected object, got {raw}"))
    })?;
    let address =
        match object.get("address") {
            Some(serde_json::Value::String(s)) => Some(Address::from_str(s).map_err(|m| {
                MapperError::Internal(anyhow::anyhow!("{field}.address {s:?}: {m}"))
            })?),
            Some(serde_json::Value::Null) | None => None,
            Some(other) => {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "{field}.address: expected string, got {other}"
                )));
            }
        };
    let id = match object.get("id") {
        Some(serde_json::Value::String(s)) => Some(
            Hex::from_str(s)
                .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field}.id {s:?}: {m}")))?,
        ),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.id: expected string, got {other}"
            )));
        }
    };
    let label = match object.get("label") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.label: expected string, got {other}"
            )));
        }
    };
    Ok(Some(MarketRef { address, id, label }))
}

/// Read an optional [`SourceRef`] from `tree.<field>`.
fn read_optional_source_ref(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<SourceRef>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let object = raw.as_object().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected object, got {raw}"))
    })?;
    let address =
        match object.get("address") {
            Some(serde_json::Value::String(s)) => Some(Address::from_str(s).map_err(|m| {
                MapperError::Internal(anyhow::anyhow!("{field}.address {s:?}: {m}"))
            })?),
            Some(serde_json::Value::Null) | None => None,
            Some(other) => {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "{field}.address: expected string, got {other}"
                )));
            }
        };
    let label = match object.get("label") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.label: expected string, got {other}"
            )));
        }
    };
    Ok(Some(SourceRef { address, label }))
}

/// Read a [`TicketRef`] from `tree.<field>`. All sub-fields are optional, so an
/// empty object yields a `TicketRef { nft: None, token_id: None, id: None }`.
/// Missing/null parent yields the same empty ref so callers don't need to
/// pre-check.
fn read_ticket_ref(tree: &serde_json::Value, field: &str) -> Result<TicketRef, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(TicketRef {
            nft: None,
            token_id: None,
            id: None,
        });
    };
    if raw.is_null() {
        return Ok(TicketRef {
            nft: None,
            token_id: None,
            id: None,
        });
    }
    let object = raw.as_object().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected object, got {raw}"))
    })?;
    let nft = match object.get("nft") {
        Some(serde_json::Value::Null) | None => None,
        Some(_) => Some(read_asset_inline(raw, "nft")?),
    };
    let token_id = match object.get("tokenId") {
        Some(serde_json::Value::String(s)) => {
            Some(DecimalString::from_str(s).map_err(|m| {
                MapperError::Internal(anyhow::anyhow!("{field}.tokenId {s:?}: {m}"))
            })?)
        }
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.tokenId: expected decimal string, got {other}"
            )));
        }
    };
    let id = match object.get("id") {
        Some(serde_json::Value::String(s)) => Some(
            Hex::from_str(s)
                .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field}.id {s:?}: {m}")))?,
        ),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.id: expected string, got {other}"
            )));
        }
    };
    Ok(TicketRef { nft, token_id, id })
}

/// Read an `Option<Vec<AssetRef>>` from `tree.<field>`. Each array element
/// must be an inline-asset object (`{ kind, address?, tokenId?, ... }`).
fn read_optional_asset_list(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<Vec<AssetRef>>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let array = raw.as_array().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected array, got {raw}"))
    })?;
    let mut result = Vec::with_capacity(array.len());
    for (idx, entry) in array.iter().enumerate() {
        let parent_key = format!("{field}[{idx}]");
        // Build a transient single-key object so we can reuse `read_asset_inline`.
        let mut wrapper = serde_json::Map::new();
        wrapper.insert(parent_key.clone(), entry.clone());
        let wrapper_value = serde_json::Value::Object(wrapper);
        let asset = read_asset_inline(&wrapper_value, &parent_key)?;
        result.push(asset);
    }
    Ok(Some(result))
}

/// Read an `Option<Vec<AmountConstraint>>` from `tree.<field>`. Each array
/// element must be an inline-amount object (`{ kind, value? }`).
fn read_optional_amount_list(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<Vec<AmountConstraint>>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let array = raw.as_array().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected array, got {raw}"))
    })?;
    let mut result = Vec::with_capacity(array.len());
    for (idx, entry) in array.iter().enumerate() {
        let parent_key = format!("{field}[{idx}]");
        let mut wrapper = serde_json::Map::new();
        wrapper.insert(parent_key.clone(), entry.clone());
        let wrapper_value = serde_json::Value::Object(wrapper);
        let amount = read_amount_inline(&wrapper_value, &parent_key)?
            .ok_or_else(|| missing_field(&parent_key, "kind"))?;
        result.push(amount);
    }
    Ok(Some(result))
}

/// Read an optional `AmountMode` enum from `tree.<field>`.
fn read_optional_amount_mode(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<AmountMode>, MapperError> {
    match tree.get(field) {
        Some(serde_json::Value::String(s)) => parse_amount_mode(s)
            .map(Some)
            .ok_or_else(|| MapperError::Internal(anyhow::anyhow!("{field} {s:?} not recognised"))),
        Some(serde_json::Value::Null) | None => Ok(None),
        Some(other) => Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected string, got {other}"
        ))),
    }
}

fn parse_amount_mode(s: &str) -> Option<AmountMode> {
    match s {
        "assets" => Some(AmountMode::Assets),
        "shares" => Some(AmountMode::Shares),
        _ => None,
    }
}

fn parse_repay_kind(s: &str) -> Option<RepayKind> {
    match s {
        "debt_asset" => Some(RepayKind::DebtAsset),
        "atoken_direct" => Some(RepayKind::AtokenDirect),
        _ => None,
    }
}

fn parse_liquidation_kind(s: &str) -> Option<LiquidationKind> {
    match s {
        "pool_share" => Some(LiquidationKind::PoolShare),
        "protocol_absorb" => Some(LiquidationKind::ProtocolAbsorb),
        "socializable" => Some(LiquidationKind::Socializable),
        "single_asset" => Some(LiquidationKind::SingleAsset),
        _ => None,
    }
}

fn parse_liquidate_mode(s: &str) -> Option<LiquidateMode> {
    match s {
        "single_step" => Some(LiquidateMode::SingleStep),
        "seize" => Some(LiquidateMode::Seize),
        "repay" => Some(LiquidateMode::Repay),
        _ => None,
    }
}

fn parse_vote_support(s: &str) -> Option<VoteSupport> {
    match s {
        "for" => Some(VoteSupport::For),
        "against" => Some(VoteSupport::Against),
        "abstain" => Some(VoteSupport::Abstain),
        _ => None,
    }
}

/// Read an `Option<i32>` from `tree.<field>`. Returns `None` if missing or
/// JSON null. Accepts both JSON numbers and decimal strings.
fn read_optional_i32(tree: &serde_json::Value, field: &str) -> Result<Option<i32>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    if let Some(n) = raw.as_i64() {
        i32::try_from(n).map(Some).map_err(|_| {
            MapperError::Internal(anyhow::anyhow!("{field}: value {n} exceeds i32 range"))
        })
    } else if let Some(s) = raw.as_str() {
        s.parse::<i32>()
            .map(Some)
            .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field} {s:?}: {m}")))
    } else {
        Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected i32, got {raw}"
        )))
    }
}

/// Read an `Option<DecimalString>` from `tree.<field>`. Returns `None` if
/// missing or JSON null. Accepts both JSON numbers (e.g. uint160 emitted as a
/// number) and decimal strings (the default for large integers).
fn read_optional_decimal(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<DecimalString>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    if let Some(s) = raw.as_str() {
        DecimalString::from_str(s)
            .map(Some)
            .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field} {s:?}: {m}")))
    } else if let Some(n) = raw.as_u64() {
        DecimalString::from_str(&n.to_string())
            .map(Some)
            .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field} {n}: {m}")))
    } else {
        Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected decimal string, got {raw}"
        )))
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Helpers for liquidity builders
// ───────────────────────────────────────────────────────────────────────────

/// Read a `PoolRef` from `tree.<field>`. `address` is required; `id` and
/// `label` are optional.
fn read_pool(tree: &serde_json::Value, field: &str) -> Result<PoolRef, MapperError> {
    let object = required_object(tree, field)?;
    let address_str =
        required_string(object, "address").map_err(|_| missing_field(field, "address"))?;
    let address = Address::from_str(address_str).map_err(|message| {
        MapperError::Internal(anyhow::anyhow!(
            "{field}.address {address_str:?}: {message}"
        ))
    })?;
    let id = match object.get("id") {
        Some(serde_json::Value::String(s)) => Some(
            Hex::from_str(s)
                .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field}.id {s:?}: {m}")))?,
        ),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.id: expected string, got {other}"
            )));
        }
    };
    let label = match object.get("label") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.label: expected string, got {other}"
            )));
        }
    };
    Ok(PoolRef { address, id, label })
}

/// Read `Vec<AssetRefWithAmountConstraint>` from `tree.<field>`. Each element
/// must be a `{ asset, amount }` object; the order is preserved.
fn read_assets_array(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Vec<AssetRefWithAmountConstraint>, MapperError> {
    let raw = tree
        .get(field)
        .ok_or_else(|| MapperError::MissingArgument(field.to_owned()))?;
    let array = raw.as_array().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected array, got {raw}"))
    })?;
    let mut result = Vec::with_capacity(array.len());
    for (index, entry) in array.iter().enumerate() {
        if !entry.is_object() {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}[{index}]: expected object, got {entry}"
            )));
        }
        let parent = format!("{field}[{index}]");
        let asset = read_asset(entry, &parent)?;
        let amount = read_amount(entry, &parent)?;
        result.push(AssetRefWithAmountConstraint { asset, amount });
    }
    Ok(result)
}

/// Read a `TickRange { lower, upper }` from `tree.<field>`. Both ticks must be
/// signed 32-bit integers.
fn read_tick_range(tree: &serde_json::Value, field: &str) -> Result<TickRange, MapperError> {
    let object = required_object(tree, field)?;
    let lower = read_i32_member(object, field, "lower")?;
    let upper = read_i32_member(object, field, "upper")?;
    Ok(TickRange { lower, upper })
}

/// Read an `AssetRef` representing an NFT position from `tree.<field>`. The
/// underlying `read_asset_inline` already enforces `kind` parsing; callers are
/// expected to pass NFPM positions (`kind = "erc721"`).
fn read_nft_asset(tree: &serde_json::Value, field: &str) -> Result<AssetRef, MapperError> {
    let mut asset = read_asset_inline(tree, field)?;
    // NFPM positions carry a `tokenId`; preserve it when present.
    if let Some(object) = tree.get(field).and_then(|v| v.as_object()) {
        if let Some(token_id) = object.get("tokenId") {
            match token_id {
                serde_json::Value::String(s) => {
                    asset.token_id = Some(DecimalString::from_str(s).map_err(|m| {
                        MapperError::Internal(anyhow::anyhow!("{field}.tokenId {s:?}: {m}"))
                    })?);
                }
                serde_json::Value::Null => {}
                other => {
                    return Err(MapperError::Internal(anyhow::anyhow!(
                        "{field}.tokenId: expected decimal string, got {other}"
                    )));
                }
            }
        }
    }
    Ok(asset)
}

/// Read an `Option<Address>` from `tree.<field>`. Returns `None` if missing or
/// JSON null.
fn read_optional_address(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<Address>, MapperError> {
    match tree.get(field) {
        Some(serde_json::Value::String(s)) => Address::from_str(s)
            .map(Some)
            .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field} {s:?}: {m}"))),
        Some(serde_json::Value::Null) | None => Ok(None),
        Some(other) => Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected string, got {other}"
        ))),
    }
}

/// Read a `u32` from `tree.<field>`. Accepts both JSON numbers and decimal
/// strings (the DSL evaluator encodes uint values as strings).
fn read_u32(tree: &serde_json::Value, field: &str) -> Result<u32, MapperError> {
    let raw = tree
        .get(field)
        .ok_or_else(|| MapperError::MissingArgument(field.to_owned()))?;
    if let Some(n) = raw.as_u64() {
        u32::try_from(n).map_err(|_| {
            MapperError::Internal(anyhow::anyhow!("{field}: value {n} exceeds u32 range"))
        })
    } else if let Some(s) = raw.as_str() {
        s.parse::<u32>()
            .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field} {s:?}: {m}")))
    } else {
        Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected u32, got {raw}"
        )))
    }
}

/// Read an `i32` from `object.<member>`. Accepts both JSON numbers and decimal
/// strings (negative ticks may be encoded either way).
fn read_i32_member(
    object: &serde_json::Value,
    parent: &str,
    member: &str,
) -> Result<i32, MapperError> {
    let raw = object
        .get(member)
        .ok_or_else(|| missing_field(parent, member))?;
    if let Some(n) = raw.as_i64() {
        i32::try_from(n).map_err(|_| {
            MapperError::Internal(anyhow::anyhow!(
                "{parent}.{member}: value {n} exceeds i32 range"
            ))
        })
    } else if let Some(s) = raw.as_str() {
        s.parse::<i32>()
            .map_err(|m| MapperError::Internal(anyhow::anyhow!("{parent}.{member} {s:?}: {m}")))
    } else {
        Err(MapperError::Internal(anyhow::anyhow!(
            "{parent}.{member}: expected i32, got {raw}"
        )))
    }
}

fn parse_permit_kind(kind: &str) -> Option<PermitKind> {
    match kind {
        "eip2612" => Some(PermitKind::Eip2612),
        "erc721_permit" => Some(PermitKind::Erc721Permit),
        "erc721_permit_for_all" => Some(PermitKind::Erc721PermitForAll),
        "permit2_single" => Some(PermitKind::Permit2Single),
        "permit2_transfer" => Some(PermitKind::Permit2Transfer),
        "permit2_batch" => Some(PermitKind::Permit2Batch),
        _ => None,
    }
}

fn read_signature_validity(tree: &serde_json::Value) -> Result<Option<Validity>, MapperError> {
    let Some(validity) = tree.get("signatureValidity") else {
        return Ok(None);
    };
    if validity.is_null() {
        return Ok(None);
    }
    let expires_at_str = required_string(validity, "expiresAt")
        .map_err(|_| missing_field("signatureValidity", "expiresAt"))?;
    let expires_at = DecimalString::from_str(expires_at_str).map_err(|message| {
        MapperError::Internal(anyhow::anyhow!(
            "signatureValidity.expiresAt {expires_at_str:?}: {message}"
        ))
    })?;
    let source_str = required_string(validity, "source")
        .map_err(|_| missing_field("signatureValidity", "source"))?;
    let source = parse_validity_source(source_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "signatureValidity.source {source_str:?} not recognised"
        ))
    })?;
    Ok(Some(Validity { expires_at, source }))
}

fn read_asset_with_amount(
    tree: &serde_json::Value,
    field: &str,
) -> Result<AssetRefWithAmountConstraint, MapperError> {
    let token = required_object(tree, field)?;
    let asset = read_asset(token, field)?;
    let amount = read_amount(token, field)?;
    Ok(AssetRefWithAmountConstraint { asset, amount })
}

/// Read an `AssetRef` directly nested under `tree.<field>` (no intermediate
/// `asset` wrapper). Used by `PermitAction.token`, which is a bare `AssetRef`.
fn read_asset_inline(tree: &serde_json::Value, field: &str) -> Result<AssetRef, MapperError> {
    let inner = tree
        .get(field)
        .ok_or_else(|| MapperError::MissingArgument(field.to_owned()))?;
    let object = inner.as_object().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected object, got {inner}"))
    })?;
    let kind_str = object
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_field(field, "kind"))?;
    let kind = parse_asset_kind(kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}.kind {kind_str:?} not recognised"))
    })?;
    let address =
        match object.get("address") {
            Some(serde_json::Value::String(s)) => Some(Address::from_str(s).map_err(|m| {
                MapperError::Internal(anyhow::anyhow!("{field}.address {s:?}: {m}"))
            })?),
            Some(serde_json::Value::Null) | None => None,
            Some(other) => {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "{field}.address: expected string, got {other}"
                )));
            }
        };
    // Asset-integrity guard — a declarative bundle must not emit an
    // `erc20`/`erc721`/`erc1155` token AssetRef without an address: the
    // evaluate stage's `AssetRef` deserialize rejects it and fail-closes the
    // engine with an opaque `__engine::invalid_input_json`. Faulting here lets
    // the orchestrator degrade to the static path instead of a false `fail`.
    if address.is_none()
        && matches!(
            kind,
            AssetKind::Erc20 | AssetKind::Erc721 | AssetKind::Erc1155
        )
    {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: asset kind {kind:?} requires an address — declarative \
             bundle emits a token the evaluate stage cannot deserialize"
        )));
    }
    // Generic tokenId — the evaluate stage's `AssetRef` invariant requires
    // `tokenId` for `erc721` / `erc1155`. NFPM `permit` / approve-style
    // bundles emit `token.tokenId` directly under `token`; reading it here
    // lets the generic AssetRef path satisfy the invariant without a
    // per-builder shim. `read_nft_asset` still adds the same field for the
    // `nft.kind=erc721` builders (claim_rewards / decrease_liquidity / ...);
    // its layer is a no-op when this generic path already populated tokenId.
    let token_id = match object.get("tokenId") {
        Some(serde_json::Value::String(s)) => {
            Some(DecimalString::from_str(s).map_err(|m| {
                MapperError::Internal(anyhow::anyhow!("{field}.tokenId {s:?}: {m}"))
            })?)
        }
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.tokenId: expected decimal string, got {other}"
            )));
        }
    };
    Ok(normalize_native_sentinel(AssetRef {
        kind,
        address,
        token_id,
        symbol: None,
        decimals: None,
    }))
}

fn read_asset(token: &serde_json::Value, parent: &str) -> Result<AssetRef, MapperError> {
    let asset = required_object(token, "asset").map_err(|_| missing_field(parent, "asset"))?;
    let kind_str =
        required_string(asset, "kind").map_err(|_| missing_field(parent, "asset.kind"))?;
    let kind = parse_asset_kind(kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "{parent}.asset.kind {kind_str:?} not recognised in Phase 1A"
        ))
    })?;
    let address = match asset.get("address") {
        Some(serde_json::Value::String(s)) => Some(Address::from_str(s).map_err(|message| {
            MapperError::Internal(anyhow::anyhow!("{parent}.asset.address {s:?}: {message}"))
        })?),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{parent}.asset.address: expected string, got {other}"
            )));
        }
    };
    // Asset-integrity guard — see `read_asset_inline`. An `erc20`/`erc721`/
    // `erc1155` token AssetRef with no address cannot be deserialized at the
    // evaluate stage; fault here so the orchestrator degrades to the static
    // path rather than fail-closing with `__engine::invalid_input_json`.
    if address.is_none()
        && matches!(
            kind,
            AssetKind::Erc20 | AssetKind::Erc721 | AssetKind::Erc1155
        )
    {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "{parent}.asset: kind {kind:?} requires an address — declarative \
             bundle emits a token the evaluate stage cannot deserialize"
        )));
    }
    Ok(normalize_native_sentinel(AssetRef {
        kind,
        address,
        token_id: None,
        symbol: None,
        decimals: None,
    }))
}

fn parse_asset_kind(kind: &str) -> Option<AssetKind> {
    match kind {
        "erc20" => Some(AssetKind::Erc20),
        "erc721" => Some(AssetKind::Erc721),
        "erc1155" => Some(AssetKind::Erc1155),
        "native" => Some(AssetKind::Native),
        "unknown" => Some(AssetKind::Unknown),
        _ => None,
    }
}

/// Rewrite an ERC-20 `AssetRef` at the zero address to `native`.
///
/// An ERC-20 token at `address(0)` is never a real token — Uniswap's
/// Universal Router (`Constants.ETH`) and V4 (`CurrencyLibrary`) both use
/// `address(0)` as the native-asset sentinel. A declarative bundle that
/// hardcodes `asset.kind = "erc20"` therefore mislabels native ETH whenever
/// the token address resolves to `0x0` (`VERIFICATION_UNISWAP_REALTX` finding
/// F2 — UR `TRANSFER`, V4 `initialize` / `MINT_POSITION`). The static UR path
/// already applies this `0x0 -> native` rule via
/// `protocols::universal_router::common::token_asset_ref`; the declarative
/// builder is the only emit path that lacked it, so the fix lives in the two
/// shared asset readers (`read_asset` / `read_asset_inline`).
fn normalize_native_sentinel(asset: AssetRef) -> AssetRef {
    const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
    let is_zero_address = matches!(
        asset.address.as_ref(),
        Some(addr) if addr.to_string().eq_ignore_ascii_case(ZERO_ADDRESS)
    );
    if asset.kind == AssetKind::Erc20 && is_zero_address {
        AssetRef {
            kind: AssetKind::Native,
            address: None,
            ..asset
        }
    } else {
        asset
    }
}

/// Read an `AmountConstraint` directly nested under `tree.<field>` (no
/// intermediate `amount` wrapper). Returns `None` when the field is missing or
/// JSON null. Used by `PermitAction.amount`, which is `Option<AmountConstraint>`.
fn read_amount_inline(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<AmountConstraint>, MapperError> {
    let Some(inner) = tree.get(field) else {
        return Ok(None);
    };
    if inner.is_null() {
        return Ok(None);
    }
    let object = inner.as_object().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected object, got {inner}"))
    })?;
    let kind_str = object
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| missing_field(field, "kind"))?;
    let kind = parse_amount_kind(kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}.kind {kind_str:?} not recognised"))
    })?;
    let value = match object.get("value") {
        Some(serde_json::Value::String(s)) => Some(
            DecimalString::from_str(s)
                .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field}.value {s:?}: {m}")))?,
        ),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.value: expected decimal string, got {other}"
            )));
        }
    };
    Ok(Some(AmountConstraint { kind, value }))
}

fn read_amount(token: &serde_json::Value, parent: &str) -> Result<AmountConstraint, MapperError> {
    let amount = required_object(token, "amount").map_err(|_| missing_field(parent, "amount"))?;
    let kind_str =
        required_string(amount, "kind").map_err(|_| missing_field(parent, "amount.kind"))?;
    let kind = parse_amount_kind(kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "{parent}.amount.kind {kind_str:?} not recognised in Phase 1A"
        ))
    })?;
    let value = match amount.get("value") {
        Some(serde_json::Value::String(s)) => {
            Some(DecimalString::from_str(s).map_err(|message| {
                MapperError::Internal(anyhow::anyhow!("{parent}.amount.value {s:?}: {message}"))
            })?)
        }
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{parent}.amount.value: expected decimal string, got {other}"
            )));
        }
    };
    Ok(AmountConstraint { kind, value })
}

fn parse_amount_kind(kind: &str) -> Option<AmountKind> {
    match kind {
        "exact" => Some(AmountKind::Exact),
        "min" => Some(AmountKind::Min),
        "max" => Some(AmountKind::Max),
        "unlimited" => Some(AmountKind::Unlimited),
        "estimated" => Some(AmountKind::Estimated),
        "unknown" => Some(AmountKind::Unknown),
        "portion" => Some(AmountKind::Portion),
        _ => None,
    }
}

fn read_address(tree: &serde_json::Value, field: &str) -> Result<Address, MapperError> {
    let raw = required_string(tree, field).map_err(|_| missing_field("$", field))?;
    Address::from_str(raw)
        .map_err(|message| MapperError::Internal(anyhow::anyhow!("{field} {raw:?}: {message}")))
}

fn read_validity(tree: &serde_json::Value) -> Result<Option<Validity>, MapperError> {
    let Some(validity) = tree.get("validity") else {
        return Ok(None);
    };
    let object = validity.as_object().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "validity must be an object, got {validity}"
        ))
    })?;
    let expires_at_str = required_string(validity, "expiresAt")
        .map_err(|_| missing_field("validity", "expiresAt"))?;
    let expires_at = DecimalString::from_str(expires_at_str).map_err(|message| {
        MapperError::Internal(anyhow::anyhow!(
            "validity.expiresAt {expires_at_str:?}: {message}"
        ))
    })?;
    let source_str =
        required_string(validity, "source").map_err(|_| missing_field("validity", "source"))?;
    let source = parse_validity_source(source_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "validity.source {source_str:?} not recognised in Phase 1A"
        ))
    })?;
    let _ = object; // suppress unused warning if validators expand
    Ok(Some(Validity { expires_at, source }))
}

fn parse_validity_source(source: &str) -> Option<ValiditySource> {
    match source {
        "tx-deadline" => Some(ValiditySource::TxDeadline),
        "signature-deadline" => Some(ValiditySource::SignatureDeadline),
        "grant-expiration" => Some(ValiditySource::GrantExpiration),
        _ => None,
    }
}

// ───────────────────────────────────────────────────────────────────────────
// JSON tree → GaugeVote / LpStake / LpUnstake / LockCreate / LockIncrease /
// LockManage (Phase 8 — Aerodrome ve(3,3))
// ───────────────────────────────────────────────────────────────────────────

fn build_gauge_vote_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let voter = read_address(tree, "voter")?;
    let token_id = read_optional_decimal(tree, "tokenId")?;
    let pools = read_address_array(tree, "pools")?;
    let weights = read_decimal_array(tree, "weights")?;
    let kind = read_optional_enum::<GaugeVoteKind>(tree, "kind")?;
    let validity = read_validity(tree)?;

    // Length consistency — pools and weights must match (parallel arrays).
    if pools.len() != weights.len() {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "gauge_vote: pools.len()={} != weights.len()={}",
            pools.len(),
            weights.len()
        )));
    }

    // Kind-shape enforcement (Round 7 P1 #4):
    //   kind=reset / kind=poke → pools and weights must both be empty.
    // Aerodrome `Voter.reset(tokenId)` and `.poke(tokenId)` take no pool /
    // weight payload; emitting non-empty arrays under those kinds would
    // indicate an adapter mis-classification (e.g. routing a vote() call to
    // the reset opcode by mistake).
    if let Some(k @ (GaugeVoteKind::Reset | GaugeVoteKind::Poke)) = kind {
        if !pools.is_empty() || !weights.is_empty() {
            let kind_name = match k {
                GaugeVoteKind::Reset => "reset",
                GaugeVoteKind::Poke => "poke",
                GaugeVoteKind::Vote => unreachable!("matched on Reset|Poke"),
            };
            return Err(MapperError::Internal(anyhow::anyhow!(
                "gauge_vote kind={} requires empty pools and weights (got pools.len()={}, weights.len()={})",
                kind_name,
                pools.len(),
                weights.len()
            )));
        }
    }

    let action = GaugeVoteAction {
        voter,
        token_id,
        pools,
        weights,
        kind,
        validity,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::GaugeVote(action),
    })
}

fn build_lp_stake_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let gauge = read_address(tree, "gauge")?;
    let lp_token = read_asset_with_amount(tree, "lpToken")?;
    let recipient = read_address(tree, "recipient")?;
    let action = LpStakeAction {
        gauge,
        lp_token,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::LpStake(action),
    })
}

fn build_lp_unstake_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let gauge = read_address(tree, "gauge")?;
    let lp_token = read_asset_with_amount(tree, "lpToken")?;
    let recipient = read_address(tree, "recipient")?;
    let action = LpUnstakeAction {
        gauge,
        lp_token,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::LpUnstake(action),
    })
}

fn build_lock_create_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let voting_escrow = read_address(tree, "votingEscrow")?;
    let asset = read_asset_with_amount(tree, "asset")?;
    let lock_duration_sec = read_optional_decimal(tree, "lockDurationSec")?;
    let unlock_time = read_optional_decimal(tree, "unlockTime")?;
    let recipient = read_address(tree, "recipient")?;

    // XOR — exactly one of relative duration / absolute unlock time.
    // Aerodrome `createLock(value, lockDuration)` emits `lockDurationSec`;
    // Curve veCRV `create_lock(_value, _unlock_time)` emits `unlockTime`.
    match (&lock_duration_sec, &unlock_time) {
        (Some(_), None) | (None, Some(_)) => {}
        (Some(_), Some(_)) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "lock_create: lockDurationSec and unlockTime are mutually exclusive"
            )))
        }
        (None, None) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "lock_create: one of lockDurationSec / unlockTime is required"
            )))
        }
    }

    let action = LockCreateAction {
        voting_escrow,
        asset,
        lock_duration_sec,
        unlock_time,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::LockCreate(action),
    })
}

fn build_lock_increase_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let voting_escrow = read_address(tree, "votingEscrow")?;
    // SX-1/SX-2: tokenId optional — Aerodrome veAERO 만 채움, Curve veCRV 는 account-bound
    // (NFT 없음) 라 생략. mapper 가 manifest emit rule 따라 채움.
    let token_id = read_optional_decimal(tree, "tokenId")?;
    let kind: LockIncreaseKind = read_optional_enum(tree, "kind")?
        .ok_or_else(|| MapperError::MissingArgument("kind".to_owned()))?;
    let additional_amount = read_amount_inline(tree, "additionalAmount")?;
    let new_lock_duration_sec = read_optional_decimal(tree, "newLockDurationSec")?;
    // SX-2: Curve veCRV `_unlock_time` (absolute timestamp). Mutually exclusive
    // with `newLockDurationSec` (Aerodrome relative seconds).
    let new_unlock_time = read_optional_decimal(tree, "newUnlockTime")?;
    // SX-4: Curve veCRV `deposit_for(_addr, _value)` 의 `_addr` — 제3자 lock 의
    // owner. 부재 = caller's own lock.
    let recipient = read_optional_address(tree, "recipient")?;

    // Kind-required-field enforcement (Round 7 P1 #2 + SX-2 확장):
    //   kind=amount       → additionalAmount must be present
    //   kind=unlock_time  → newLockDurationSec 또는 newUnlockTime 중 하나 must be present
    // Without this an adapter typo would silently produce an envelope with
    // the wrong discriminator, and Cedar policies that branch on `kind`
    // would evaluate against a half-populated context.
    match kind {
        LockIncreaseKind::Amount => {
            if additional_amount.is_none() {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "lock_increase kind=amount requires additionalAmount"
                )));
            }
        }
        LockIncreaseKind::UnlockTime => {
            if new_lock_duration_sec.is_none() && new_unlock_time.is_none() {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "lock_increase kind=unlock_time requires newLockDurationSec (Aerodrome) or newUnlockTime (Curve)"
                )));
            }
        }
    }

    let action = LockIncreaseAction {
        voting_escrow,
        token_id,
        kind,
        additional_amount,
        new_lock_duration_sec,
        new_unlock_time,
        recipient,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::LockIncrease(action),
    })
}

fn build_lock_manage_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let voting_escrow = read_address(tree, "votingEscrow")?;
    let kind: LockManageKind = read_optional_enum(tree, "kind")?
        .ok_or_else(|| MapperError::MissingArgument("kind".to_owned()))?;
    let from_token_id = read_decimal(tree, "fromTokenId")?;
    let to_token_id = read_optional_decimal(tree, "toTokenId")?;
    let split_ratio = read_optional_decimal(tree, "splitRatio")?;

    // Kind-required-field enforcement (Round 7 P1 #3):
    //   kind=merge → toTokenId must be present (destination of the merge)
    //   kind=split → splitRatio must be present (fraction of source consumed)
    match kind {
        LockManageKind::Merge => {
            if to_token_id.is_none() {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "lock_manage kind=merge requires toTokenId"
                )));
            }
        }
        LockManageKind::Split => {
            if split_ratio.is_none() {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "lock_manage kind=split requires splitRatio"
                )));
            }
        }
    }

    let action = LockManageAction {
        voting_escrow,
        kind,
        from_token_id,
        to_token_id,
        split_ratio,
    };
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::LockManage(action),
    })
}

/// Read an optional bare `AssetRef` from `tree.<field>`. Returns `None` when
/// the field is missing or JSON null. Compared to [`read_asset_inline`] which
/// is required.
fn read_optional_asset_inline(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<AssetRef>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let asset = read_asset_inline(tree, field)?;
    // Capture the optional `tokenId` if the manifest set one (mirrors
    // [`read_nft_asset`] — used by Slipstream NFPM collect).
    let mut asset = asset;
    if let Some(object) = raw.as_object() {
        if let Some(token_id) = object.get("tokenId") {
            match token_id {
                serde_json::Value::String(s) => {
                    asset.token_id = Some(DecimalString::from_str(s).map_err(|m| {
                        MapperError::Internal(anyhow::anyhow!("{field}.tokenId {s:?}: {m}"))
                    })?);
                }
                serde_json::Value::Null => {}
                other => {
                    return Err(MapperError::Internal(anyhow::anyhow!(
                        "{field}.tokenId: expected string, got {other}"
                    )));
                }
            }
        }
    }
    Ok(Some(asset))
}

/// Read `Option<Vec<AssetRef>>` from `tree.<field>`. Each element is a bare
/// `AssetRef { kind, address }` (no `asset` wrapper, matching how the bundle
/// emits `rewardTokens[N].kind` + `rewardTokens[N].address`).
///
/// Returns `Ok(None)` if the field is missing or JSON null. Returns
/// `Ok(Some(vec))` (which may be empty) when the field is present.
#[allow(dead_code)]
fn read_optional_asset_inline_array(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<Vec<AssetRef>>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let array = raw.as_array().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected array, got {raw}"))
    })?;
    let mut result = Vec::with_capacity(array.len());
    for (index, entry) in array.iter().enumerate() {
        if entry.is_null() {
            // `set_nested` auto-pads arrays with nulls when manifest writes
            // a sparse index — reject such gaps loudly rather than silently
            // dropping the slot.
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}[{index}]: unexpected null (sparse array)"
            )));
        }
        // Build a one-element synthetic tree so we can reuse the bare-asset
        // reader. The synthetic key is "_" to avoid shadowing.
        let synthetic = serde_json::json!({ "_": entry.clone() });
        let asset = read_asset_inline(&synthetic, "_").map_err(|err| match err {
            MapperError::MissingArgument(_) => MapperError::Internal(anyhow::anyhow!(
                "{field}[{index}]: missing required asset field"
            )),
            MapperError::Internal(e) => {
                MapperError::Internal(anyhow::anyhow!("{field}[{index}]: {e}"))
            }
            other => other,
        })?;
        result.push(asset);
    }
    Ok(Some(result))
}

/// Read `Option<Vec<AmountConstraint>>` from `tree.<field>`. Each element is
/// a bare `{ kind, value }` object. Returns `Ok(None)` when the field is
/// missing or JSON null.
#[allow(dead_code)]
fn read_optional_amount_constraint_array(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<Vec<AmountConstraint>>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let array = raw.as_array().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected array, got {raw}"))
    })?;
    let mut result = Vec::with_capacity(array.len());
    for (index, entry) in array.iter().enumerate() {
        if entry.is_null() {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}[{index}]: unexpected null (sparse array)"
            )));
        }
        let synthetic = serde_json::json!({ "_": entry.clone() });
        let amount = read_amount_inline(&synthetic, "_")?.ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "{field}[{index}]: amount_inline returned None"
            ))
        })?;
        result.push(amount);
    }
    Ok(Some(result))
}

/// Read a required `DecimalString` from `tree.<field>`. Accepts JSON strings
/// only (large-integer decimals are not safe to round-trip through JSON
/// numbers).
fn read_decimal(tree: &serde_json::Value, field: &str) -> Result<DecimalString, MapperError> {
    let raw = required_string(tree, field).map_err(|_| missing_field("$", field))?;
    DecimalString::from_str(raw)
        .map_err(|message| MapperError::Internal(anyhow::anyhow!("{field} {raw:?}: {message}")))
}

/// Read a JSON array of `Address` values from `tree.<field>`. Returns an empty
/// `Vec` if the field is missing or JSON null (matches the Solidly "reset"
/// case where `pools` arrives as an empty array). Returns `Err` if the field
/// exists but is not an array, or contains non-string / invalid elements.
fn read_address_array(tree: &serde_json::Value, field: &str) -> Result<Vec<Address>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(Vec::new());
    };
    if raw.is_null() {
        return Ok(Vec::new());
    }
    let array = raw.as_array().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected array, got {raw}"))
    })?;
    let mut result = Vec::with_capacity(array.len());
    for (index, entry) in array.iter().enumerate() {
        let s = entry.as_str().ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "{field}[{index}]: expected string address, got {entry}"
            ))
        })?;
        let addr = Address::from_str(s)
            .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field}[{index}] {s:?}: {m}")))?;
        result.push(addr);
    }
    Ok(result)
}

/// Read a JSON array of `DecimalString` values from `tree.<field>`. Returns an
/// empty `Vec` if the field is missing or JSON null.
fn read_decimal_array(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Vec<DecimalString>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(Vec::new());
    };
    if raw.is_null() {
        return Ok(Vec::new());
    }
    let array = raw.as_array().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected array, got {raw}"))
    })?;
    let mut result = Vec::with_capacity(array.len());
    for (index, entry) in array.iter().enumerate() {
        let s = entry.as_str().ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "{field}[{index}]: expected decimal string, got {entry}"
            ))
        })?;
        let value = DecimalString::from_str(s)
            .map_err(|m| MapperError::Internal(anyhow::anyhow!("{field}[{index}] {s:?}: {m}")))?;
        result.push(value);
    }
    Ok(result)
}

/// Read an optional enum field (deserializable from JSON via serde). Returns
/// `Ok(None)` if the field is missing or JSON null.
fn read_optional_enum<T: serde::de::DeserializeOwned>(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<T>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    serde_json::from_value(raw.clone()).map(Some).map_err(|e| {
        MapperError::Internal(anyhow::anyhow!("{field}: enum deserialize failed: {e}"))
    })
}

// ───────────────────────────────────────────────────────────────────────────
// JSON helpers
// ───────────────────────────────────────────────────────────────────────────

fn required_object<'a>(
    tree: &'a serde_json::Value,
    field: &str,
) -> Result<&'a serde_json::Value, MapperError> {
    let value = tree
        .get(field)
        .ok_or_else(|| MapperError::MissingArgument(field.to_owned()))?;
    if !value.is_object() {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "{field}: expected object, got {value}"
        )));
    }
    Ok(value)
}

fn required_string<'a>(tree: &'a serde_json::Value, field: &str) -> Result<&'a str, MapperError> {
    tree.get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| MapperError::MissingArgument(field.to_owned()))
}

fn missing_field(parent: &str, field: &str) -> MapperError {
    MapperError::MissingArgument(format!("{parent}.{field}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn set_nested_builds_dot_path_tree() {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        set_nested(&mut root, "a.b.c", json!(1)).unwrap();
        set_nested(&mut root, "a.b.d", json!(2)).unwrap();
        set_nested(&mut root, "a.e", json!(3)).unwrap();
        assert_eq!(
            root,
            json!({
                "a": {
                    "b": { "c": 1, "d": 2 },
                    "e": 3
                }
            })
        );
    }

    #[test]
    fn set_nested_rejects_empty_segment() {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "a..c", json!(1)).unwrap_err();
        assert!(err.to_string().contains("empty segment"));
    }

    #[test]
    fn set_nested_builds_bracket_array_path() {
        // Phase 12.4 — Curve / Uniswap V2 liquidity bundles use
        // `inputTokens[0].asset.kind` notation. The interpreter must
        // synthesise a JSON array under `inputTokens` and merge subsequent
        // index writes into the same array.
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        set_nested(&mut root, "inputTokens[0].asset.kind", json!("erc20")).unwrap();
        set_nested(&mut root, "inputTokens[0].asset.address", json!("0xaaaa")).unwrap();
        set_nested(&mut root, "inputTokens[1].asset.kind", json!("erc20")).unwrap();
        set_nested(&mut root, "inputTokens[2].asset.address", json!("0xbbbb")).unwrap();
        assert_eq!(
            root,
            json!({
                "inputTokens": [
                    { "asset": { "kind": "erc20", "address": "0xaaaa" } },
                    { "asset": { "kind": "erc20" } },
                    { "asset": { "address": "0xbbbb" } }
                ]
            })
        );
    }

    #[test]
    fn set_nested_supports_array_index_suffix() {
        // `inputTokens[0].asset.kind` + `inputTokens[1].asset.kind` should
        // materialise as a 2-element JSON array. This is the bundle pattern
        // used by Uniswap V2/V3 + Aerodrome Slipstream NPM mint manifests.
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        set_nested(&mut root, "inputTokens[0].asset.kind", json!("erc20")).unwrap();
        set_nested(&mut root, "inputTokens[0].asset.address", json!("0xaa")).unwrap();
        set_nested(&mut root, "inputTokens[1].asset.kind", json!("erc20")).unwrap();
        set_nested(&mut root, "inputTokens[1].asset.address", json!("0xbb")).unwrap();
        assert_eq!(
            root,
            json!({
                "inputTokens": [
                    { "asset": { "kind": "erc20", "address": "0xaa" } },
                    { "asset": { "kind": "erc20", "address": "0xbb" } }
                ]
            })
        );
    }

    #[test]
    fn set_nested_supports_out_of_order_array_writes() {
        // Writing index 2 before index 0 / 1 must pad with `Null`s that get
        // populated later. The final tree must reflect all three writes.
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        set_nested(&mut root, "xs[2]", json!("c")).unwrap();
        set_nested(&mut root, "xs[0]", json!("a")).unwrap();
        set_nested(&mut root, "xs[1]", json!("b")).unwrap();
        assert_eq!(root, json!({ "xs": ["a", "b", "c"] }));
    }

    #[test]
    fn set_nested_rejects_non_numeric_bracket() {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "xs[abc]", json!(1)).unwrap_err();
        assert!(err.to_string().contains("bracket index"));
    }

    #[test]
    fn set_nested_rejects_unterminated_bracket() {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "xs[0", json!(1)).unwrap_err();
        assert!(err.to_string().contains("unterminated"));
    }

    #[test]
    fn set_nested_supports_single_index_array() {
        // The Aerodrome voter/claimBribes pattern: only `rewardTokens[0].*`
        // is set — the result should be a 1-element array, not an object.
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        set_nested(&mut root, "rewardTokens[0].kind", json!("erc20")).unwrap();
        set_nested(&mut root, "rewardTokens[0].address", json!("0xabcdef")).unwrap();
        assert_eq!(
            root,
            json!({
                "rewardTokens": [
                    { "kind": "erc20", "address": "0xabcdef" }
                ]
            })
        );
    }

    #[test]
    fn set_nested_supports_sparse_index_with_null_padding() {
        // If a manifest writes `[2]` before `[0]`, the intervening slots
        // become JSON null. The downstream array reader is responsible for
        // rejecting these holes.
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        set_nested(&mut root, "rewardTokens[2].kind", json!("erc20")).unwrap();
        assert_eq!(
            root,
            json!({
                "rewardTokens": [null, null, { "kind": "erc20" }]
            })
        );
    }

    #[test]
    fn set_nested_rejects_segment_starting_with_bracket() {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "[0].kind", json!("erc20")).unwrap_err();
        // phase7 (Curve) impl reports "empty bareword before '['" — same
        // semantic as phase8's "starts with '['".
        assert!(
            err.to_string().contains("empty bareword"),
            "expected empty-bareword error, got: {err}"
        );
    }

    #[test]
    fn set_nested_rejects_unbalanced_bracket() {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "inputs[0", json!(1)).unwrap_err();
        // phase7 reports "unterminated '['"; phase8 wording was "missing closing".
        assert!(
            err.to_string().contains("unterminated"),
            "expected unterminated-[ error, got: {err}"
        );
    }

    #[test]
    fn set_nested_rejects_empty_index() {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "inputs[]", json!(1)).unwrap_err();
        // phase7 surfaces the underlying integer-parse error verbatim:
        // "cannot parse integer from empty string". Same root cause as
        // phase8's "empty index" wording.
        assert!(
            err.to_string().contains("empty string"),
            "expected parse-from-empty-string error, got: {err}"
        );
    }

    #[test]
    fn set_nested_rejects_non_numeric_index() {
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "inputs[abc]", json!(1)).unwrap_err();
        // phase7 surfaces "invalid digit found in string" verbatim. Same
        // root cause as phase8's "not a non-negative integer" wording.
        assert!(
            err.to_string().contains("invalid digit"),
            "expected invalid-digit error, got: {err}"
        );
    }

    // ── AUDIT_PHASE8 #8 — `[N]` array-index DoS bound ────────────────────
    // `set_nested` null-pads arrays up to the written index, so a giant
    // index would OOM. `parse_path_segment` must reject `N > 64`.

    #[test]
    fn set_nested_rejects_giant_array_index() {
        // `field[1000000000]` would null-pad a billion-element array.
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "xs[1000000000]", json!(1)).unwrap_err();
        assert!(
            err.to_string().contains("exceeds maximum"),
            "expected exceeds-maximum error, got: {err}"
        );
        // No allocation happened — root is untouched.
        assert_eq!(root, json!({}));
    }

    #[test]
    fn set_nested_rejects_index_just_over_max() {
        // 65 = MAX_FIELD_ARRAY_INDEX + 1 — first rejected value.
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "xs[65]", json!(1)).unwrap_err();
        assert!(
            err.to_string().contains("exceeds maximum"),
            "expected exceeds-maximum error, got: {err}"
        );
    }

    #[test]
    fn set_nested_accepts_boundary_array_index() {
        // 64 = MAX_FIELD_ARRAY_INDEX — the largest accepted index. The array
        // is null-padded to length 65 (indices 0..=64).
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        set_nested(&mut root, "xs[64]", json!("ok")).unwrap();
        let arr = root["xs"].as_array().unwrap();
        assert_eq!(arr.len(), 65);
        assert_eq!(arr[64], json!("ok"));
        assert_eq!(arr[0], json!(null));
    }

    #[test]
    fn set_nested_accepts_small_index_after_cap_added() {
        // Regression — ordinary small indices still work unchanged.
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        set_nested(&mut root, "xs[0]", json!("a")).unwrap();
        set_nested(&mut root, "xs[1]", json!("b")).unwrap();
        assert_eq!(root, json!({ "xs": ["a", "b"] }));
    }

    #[test]
    fn set_nested_rejects_giant_index_in_multi_dimensional_path() {
        // The cap applies to every `[N]` in a multi-index segment, not just
        // the first.
        let mut root = serde_json::Value::Object(serde_json::Map::new());
        let err = set_nested(&mut root, "xs[0][999999]", json!(1)).unwrap_err();
        assert!(
            err.to_string().contains("exceeds maximum"),
            "expected exceeds-maximum error, got: {err}"
        );
    }

    // ───────────────────────────────────────────────────────────────────────
    // Phase 7 T-B2 — liquidity builder fixtures
    // ───────────────────────────────────────────────────────────────────────

    fn weth_amount_pair() -> serde_json::Value {
        json!([
            {
                "asset": {
                    "kind": "erc20",
                    "address": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
                },
                "amount": { "kind": "min", "value": "1000000000000000000" }
            },
            {
                "asset": {
                    "kind": "erc20",
                    "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                },
                "amount": { "kind": "min", "value": "2000000000" }
            }
        ])
    }

    fn nft_position_object() -> serde_json::Value {
        json!({
            "kind": "erc721",
            "address": "0xc36442b4a4522e871399cd717abdd847ab11fe88",
            "tokenId": "42"
        })
    }

    fn pool_object() -> serde_json::Value {
        json!({
            "address": "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc"
        })
    }

    fn validity_object() -> serde_json::Value {
        json!({
            "expiresAt": "1700000000",
            "source": "tx-deadline"
        })
    }

    #[test]
    fn build_add_liquidity_envelope_from_v2_args() {
        let tree = json!({
            "pool": pool_object(),
            "inputTokens": weth_amount_pair(),
            "outputLp": {
                "asset": {
                    "kind": "erc20",
                    "address": "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc"
                },
                "amount": { "kind": "min", "value": "1000000000000000000" }
            },
            "recipient": "0x3333333333333333333333333333333333333333",
            "validity": validity_object()
        });

        let envelope = build_add_liquidity_envelope(&tree).unwrap();
        assert_eq!(envelope.category, Category::Dex);
        let Action::AddLiquidity(action) = envelope.action else {
            panic!("expected Action::AddLiquidity, got {:?}", envelope.action);
        };
        assert_eq!(action.inputs.len(), 2);
        assert!(action.validity.is_some());
        assert_eq!(
            action.recipient.to_string(),
            "0x3333333333333333333333333333333333333333"
        );
        assert_eq!(action.pool.id, None);
    }

    #[test]
    fn build_remove_liquidity_envelope_from_v2_args() {
        let tree = json!({
            "exitMode": "proportional",
            "pool": pool_object(),
            "inputLp": {
                "asset": {
                    "kind": "erc20",
                    "address": "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc"
                },
                "amount": { "kind": "exact", "value": "100000000000000000" }
            },
            "outputTokens": weth_amount_pair(),
            "recipient": "0x3333333333333333333333333333333333333333"
        });

        let envelope = build_remove_liquidity_envelope(&tree).unwrap();
        let Action::RemoveLiquidity(action) = envelope.action else {
            panic!("expected Action::RemoveLiquidity");
        };
        assert_eq!(action.exit_mode, RemoveLiquidityExitMode::Proportional);
        assert_eq!(action.outputs.len(), 2);
        assert!(action.validity.is_none());
    }

    #[test]
    fn build_mint_liquidity_nft_envelope_from_v3_args() {
        let tree = json!({
            "pool": pool_object(),
            "feeBps": 30,
            "tickRange": { "lower": -887220, "upper": 887220 },
            "inputTokens": weth_amount_pair(),
            "recipient": "0x3333333333333333333333333333333333333333",
            "validity": validity_object()
        });

        let envelope = build_mint_liquidity_nft_envelope(&tree).unwrap();
        let Action::MintLiquidityNft(action) = envelope.action else {
            panic!("expected Action::MintLiquidityNft");
        };
        assert_eq!(action.fee_tier_bps, 30);
        assert_eq!(action.tick_range.lower, -887220);
        assert_eq!(action.tick_range.upper, 887220);
        assert_eq!(action.inputs.len(), 2);
    }

    #[test]
    fn build_increase_liquidity_envelope_from_v3_args() {
        let tree = json!({
            "nft": nft_position_object(),
            "inputTokens": weth_amount_pair(),
            "validity": validity_object()
        });

        let envelope = build_increase_liquidity_envelope(&tree).unwrap();
        let Action::IncreaseLiquidity(action) = envelope.action else {
            panic!("expected Action::IncreaseLiquidity");
        };
        assert_eq!(action.nft.kind, AssetKind::Erc721);
        assert_eq!(
            action.nft.token_id.as_ref().map(ToString::to_string),
            Some("42".to_owned())
        );
        assert_eq!(action.inputs.len(), 2);
        assert!(action.validity.is_some());
    }

    #[test]
    fn build_decrease_liquidity_envelope_from_v3_args() {
        let tree = json!({
            "nft": nft_position_object(),
            "liquidityDelta": { "kind": "exact", "value": "1000000000000000000" },
            "outputTokens": weth_amount_pair(),
            "recipient": "0x3333333333333333333333333333333333333333"
        });

        let envelope = build_decrease_liquidity_envelope(&tree).unwrap();
        let Action::DecreaseLiquidity(action) = envelope.action else {
            panic!("expected Action::DecreaseLiquidity");
        };
        assert_eq!(action.liquidity_delta.kind, AmountKind::Exact);
        assert_eq!(
            action
                .liquidity_delta
                .value
                .as_ref()
                .map(ToString::to_string),
            Some("1000000000000000000".to_owned())
        );
        assert_eq!(action.outputs.len(), 2);
        assert_eq!(
            action.recipient.as_ref().map(ToString::to_string),
            Some("0x3333333333333333333333333333333333333333".to_owned())
        );
    }

    #[test]
    fn build_initialize_pool_envelope_from_v4_args() {
        // V4_INITIALIZE_POOL (UR 0x13) — flat field tree after the manifest
        // evaluates `$.args.poolKey.{currency0,currency1,fee,tickSpacing,hooks}`
        // + `$.args.sqrtPriceX96`. The manifest's per-opcode rule maps each
        // selector into the `set_nested` dot-path that this builder consumes.
        let tree = json!({
            "pool": { "address": "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee" },
            "token0": {
                "kind": "erc20",
                "address": "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
            },
            "token1": {
                "kind": "erc20",
                "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
            },
            "feeBps": 3000,
            "tickSpacing": 60,
            "hooks": "0x9999999999999999999999999999999999999999",
            "sqrtPriceX96": "79228162514264337593543950336"
        });

        let envelope = build_initialize_pool_envelope(&tree).unwrap();
        assert_eq!(envelope.category, Category::Dex);
        let Action::InitializePool(action) = envelope.action else {
            panic!("expected Action::InitializePool, got {:?}", envelope.action);
        };
        assert_eq!(action.fee_bps, 3000);
        assert_eq!(action.tick_spacing, Some(60));
        assert_eq!(
            action.token0.address.as_ref().map(ToString::to_string),
            Some("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_owned())
        );
        assert_eq!(
            action.token1.address.as_ref().map(ToString::to_string),
            Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_owned())
        );
        assert_eq!(
            action.hooks.as_ref().map(ToString::to_string),
            Some("0x9999999999999999999999999999999999999999".to_owned())
        );
        assert_eq!(
            action.sqrt_price_x96.as_ref().map(ToString::to_string),
            Some("79228162514264337593543950336".to_owned())
        );
        // is_dynamic_fee and hook_permissions are host-derive territory.
        assert!(action.is_dynamic_fee.is_none());
        assert!(action.hook_permissions.is_none());
    }

    #[test]
    fn build_initialize_pool_envelope_from_v4_args_no_hooks() {
        // Non-hooked pool — manifest may either emit the zero address or
        // omit the field entirely. Builder must accept both.
        let tree = json!({
            "pool": { "address": "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee" },
            "token0": {
                "kind": "erc20",
                "address": "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
            },
            "token1": {
                "kind": "erc20",
                "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
            },
            "feeBps": 500
        });

        let envelope = build_initialize_pool_envelope(&tree).unwrap();
        let Action::InitializePool(action) = envelope.action else {
            panic!("expected Action::InitializePool");
        };
        assert_eq!(action.fee_bps, 500);
        assert!(action.tick_spacing.is_none());
        assert!(action.sqrt_price_x96.is_none());
        assert!(action.hooks.is_none());
    }

    #[test]
    fn permit_with_permit2_batch_kind_succeeds() {
        // Mirror of the Permit2Single tree fixture — Permit2Batch follows
        // the same validation contract (requires amount, spender,
        // signatureValidity). The mapper layer collapses the on-chain
        // PermitBatch.details[] down to a single details[0] entry so the
        // schema can carry a single `token` slot; full fan-out is a
        // follow-up.
        let tree = json!({
            "permitKind": "permit2_batch",
            "token": {
                "kind": "erc20",
                "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
            },
            "owner": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "spender": "0x1111111111111111111111111111111111111111",
            "amount": { "kind": "max", "value": "1000000" },
            "validity": { "expiresAt": "1800000000", "source": "grant-expiration" },
            "signatureValidity": {
                "expiresAt": "1700000900",
                "source": "signature-deadline"
            }
        });
        let envelope = build_permit_envelope(&tree).unwrap();
        let Action::Permit(action) = envelope.action else {
            panic!("expected Action::Permit");
        };
        assert_eq!(action.permit_kind, PermitKind::Permit2Batch);
        assert_eq!(
            action
                .signature_validity
                .as_ref()
                .map(|v| v.expires_at.to_string()),
            Some("1700000900".to_owned())
        );
    }

    #[test]
    fn build_burn_liquidity_nft_envelope_from_v3_args() {
        let tree = json!({
            "nft": nft_position_object(),
            "burnKind": "empty_only"
        });

        let envelope = build_burn_liquidity_nft_envelope(&tree).unwrap();
        let Action::BurnLiquidityNft(action) = envelope.action else {
            panic!("expected Action::BurnLiquidityNft");
        };
        assert_eq!(action.burn_kind, BurnKind::EmptyOnly);
        assert!(action.outputs.is_none());
        assert!(action.recipient.is_none());
        assert!(action.validity.is_none());
        assert_eq!(
            action.nft.token_id.as_ref().map(ToString::to_string),
            Some("42".to_owned())
        );
    }

    // ───────────────────────────────────────────────────────────────────────
    // T-TEST-PERMIT — PERMIT2_PERMIT / PERMIT2_TRANSFER_FROM / 0x0d edge
    // cases. These exercise the JSON-tree → envelope builder boundary that
    // the UR `0x0a`, `0x02`, and `0x0d` manifest rules ultimately reach.
    // The mapper produces an envelope as long as the structural inputs are
    // well-typed; Cedar (`permit-max-amount`, `expired-deadline`) is
    // responsible for any value-level deny/warn verdicts.
    // ───────────────────────────────────────────────────────────────────────

    /// Manifest §0x0a maps token+owner from `permitSingle[0][0]` (same address
    /// — the token contract under permit). `permit-max-amount` policy keys
    /// off `context.amount.value`, so the JSON helper here mirrors what the
    /// UR bundle produces after JsonPath evaluation.
    fn permit2_single_tree(
        token: &str,
        spender: Option<&str>,
        amount_value: &str,
        expiration: &str,
        sig_deadline: &str,
    ) -> serde_json::Value {
        let mut tree = json!({
            "permitKind": "permit2_single",
            "token": { "kind": "erc20", "address": token },
            "owner": token,
            "amount": { "kind": "max", "value": amount_value },
            "validity": { "expiresAt": expiration, "source": "grant-expiration" },
            "signatureValidity": { "expiresAt": sig_deadline, "source": "signature-deadline" }
        });
        if let Some(spender_addr) = spender {
            tree.as_object_mut().unwrap().insert(
                "spender".to_owned(),
                serde_json::Value::String(spender_addr.to_owned()),
            );
        }
        tree
    }

    #[test]
    fn permit2_permit_with_expired_deadline_succeeds_at_mapper_level() {
        // Cedar `expired-deadline` is the layer that flags `validityDeltaSec <= 0`;
        // the mapper itself just emits a well-formed envelope and surfaces the
        // expiresAt timestamp.
        let tree = permit2_single_tree(
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            Some("0x1111111111111111111111111111111111111111"),
            "1000000",
            "1", // expired (Unix epoch + 1 second)
            "2", // also expired
        );
        let envelope = build_permit_envelope(&tree).unwrap();
        let Action::Permit(action) = envelope.action else {
            panic!("expected Action::Permit");
        };
        assert_eq!(action.permit_kind, PermitKind::Permit2Single);
        assert_eq!(action.validity.expires_at.to_string(), "1".to_owned());
        assert_eq!(action.validity.source, ValiditySource::GrantExpiration);
        assert_eq!(envelope.category, Category::Misc);
    }

    #[test]
    fn permit2_permit_with_max_uint160_amount_succeeds() {
        // `2^160 - 1` — the canonical "drainer signature" value Cedar's
        // `permit-max-amount` policy matches exactly. The mapper must preserve
        // the decimal string byte-for-byte (no scientific notation, no leading
        // zeros) so the policy comparison hits.
        let max_uint160 = "1461501637330902918203684832716283019655932542975";
        let tree = permit2_single_tree(
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            Some("0x1111111111111111111111111111111111111111"),
            max_uint160,
            "1700000000",
            "1700000900",
        );
        let envelope = build_permit_envelope(&tree).unwrap();
        let Action::Permit(action) = envelope.action else {
            panic!("expected Action::Permit");
        };
        let amount = action
            .amount
            .as_ref()
            .expect("amount must be present for permit2_single");
        assert_eq!(amount.kind, AmountKind::Max);
        assert_eq!(
            amount.value.as_ref().map(ToString::to_string),
            Some(max_uint160.to_owned()),
            "Cedar permit-max-amount keys off byte-exact equality with this string"
        );
    }

    #[test]
    fn permit2_permit_with_zero_spender_succeeds() {
        let tree = permit2_single_tree(
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            Some("0x0000000000000000000000000000000000000000"),
            "1000",
            "1700000000",
            "1700000900",
        );
        let envelope = build_permit_envelope(&tree).unwrap();
        let Action::Permit(action) = envelope.action else {
            panic!("expected Action::Permit");
        };
        // Spender stays as the zero address — schema-valid even if economically
        // suspicious; Cedar may flag it via a custom blocklist policy.
        assert_eq!(
            action.spender.as_ref().map(ToString::to_string),
            Some("0x0000000000000000000000000000000000000000".to_owned())
        );
    }

    #[test]
    fn permit2_permit_sig_deadline_separate_from_expiration() {
        // Permit2's `details.expiration` (uint48) is the allowance lifetime —
        // mapped to `validity` (`grant-expiration`). `sigDeadline` (uint256) is
        // the EIP-712 signature relay window — mapped to `signatureValidity`
        // (`signature-deadline`). They are independent fields and must NOT
        // collapse into a single Validity.
        let tree = permit2_single_tree(
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            Some("0x1111111111111111111111111111111111111111"),
            "1000",
            "1800000000", // expiration far in the future
            "1700000900", // sigDeadline ~90s after current block
        );
        let envelope = build_permit_envelope(&tree).unwrap();
        let Action::Permit(action) = envelope.action else {
            panic!("expected Action::Permit");
        };
        assert_eq!(action.validity.expires_at.to_string(), "1800000000");
        assert_eq!(action.validity.source, ValiditySource::GrantExpiration);
        let sig_validity = action
            .signature_validity
            .as_ref()
            .expect("permit2_single MUST carry a signature_validity");
        assert_eq!(sig_validity.expires_at.to_string(), "1700000900");
        assert_eq!(sig_validity.source, ValiditySource::SignatureDeadline);
        // The two windows must remain distinct.
        assert_ne!(
            action.validity.expires_at.to_string(),
            sig_validity.expires_at.to_string()
        );
    }

    /// Build a transfer tree mirroring what manifest 0x0d emits when the JSON
    /// path resolves `transferDetails[0][2]` (amount) and `transferDetails[0][3]`
    /// (token). The mapper itself doesn't see the array — it only sees the
    /// flattened single-element fields.
    fn transfer_tree(
        token: &str,
        from: &str,
        recipient: &str,
        amount_value: &str,
    ) -> serde_json::Value {
        json!({
            "token": {
                "asset": { "kind": "erc20", "address": token },
                "amount": { "kind": "exact", "value": amount_value }
            },
            "from": from,
            "recipient": recipient
        })
    }

    #[test]
    fn permit2_transfer_from_batch_with_single_element_emit() {
        // transferDetails = [(from, to, amount, token)] — the manifest emits
        // exactly the first element. With a single element this is the full
        // batch faithfully represented.
        let tree = transfer_tree(
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "0x2222222222222222222222222222222222222222",
            "0x3333333333333333333333333333333333333333",
            "500000",
        );
        let envelope = build_transfer_envelope(&tree).unwrap();
        let Action::Transfer(action) = envelope.action else {
            panic!("expected Action::Transfer");
        };
        assert_eq!(action.token.asset.kind, AssetKind::Erc20);
        assert_eq!(
            action.token.asset.address.as_ref().map(ToString::to_string),
            Some("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_owned())
        );
        assert_eq!(action.token.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.token.amount.value.as_ref().map(ToString::to_string),
            Some("500000".to_owned())
        );
        assert_eq!(
            action.recipient.to_string(),
            "0x3333333333333333333333333333333333333333"
        );
    }

    #[test]
    fn permit2_transfer_from_batch_with_multi_element_emits_only_first() {
        // Documented PoC limitation: array fan-out is not supported. Manifest
        // 0x0d hard-codes `transferDetails[0][...]` — the second and third
        // entries are silently ignored. This test pins that behaviour so any
        // future array-fan-out work explicitly updates it.
        // Caller-side: imagine the original transferDetails was
        //   [ (alice, bob,   100, USDC),
        //     (alice, carol, 200, DAI),
        //     (alice, dave,  300, WETH) ].
        // Only the first row reaches the mapper after manifest evaluation.
        let tree = transfer_tree(
            "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
            "0x2222222222222222222222222222222222222222",
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "100",
        );
        let envelope = build_transfer_envelope(&tree).unwrap();
        let Action::Transfer(action) = envelope.action else {
            panic!("expected Action::Transfer");
        };
        // First element's recipient is preserved — second/third are lost.
        assert_eq!(
            action.recipient.to_string(),
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        assert_eq!(
            action.token.amount.value.as_ref().map(ToString::to_string),
            Some("100".to_owned()),
            "the 200 and 300 entries from rows 2/3 must not bleed into row 1"
        );
    }

    // ───────────────────────────────────────────────────────────────────────
    // Phase 8 — Aerodrome ve(3,3) builders (gauge_vote / lp_stake /
    // lp_unstake / lock_create / lock_increase / lock_manage)
    // ───────────────────────────────────────────────────────────────────────

    fn aero_voter() -> &'static str {
        "0x16613524e02ad97edfef371bc883f2f5d6c480a5"
    }

    fn aero_voting_escrow() -> &'static str {
        "0xfaf8fd17d9840595845582fcb047df13f006787d"
    }

    fn aero_lp_token() -> serde_json::Value {
        json!({
            "kind": "erc20",
            "address": "0x0b25c51637c43decd6cc1c1e3da4395f74dfdb98"
        })
    }

    fn aero_asset() -> serde_json::Value {
        // AERO ERC20 on Base.
        json!({
            "kind": "erc20",
            "address": "0x940181a94a35a4569e4529a3cdfb74e38fd98631"
        })
    }

    #[test]
    fn build_gauge_vote_envelope_minimal() {
        let tree = json!({
            "voter": aero_voter(),
            "tokenId": "1",
            "pools": ["0x0000000000000000000000000000000000000001"],
            "weights": ["100"]
        });
        let envelope = build_gauge_vote_envelope(&tree).expect("build OK");
        assert_eq!(envelope.category, Category::Misc);
        assert_eq!(envelope.action.kind(), "gauge_vote");
        let Action::GaugeVote(action) = envelope.action else {
            panic!("expected Action::GaugeVote");
        };
        assert_eq!(action.pools.len(), 1);
        assert_eq!(action.weights.len(), 1);
        assert_eq!(action.token_id.as_ref().unwrap().to_string(), "1");
        assert!(action.kind.is_none());
        assert!(action.validity.is_none());
    }

    #[test]
    fn build_gauge_vote_envelope_reset_empty_pools() {
        let tree = json!({
            "voter": aero_voter(),
            "tokenId": "1",
            "pools": [],
            "weights": [],
            "kind": "reset"
        });
        let envelope = build_gauge_vote_envelope(&tree).expect("build OK");
        let Action::GaugeVote(action) = envelope.action else {
            panic!("expected Action::GaugeVote");
        };
        assert_eq!(action.pools.len(), 0);
        assert_eq!(action.weights.len(), 0);
        assert_eq!(action.kind, Some(GaugeVoteKind::Reset));
    }

    #[test]
    fn build_gauge_vote_envelope_pools_weights_length_mismatch_errors() {
        let tree = json!({
            "voter": aero_voter(),
            "tokenId": "1",
            "pools": ["0x0000000000000000000000000000000000000001"],
            "weights": []
        });
        let err = build_gauge_vote_envelope(&tree).unwrap_err();
        assert!(
            err.to_string().contains("pools.len()=1 != weights.len()=0"),
            "expected length mismatch error, got: {err}"
        );
    }

    #[test]
    fn build_gauge_vote_envelope_with_validity() {
        let tree = json!({
            "voter": aero_voter(),
            "tokenId": "42",
            "pools": [
                "0x0000000000000000000000000000000000000001",
                "0x0000000000000000000000000000000000000002"
            ],
            "weights": ["50", "50"],
            "kind": "vote",
            "validity": validity_object()
        });
        let envelope = build_gauge_vote_envelope(&tree).expect("build OK");
        let Action::GaugeVote(action) = envelope.action else {
            panic!("expected Action::GaugeVote");
        };
        assert_eq!(action.pools.len(), 2);
        assert_eq!(action.kind, Some(GaugeVoteKind::Vote));
        assert!(action.validity.is_some());
    }

    #[test]
    fn build_lp_stake_envelope_minimal() {
        let tree = json!({
            "gauge": "0x1111111111111111111111111111111111111111",
            "lpToken": {
                "asset": aero_lp_token(),
                "amount": { "kind": "exact", "value": "1000000000000000000" }
            },
            "recipient": "0x3333333333333333333333333333333333333333"
        });
        let envelope = build_lp_stake_envelope(&tree).expect("build OK");
        assert_eq!(envelope.category, Category::Misc);
        assert_eq!(envelope.action.kind(), "lp_stake");
        let Action::LpStake(action) = envelope.action else {
            panic!("expected Action::LpStake");
        };
        assert_eq!(
            action.gauge.to_string(),
            "0x1111111111111111111111111111111111111111"
        );
        assert_eq!(action.lp_token.asset.kind, AssetKind::Erc20);
        assert_eq!(action.lp_token.amount.kind, AmountKind::Exact);
        assert_eq!(
            action
                .lp_token
                .amount
                .value
                .as_ref()
                .map(ToString::to_string),
            Some("1000000000000000000".to_owned())
        );
    }

    #[test]
    fn build_lp_stake_envelope_missing_amount_errors() {
        let tree = json!({
            "gauge": "0x1111111111111111111111111111111111111111",
            "lpToken": {
                "asset": aero_lp_token()
            },
            "recipient": "0x3333333333333333333333333333333333333333"
        });
        let err = build_lp_stake_envelope(&tree).unwrap_err();
        match err {
            MapperError::MissingArgument(name) => assert_eq!(name, "lpToken.amount"),
            other => panic!("expected MissingArgument(\"lpToken.amount\"), got {other:?}"),
        }
    }

    #[test]
    fn build_lp_unstake_envelope_minimal() {
        let tree = json!({
            "gauge": "0x1111111111111111111111111111111111111111",
            "lpToken": {
                "asset": aero_lp_token(),
                "amount": { "kind": "exact", "value": "500000000000000000" }
            },
            "recipient": "0x3333333333333333333333333333333333333333"
        });
        let envelope = build_lp_unstake_envelope(&tree).expect("build OK");
        assert_eq!(envelope.action.kind(), "lp_unstake");
        let Action::LpUnstake(action) = envelope.action else {
            panic!("expected Action::LpUnstake");
        };
        assert_eq!(action.lp_token.amount.kind, AmountKind::Exact);
        assert_eq!(
            action
                .lp_token
                .amount
                .value
                .as_ref()
                .map(ToString::to_string),
            Some("500000000000000000".to_owned())
        );
        assert_eq!(
            action.recipient.to_string(),
            "0x3333333333333333333333333333333333333333"
        );
    }

    #[test]
    fn build_lp_unstake_envelope_missing_gauge_errors() {
        let tree = json!({
            "lpToken": {
                "asset": aero_lp_token(),
                "amount": { "kind": "exact", "value": "1" }
            },
            "recipient": "0x3333333333333333333333333333333333333333"
        });
        let err = build_lp_unstake_envelope(&tree).unwrap_err();
        match err {
            MapperError::MissingArgument(name) => assert_eq!(name, "$.gauge"),
            other => panic!("expected MissingArgument, got {other:?}"),
        }
    }

    #[test]
    fn build_lock_create_envelope_minimal() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "asset": {
                "asset": aero_asset(),
                "amount": { "kind": "exact", "value": "1000000000000000000" }
            },
            "lockDurationSec": "126144000",
            "recipient": "0x3333333333333333333333333333333333333333"
        });
        let envelope = build_lock_create_envelope(&tree).expect("build OK");
        assert_eq!(envelope.category, Category::Misc);
        assert_eq!(envelope.action.kind(), "lock_create");
        let Action::LockCreate(action) = envelope.action else {
            panic!("expected Action::LockCreate");
        };
        assert_eq!(action.voting_escrow.to_string(), aero_voting_escrow());
        assert_eq!(action.asset.asset.kind, AssetKind::Erc20);
        assert_eq!(
            action.lock_duration_sec.as_ref().unwrap().to_string(),
            "126144000"
        );
    }

    #[test]
    fn build_lock_create_envelope_missing_both_errors() {
        // F7 — lock_create requires exactly one of lockDurationSec / unlockTime.
        // Neither present → XOR error.
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "asset": {
                "asset": aero_asset(),
                "amount": { "kind": "exact", "value": "1" }
            },
            "recipient": "0x3333333333333333333333333333333333333333"
        });
        let err = build_lock_create_envelope(&tree).unwrap_err();
        match err {
            MapperError::Internal(e) => assert!(
                e.to_string()
                    .contains("one of lockDurationSec / unlockTime"),
                "unexpected error: {e}"
            ),
            other => panic!("expected Internal XOR error, got {other:?}"),
        }
    }

    #[test]
    fn build_lock_create_envelope_unlock_time_only() {
        // F7 — Curve veCRV path: absolute unlockTime, no relative lockDurationSec.
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "asset": {
                "asset": aero_asset(),
                "amount": { "kind": "exact", "value": "1" }
            },
            "unlockTime": "1905548203",
            "recipient": "0x3333333333333333333333333333333333333333"
        });
        let envelope = build_lock_create_envelope(&tree).expect("unlockTime-only build OK");
        let Action::LockCreate(action) = envelope.action else {
            panic!("expected Action::LockCreate");
        };
        assert!(action.lock_duration_sec.is_none());
        assert_eq!(
            action.unlock_time.as_ref().unwrap().to_string(),
            "1905548203"
        );
    }

    #[test]
    fn build_lock_increase_envelope_amount_kind() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "tokenId": "42",
            "kind": "amount",
            "additionalAmount": { "kind": "exact", "value": "500000000000000000" }
        });
        let envelope = build_lock_increase_envelope(&tree).expect("build OK");
        assert_eq!(envelope.action.kind(), "lock_increase");
        let Action::LockIncrease(action) = envelope.action else {
            panic!("expected Action::LockIncrease");
        };
        assert_eq!(action.kind, LockIncreaseKind::Amount);
        assert_eq!(
            action.token_id.as_ref().map(ToString::to_string),
            Some("42".to_owned())
        );
        let additional = action
            .additional_amount
            .as_ref()
            .expect("amount kind requires additionalAmount");
        assert_eq!(additional.kind, AmountKind::Exact);
        assert!(action.new_lock_duration_sec.is_none());
    }

    #[test]
    fn build_lock_increase_envelope_unlock_time_kind() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "tokenId": "42",
            "kind": "unlock_time",
            "newLockDurationSec": "126144000"
        });
        let envelope = build_lock_increase_envelope(&tree).expect("build OK");
        let Action::LockIncrease(action) = envelope.action else {
            panic!("expected Action::LockIncrease");
        };
        assert_eq!(action.kind, LockIncreaseKind::UnlockTime);
        assert!(action.additional_amount.is_none());
        assert_eq!(
            action
                .new_lock_duration_sec
                .as_ref()
                .map(ToString::to_string),
            Some("126144000".to_owned())
        );
    }

    #[test]
    fn build_lock_increase_envelope_missing_kind_errors() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "tokenId": "42",
            "additionalAmount": { "kind": "exact", "value": "1" }
        });
        let err = build_lock_increase_envelope(&tree).unwrap_err();
        match err {
            MapperError::MissingArgument(name) => assert_eq!(name, "kind"),
            other => panic!("expected MissingArgument(\"kind\"), got {other:?}"),
        }
    }

    #[test]
    fn build_lock_manage_envelope_merge() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "kind": "merge",
            "fromTokenId": "1",
            "toTokenId": "2"
        });
        let envelope = build_lock_manage_envelope(&tree).expect("build OK");
        assert_eq!(envelope.category, Category::Misc);
        assert_eq!(envelope.action.kind(), "lock_manage");
        let Action::LockManage(action) = envelope.action else {
            panic!("expected Action::LockManage");
        };
        assert_eq!(action.kind, LockManageKind::Merge);
        assert_eq!(action.from_token_id.to_string(), "1");
        assert_eq!(
            action.to_token_id.as_ref().map(ToString::to_string),
            Some("2".to_owned())
        );
        assert!(action.split_ratio.is_none());
    }

    #[test]
    fn build_lock_manage_envelope_split() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "kind": "split",
            "fromTokenId": "1",
            "splitRatio": "500000000000000000"
        });
        let envelope = build_lock_manage_envelope(&tree).expect("build OK");
        let Action::LockManage(action) = envelope.action else {
            panic!("expected Action::LockManage");
        };
        assert_eq!(action.kind, LockManageKind::Split);
        assert!(action.to_token_id.is_none());
        assert_eq!(
            action.split_ratio.as_ref().map(ToString::to_string),
            Some("500000000000000000".to_owned())
        );
    }

    #[test]
    fn build_lock_manage_envelope_missing_kind_errors() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "fromTokenId": "1"
        });
        let err = build_lock_manage_envelope(&tree).unwrap_err();
        match err {
            MapperError::MissingArgument(name) => assert_eq!(name, "kind"),
            other => panic!("expected MissingArgument(\"kind\"), got {other:?}"),
        }
    }

    // ───────────────────────────────────────────────────────────────────────
    // Phase 8 Round 5 fix — claim_rewards builder (Aerodrome voter, gauge,
    // Slipstream NPM collect)
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn build_claim_rewards_envelope_minimal() {
        let tree = json!({
            "from": "0x1111111111111111111111111111111111111111",
            "recipient": "0x2222222222222222222222222222222222222222"
        });
        let envelope = build_claim_rewards_envelope(&tree).expect("build OK");
        assert_eq!(envelope.category, Category::Misc);
        assert_eq!(envelope.action.kind(), "claim_rewards");
        let Action::ClaimRewards(action) = envelope.action else {
            panic!("expected Action::ClaimRewards");
        };
        assert!(action.source.is_none());
        assert!(action.nft.is_none());
        assert!(action.token_id.is_none());
        assert!(action.reward_tokens.is_none());
        assert!(action.max_amounts.is_none());
    }

    #[test]
    fn build_claim_rewards_envelope_full() {
        // Mirrors what set_nested produces for the Aerodrome voter/claimBribes
        // bundle: source.{address,label} merge into a SourceRef, and
        // rewardTokens[0].{kind,address} merges into a single-element array.
        let tree = json!({
            "source": {
                "address": "0x3333333333333333333333333333333333333333",
                "label": "Aerodrome Voter"
            },
            "nft": {
                "kind": "erc721",
                "address": "0x4444444444444444444444444444444444444444"
            },
            "tokenId": "42",
            "from": "0x1111111111111111111111111111111111111111",
            "recipient": "0x2222222222222222222222222222222222222222",
            "rewardTokens": [
                { "kind": "erc20", "address": "0x5555555555555555555555555555555555555555" }
            ]
        });
        let envelope = build_claim_rewards_envelope(&tree).expect("build OK");
        let Action::ClaimRewards(action) = envelope.action else {
            panic!("expected Action::ClaimRewards");
        };
        let source = action.source.as_ref().expect("source present");
        assert_eq!(
            source.address.as_ref().map(ToString::to_string),
            Some("0x3333333333333333333333333333333333333333".to_owned())
        );
        assert_eq!(source.label.as_deref(), Some("Aerodrome Voter"));
        let nft = action.nft.as_ref().expect("nft present");
        assert_eq!(nft.kind, AssetKind::Erc721);
        // dual-tokenId 정합성: root-level tokenId 가 AssetRef.token_id 로 inject.
        // schema 의 dual-emit 패턴 (nft.AssetRef + root tokenId) 에서 evaluate
        // engine 의 AssetRef invariant (kind=erc721/1155 → tokenId required) 가
        // 만족되는지 검증.
        assert_eq!(
            nft.token_id.as_ref().map(ToString::to_string),
            Some("42".to_owned()),
            "nft.token_id should be injected from root-level tokenId"
        );
        assert_eq!(
            action.token_id.as_ref().map(ToString::to_string),
            Some("42".to_owned())
        );
        let rewards = action.reward_tokens.as_ref().expect("rewardTokens present");
        assert_eq!(rewards.len(), 1);
        assert_eq!(rewards[0].kind, AssetKind::Erc20);
    }

    #[test]
    fn build_claim_rewards_envelope_injects_root_tokenid_into_nft() {
        // Track A fix — NFPM `collect` 의 dual-emit manifest (nft.kind + nft.address
        // + root tokenId) 에서 nft AssetRef 가 token_id 없이 생성되면 evaluate
        // engine 의 AssetRef invariant 위반. root-level tokenId 가 있으면 inject.
        let tree = json!({
            "nft": {
                "kind": "erc721",
                "address": "0x03a520b32c04bf3beef7beb72e919cf822ed34f1"
            },
            "tokenId": "5181335",
            "from": "0x676fa5b94067c2be14bc025df6c5c80dedf49a54",
            "recipient": "0x676fa5b94067c2be14bc025df6c5c80dedf49a54"
        });
        let envelope = build_claim_rewards_envelope(&tree).expect("build OK");
        let Action::ClaimRewards(action) = envelope.action else {
            panic!("expected Action::ClaimRewards");
        };
        let nft = action.nft.as_ref().expect("nft present");
        assert_eq!(nft.kind, AssetKind::Erc721);
        assert_eq!(
            nft.token_id.as_ref().map(ToString::to_string),
            Some("5181335".to_owned()),
            "nft.token_id must inherit root-level tokenId"
        );
        assert_eq!(
            action.token_id.as_ref().map(ToString::to_string),
            Some("5181335".to_owned())
        );
    }

    #[test]
    fn build_claim_rewards_envelope_no_inject_when_nft_absent() {
        // nft 가 없으면 root-level tokenId 가 있어도 inject 대상 없음. 단순
        // action.token_id 로만 채워짐 (Aerodrome voter/Compound rewards 같은
        // 비-NFT claim 시나리오).
        let tree = json!({
            "tokenId": "999",
            "from": "0x1111111111111111111111111111111111111111",
            "recipient": "0x2222222222222222222222222222222222222222"
        });
        let envelope = build_claim_rewards_envelope(&tree).expect("build OK");
        let Action::ClaimRewards(action) = envelope.action else {
            panic!("expected Action::ClaimRewards");
        };
        assert!(action.nft.is_none());
        assert_eq!(
            action.token_id.as_ref().map(ToString::to_string),
            Some("999".to_owned())
        );
    }

    #[test]
    fn build_claim_rewards_envelope_via_set_nested_paths() {
        // End-to-end: a fields-tree built via [`set_nested`] from the
        // Aerodrome voter/claimBribes manifest's dot-paths should feed
        // straight into the builder. This pins the `[N]` indexing contract
        // between manifest evaluation and the builder.
        let mut tree = serde_json::Value::Object(serde_json::Map::new());
        set_nested(
            &mut tree,
            "source.address",
            json!("0x16613524e02ad97edfef371bc883f2f5d6c480a5"),
        )
        .unwrap();
        set_nested(&mut tree, "source.label", json!("Aerodrome Voter (Bribes)")).unwrap();
        set_nested(&mut tree, "tokenId", json!("42")).unwrap();
        set_nested(
            &mut tree,
            "from",
            json!("0x1111111111111111111111111111111111111111"),
        )
        .unwrap();
        set_nested(
            &mut tree,
            "recipient",
            json!("0x1111111111111111111111111111111111111111"),
        )
        .unwrap();
        set_nested(&mut tree, "rewardTokens[0].kind", json!("erc20")).unwrap();
        set_nested(
            &mut tree,
            "rewardTokens[0].address",
            json!("0x940181a94a35a4569e4529a3cdfb74e38fd98631"),
        )
        .unwrap();

        let envelope = build_claim_rewards_envelope(&tree).expect("build OK");
        let Action::ClaimRewards(action) = envelope.action else {
            panic!("expected Action::ClaimRewards");
        };
        assert_eq!(
            action.source.as_ref().and_then(|s| s.label.as_deref()),
            Some("Aerodrome Voter (Bribes)")
        );
        let rewards = action.reward_tokens.as_ref().expect("rewardTokens present");
        assert_eq!(rewards.len(), 1);
        assert_eq!(
            rewards[0].address.as_ref().map(ToString::to_string),
            Some("0x940181a94a35a4569e4529a3cdfb74e38fd98631".to_owned())
        );
    }

    #[test]
    fn build_claim_rewards_envelope_missing_from_errors() {
        let tree = json!({
            "recipient": "0x2222222222222222222222222222222222222222"
        });
        let err = build_claim_rewards_envelope(&tree).unwrap_err();
        match err {
            MapperError::MissingArgument(name) => assert_eq!(name, "$.from"),
            other => panic!("expected MissingArgument(\"$.from\"), got {other:?}"),
        }
    }

    #[test]
    fn build_claim_rewards_envelope_missing_recipient_errors() {
        let tree = json!({
            "from": "0x1111111111111111111111111111111111111111"
        });
        let err = build_claim_rewards_envelope(&tree).unwrap_err();
        match err {
            MapperError::MissingArgument(name) => assert_eq!(name, "$.recipient"),
            other => panic!("expected MissingArgument(\"$.recipient\"), got {other:?}"),
        }
    }

    #[test]
    fn read_optional_source_ref_returns_none_when_both_fields_null() {
        // A bundle that emits neither source.address nor source.label leaves
        // `source` absent — the helper must NOT manufacture an empty SourceRef.
        let tree = json!({});
        let result = read_optional_source_ref(&tree, "source").expect("OK");
        assert!(result.is_none());
    }

    #[test]
    fn read_optional_asset_inline_array_rejects_sparse_null() {
        let tree = json!({ "rewardTokens": [null, { "kind": "erc20" }] });
        let err = read_optional_asset_inline_array(&tree, "rewardTokens").unwrap_err();
        assert!(
            err.to_string().contains("unexpected null"),
            "expected sparse-array error, got: {err}"
        );
    }

    #[test]
    fn read_address_array_missing_returns_empty() {
        let tree = json!({});
        let result = read_address_array(&tree, "pools").expect("OK");
        assert!(result.is_empty());
    }

    #[test]
    fn read_address_array_rejects_non_string_element() {
        let tree = json!({ "pools": [42] });
        let err = read_address_array(&tree, "pools").unwrap_err();
        assert!(err.to_string().contains("expected string address"));
    }

    #[test]
    fn read_optional_enum_null_returns_none() {
        let tree = json!({ "kind": null });
        let result: Option<GaugeVoteKind> = read_optional_enum(&tree, "kind").expect("OK");
        assert!(result.is_none());
    }

    #[test]
    fn permit2_transfer_from_batch_with_empty_array_yields_error() {
        // If `transferDetails == []`, the manifest's `$.args.transferDetails[0][...]`
        // selector blows up during JsonPath evaluation (out-of-bounds index).
        // The mapper layer never sees a fully-populated tree, so it can't
        // simulate that path end-to-end here. Instead we assert the mapper
        // refuses an envelope that is structurally missing the `token` field —
        // which is what a failed JsonPath lookup leaves behind once the
        // surrounding executor stops populating fields. (Production code's
        // `walk_args` `index ... out of bounds` error is exercised by the
        // `eval.rs` test suite; this test pins the builder's fail-closed
        // contract.)
        let tree = json!({
            // `token` deliberately omitted to mirror an aborted manifest fan-out.
            "from": "0x2222222222222222222222222222222222222222",
            "recipient": "0x3333333333333333333333333333333333333333"
        });
        let err = build_transfer_envelope(&tree).unwrap_err();
        match err {
            MapperError::MissingArgument(name) => {
                assert_eq!(
                    name, "token",
                    "expected MissingArgument(\"token\") for empty-batch fan-out"
                );
            }
            other => panic!("expected MissingArgument(\"token\"), got {other:?}"),
        }
    }

    // ───────────────────────────────────────────────────────────────────────
    // Phase 12.5 — Lending builders unit tests
    //
    // Minimal happy-path assertions for each new builder. End-to-end mapper
    // wiring tests live in `mapper.rs` (read JSON fixture → decoded args →
    // envelope assertions).
    // ───────────────────────────────────────────────────────────────────────

    // Phase B / F1 — build_supply_envelope direct unit test. Mirrors the
    // crvUSD Controller `addCollateral(uint256)` manifest shape (wstETH
    // collateral asset literal + amount + recipient = `$.tx.from`). Pairs
    // with the dispatch-arm regression in
    // `crates/integration-tests/tests/p0_1_action_lowering.rs::
    // supply_lowers_and_forbid_denies`.
    #[test]
    fn build_supply_envelope_from_add_collateral_tree() {
        let tree = json!({
            "market": {
                "address": "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce",
                "label": "Curve crvUSD wstETH Controller"
            },
            "asset": {
                "kind": "erc20",
                "address": "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
            },
            "amount": { "kind": "exact", "value": "1000000000000000000" },
            "recipient": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        });
        let envelope = build_supply_envelope(&tree).unwrap();
        assert_eq!(envelope.category, Category::Lending);
        let Action::Supply(action) = envelope.action else {
            panic!("expected Supply, got {:?}", envelope.action);
        };
        let market = action.market.expect("market present");
        assert_eq!(
            market.address.unwrap().to_string(),
            "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce"
        );
        assert_eq!(
            market.label.as_deref(),
            Some("Curve crvUSD wstETH Controller")
        );
        assert_eq!(action.asset.kind, AssetKind::Erc20);
        assert_eq!(
            action.asset.address.unwrap().to_string(),
            "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
        );
        assert_eq!(action.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.amount.value.unwrap().to_string(),
            "1000000000000000000"
        );
        assert_eq!(
            action.recipient.to_string(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        // 1-arg `addCollateral` carries no `from` (provider = recipient),
        // no `amountMode`, no `validity`.
        assert!(action.from.is_none());
        assert!(action.amount_mode.is_none());
        assert!(action.validity.is_none());
    }

    /// Phase B / F1 — `build_supply_envelope` must reject calldata that lacks
    /// the required `amount` field (the same fail-fast posture as
    /// `build_borrow_envelope`'s missing-amount path).
    #[test]
    fn build_supply_envelope_rejects_missing_amount() {
        let tree = json!({
            "asset": { "kind": "erc20", "address": "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0" },
            "recipient": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        });
        let err = build_supply_envelope(&tree).unwrap_err();
        assert!(
            matches!(err, MapperError::MissingArgument(ref s) if s == "amount"),
            "expected MissingArgument('amount'), got: {err:?}"
        );
    }

    #[test]
    fn build_borrow_envelope_from_args() {
        let tree = json!({
            "market": {
                "address": "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce",
                "label": "Curve crvUSD wstETH Controller"
            },
            "asset": {
                "kind": "erc20",
                "address": "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e"
            },
            "amount": { "kind": "exact", "value": "1000000000000000000000" },
            "recipient": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "onBehalf":  "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        });
        let envelope = build_borrow_envelope(&tree).unwrap();
        assert_eq!(envelope.category, Category::Lending);
        let Action::Borrow(action) = envelope.action else {
            panic!("expected Borrow, got something else");
        };
        let market = action.market.expect("market present");
        assert_eq!(
            market.address.unwrap().to_string(),
            "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce"
        );
        assert_eq!(
            market.label.as_deref(),
            Some("Curve crvUSD wstETH Controller")
        );
        assert_eq!(action.asset.kind, AssetKind::Erc20);
        assert_eq!(
            action.asset.address.unwrap().to_string(),
            "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e"
        );
        assert_eq!(action.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.amount.value.unwrap().to_string(),
            "1000000000000000000000"
        );
        assert_eq!(
            action.recipient.to_string(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(
            action.on_behalf.to_string(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert!(action.validity.is_none());
        assert!(action.amount_mode.is_none());
        // F6 — borrow-only tree (no collateral fields) leaves both `None`.
        assert!(action.collateral_asset.is_none());
        assert!(action.collateral_amount.is_none());
    }

    /// F6 — `create_loan` / `borrow_more` style tree carries a collateral leg.
    #[test]
    fn build_borrow_envelope_with_collateral() {
        let tree = json!({
            "market": {
                "address": "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce",
                "label": "Curve crvUSD wstETH Controller"
            },
            "asset": {
                "kind": "erc20",
                "address": "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e"
            },
            "amount": { "kind": "exact", "value": "6000000000000000000000" },
            "collateralAsset": {
                "kind": "erc20",
                "address": "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
            },
            "collateralAmount": { "kind": "exact", "value": "4130013979725197349" },
            "recipient": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "onBehalf":  "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        });
        let envelope = build_borrow_envelope(&tree).unwrap();
        let Action::Borrow(action) = envelope.action else {
            panic!("expected Borrow, got something else");
        };
        let collateral_asset = action.collateral_asset.expect("collateral asset present");
        assert_eq!(collateral_asset.kind, AssetKind::Erc20);
        assert_eq!(
            collateral_asset.address.unwrap().to_string(),
            "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"
        );
        let collateral_amount = action.collateral_amount.expect("collateral amount present");
        assert_eq!(collateral_amount.kind, AmountKind::Exact);
        assert_eq!(
            collateral_amount.value.unwrap().to_string(),
            "4130013979725197349"
        );
    }

    #[test]
    fn build_repay_envelope_from_args() {
        let tree = json!({
            "market": {
                "address": "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce"
            },
            "asset": {
                "kind": "erc20",
                "address": "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e"
            },
            "amount": { "kind": "exact", "value": "500000000000000000000" },
            "onBehalf": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "repayKind": "debt_asset"
        });
        let envelope = build_repay_envelope(&tree).unwrap();
        assert_eq!(envelope.category, Category::Lending);
        let Action::Repay(action) = envelope.action else {
            panic!("expected Repay, got something else");
        };
        assert_eq!(action.asset.kind, AssetKind::Erc20);
        assert_eq!(action.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.amount.value.unwrap().to_string(),
            "500000000000000000000"
        );
        assert!(matches!(
            action.repay_kind,
            policy_engine::action::lending::RepayKind::DebtAsset
        ));
        assert!(action.validity.is_none());
    }

    #[test]
    fn build_liquidate_envelope_from_args() {
        let tree = json!({
            "market": {
                "address": "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce"
            },
            "borrower": "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "debtAsset": {
                "kind": "erc20",
                "address": "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e"
            },
            "liquidationKind": "pool_share"
        });
        let envelope = build_liquidate_envelope(&tree).unwrap();
        assert_eq!(envelope.category, Category::Lending);
        let Action::Liquidate(action) = envelope.action else {
            panic!("expected Liquidate, got something else");
        };
        assert_eq!(
            action.borrower.to_string(),
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        assert!(action.collateral_asset.is_none());
        assert_eq!(action.debt_asset.kind, AssetKind::Erc20);
        assert!(matches!(
            action.liquidation_kind,
            policy_engine::action::lending::LiquidationKind::PoolShare
        ));
        assert!(action.recipient.is_none());
    }

    // ───────────────────────────────────────────────────────────────────────
    // Phase 12.6 — Staking / Claim / Vote builders unit tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn build_stake_envelope_from_args() {
        let tree = json!({
            "tokenIn": {
                "kind": "erc20",
                "address": "0xd533a949740bb3306d119cc777fa900ba034cd52"
            },
            "receiptToken": {
                "kind": "erc20",
                "address": "0x5f3b5dfeb7b28cdbd7faba78963ee202a494e2a2"
            },
            "amountIn": { "kind": "exact", "value": "1000000000000000000000" },
            "recipient": "0xcccccccccccccccccccccccccccccccccccccccc"
        });
        let envelope = build_stake_envelope(&tree).unwrap();
        assert_eq!(envelope.category, Category::LiquidStaking);
        let Action::Stake(action) = envelope.action else {
            panic!("expected Stake, got something else");
        };
        assert_eq!(action.token_in.kind, AssetKind::Erc20);
        assert_eq!(
            action.token_in.address.unwrap().to_string(),
            "0xd533a949740bb3306d119cc777fa900ba034cd52"
        );
        assert_eq!(
            action.receipt_token.address.unwrap().to_string(),
            "0x5f3b5dfeb7b28cdbd7faba78963ee202a494e2a2"
        );
        assert_eq!(action.amount_in.kind, AmountKind::Exact);
        assert!(action.amount_out.is_none());
    }

    #[test]
    fn build_claim_unstake_envelope_from_args() {
        let tree = json!({
            "tokenOut": {
                "kind": "erc20",
                "address": "0xd533a949740bb3306d119cc777fa900ba034cd52"
            },
            "ticket": {},
            "recipient": "0xcccccccccccccccccccccccccccccccccccccccc"
        });
        let envelope = build_claim_unstake_envelope(&tree).unwrap();
        assert_eq!(envelope.category, Category::LiquidStaking);
        let Action::ClaimUnstake(action) = envelope.action else {
            panic!("expected ClaimUnstake, got something else");
        };
        assert_eq!(action.token_out.kind, AssetKind::Erc20);
        assert!(action.amount_out.is_none());
        assert!(action.ticket.nft.is_none());
        assert!(action.ticket.token_id.is_none());
        assert!(action.ticket.id.is_none());
    }

    #[test]
    fn build_claim_rewards_envelope_from_args_minimal() {
        let tree = json!({
            "from": "0xcccccccccccccccccccccccccccccccccccccccc",
            "recipient": "0xcccccccccccccccccccccccccccccccccccccccc"
        });
        let envelope = build_claim_rewards_envelope(&tree).unwrap();
        assert_eq!(envelope.category, Category::Misc);
        let Action::ClaimRewards(action) = envelope.action else {
            panic!("expected ClaimRewards");
        };
        assert!(action.source.is_none());
        assert!(action.reward_tokens.is_none());
        assert!(action.max_amounts.is_none());
    }

    #[test]
    fn build_claim_rewards_envelope_from_args_with_source() {
        let tree = json!({
            "source": {
                "address": "0xbfcf63294ad7105dea65aa58f8ae5be2d9d0952a",
                "label": "Curve stETH Gauge"
            },
            "from": "0xcccccccccccccccccccccccccccccccccccccccc",
            "recipient": "0xcccccccccccccccccccccccccccccccccccccccc",
            "rewardTokens": [
                {
                    "kind": "erc20",
                    "address": "0xd533a949740bb3306d119cc777fa900ba034cd52"
                }
            ]
        });
        let envelope = build_claim_rewards_envelope(&tree).unwrap();
        let Action::ClaimRewards(action) = envelope.action else {
            panic!("expected ClaimRewards");
        };
        let source = action.source.expect("source present");
        assert_eq!(
            source.address.unwrap().to_string(),
            "0xbfcf63294ad7105dea65aa58f8ae5be2d9d0952a"
        );
        assert_eq!(source.label.as_deref(), Some("Curve stETH Gauge"));
        let reward_tokens = action.reward_tokens.expect("reward tokens present");
        assert_eq!(reward_tokens.len(), 1);
        assert_eq!(reward_tokens[0].kind, AssetKind::Erc20);
    }

    #[test]
    fn build_vote_envelope_from_args() {
        let tree = json!({
            "governance": "0xbfcf63294ad7105dea65aa58f8ae5be2d9d0952a",
            "proposalId": "0",
            "support": "for",
            "votingPower": "10000"
        });
        let envelope = build_vote_envelope(&tree).unwrap();
        assert_eq!(envelope.category, Category::Misc);
        let Action::Vote(action) = envelope.action else {
            panic!("expected Vote, got something else");
        };
        assert_eq!(
            action.governance.to_string(),
            "0xbfcf63294ad7105dea65aa58f8ae5be2d9d0952a"
        );
        assert_eq!(action.proposal_id.to_string(), "0");
        assert!(matches!(
            action.support,
            policy_engine::action::misc::VoteSupport::For
        ));
        assert_eq!(action.voting_power.unwrap().to_string(), "10000");
        assert!(action.validity.is_none());
    }

    // ───────────────────────────────────────────────────────────────────────
    // Round 7 P1 fixes — kind-required-field enforcement for builders that
    // accept multiple sub-kinds. Each kind has its own required field set,
    // and the builder must reject an adapter that pairs a kind discriminator
    // with the wrong payload (silent mis-classification would otherwise
    // produce an envelope Cedar policies cannot evaluate correctly).
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn build_lock_increase_envelope_amount_kind_missing_additional_amount_errors() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "tokenId": "42",
            "kind": "amount"
            // additionalAmount intentionally omitted
        });
        let err = build_lock_increase_envelope(&tree).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("kind=amount requires additionalAmount"),
            "expected lock_increase kind=amount enforcement error, got: {msg}"
        );
    }

    #[test]
    fn build_lock_increase_envelope_unlock_time_kind_missing_duration_errors() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "tokenId": "42",
            "kind": "unlock_time"
            // newLockDurationSec intentionally omitted
        });
        let err = build_lock_increase_envelope(&tree).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("kind=unlock_time requires newLockDurationSec"),
            "expected lock_increase kind=unlock_time enforcement error, got: {msg}"
        );
    }

    #[test]
    fn build_lock_manage_envelope_merge_missing_to_token_id_errors() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "kind": "merge",
            "fromTokenId": "1"
            // toTokenId intentionally omitted
        });
        let err = build_lock_manage_envelope(&tree).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("kind=merge requires toTokenId"),
            "expected lock_manage kind=merge enforcement error, got: {msg}"
        );
    }

    #[test]
    fn build_lock_manage_envelope_split_missing_split_ratio_errors() {
        let tree = json!({
            "votingEscrow": aero_voting_escrow(),
            "kind": "split",
            "fromTokenId": "1"
            // splitRatio intentionally omitted
        });
        let err = build_lock_manage_envelope(&tree).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("kind=split requires splitRatio"),
            "expected lock_manage kind=split enforcement error, got: {msg}"
        );
    }

    #[test]
    fn build_gauge_vote_envelope_reset_with_non_empty_pools_errors() {
        let tree = json!({
            "voter": aero_voter(),
            "tokenId": "1",
            "pools": ["0x0000000000000000000000000000000000000001"],
            "weights": ["100"],
            "kind": "reset"
        });
        let err = build_gauge_vote_envelope(&tree).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("kind=reset requires empty pools and weights"),
            "expected gauge_vote kind=reset enforcement error, got: {msg}"
        );
    }

    #[test]
    fn build_gauge_vote_envelope_poke_with_non_empty_weights_errors() {
        let tree = json!({
            "voter": aero_voter(),
            "tokenId": "1",
            "pools": ["0x0000000000000000000000000000000000000001"],
            "weights": ["50"],
            "kind": "poke"
        });
        let err = build_gauge_vote_envelope(&tree).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("kind=poke requires empty pools and weights"),
            "expected gauge_vote kind=poke enforcement error, got: {msg}"
        );
    }

    // ── Phase 7B — approve / set_approval_for_all builders ────────────────

    /// Permit2 `approve` field tree → `ApproveAction` with `approval_kind =
    /// Permit2` and a `grant-expiration` validity window.
    #[test]
    fn build_approve_envelope_permit2() {
        let tree = json!({
            "token": { "kind": "erc20", "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" },
            "spender": "0x000000000022d473030f116ddee9f6b43ac78ba3",
            "amount": { "kind": "max", "value": "1461501637330902918203684832716283019655932542975" },
            "approvalKind": "permit2",
            "validity": { "expiresAt": "1700000000", "source": "grant-expiration" }
        });
        let envelope = build_approve_envelope(&tree).expect("approve builds");
        assert_eq!(envelope.category, Category::Misc);
        let Action::Approve(action) = &envelope.action else {
            panic!("expected Approve, got {:?}", envelope.action);
        };
        assert_eq!(action.approval_kind, ApprovalKind::Permit2);
        assert_eq!(action.token.kind, AssetKind::Erc20);
        assert_eq!(action.amount.kind, AmountKind::Max);
        assert_eq!(
            action.spender.to_string(),
            "0x000000000022d473030f116ddee9f6b43ac78ba3"
        );
        let validity = action.validity.as_ref().expect("validity present");
        assert_eq!(validity.source, ValiditySource::GrantExpiration);
        assert_eq!(validity.expires_at.to_string(), "1700000000");
    }

    /// Minimal ERC-20 `approve` — no validity, no optional label/allowance.
    #[test]
    fn build_approve_envelope_erc20_minimal() {
        let tree = json!({
            "token": { "kind": "erc20", "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" },
            "spender": "0x1111111111111111111111111111111111111111",
            "amount": { "kind": "exact", "value": "1000000" },
            "approvalKind": "erc20"
        });
        let envelope = build_approve_envelope(&tree).expect("approve builds");
        let Action::Approve(action) = &envelope.action else {
            panic!("expected Approve, got {:?}", envelope.action);
        };
        assert_eq!(action.approval_kind, ApprovalKind::Erc20);
        assert_eq!(action.amount.kind, AmountKind::Exact);
        assert_eq!(
            action.amount.value.as_ref().map(ToString::to_string),
            Some("1000000".to_owned())
        );
        assert!(action.validity.is_none());
        assert!(action.spender_label.is_none());
        assert!(action.current_allowance.is_none());
    }

    /// An unrecognised `approvalKind` literal is a hard error — the builder
    /// must not silently coerce it.
    #[test]
    fn build_approve_envelope_rejects_bad_kind() {
        let tree = json!({
            "token": { "kind": "erc20", "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" },
            "spender": "0x1111111111111111111111111111111111111111",
            "amount": { "kind": "exact", "value": "1" },
            "approvalKind": "bogus_kind"
        });
        let err = build_approve_envelope(&tree).unwrap_err();
        assert!(
            err.to_string().contains("approvalKind") && err.to_string().contains("not recognised"),
            "unexpected error: {err}"
        );
    }

    /// `approve` with a missing `amount` field is rejected (the schema field
    /// is required even though the underlying reader is optional).
    #[test]
    fn build_approve_envelope_rejects_missing_amount() {
        let tree = json!({
            "token": { "kind": "erc20", "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" },
            "spender": "0x1111111111111111111111111111111111111111",
            "approvalKind": "erc20"
        });
        let err = build_approve_envelope(&tree).unwrap_err();
        assert!(
            matches!(err, MapperError::MissingArgument(ref f) if f == "amount"),
            "unexpected error: {err:?}"
        );
    }

    /// ERC-721 / NFPM `setApprovalForAll` field tree → `SetApprovalForAllAction`.
    #[test]
    fn build_set_approval_for_all_envelope_grant() {
        let tree = json!({
            "collection": { "kind": "erc721", "address": "0xc36442b4a4522e871399cd717abdd847ab11fe88" },
            "operator": "0x2222222222222222222222222222222222222222",
            "approved": true
        });
        let envelope =
            build_set_approval_for_all_envelope(&tree).expect("set_approval_for_all builds");
        assert_eq!(envelope.category, Category::Misc);
        let Action::SetApprovalForAll(action) = &envelope.action else {
            panic!("expected SetApprovalForAll, got {:?}", envelope.action);
        };
        assert_eq!(action.collection.kind, AssetKind::Erc721);
        assert!(action.approved);
        assert_eq!(
            action.operator.to_string(),
            "0x2222222222222222222222222222222222222222"
        );
        assert!(action.operator_label.is_none());
        assert!(action.previously_approved.is_none());
    }

    /// `setApprovalForAll` revocation — `approved: false` round-trips.
    #[test]
    fn build_set_approval_for_all_envelope_revoke() {
        let tree = json!({
            "collection": { "kind": "erc721", "address": "0xc36442b4a4522e871399cd717abdd847ab11fe88" },
            "operator": "0x2222222222222222222222222222222222222222",
            "approved": false
        });
        let envelope =
            build_set_approval_for_all_envelope(&tree).expect("set_approval_for_all builds");
        let Action::SetApprovalForAll(action) = &envelope.action else {
            panic!("expected SetApprovalForAll, got {:?}", envelope.action);
        };
        assert!(!action.approved);
    }

    /// A non-boolean `approved` is a hard error — never coerced.
    #[test]
    fn build_set_approval_for_all_envelope_rejects_non_bool_approved() {
        let tree = json!({
            "collection": { "kind": "erc721", "address": "0xc36442b4a4522e871399cd717abdd847ab11fe88" },
            "operator": "0x2222222222222222222222222222222222222222",
            "approved": "true"
        });
        let err = build_set_approval_for_all_envelope(&tree).unwrap_err();
        assert!(
            err.to_string().contains("approved") && err.to_string().contains("expected bool"),
            "unexpected error: {err}"
        );
    }
}
