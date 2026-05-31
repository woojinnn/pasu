//! Per-policy Cedar schema synthesis.
//!
//! Where [`super::compose_enriched`] produces ONE unified `.cedarschema`
//! covering every shipped action (the legacy / installed-set path), this module
//! synthesizes an ISOLATED `.cedarschema` for a single policy bundle
//! ([`ManifestV2`]). The synthesized text contains only:
//!
//! 1. the shared `core.cedarschema` (entities + shared types), plus
//! 2. the base schema file(s) of every action whose `(domain, action_tag)` the
//!    manifest's [`Trigger`] can match, with
//! 3. the manifest's [`CustomContext`] fields injected into each matched
//!    action's `type <Action>CustomContext = {};` stub.
//!
//! The result is intended to parse with
//! [`cedar_policy::Schema::from_cedarschema_str`].
//!
//! This path is purely additive and does not alter [`super::compose_enriched`].

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::merge_namespace_blocks;
use crate::policy_rpc::{
    evaluate_trigger, ManifestV2, PolicyRpcError, Trigger, TriggerField, TxView,
};
use simulation_reducer::action::ActionView;

use super::{
    AIRDROP_CLAIM_SCHEMA, AIRDROP_DELEGATE_SCHEMA, AMM_ADD_LIQUIDITY_SCHEMA,
    AMM_CANCEL_INTENT_ORDER_SCHEMA, AMM_COLLECT_FEES_SCHEMA, AMM_REMOVE_LIQUIDITY_SCHEMA,
    AMM_SIGN_INTENT_ORDER_SCHEMA, AMM_SWAP_SCHEMA, CORE_MULTICALL_SCHEMA, CORE_SCHEMA,
    CORE_UNKNOWN_SCHEMA, HL_APPROVE_AGENT_SCHEMA, HL_ORDER_SCHEMA, HL_UPDATE_LEVERAGE_SCHEMA,
    HL_USD_SEND_SCHEMA, HL_WITHDRAW_SCHEMA, LAUNCHPAD_CLAIM_ALLOCATION_SCHEMA,
    LAUNCHPAD_CLAIM_VESTED_SCHEMA, LAUNCHPAD_COMMIT_SCHEMA, LAUNCHPAD_REFUND_SCHEMA,
    LAUNCHPAD_WITHDRAW_COMMIT_SCHEMA, LENDING_BORROW_SCHEMA, LENDING_BUY_COLLATERAL_SCHEMA,
    LENDING_DELEGATE_BORROW_SCHEMA, LENDING_DISABLE_COLLATERAL_SCHEMA,
    LENDING_ENABLE_COLLATERAL_SCHEMA, LENDING_LIQUIDATE_SCHEMA, LENDING_REPAY_SCHEMA,
    LENDING_SET_AUTHORIZATION_SCHEMA, LENDING_SET_EMODE_SCHEMA, LENDING_SUPPLY_SCHEMA,
    LENDING_SWAP_RATE_MODE_SCHEMA, LENDING_WITHDRAW_SCHEMA, LIQUID_STAKING_CLAIM_WITHDRAWAL_SCHEMA,
    LIQUID_STAKING_REQUEST_WITHDRAWAL_SCHEMA, LIQUID_STAKING_STAKE_SCHEMA,
    LIQUID_STAKING_TRANSFER_SHARES_SCHEMA, LIQUID_STAKING_UNWRAP_SCHEMA,
    LIQUID_STAKING_WRAP_SCHEMA, PERMISSION_PROTOCOL_AUTHORIZATION_SCHEMA,
    PERP_ADJUST_MARGIN_SCHEMA, PERP_CANCEL_ORDER_SCHEMA, PERP_CHANGE_LEVERAGE_SCHEMA,
    PERP_CHANGE_MARGIN_MODE_SCHEMA, PERP_CLAIM_FUNDING_SCHEMA, PERP_CLOSE_POSITION_SCHEMA,
    PERP_DECREASE_POSITION_SCHEMA, PERP_INCREASE_POSITION_SCHEMA, PERP_OPEN_POSITION_SCHEMA,
    PERP_PLACE_LIMIT_ORDER_SCHEMA, PERP_PLACE_STOP_ORDER_SCHEMA,
    RESTAKING_COMPLETE_WITHDRAWAL_SCHEMA, RESTAKING_DELEGATE_TO_SCHEMA, RESTAKING_DEPOSIT_SCHEMA,
    RESTAKING_QUEUE_WITHDRAWAL_SCHEMA, RESTAKING_REDELEGATE_SCHEMA,
    RESTAKING_REGISTER_OPERATOR_SCHEMA, RESTAKING_UNDELEGATE_SCHEMA, STAKING_CLAIM_REWARDS_SCHEMA,
    STAKING_GAUGE_DEPOSIT_SCHEMA, STAKING_GAUGE_WITHDRAW_SCHEMA,
    STAKING_INCREASE_LOCK_AMOUNT_SCHEMA, STAKING_INCREASE_LOCK_TIME_SCHEMA, STAKING_LOCK_SCHEMA,
    STAKING_UNLOCK_SCHEMA, STAKING_VOTE_FOR_GAUGE_SCHEMA, TOKEN_ERC20_APPROVE_SCHEMA,
    TOKEN_ERC20_PERMIT_SCHEMA, TOKEN_ERC20_TRANSFER_SCHEMA, TOKEN_NFT_APPROVE_SCHEMA,
    TOKEN_NFT_SET_APPROVAL_FOR_ALL_SCHEMA, TOKEN_NFT_TRANSFER_SCHEMA, TOKEN_PERMIT2_APPROVE_SCHEMA,
    TOKEN_PERMIT2_SIGN_ALLOWANCE_SCHEMA, TOKEN_REVOKE_APPROVAL_SCHEMA,
};

