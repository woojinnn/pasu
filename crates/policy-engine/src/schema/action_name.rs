//! Action-name conventions and the registry of actions whose cedarschemas
//! ship with the policy engine.
//!
//! ## Two coexisting forms
//!
//! 1. **`snake_case` bare action name** — used internally by composer /
//!    `manifest_fragment` paths. `REGISTERED_ACTIONS` is the Phase 1 action set
//!    derived from `ActionBody`'s domain enums (60 entries).
//!
//! 2. **`<Namespace>::<PascalCaseAction>`** — the fully-qualified Cedar action
//!    id under the namespace migration. Composed via [`namespace_action_id`].
//!    The Cedar context type id is `<Namespace>::<PascalCase>Context` via
//!    [`namespace_context_type_id`].
//!
//! The `composer/manifest_fragment` paths continue to operate on bare `PascalCase`
//! type names (e.g. `SwapCustomContext`) — text search in the concatenated
//! `base_schema_text()` finds them inside their namespace block. Migrating
//! those paths to namespace-qualified ids is tracked as follow-up work.

/// Convert a `snake_case` action name to its `PascalCase` Cedar type prefix.
#[must_use]
pub fn snake_to_pascal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = true;
    for ch in s.chars() {
        if ch == '_' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Compose a fully-qualified Cedar action id (`<Namespace>::<PascalCaseAction>`)
/// from a `snake_case` namespace / action pair.
///
/// ```
/// # use policy_engine::schema::action_name::namespace_action_id;
/// assert_eq!(namespace_action_id("amm", "swap"), "Amm::Swap");
/// assert_eq!(namespace_action_id("token", "erc20_approve"), "Token::Erc20Approve");
/// assert_eq!(namespace_action_id("core", "multicall"), "Core::Multicall");
/// ```
#[must_use]
pub fn namespace_action_id(domain: &str, action: &str) -> String {
    format!("{}::{}", snake_to_pascal(domain), snake_to_pascal(action))
}

/// Compose the bare `PascalCase` context type name for an action
/// (`<PascalCase>Context`). For the fully-qualified Cedar id including the
/// namespace prefix, use [`namespace_context_type_id`].
#[must_use]
pub fn context_type_name(action: &str) -> String {
    format!("{}Context", snake_to_pascal(action))
}

/// Compose the fully-qualified Cedar context type id `<Namespace>::<PascalCase>Context`.
#[must_use]
pub fn namespace_context_type_id(domain: &str, action: &str) -> String {
    format!(
        "{}::{}Context",
        snake_to_pascal(domain),
        snake_to_pascal(action)
    )
}

/// Phase 1 `snake_case` action set (60 entries) — produced by `ActionBody`'s
/// domain enums. Each maps to a `.cedarschema` file under
/// `schema/policy-schema/actions/{token,amm,lending,airdrop,launchpad,perp,permission}/`
/// and to a Cedar action id `<Namespace>::<PascalCase>` (via
/// [`namespace_action_id`]).
pub const REGISTERED_ACTIONS: &[&str] = &[
    // Core structural (ActionBody::Multicall / ActionBody::Unknown)
    "multicall",
    "unknown",
    // Airdrop (2)
    "claim",
    "delegate",
    // Amm (8)
    "add_liquidity",
    "cancel_intent_order",
    "collect_fees",
    "gsm_swap",
    "pre_sign_intent_order",
    "remove_liquidity",
    "settle_intent_order",
    "sign_intent_order",
    "swap",
    // Governance (10) — `delegate` already listed above under Airdrop (dedup;
    // per-domain disambiguation in per_policy::RESOLVER_TABLE).
    "activate_voting",
    "cancel",
    "close_vote",
    "execute",
    "propose",
    "queue",
    "redeem_cancellation_fee",
    "start_vote",
    "update_representative",
    "vote",
    // Lending (13)
    "borrow",
    "buy_collateral",
    "delegate_borrow",
    "disable_collateral",
    "enable_collateral",
    "liquidate",
    "periphery_operation",
    "repay",
    "set_authorization",
    "set_emode",
    "supply",
    "swap_rate_mode",
    "withdraw",
    // LiquidStaking (6)
    "claim_withdrawal",
    "request_withdrawal",
    "stake",
    "transfer_shares",
    "unwrap",
    "wrap",
    // Yield (11)
    "add_market_liquidity",
    "cancel_limit_order",
    "claim_yield",
    "mint_py",
    "mint_sy",
    "pt_swap",
    "redeem_py",
    "redeem_sy",
    "remove_market_liquidity",
    "sign_limit_order",
    "yt_swap",
    // Launchpad (5)
    "claim_allocation",
    "claim_vested",
    "commit",
    "refund",
    "withdraw_commit",
    // Perp (11)
    "adjust_margin",
    "cancel_order",
    "change_leverage",
    "change_margin_mode",
    "claim_funding",
    "close_position",
    "decrease_position",
    "increase_position",
    "open_position",
    "place_limit_order",
    "place_stop_order",
    // Permission (1)
    "protocol_authorization",
    // Restaking (7) — `delegate` already listed above under Airdrop, so `delegate_to`
    "complete_withdrawal",
    "delegate_to",
    "deposit",
    "queue_withdrawal",
    "redelegate",
    "register_operator",
    "undelegate",
    // Staking (10) — `stake` already listed above under LiquidStaking (the
    // tag set is deduplicated; per-domain disambiguation lives in
    // per_policy::RESOLVER_TABLE, which keys on (domain, tag)).
    "claim_rewards",
    "cooldown",
    "gauge_deposit",
    "gauge_withdraw",
    "increase_lock_amount",
    "increase_lock_time",
    "lock",
    "redeem",
    "unlock",
    "vote_for_gauge",
    // Token (13) — `delegate` already listed above under Airdrop
    "erc20_approve",
    "erc20_permit",
    "erc20_transfer",
    "nft_approve",
    "nft_set_approval_for_all",
    "nft_transfer",
    "permit2_approve",
    "permit2_sign_allowance",
    "permit2_sign_transfer",
    "permit2_transfer_from",
    "revoke_approval",
    "unwrap_native",
    "wrap_native",
    // HyperliquidCore (18) — thin off-chain L1 action model. `hl_`-prefixed so
    // the tags stay globally unique (e.g. `withdraw` already exists in Lending).
    "hl_order",
    "hl_update_leverage",
    "hl_withdraw",
    "hl_usd_send",
    "hl_approve_agent",
    "hl_unknown",
    "hl_spot_send",
    "hl_usd_class_transfer",
    "hl_send_asset",
    "hl_send_to_evm_with_data",
    "hl_c_deposit",
    "hl_c_withdraw",
    "hl_vault_transfer",
    "hl_sub_account_transfer",
    "hl_approve_builder_fee",
    "hl_token_delegate",
    "hl_twap_order",
    "hl_update_isolated_margin",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_to_pascal_basic() {
        assert_eq!(snake_to_pascal("swap"), "Swap");
        assert_eq!(snake_to_pascal("add_liquidity"), "AddLiquidity");
        assert_eq!(snake_to_pascal("erc20_approve"), "Erc20Approve");
        assert_eq!(snake_to_pascal("open_position"), "OpenPosition");
        assert_eq!(snake_to_pascal("mint_liquidity_nft"), "MintLiquidityNft");
    }

    #[test]
    fn namespace_action_id_basic() {
        assert_eq!(namespace_action_id("amm", "swap"), "Amm::Swap");
        assert_eq!(
            namespace_action_id("token", "erc20_approve"),
            "Token::Erc20Approve"
        );
        assert_eq!(namespace_action_id("core", "multicall"), "Core::Multicall");
    }

    #[test]
    fn namespace_context_type_id_basic() {
        assert_eq!(namespace_context_type_id("amm", "swap"), "Amm::SwapContext");
        assert_eq!(
            namespace_context_type_id("lending", "borrow"),
            "Lending::BorrowContext"
        );
    }

    #[test]
    fn context_type_name_basic() {
        assert_eq!(context_type_name("swap"), "SwapContext");
        assert_eq!(context_type_name("erc20_approve"), "Erc20ApproveContext");
    }

    #[test]
    fn registered_action_names_round_trip() {
        for a in REGISTERED_ACTIONS {
            let p = snake_to_pascal(a);
            assert!(!p.is_empty());
        }
    }

    #[test]
    fn registry_size_matches_phase1() {
        // Union of feat/registry-v2 (74: + 7 Restaking + 8 Staking + 5 HyperliquidCore)
        // and the 11 Pendle `yield` rows (pt_swap / yt_swap / add+remove_market_liquidity
        // / mint_py / redeem_py / mint_sy / redeem_sy / claim_yield / sign_limit_order
        // / cancel_limit_order) = 85, plus 13 more HyperliquidCore actions: the
        // `hl_unknown` catch-all + 8 fund-movement (spot_send / usd_class_transfer
        // / send_asset / send_to_evm_with_data / c_deposit / c_withdraw /
        // vault_transfer / sub_account_transfer) + 2 permission (approve_builder_fee
        // / token_delegate) + 2 trading/margin (twap_order / update_isolated_margin)
        // = 98, plus `settle_intent_order` for on-chain intent settlement = 99.
        // Union of feat/registry-v2 (incl. weth-wrap `wrap_native`/`unwrap_native`
        // + CoW Swap `pre_sign_intent_order`) and feat/morpho-onboarding (Compound
        // + Aave `gsm_swap` + governance + lending periphery + staking
        // redeem/stake/cooldown) = 118.
        assert_eq!(REGISTERED_ACTIONS.len(), 118);
    }

    #[test]
    fn registered_actions_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for a in REGISTERED_ACTIONS {
            assert!(
                seen.insert(*a),
                "duplicate action `{a}` in REGISTERED_ACTIONS"
            );
        }
    }
}
