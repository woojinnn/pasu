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
    RepayAction, RepayKind,
};
use policy_engine::action::misc::{
    ClaimRewardsAction, PermitAction, PermitKind, SourceRef, TransferAction, UnwrapAction,
    VoteAction, VoteSupport, WrapAction,
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
        ("dex", "add_liquidity") => Ok(build_add_liquidity_envelope(&tree)?),
        ("dex", "remove_liquidity") => Ok(build_remove_liquidity_envelope(&tree)?),
        ("dex", "mint_liquidity_nft") => Ok(build_mint_liquidity_nft_envelope(&tree)?),
        ("dex", "increase_liquidity") => Ok(build_increase_liquidity_envelope(&tree)?),
        ("dex", "decrease_liquidity") => Ok(build_decrease_liquidity_envelope(&tree)?),
        ("dex", "burn_liquidity_nft") => Ok(build_burn_liquidity_nft_envelope(&tree)?),
        ("dex", "initialize_pool") => Ok(build_initialize_pool_envelope(&tree)?),
        // Phase 12.5 — lending builders for crvUSD Controller (LLAMMA).
        ("lending", "borrow") => Ok(build_borrow_envelope(&tree)?),
        ("lending", "repay") => Ok(build_repay_envelope(&tree)?),
        ("lending", "liquidate") => Ok(build_liquidate_envelope(&tree)?),
        // Phase 12.6 — staking / claim / vote builders for veCRV + Gauge +
        // GaugeController.
        ("staking", "stake") => Ok(build_stake_envelope(&tree)?),
        ("staking", "claim_unstake") => Ok(build_claim_unstake_envelope(&tree)?),
        ("misc", "claim_rewards") => Ok(build_claim_rewards_envelope(&tree)?),
        ("misc", "vote") => Ok(build_vote_envelope(&tree)?),
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
        return Ok(PathStep { key: segment, indices: vec![] });
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
        let entry = map
            .entry(step.key.to_owned())
            .or_insert_with(|| {
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
    let permit_kind_str = required_string(tree, "permitKind")
        .map_err(|_| missing_field("$", "permitKind"))?;
    let permit_kind = parse_permit_kind(permit_kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "permitKind {permit_kind_str:?} not recognised"
        ))
    })?;

    let token = read_asset_inline(tree, "token")?;
    let owner = read_address(tree, "owner")?;
    let spender = match tree.get("spender") {
        Some(serde_json::Value::String(s)) => Some(Address::from_str(s).map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("spender {s:?}: {m}"))
        })?),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "spender: expected string, got {other}"
            )));
        }
    };
    let amount = read_amount_inline(tree, "amount")?;
    let validity = read_validity(tree)?
        .ok_or_else(|| MapperError::MissingArgument("validity".to_owned()))?;
    let signature_validity = read_signature_validity(tree)?;

    let action = PermitAction {
        permit_kind,
        token,
        owner,
        spender,
        recipient: None,
        amount,
        requested_amount: None,
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

fn build_remove_liquidity_envelope(tree: &serde_json::Value) -> Result<ActionEnvelope, MapperError> {
    let exit_mode_str = required_string(tree, "exitMode")
        .map_err(|_| missing_field("$", "exitMode"))?;
    let exit_mode = parse_remove_liquidity_exit_mode(exit_mode_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "exitMode {exit_mode_str:?} not recognised"
        ))
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
    let burn_kind_str = required_string(tree, "burnKind")
        .map_err(|_| missing_field("$", "burnKind"))?;
    let burn_kind = parse_burn_kind(burn_kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "burnKind {burn_kind_str:?} not recognised"
        ))
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
fn build_initialize_pool_envelope(
    tree: &serde_json::Value,
) -> Result<ActionEnvelope, MapperError> {
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

    let action = BorrowAction {
        market,
        asset,
        amount,
        amount_mode,
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
    let repay_kind_str = required_string(tree, "repayKind")
        .map_err(|_| missing_field("$", "repayKind"))?;
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
/// crvUSD `liquidate(address user, uint256 min_x)` / `self_liquidate(uint256 min_x)`
/// both end here. For `self_liquidate` the bundle resolves `borrower` to
/// `$.tx.from`; for `liquidate` it resolves to `$.args.user`. `min_x` is the
/// minimum debt-token received (Curve-specific) — recorded as
/// `seizedCollateralAmount` to keep the schema generic.
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
            MapperError::Internal(anyhow::anyhow!(
                "liquidateMode {s:?} not recognised"
            ))
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
fn build_claim_unstake_envelope(
    tree: &serde_json::Value,
) -> Result<ActionEnvelope, MapperError> {
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
fn build_claim_rewards_envelope(
    tree: &serde_json::Value,
) -> Result<ActionEnvelope, MapperError> {
    let source = read_optional_source_ref(tree, "source")?;
    let nft = match tree.get("nft") {
        Some(serde_json::Value::Null) | None => None,
        Some(_) => Some(read_asset_inline(tree, "nft")?),
    };
    let token_id = match tree.get("tokenId") {
        Some(serde_json::Value::String(s)) => Some(DecimalString::from_str(s).map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("tokenId {s:?}: {m}"))
        })?),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "tokenId: expected decimal string, got {other}"
            )));
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
    let proposal_id_str = required_string(tree, "proposalId")
        .map_err(|_| missing_field("$", "proposalId"))?;
    let proposal_id = DecimalString::from_str(proposal_id_str).map_err(|m| {
        MapperError::Internal(anyhow::anyhow!("proposalId {proposal_id_str:?}: {m}"))
    })?;
    let support_str = required_string(tree, "support")
        .map_err(|_| missing_field("$", "support"))?;
    let support = parse_vote_support(support_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "support {support_str:?} not recognised"
        ))
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
    let address = match object.get("address") {
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
        Some(serde_json::Value::String(s)) => Some(Hex::from_str(s).map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("{field}.id {s:?}: {m}"))
        })?),
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
    let address = match object.get("address") {
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
fn read_ticket_ref(
    tree: &serde_json::Value,
    field: &str,
) -> Result<TicketRef, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(TicketRef { nft: None, token_id: None, id: None });
    };
    if raw.is_null() {
        return Ok(TicketRef { nft: None, token_id: None, id: None });
    }
    let object = raw.as_object().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!("{field}: expected object, got {raw}"))
    })?;
    let nft = match object.get("nft") {
        Some(serde_json::Value::Null) | None => None,
        Some(_) => Some(read_asset_inline(raw, "nft")?),
    };
    let token_id = match object.get("tokenId") {
        Some(serde_json::Value::String(s)) => Some(DecimalString::from_str(s).map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("{field}.tokenId {s:?}: {m}"))
        })?),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.tokenId: expected decimal string, got {other}"
            )));
        }
    };
    let id = match object.get("id") {
        Some(serde_json::Value::String(s)) => Some(Hex::from_str(s).map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("{field}.id {s:?}: {m}"))
        })?),
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
            .ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "{field} {s:?} not recognised"
                ))
            }),
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
fn read_optional_i32(
    tree: &serde_json::Value,
    field: &str,
) -> Result<Option<i32>, MapperError> {
    let Some(raw) = tree.get(field) else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    if let Some(n) = raw.as_i64() {
        i32::try_from(n)
            .map(Some)
            .map_err(|_| MapperError::Internal(anyhow::anyhow!(
                "{field}: value {n} exceeds i32 range"
            )))
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
    let address_str = required_string(object, "address")
        .map_err(|_| missing_field(field, "address"))?;
    let address = Address::from_str(address_str).map_err(|message| {
        MapperError::Internal(anyhow::anyhow!(
            "{field}.address {address_str:?}: {message}"
        ))
    })?;
    let id = match object.get("id") {
        Some(serde_json::Value::String(s)) => Some(Hex::from_str(s).map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("{field}.id {s:?}: {m}"))
        })?),
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
    Ok(PoolRef {
        address,
        id,
        label,
    })
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
        MapperError::Internal(anyhow::anyhow!(
            "{field}: expected array, got {raw}"
        ))
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
                        MapperError::Internal(anyhow::anyhow!(
                            "{field}.tokenId {s:?}: {m}"
                        ))
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
            MapperError::Internal(anyhow::anyhow!(
                "{field}: value {n} exceeds u32 range"
            ))
        })
    } else if let Some(s) = raw.as_str() {
        s.parse::<u32>().map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("{field} {s:?}: {m}"))
        })
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
    let raw = object.get(member).ok_or_else(|| missing_field(parent, member))?;
    if let Some(n) = raw.as_i64() {
        i32::try_from(n).map_err(|_| {
            MapperError::Internal(anyhow::anyhow!(
                "{parent}.{member}: value {n} exceeds i32 range"
            ))
        })
    } else if let Some(s) = raw.as_str() {
        s.parse::<i32>().map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("{parent}.{member} {s:?}: {m}"))
        })
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
        MapperError::Internal(anyhow::anyhow!(
            "{field}.kind {kind_str:?} not recognised"
        ))
    })?;
    let address = match object.get("address") {
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
    Ok(AssetRef {
        kind,
        address,
        token_id: None,
        symbol: None,
        decimals: None,
    })
}