/// One row of the action resolver: the `(domain, action_tag)` a trigger can
/// match, the shipped base `.cedarschema` to include, and the bare `PascalCase`
/// stub name whose `type <Stub>CustomContext = {};` placeholder receives the
/// manifest's custom fields.
struct ActionEntry {
    /// `ActionBody` domain serde tag (e.g. `"amm"`, `"multicall"`).
    domain: &'static str,
    /// Inner action serde tag (e.g. `"swap"`, `"set_e_mode"`); `None` for the
    /// structural `multicall` / `unknown` bodies, which carry no tag.
    action_tag: Option<&'static str>,
    /// The action's shipped base `.cedarschema` (`include_str!` const).
    schema_text: &'static str,
    /// Bare `PascalCase` prefix of the custom-context stub
    /// (`<Stub>CustomContext`). May differ from `snake_to_pascal(action_tag)`
    /// — notably `set_e_mode` → `SetEMode` (`SetEModeCustomContext`).
    pascal_stub: &'static str,
}

/// The authoritative `(domain, action_tag)` → `(schema, stub)` table.
///
/// The `action_tag` column is the runtime serde discriminant returned by
/// [`ActionView::action_tag`] (verified against the reducer `action_tag()`
/// implementations), and the `pascal_stub` column is taken verbatim from the
/// `type <Stub>CustomContext = {};` declaration grepped out of each shipped
/// `.cedarschema`. Notably the serde tag `set_e_mode` maps to schema file
/// `set_emode.cedarschema` whose stub is `SetEModeCustomContext`.
const RESOLVER_TABLE: &[ActionEntry] = &[
    // core structural (domain is "multicall" / "unknown", no inner tag)
    ActionEntry {
        domain: "multicall",
        action_tag: None,
        schema_text: CORE_MULTICALL_SCHEMA,
        pascal_stub: "Multicall",
    },
    ActionEntry {
        domain: "unknown",
        action_tag: None,
        schema_text: CORE_UNKNOWN_SCHEMA,
        pascal_stub: "Unknown",
    },
    // airdrop
    ActionEntry {
        domain: "airdrop",
        action_tag: Some("claim"),
        schema_text: AIRDROP_CLAIM_SCHEMA,
        pascal_stub: "Claim",
    },
    ActionEntry {
        domain: "airdrop",
        action_tag: Some("delegate"),
        schema_text: AIRDROP_DELEGATE_SCHEMA,
        pascal_stub: "Delegate",
    },
    // amm
    ActionEntry {
        domain: "amm",
        action_tag: Some("swap"),
        schema_text: AMM_SWAP_SCHEMA,
        pascal_stub: "Swap",
    },
    ActionEntry {
        domain: "amm",
        action_tag: Some("add_liquidity"),
        schema_text: AMM_ADD_LIQUIDITY_SCHEMA,
        pascal_stub: "AddLiquidity",
    },
    ActionEntry {
        domain: "amm",
        action_tag: Some("remove_liquidity"),
        schema_text: AMM_REMOVE_LIQUIDITY_SCHEMA,
        pascal_stub: "RemoveLiquidity",
    },
    ActionEntry {
        domain: "amm",
        action_tag: Some("collect_fees"),
        schema_text: AMM_COLLECT_FEES_SCHEMA,
        pascal_stub: "CollectFees",
    },
    ActionEntry {
        domain: "amm",
        action_tag: Some("sign_intent_order"),
        schema_text: AMM_SIGN_INTENT_ORDER_SCHEMA,
        pascal_stub: "SignIntentOrder",
    },
    ActionEntry {
        domain: "amm",
        action_tag: Some("cancel_intent_order"),
        schema_text: AMM_CANCEL_INTENT_ORDER_SCHEMA,
        pascal_stub: "CancelIntentOrder",
    },
    // lending
    ActionEntry {
        domain: "lending",
        action_tag: Some("supply"),
        schema_text: LENDING_SUPPLY_SCHEMA,
        pascal_stub: "Supply",
    },
    ActionEntry {
        domain: "lending",
        action_tag: Some("withdraw"),
        schema_text: LENDING_WITHDRAW_SCHEMA,
        pascal_stub: "Withdraw",
    },
    ActionEntry {
        domain: "lending",
        action_tag: Some("borrow"),
        schema_text: LENDING_BORROW_SCHEMA,
        pascal_stub: "Borrow",
    },
    ActionEntry {
        domain: "lending",
        action_tag: Some("buy_collateral"),
        schema_text: LENDING_BUY_COLLATERAL_SCHEMA,
        pascal_stub: "BuyCollateral",
    },
    ActionEntry {
        domain: "lending",
        action_tag: Some("repay"),
        schema_text: LENDING_REPAY_SCHEMA,
        pascal_stub: "Repay",
    },
    ActionEntry {
        domain: "lending",
        action_tag: Some("swap_rate_mode"),
        schema_text: LENDING_SWAP_RATE_MODE_SCHEMA,
        pascal_stub: "SwapRateMode",
    },
    // Serde tag is `set_e_mode`; schema file is `set_emode.cedarschema`; stub
    // is `SetEModeCustomContext`. This row pins that three-way mismatch.
    ActionEntry {
        domain: "lending",
        action_tag: Some("set_e_mode"),
        schema_text: LENDING_SET_EMODE_SCHEMA,
        pascal_stub: "SetEMode",
    },
    ActionEntry {
        domain: "lending",
        action_tag: Some("enable_collateral"),
        schema_text: LENDING_ENABLE_COLLATERAL_SCHEMA,
        pascal_stub: "EnableCollateral",
    },
    ActionEntry {
        domain: "lending",
        action_tag: Some("disable_collateral"),
        schema_text: LENDING_DISABLE_COLLATERAL_SCHEMA,
        pascal_stub: "DisableCollateral",
    },
    ActionEntry {
        domain: "lending",
        action_tag: Some("delegate_borrow"),
        schema_text: LENDING_DELEGATE_BORROW_SCHEMA,
        pascal_stub: "DelegateBorrow",
    },
    ActionEntry {
        domain: "lending",
        action_tag: Some("liquidate"),
        schema_text: LENDING_LIQUIDATE_SCHEMA,
        pascal_stub: "Liquidate",
    },
    ActionEntry {
        domain: "lending",
        action_tag: Some("set_authorization"),
        schema_text: LENDING_SET_AUTHORIZATION_SCHEMA,
        pascal_stub: "SetAuthorization",
    },
    // liquid_staking
    ActionEntry {
        domain: "liquid_staking",
        action_tag: Some("claim_withdrawal"),
        schema_text: LIQUID_STAKING_CLAIM_WITHDRAWAL_SCHEMA,
        pascal_stub: "ClaimWithdrawal",
    },
    ActionEntry {
        domain: "liquid_staking",
        action_tag: Some("request_withdrawal"),
        schema_text: LIQUID_STAKING_REQUEST_WITHDRAWAL_SCHEMA,
        pascal_stub: "RequestWithdrawal",
    },
    ActionEntry {
        domain: "liquid_staking",
        action_tag: Some("stake"),
        schema_text: LIQUID_STAKING_STAKE_SCHEMA,
        pascal_stub: "Stake",
    },
    ActionEntry {
        domain: "liquid_staking",
        action_tag: Some("transfer_shares"),
        schema_text: LIQUID_STAKING_TRANSFER_SHARES_SCHEMA,
        pascal_stub: "TransferShares",
    },
    ActionEntry {
        domain: "liquid_staking",
        action_tag: Some("unwrap"),
        schema_text: LIQUID_STAKING_UNWRAP_SCHEMA,
        pascal_stub: "Unwrap",
    },
    ActionEntry {
        domain: "liquid_staking",
        action_tag: Some("wrap"),
        schema_text: LIQUID_STAKING_WRAP_SCHEMA,
        pascal_stub: "Wrap",
    },
    // launchpad
    ActionEntry {
        domain: "launchpad",
        action_tag: Some("commit"),
        schema_text: LAUNCHPAD_COMMIT_SCHEMA,
        pascal_stub: "Commit",
    },
    ActionEntry {
        domain: "launchpad",
        action_tag: Some("claim_allocation"),
        schema_text: LAUNCHPAD_CLAIM_ALLOCATION_SCHEMA,
        pascal_stub: "ClaimAllocation",
    },
    ActionEntry {
        domain: "launchpad",
        action_tag: Some("claim_vested"),
        schema_text: LAUNCHPAD_CLAIM_VESTED_SCHEMA,
        pascal_stub: "ClaimVested",
    },
    ActionEntry {
        domain: "launchpad",
        action_tag: Some("refund"),
        schema_text: LAUNCHPAD_REFUND_SCHEMA,
        pascal_stub: "Refund",
    },
    ActionEntry {
        domain: "launchpad",
        action_tag: Some("withdraw_commit"),
        schema_text: LAUNCHPAD_WITHDRAW_COMMIT_SCHEMA,
        pascal_stub: "WithdrawCommit",
    },
    // perp
    ActionEntry {
        domain: "perp",
        action_tag: Some("open_position"),
        schema_text: PERP_OPEN_POSITION_SCHEMA,
        pascal_stub: "OpenPosition",
    },
    ActionEntry {
        domain: "perp",
        action_tag: Some("close_position"),
        schema_text: PERP_CLOSE_POSITION_SCHEMA,
        pascal_stub: "ClosePosition",
    },
    ActionEntry {
        domain: "perp",
        action_tag: Some("increase_position"),
        schema_text: PERP_INCREASE_POSITION_SCHEMA,
        pascal_stub: "IncreasePosition",
    },
    ActionEntry {
        domain: "perp",
        action_tag: Some("decrease_position"),
        schema_text: PERP_DECREASE_POSITION_SCHEMA,
        pascal_stub: "DecreasePosition",
    },
    ActionEntry {
        domain: "perp",
        action_tag: Some("adjust_margin"),
        schema_text: PERP_ADJUST_MARGIN_SCHEMA,
        pascal_stub: "AdjustMargin",
    },
    ActionEntry {
        domain: "perp",
        action_tag: Some("change_leverage"),
        schema_text: PERP_CHANGE_LEVERAGE_SCHEMA,
        pascal_stub: "ChangeLeverage",
    },
    ActionEntry {
        domain: "perp",
        action_tag: Some("change_margin_mode"),
        schema_text: PERP_CHANGE_MARGIN_MODE_SCHEMA,
        pascal_stub: "ChangeMarginMode",
    },
    ActionEntry {
        domain: "perp",
        action_tag: Some("place_limit_order"),
        schema_text: PERP_PLACE_LIMIT_ORDER_SCHEMA,
        pascal_stub: "PlaceLimitOrder",
    },
    ActionEntry {
        domain: "perp",
        action_tag: Some("place_stop_order"),
        schema_text: PERP_PLACE_STOP_ORDER_SCHEMA,
        pascal_stub: "PlaceStopOrder",
    },
    ActionEntry {
        domain: "perp",
        action_tag: Some("cancel_order"),
        schema_text: PERP_CANCEL_ORDER_SCHEMA,
        pascal_stub: "CancelOrder",
    },
    ActionEntry {
        domain: "perp",
        action_tag: Some("claim_funding"),
        schema_text: PERP_CLAIM_FUNDING_SCHEMA,
        pascal_stub: "ClaimFunding",
    },
    // permission
    ActionEntry {
        domain: "permission",
        action_tag: Some("protocol_authorization"),
        schema_text: PERMISSION_PROTOCOL_AUTHORIZATION_SCHEMA,
        pascal_stub: "ProtocolAuthorization",
    },
    // restaking
    ActionEntry {
        domain: "restaking",
        action_tag: Some("complete_withdrawal"),
        schema_text: RESTAKING_COMPLETE_WITHDRAWAL_SCHEMA,
        pascal_stub: "CompleteWithdrawal",
    },
    ActionEntry {
        domain: "restaking",
        action_tag: Some("delegate_to"),
        schema_text: RESTAKING_DELEGATE_TO_SCHEMA,
        pascal_stub: "DelegateTo",
    },
    ActionEntry {
        domain: "restaking",
        action_tag: Some("deposit"),
        schema_text: RESTAKING_DEPOSIT_SCHEMA,
        pascal_stub: "Deposit",
    },
    ActionEntry {
        domain: "restaking",
        action_tag: Some("queue_withdrawal"),
        schema_text: RESTAKING_QUEUE_WITHDRAWAL_SCHEMA,
        pascal_stub: "QueueWithdrawal",
    },
    ActionEntry {
        domain: "restaking",
        action_tag: Some("redelegate"),
        schema_text: RESTAKING_REDELEGATE_SCHEMA,
        pascal_stub: "Redelegate",
    },
    ActionEntry {
        domain: "restaking",
        action_tag: Some("register_operator"),
        schema_text: RESTAKING_REGISTER_OPERATOR_SCHEMA,
        pascal_stub: "RegisterOperator",
    },
    ActionEntry {
        domain: "restaking",
        action_tag: Some("undelegate"),
        schema_text: RESTAKING_UNDELEGATE_SCHEMA,
        pascal_stub: "Undelegate",
    },
    // staking
    ActionEntry {
        domain: "staking",
        action_tag: Some("claim_rewards"),
        schema_text: STAKING_CLAIM_REWARDS_SCHEMA,
        pascal_stub: "ClaimRewards",
    },
    ActionEntry {
        domain: "staking",
        action_tag: Some("gauge_deposit"),
        schema_text: STAKING_GAUGE_DEPOSIT_SCHEMA,
        pascal_stub: "GaugeDeposit",
    },
    ActionEntry {
        domain: "staking",
        action_tag: Some("gauge_withdraw"),
        schema_text: STAKING_GAUGE_WITHDRAW_SCHEMA,
        pascal_stub: "GaugeWithdraw",
    },
    ActionEntry {
        domain: "staking",
        action_tag: Some("increase_lock_amount"),
        schema_text: STAKING_INCREASE_LOCK_AMOUNT_SCHEMA,
        pascal_stub: "IncreaseLockAmount",
    },
    ActionEntry {
        domain: "staking",
        action_tag: Some("increase_lock_time"),
        schema_text: STAKING_INCREASE_LOCK_TIME_SCHEMA,
        pascal_stub: "IncreaseLockTime",
    },
    ActionEntry {
        domain: "staking",
        action_tag: Some("lock"),
        schema_text: STAKING_LOCK_SCHEMA,
        pascal_stub: "Lock",
    },
    ActionEntry {
        domain: "staking",
        action_tag: Some("unlock"),
        schema_text: STAKING_UNLOCK_SCHEMA,
        pascal_stub: "Unlock",
    },
    ActionEntry {
        domain: "staking",
        action_tag: Some("vote_for_gauge"),
        schema_text: STAKING_VOTE_FOR_GAUGE_SCHEMA,
        pascal_stub: "VoteForGauge",
    },
    // token
    ActionEntry {
        domain: "token",
        action_tag: Some("erc20_approve"),
        schema_text: TOKEN_ERC20_APPROVE_SCHEMA,
        pascal_stub: "Erc20Approve",
    },
    ActionEntry {
        domain: "token",
        action_tag: Some("erc20_permit"),
        schema_text: TOKEN_ERC20_PERMIT_SCHEMA,
        pascal_stub: "Erc20Permit",
    },
    ActionEntry {
        domain: "token",
        action_tag: Some("permit2_approve"),
        schema_text: TOKEN_PERMIT2_APPROVE_SCHEMA,
        pascal_stub: "Permit2Approve",
    },
    ActionEntry {
        domain: "token",
        action_tag: Some("permit2_sign_allowance"),
        schema_text: TOKEN_PERMIT2_SIGN_ALLOWANCE_SCHEMA,
        pascal_stub: "Permit2SignAllowance",
    },
    ActionEntry {
        domain: "token",
        action_tag: Some("erc20_transfer"),
        schema_text: TOKEN_ERC20_TRANSFER_SCHEMA,
        pascal_stub: "Erc20Transfer",
    },
    ActionEntry {
        domain: "token",
        action_tag: Some("nft_approve"),
        schema_text: TOKEN_NFT_APPROVE_SCHEMA,
        pascal_stub: "NftApprove",
    },
    ActionEntry {
        domain: "token",
        action_tag: Some("nft_set_approval_for_all"),
        schema_text: TOKEN_NFT_SET_APPROVAL_FOR_ALL_SCHEMA,
        pascal_stub: "NftSetApprovalForAll",
    },
    ActionEntry {
        domain: "token",
        action_tag: Some("nft_transfer"),
        schema_text: TOKEN_NFT_TRANSFER_SCHEMA,
        pascal_stub: "NftTransfer",
    },
    ActionEntry {
        domain: "token",
        action_tag: Some("revoke_approval"),
        schema_text: TOKEN_REVOKE_APPROVAL_SCHEMA,
        pascal_stub: "RevokeApproval",
    },
    // hyperliquid_core — `hl_`-prefixed tags; namespace `HyperliquidCore`.
    ActionEntry {
        domain: "hyperliquid_core",
        action_tag: Some("hl_order"),
        schema_text: HL_ORDER_SCHEMA,
        pascal_stub: "HlOrder",
    },
    ActionEntry {
        domain: "hyperliquid_core",
        action_tag: Some("hl_update_leverage"),
        schema_text: HL_UPDATE_LEVERAGE_SCHEMA,
        pascal_stub: "HlUpdateLeverage",
    },
    ActionEntry {
        domain: "hyperliquid_core",
        action_tag: Some("hl_withdraw"),
        schema_text: HL_WITHDRAW_SCHEMA,
        pascal_stub: "HlWithdraw",
    },
    ActionEntry {
        domain: "hyperliquid_core",
        action_tag: Some("hl_usd_send"),
        schema_text: HL_USD_SEND_SCHEMA,
        pascal_stub: "HlUsdSend",
    },
    ActionEntry {
        domain: "hyperliquid_core",
        action_tag: Some("hl_approve_agent"),
        schema_text: HL_APPROVE_AGENT_SCHEMA,
        pascal_stub: "HlApproveAgent",
    },
];

