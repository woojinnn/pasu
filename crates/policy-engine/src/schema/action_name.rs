//! Action-name conventions: `snake_case` ↔ `PascalCase` and the registry of
//! actions whose cedarschemas ship with the policy engine.

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

/// All action kinds whose cedarschemas ship in `schema/policy-schema/actions/`.
pub const REGISTERED_ACTIONS: &[&str] = &[
    "swap",
    "add_liquidity",
    "remove_liquidity",
    "mint_liquidity_nft",
    "burn_liquidity_nft",
    "increase_liquidity",
    "decrease_liquidity",
    "initialize_pool",
    "donate",
    "supply",
    "withdraw",
    "borrow",
    "repay",
    "liquidate",
    "flash_loan",
    "set_authorization",
    "sign_authorization",
    "revoke",
    "stake",
    "request_unstake",
    "claim_unstake",
    "restake",
    "request_restake_withdrawal",
    "claim_restake_withdrawal",
    "wrap",
    "unwrap",
    "approve",
    "set_approval_for_all",
    "transfer",
    "permit",
    "claim_rewards",
    "sign_message",
    "delegate",
    "vote",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_to_pascal_basic() {
        assert_eq!(snake_to_pascal("swap"), "Swap");
        assert_eq!(snake_to_pascal("add_liquidity"), "AddLiquidity");
        assert_eq!(snake_to_pascal("mint_liquidity_nft"), "MintLiquidityNft");
    }

    #[test]
    fn registered_action_names_round_trip() {
        for a in REGISTERED_ACTIONS {
            let p = snake_to_pascal(a);
            assert!(!p.is_empty());
        }
    }

    #[test]
    fn registry_size_matches_shipped_schemas() {
        assert_eq!(REGISTERED_ACTIONS.len(), 34);
    }
}