fn read_asset(token: &serde_json::Value, parent: &str) -> Result<AssetRef, MapperError> {
    let asset = required_object(token, "asset").map_err(|_| missing_field(parent, "asset"))?;
    let kind_str = required_string(asset, "kind").map_err(|_| missing_field(parent, "asset.kind"))?;
    let kind = parse_asset_kind(kind_str)
        .ok_or_else(|| MapperError::Internal(anyhow::anyhow!(
            "{parent}.asset.kind {kind_str:?} not recognised in Phase 1A"
        )))?;
    let address = match asset.get("address") {
        Some(serde_json::Value::String(s)) => Some(
            Address::from_str(s).map_err(|message| MapperError::Internal(anyhow::anyhow!(
                "{parent}.asset.address {s:?}: {message}"
            )))?,
        ),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{parent}.asset.address: expected string, got {other}"
            )));
        }
    };
    Ok(AssetRef {
        kind,
        address,
        token_id: None,
        symbol: None,
        decimals: None,
    })
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
        MapperError::Internal(anyhow::anyhow!(
            "{field}.kind {kind_str:?} not recognised"
        ))
    })?;
    let value = match object.get("value") {
        Some(serde_json::Value::String(s)) => Some(DecimalString::from_str(s).map_err(|m| {
            MapperError::Internal(anyhow::anyhow!("{field}.value {s:?}: {m}"))
        })?),
        Some(serde_json::Value::Null) | None => None,
        Some(other) => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "{field}.value: expected decimal string, got {other}"
            )));
        }
    };
    Ok(Some(AmountConstraint { kind, value }))
}