/// Synthesize an isolated per-policy `.cedarschema` for one bundle.
///
/// The result starts from `core.cedarschema`, adds the base schema of every
/// action whose `(domain, action_tag)` the manifest's [`Trigger`] can match,
/// and injects [`ManifestV2::custom_context`] fields into each matched
/// action's `type <Action>CustomContext = {};` stub.
///
/// # Matched-action resolution
///
/// Only [`TriggerField::ActionDomain`] and [`TriggerField::ActionTag`]
/// constraints affect which action *types* are included — venue / transaction
/// constraints narrow which concrete actions a policy evaluates but never
/// change the action type, so they are ignored here. An action row is included
/// when the domain/tag-only projection of the trigger matches it (reusing the
/// exact `eq` / `ne` / `in` / `nin` semantics of
/// [`crate::policy_rpc::evaluate_trigger`]). An empty trigger matches every row.
///
/// # Custom-context injection
///
/// When [`ManifestV2::custom_context`] is non-empty, its fields are injected
/// into the `<Stub>CustomContext` of *every* matched action (fields sorted by
/// name for determinism; a `"Decimal"` type spelling is normalized to the Cedar
/// `decimal` extension type). In practice a manifest declaring custom context
/// almost always pins a single action via `action.tag` `eq`, so the same fields
/// landing on several stubs only arises for deliberately broad triggers.
///
/// # Errors
///
/// Returns [`PolicyRpcError::Schema`] when a matched action's
/// `type <Stub>CustomContext = {};` stub is absent from the assembled text
/// (mirroring [`super::compose_enriched`]), or when a custom field name
/// collides with one of that action's base context fields.
pub fn compose_per_policy(manifest: &ManifestV2) -> Result<String, PolicyRpcError> {
    let matched = matched_entries(&manifest.trigger);

    // Assemble: core first, then each matched action's base schema. Reuse the
    // unified path's namespace merge so per-namespace blocks collapse into a
    // single `namespace <Name> { ... }` (Cedar rejects duplicates).
    let mut inputs: Vec<&str> = Vec::with_capacity(matched.len() + 1);
    inputs.push(CORE_SCHEMA);
    for entry in &matched {
        inputs.push(entry.schema_text);
    }
    let mut text = merge_namespace_blocks(&inputs);

    // Inject custom context fields into each matched action's stub.
    if !manifest.custom_context.fields.is_empty() {
        for entry in &matched {
            inject_custom_context(&mut text, entry, &manifest.custom_context.fields)?;
        }
    }

    Ok(text)
}

/// Resolve which action rows the trigger can match, considering only the
/// domain/tag constraints (venue/tx constraints do not change the action type).
fn matched_entries(trigger: &Trigger) -> Vec<&'static ActionEntry> {
    // Project the trigger down to its action-type-relevant constraints so a
    // venue/tx miss never excludes an action row.
    let mut where_ = BTreeMap::new();
    if let Some(c) = trigger.where_.get(&TriggerField::ActionDomain) {
        where_.insert(TriggerField::ActionDomain, c.clone());
    }
    if let Some(c) = trigger.where_.get(&TriggerField::ActionTag) {
        where_.insert(TriggerField::ActionTag, c.clone());
    }
    let projected = Trigger {
        scope: trigger.scope,
        where_,
    };

    // Transaction fields are irrelevant after projection; placeholders suffice.
    let tx = TxView {
        chain_id: "",
        from: "",
        to: "",
    };

    RESOLVER_TABLE
        .iter()
        .filter(|entry| {
            let view = ActionView {
                domain: entry.domain,
                action_tag: entry.action_tag,
                venue_name: None,
            };
            evaluate_trigger(&projected, &view, &tx)
        })
        .collect()
}

/// Replace one matched action's `type <Stub>CustomContext = {};` stub with a
/// populated type carrying the manifest's custom fields.
fn inject_custom_context(
    text: &mut String,
    entry: &ActionEntry,
    fields: &BTreeMap<String, String>,
) -> Result<(), PolicyRpcError> {
    let stub = format!("type {}CustomContext = {{}};\n", entry.pascal_stub);
    if !text.contains(&stub) {
        return Err(PolicyRpcError::Schema(format!(
            "per-policy base schema missing `type {}CustomContext = {{}};` stub \
             for action `{}`",
            entry.pascal_stub,
            entry.action_tag.unwrap_or(entry.domain),
        )));
    }
    let body = render_custom_body(entry, fields)?;
    *text = text.replace(&stub, &body);
    Ok(())
}