fn read_amount(
    token: &serde_json::Value,
    parent: &str,
) -> Result<AmountConstraint, MapperError> {
    let amount =
        required_object(token, "amount").map_err(|_| missing_field(parent, "amount"))?;
    let kind_str = required_string(amount, "kind")
        .map_err(|_| missing_field(parent, "amount.kind"))?;
    let kind = parse_amount_kind(kind_str).ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "{parent}.amount.kind {kind_str:?} not recognised in Phase 1A"
        ))
    })?;
    let value = match amount.get("value") {
        Some(serde_json::Value::String(s)) => Some(
            DecimalString::from_str(s).map_err(|message| MapperError::Internal(anyhow::anyhow!(
                "{parent}.amount.value {s:?}: {message}"
            )))?,
        ),
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
    Address::from_str(raw).map_err(|message| {
        MapperError::Internal(anyhow::anyhow!("{field} {raw:?}: {message}"))
    })
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
    let expires_at_str =
        required_string(validity, "expiresAt").map_err(|_| missing_field("validity", "expiresAt"))?;
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

fn required_string<'a>(
    tree: &'a serde_json::Value,
    field: &str,
) -> Result<&'a str, MapperError> {
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
        set_nested(
            &mut root,
            "inputTokens[0].asset.address",
            json!("0xaaaa"),
        )
        .unwrap();
        set_nested(&mut root, "inputTokens[1].asset.kind", json!("erc20")).unwrap();
        set_nested(
            &mut root,
            "inputTokens[2].asset.address",
            json!("0xbbbb"),
        )
        .unwrap();
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
        assert_eq!(action.recipient.to_string(), "0x3333333333333333333333333333333333333333");
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
        assert_eq!(action.nft.token_id.as_ref().map(ToString::to_string), Some("42".to_owned()));
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
            action.liquidity_delta.value.as_ref().map(ToString::to_string),
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
            action.signature_validity.as_ref().map(|v| v.expires_at.to_string()),
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
        assert_eq!(action.nft.token_id.as_ref().map(ToString::to_string), Some("42".to_owned()));
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
        assert_eq!(
            action.validity.expires_at.to_string(),
            "1".to_owned()
        );
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
                assert_eq!(name, "token", "expected MissingArgument(\"token\") for empty-batch fan-out");
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
        assert_eq!(market.label.as_deref(), Some("Curve crvUSD wstETH Controller"));
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
}