/// Render the populated `type <Stub>CustomContext = { ... };` text.
///
/// Fields are sorted by name (the source map is already a [`BTreeMap`], so this
/// is inherent) and each is declared optional (`?:`) per the fail-open
/// enrichment model. A `"Decimal"` type spelling is normalized to the Cedar
/// `decimal` extension type.
fn render_custom_body(
    entry: &ActionEntry,
    fields: &BTreeMap<String, String>,
) -> Result<String, PolicyRpcError> {
    let snake_tag = entry.action_tag.unwrap_or(entry.domain);
    // Source of truth for an action's base context fields is its OWN shipped
    // `.cedarschema` (`schema/policy-schema/actions/...`), parsed from the
    // `type <Stub>Context = { ... }` block — NOT a hardcoded table. This keeps
    // the collision check aligned with the `simulation-reducer` action shapes.
    let base_fields = context_base_fields(entry.schema_text, entry.pascal_stub);

    let mut lines = String::new();
    for (name, raw_type) in fields {
        if base_fields.contains(name) {
            return Err(PolicyRpcError::Schema(format!(
                "custom_context field `{name}` collides with base context field \
                 of action `{snake_tag}`"
            )));
        }
        let cedar_type = normalize_type(raw_type);
        // Writing to a `String` is infallible; the `Result` is discarded.
        let _ = writeln!(lines, "  {name}?: {cedar_type},");
    }
    Ok(format!(
        "type {}CustomContext = {{\n{lines}}};\n",
        entry.pascal_stub
    ))
}

/// Extract the top-level field names of an action's `type <Stub>Context = {...}`
/// block from its cedarschema text. The shipped action `.cedarschema` files are
/// the single source of truth for an action's base context fields; the
/// manifest-extensible `custom` slot is excluded.
fn context_base_fields(schema_text: &str, pascal_stub: &str) -> std::collections::BTreeSet<String> {
    let mut fields = std::collections::BTreeSet::new();
    let needle = format!("type {pascal_stub}Context = {{");
    let Some(start) = schema_text.find(&needle) else {
        return fields;
    };
    let body = &schema_text[start + needle.len()..];
    // The `<Stub>Context` block ends at the first `};`. Action context fields
    // reference NAMED types (e.g. `Core::ActionMeta`), never inline records, so
    // no nested `};` can appear before the block's own terminator.
    let end = body.find("};").unwrap_or(body.len());
    for line in body[..end].lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        // Field shape: `name: Type,` or `name?: Type,`. The first `:` precedes
        // any `::` in the type, so the ident is everything before it.
        if let Some(colon) = trimmed.find(':') {
            let name = trimmed[..colon].trim().trim_end_matches('?').trim();
            if !name.is_empty()
                && name != "custom"
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                fields.insert(name.to_owned());
            }
        }
    }
    fields
}

/// Normalize a manifest type spelling to its Cedar form. Only `Decimal` differs
/// (Cedar's extension type is the lowercase `decimal`); every other spelling is
/// passed through verbatim.
fn normalize_type(raw: &str) -> &str {
    match raw {
        "Decimal" => "decimal",
        other => other,
    }
}

/// Lint a policy body's custom-field references against the manifest's declared
/// fields, closing the `has`-guard validation gap (Cedar spike finding #2).
///
/// Per-policy strict validation (run at install) rejects an UNGUARDED reference
/// to an undeclared `context.custom.<field>`, but a `has`-guarded reference
/// (`context.custom has X && context.custom.X`) is dead-code-eliminated by
/// Cedar's type checker and slips through. This lint catches it: every field
/// reached via `context.custom.<field>` or `context.custom has <field>` must be
/// declared in [`ManifestV2::custom_context`].
///
/// Pragmatic `PoC`: a textual scan (not a full Cedar AST walk) — sufficient
/// because custom fields are always reached through the literal `context.custom`
/// path. A bundle author who writes that path inside a comment for an
/// undeclared field would get a false positive (documented; don't do that).
///
/// # Errors
///
/// Returns [`PolicyRpcError::Schema`] naming the first undeclared field.
pub fn lint_custom_field_refs(
    policy_cedar: &str,
    manifest: &ManifestV2,
) -> Result<(), PolicyRpcError> {
    let declared = &manifest.custom_context.fields;
    for field in custom_field_refs(policy_cedar) {
        if !declared.contains_key(&field) {
            return Err(PolicyRpcError::Schema(format!(
                "policy `{}` references undeclared custom field `context.custom.{field}` \
                 — declare it in manifest.custom_context.fields",
                manifest.id
            )));
        }
    }
    Ok(())
}

/// Collect every identifier reached via `context.custom.<ident>` or
/// `context.custom has <ident>` in a Cedar policy body.
fn custom_field_refs(policy_cedar: &str) -> std::collections::BTreeSet<String> {
    const PREFIX: &str = "context.custom";
    let mut refs = std::collections::BTreeSet::new();
    for (idx, _) in policy_cedar.match_indices(PREFIX) {
        let rest = &policy_cedar[idx + PREFIX.len()..];
        if let Some(after_dot) = rest.strip_prefix('.') {
            if let Some(ident) = leading_ident(after_dot) {
                refs.insert(ident);
            }
        } else if let Some(after_has) = rest.trim_start().strip_prefix("has ") {
            if let Some(ident) = leading_ident(after_has.trim_start()) {
                refs.insert(ident);
            }
        }
    }
    refs
}

/// Take the leading Cedar identifier (`[A-Za-z_][A-Za-z0-9_]*`) of `s`, if any.
fn leading_ident(s: &str) -> Option<String> {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    if end == 0 || s.as_bytes()[0].is_ascii_digit() {
        return None;
    }
    Some(s[..end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_rpc::{CustomContext, TriggerConstraint, TriggerScope};

    fn manifest(trigger: Trigger, fields: &[(&str, &str)]) -> ManifestV2 {
        let mut map = BTreeMap::new();
        for (k, v) in fields {
            map.insert((*k).to_owned(), (*v).to_owned());
        }
        ManifestV2 {
            id: "test".to_owned(),
            schema_version: 2,
            trigger,
            policy_rpc: Vec::new(),
            custom_context: CustomContext { fields: map },
        }
    }

    fn trigger_of(pairs: &[(TriggerField, TriggerConstraint)]) -> Trigger {
        let mut where_ = BTreeMap::new();
        for (field, constraint) in pairs {
            where_.insert(*field, constraint.clone());
        }
        Trigger {
            scope: TriggerScope::Inner,
            where_,
        }
    }

    /// Trigger `{ action.tag: eq "swap" }` + a custom field injects into the
    /// swap stub, parses, and includes only core + swap (no other domains).
    ///
    /// NOTE: the field type is `Decimal` (→ normalized `decimal`), not the
    /// spec's `UsdValuation`. `UsdValuation`'s Cedar `type` definition was
    /// removed in the pre-migration cleanup, so a schema referencing it no
    /// longer parses; `decimal` is the Cedar extension type the swap schema
    /// already uses for USD fields, which keeps the parse-success assertion
    /// meaningful while still exercising the injection + stub replacement.
    #[test]
    fn swap_with_custom_field_parses() {
        let trigger = trigger_of(&[(
            TriggerField::ActionTag,
            TriggerConstraint::Eq("swap".into()),
        )]);
        let m = manifest(trigger, &[("totalInputUsd", "Decimal")]);
        let text = compose_per_policy(&m).expect("compose");

        let parsed = cedar_policy::Schema::from_cedarschema_str(&text);
        assert!(
            parsed.is_ok(),
            "per-policy swap schema must parse: {:?}",
            parsed.err()
        );
        assert!(text.contains("type SwapCustomContext = {"));
        assert!(text.contains("totalInputUsd?: decimal"));
        assert!(
            !text.contains("type SwapCustomContext = {};"),
            "stub must be replaced"
        );
        // Only core + swap: no other domain's action context type.
        assert!(!text.contains("BorrowContext"));
        assert!(!text.contains("OpenPositionContext"));
        assert!(!text.contains("AddLiquidityContext"));
    }

    /// Same trigger, but no `custom_context` → the swap stub is left untouched
    /// and the schema still parses.
    #[test]
    fn empty_custom_context_leaves_stub() {
        let trigger = trigger_of(&[(
            TriggerField::ActionTag,
            TriggerConstraint::Eq("swap".into()),
        )]);
        let m = manifest(trigger, &[]);
        let text = compose_per_policy(&m).expect("compose");

        assert!(text.contains("type SwapCustomContext = {};"));
        let parsed = cedar_policy::Schema::from_cedarschema_str(&text);
        assert!(parsed.is_ok(), "{:?}", parsed.err());
    }

    /// Trigger pinning the `set_e_mode` serde tag resolves to the
    /// `set_emode.cedarschema` file (whose context type is `SetEModeContext`)
    /// and parses. Pins the serde-tag ↔ filename ↔ stub three-way mismatch.
    #[test]
    fn set_e_mode_resolves_to_set_emode_schema() {
        let trigger = trigger_of(&[
            (
                TriggerField::ActionDomain,
                TriggerConstraint::Eq("lending".into()),
            ),
            (
                TriggerField::ActionTag,
                TriggerConstraint::Eq("set_e_mode".into()),
            ),
        ]);
        let m = manifest(trigger, &[]);
        let text = compose_per_policy(&m).expect("compose");

        assert!(
            text.contains("type SetEModeContext = {"),
            "set_e_mode trigger must include the set_emode action context"
        );
        let parsed = cedar_policy::Schema::from_cedarschema_str(&text);
        assert!(parsed.is_ok(), "{:?}", parsed.err());
    }

    /// Domain-only trigger `{ action.domain: eq "amm" }` includes all 6 amm
    /// action contexts and parses.
    #[test]
    fn domain_only_trigger_includes_all_amm() {
        let trigger = trigger_of(&[(
            TriggerField::ActionDomain,
            TriggerConstraint::Eq("amm".into()),
        )]);
        let m = manifest(trigger, &[]);
        let text = compose_per_policy(&m).expect("compose");

        for ctx in [
            "SwapContext",
            "AddLiquidityContext",
            "RemoveLiquidityContext",
            "CollectFeesContext",
            "SignIntentOrderContext",
            "CancelIntentOrderContext",
        ] {
            assert!(text.contains(ctx), "amm trigger must include `{ctx}`");
        }
        // No other domain leaked in.
        assert!(!text.contains("BorrowContext"));
        let parsed = cedar_policy::Schema::from_cedarschema_str(&text);
        assert!(parsed.is_ok(), "{:?}", parsed.err());
    }

    /// Empty trigger → core + all 45 actions. Confirms no namespace / type
    /// collisions across the full set.
    #[test]
    fn always_trigger_parses() {
        let m = manifest(Trigger::default(), &[]);
        let text = compose_per_policy(&m).expect("compose");

        let parsed = cedar_policy::Schema::from_cedarschema_str(&text);
        assert!(
            parsed.is_ok(),
            "all-actions per-policy schema must parse: {:?}",
            parsed.err()
        );
        // Spot-check one action per namespace is present.
        for ctx in [
            "SwapContext",
            "BorrowContext",
            "OpenPositionContext",
            "Erc20ApproveContext",
            "CommitContext",
            "ClaimContext",
        ] {
            assert!(text.contains(ctx), "always trigger must include `{ctx}`");
        }
    }

    /// Every resolver row's `(schema_text, pascal_stub)` pairing must be
    /// internally consistent: the named schema const must actually declare
    /// `type <Stub>CustomContext = {};`. This fires loudly if a row points a
    /// stub at the wrong schema file (the kind of mistake no
    /// parse-only test catches, because the stub-replacement path only runs
    /// when a manifest declares `custom_context`).
    #[test]
    fn resolver_table_stubs_exist() {
        for entry in RESOLVER_TABLE {
            let stub = format!("type {}CustomContext = {{}};", entry.pascal_stub);
            assert!(
                entry.schema_text.contains(&stub),
                "resolver row (domain={}, tag={:?}) names stub `{}` but its schema \
                 const does not declare `{stub}`",
                entry.domain,
                entry.action_tag,
                entry.pascal_stub,
            );
        }
        // The table covers exactly the 74 shipped actions (multicall + unknown +
        // 6 liquid_staking + 1 permission + 7 restaking + 8 staking +
        // 5 hyperliquid_core included). Guards against a row being dropped or duplicated.
        assert_eq!(RESOLVER_TABLE.len(), 74, "resolver table must have 74 rows");
    }

    /// A `custom_context` field whose name collides with one of the matched
    /// action's base context fields must be rejected with
    /// [`PolicyRpcError::Schema`] (mirroring the unified composer's Rule 4).
    /// `recipient` is a declared base field of `SwapContext`.
    #[test]
    fn rejects_custom_field_colliding_with_base() {
        let trigger = trigger_of(&[(
            TriggerField::ActionTag,
            TriggerConstraint::Eq("swap".into()),
        )]);
        let m = manifest(trigger, &[("recipient", "String")]);
        let err = compose_per_policy(&m).expect_err("collision must be rejected");
        match err {
            PolicyRpcError::Schema(msg) => {
                assert!(
                    msg.contains("recipient"),
                    "error must name the colliding field: {msg}"
                );
                assert!(msg.contains("swap"), "error must name the action: {msg}");
            }
            other => panic!("expected PolicyRpcError::Schema, got {other:?}"),
        }
    }

    #[test]
    fn base_fields_derive_from_cedarschema_not_legacy_table() {
        // `tokenIn` is a REAL base field of the new `Amm::SwapContext`
        // (schema/policy-schema/actions/amm/swap.cedarschema) but is ABSENT
        // from the legacy `manifest_fragment::base_field_names("swap")` table
        // (which lists the stale `inputToken`/`outputToken`). Deriving base
        // fields from the cedarschema (source of truth) must reject it.
        let base = context_base_fields(super::AMM_SWAP_SCHEMA, "Swap");
        assert!(base.contains("tokenIn"), "new model field must be detected");
        assert!(base.contains("venue"));
        assert!(!base.contains("inputToken"), "legacy field must NOT appear");
        assert!(!base.contains("custom"), "the custom slot is excluded");

        let m = ManifestV2 {
            id: "collide-new-base".to_owned(),
            schema_version: 2,
            trigger: Trigger {
                scope: TriggerScope::Inner,
                where_: [(
                    TriggerField::ActionTag,
                    TriggerConstraint::Eq("swap".to_owned()),
                )]
                .into_iter()
                .collect(),
            },
            policy_rpc: Vec::new(),
            custom_context: CustomContext {
                fields: [("tokenIn".to_owned(), "String".to_owned())]
                    .into_iter()
                    .collect(),
            },
        };
        let err = compose_per_policy(&m).expect_err("tokenIn collides with a new base field");
        assert!(matches!(err, PolicyRpcError::Schema(msg) if msg.contains("tokenIn")));
    }

    // ----- Task 7: custom-field reference lint (has-guard gap) -----

    fn manifest_declaring(fields: &[(&str, &str)]) -> ManifestV2 {
        ManifestV2 {
            id: "lint-test".to_owned(),
            schema_version: 2,
            trigger: Trigger::default(),
            policy_rpc: Vec::new(),
            custom_context: CustomContext {
                fields: fields
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                    .collect(),
            },
        }
    }

    #[test]
    fn lint_passes_when_all_custom_refs_declared() {
        let policy = "@id(\"p\") forbid(principal, action, resource) when { \
                      context has custom && context.custom has totalInputUsd && \
                      context.custom.totalInputUsd > decimal(\"1.0\") };";
        let manifest = manifest_declaring(&[("totalInputUsd", "decimal")]);
        lint_custom_field_refs(policy, &manifest).expect("declared field passes");
    }

    #[test]
    fn lint_rejects_undeclared_has_guarded_field() {
        // The reference IS `has`-guarded, so per-policy strict validation would
        // dead-code-eliminate and miss it — this lint must catch it.
        let policy = "@id(\"p\") forbid(principal, action, resource) when { \
                      context.custom has tokenRiskScore && \
                      context.custom.tokenRiskScore > 50 };";
        let manifest = manifest_declaring(&[("totalInputUsd", "decimal")]);
        let err = lint_custom_field_refs(policy, &manifest).expect_err("undeclared must fail");
        match err {
            PolicyRpcError::Schema(msg) => assert!(
                msg.contains("tokenRiskScore"),
                "error must name the undeclared field: {msg}"
            ),
            other => panic!("expected Schema error, got {other:?}"),
        }
    }

    #[test]
    fn lint_ignores_non_custom_context_refs() {
        // Base-context refs (not under `context.custom`) are validated by the
        // per-policy schema, not this lint; an empty custom_context is fine.
        let policy = "@id(\"p\") forbid(principal, action, resource) when { \
                      context.recipient == \"0x0000000000000000000000000000000000000000\" };";
        let manifest = manifest_declaring(&[]);
        lint_custom_field_refs(policy, &manifest).expect("no custom refs → ok");
    }

    // ----- End-to-end: the full PR2 pipeline against the REAL swap schema -----

    #[test]
    fn end_to_end_swap_bundle_compose_validate_evaluate() {
        use crate::policy::{PolicyEngine, Verdict};
        use serde_json::json;

        // A realistic marketplace bundle: a swap policy that warns when the
        // oracle-priced input value exceeds a USD threshold. Uses `decimal`
        // (the enrichment value types like UsdValuation were dropped from the
        // base schema), action id `Amm::Action::"Swap"`, and the `context.custom`
        // slot the manifest declares.
        // Cedar idioms exercised here (policy-authoring guidance):
        //  - `custom` is optional, so guard `context has custom` BEFORE
        //    `context.custom has <field>` before the access;
        //  - `decimal` has no `>` operator — use `.greaterThan(decimal(..))`.
        let policy_cedar = "@id(\"large-swap-usd-warning\")\n@severity(\"warn\")\n\
             forbid(principal, action == Amm::Action::\"Swap\", resource)\n\
             when { context has custom && context.custom has totalInputUsd && \
             context.custom.totalInputUsd.greaterThan(decimal(\"10000.0\")) };\n";

        let manifest: ManifestV2 = serde_json::from_value(json!({
            "id": "large-swap-usd-warning",
            "schema_version": 2,
            "trigger": { "where": { "action.tag": { "eq": "swap" } } },
            "policy_rpc": [{
                "id": "input-usd",
                "method": "oracle.usd_value",
                "params": {},
                "optional": true,
                "outputs": [{
                    "kind": "context", "field": "totalInputUsd",
                    "type": "Decimal", "from": "$.result.usd", "required": false
                }]
            }],
            "custom_context": { "fields": { "totalInputUsd": "decimal" } }
        }))
        .unwrap();
        manifest.validate().expect("manifest valid");

        // 1. synthesize the isolated per-policy schema from the REAL swap base.
        let schema = compose_per_policy(&manifest).expect("schema synthesizes");
        assert!(schema.contains("totalInputUsd?: decimal"));
        // 2. lint custom-field references against the manifest.
        lint_custom_field_refs(policy_cedar, &manifest).expect("refs declared");
        // 3. install: the policy strict-validates against its own schema.
        let engine = PolicyEngine::build_from_per_policy(&[(policy_cedar.to_owned(), schema)])
            .expect("policy validates against its synthesized schema");

        // 4a. input over the threshold → warn-severity forbid fires.
        let over = json!({
            "custom": { "totalInputUsd": { "__extn": { "fn": "decimal", "arg": "15000.0" } } }
        });
        let verdict = engine
            .evaluate(
                "Wallet::\"w\"",
                "Amm::Action::\"Swap\"",
                "Protocol::\"p\"",
                &json!([]),
                &over,
            )
            .expect("evaluate over");
        assert!(
            matches!(verdict, Verdict::Warn(_)),
            "expected Warn, got {verdict:?}"
        );

        // 4b. input under the threshold → baseline permit → pass.
        let under = json!({
            "custom": { "totalInputUsd": { "__extn": { "fn": "decimal", "arg": "5000.0" } } }
        });
        let verdict = engine
            .evaluate(
                "Wallet::\"w\"",
                "Amm::Action::\"Swap\"",
                "Protocol::\"p\"",
                &json!([]),
                &under,
            )
            .expect("evaluate under");
        assert!(
            matches!(verdict, Verdict::Pass),
            "expected Pass, got {verdict:?}"
        );
    }
}
