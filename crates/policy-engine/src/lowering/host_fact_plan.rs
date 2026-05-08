//! Host fact plan extraction.
//!
//! `required_host_facts(&Action) -> HostFactPlan` describes what host data
//! must be fetched before enrichment runs. The plan is the contract the
//! engine exposes to external orchestrators (notably the Chrome extension's
//! WASM bridge) so they can prefetch RPC reads and price quotes in parallel.
//!
//! Two tiers exist because windowing depends on already-stamped USD values:
//! - Tier 1: oracle, balances, allowances, clock — derivable from a bare Action.
//! - Tier 2: window keys — requires an `OracleSnapshot` because window keys
//!   are derived per-actor from USD-stamped enrichment output.

use crate::core::{Action, Address, OracleRequirement, Token};
use crate::host::oracle::SnapshotOracle;
use crate::host::stat_windows::StatKey;

/// Tier-1 host facts the engine needs from a precomputed snapshot.
///
/// Returned by [`required_host_facts`]. Each field enumerates a distinct
/// host capability lookup the snapshot must satisfy. Empty fields mean the
/// action does not require that capability.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostFactPlan {
    /// Tokens for which oracle USD prices are required.
    pub tokens_for_oracle: Vec<Token>,
    /// `(owner, token)` tuples for which `balanceOf(owner)` is required.
    pub balances: Vec<(Address, Token)>,
    /// `(owner, token, spender)` tuples for which `allowance(owner, spender)` is required.
    pub allowances: Vec<(Address, Token, Address)>,
    /// Whether evaluation requires the host clock (`nowTs` stamping).
    pub clock_required: bool,
    /// Signature-side oracle requirements that mirror DEX `oracle_requirements`.
    /// Used by the orchestrator when richer USD provenance metadata is desired
    /// (e.g., distinguishing "approve token X" vs "transfer token X").
    pub sig_oracle_requirements: Vec<OracleRequirement>,
}

/// Tier-2 host facts: window keys derivable only after USD enrichment.
///
/// Returned by [`required_window_keys`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowKeyPlan {
    /// Per-actor window keys to read from `StatWindows` before evaluation.
    pub keys: Vec<WindowKey>,
}

/// One key into the host's stat-window store.
///
/// Uses the engine's canonical `StatKey` newtype rather than a raw string
/// so that wire emission goes through `StatKey::as_str()` exactly once
/// (in the WASM bridge), and Rust code can match against `StatKey::*`
/// constants without typo risk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowKey {
    /// Wallet actor.
    pub actor: Address,
    /// Canonical stat key — see `crates/policy-engine/src/host/stat_windows.rs`.
    pub key: StatKey,
}

/// Tier-1 plan extraction. Pure function over a built Action.
#[must_use]
pub fn required_host_facts(action: &Action) -> HostFactPlan {
    let mut plan = HostFactPlan::default();
    match action {
        Action::Dex(dex) => {
            // Oracle: union of input + output tokens (deduped by chain-qualified key).
            let mut seen = std::collections::HashSet::new();
            for token in dex
                .facts
                .input_tokens
                .iter()
                .chain(dex.facts.output_tokens.iter())
            {
                if seen.insert(token.key()) {
                    plan.tokens_for_oracle.push(token.clone());
                }
            }

            // Balance + allowance: actor against each non-native input token.
            // Allowances target the contract that received the calldata (DEX router/Permit2).
            for token in &dex.facts.input_tokens {
                if token.is_native {
                    continue;
                }
                plan.balances.push((dex.actor.clone(), token.clone()));
                plan.allowances
                    .push((dex.actor.clone(), token.clone(), dex.target.clone()));
            }
        }
        Action::Other(_) | Action::Permit2(_) | Action::Eip2612(_) | Action::Eip712Other(_) => {
            // Filled in subsequent tasks.
        }
    }
    plan
}

/// Tier-2 plan extraction. Pure function over a built Action plus the
/// already-fetched oracle snapshot.
#[must_use]
pub fn required_window_keys(_action: &Action, _oracle: &SnapshotOracle) -> WindowKeyPlan {
    WindowKeyPlan::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        Address, DexAction, DexFacts, DexTrace, OracleRequirement, OracleRequirementKind, Token,
    };

    fn addr(hex: &str) -> Address {
        Address::new(hex).unwrap()
    }
    fn weth() -> Token {
        Token {
            chain_id: 1,
            address: addr("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"),
            symbol: "WETH".into(),
            decimals: 18,
            is_native: false,
        }
    }
    fn usdc() -> Token {
        Token {
            chain_id: 1,
            address: addr("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
            symbol: "USDC".into(),
            decimals: 6,
            is_native: false,
        }
    }
    fn native_eth() -> Token {
        Token {
            chain_id: 1,
            address: addr("0x0000000000000000000000000000000000000000"),
            symbol: "ETH".into(),
            decimals: 18,
            is_native: true,
        }
    }

    fn dex_swap_weth_to_usdc(actor: Address, target: Address) -> Action {
        Action::Dex(DexAction {
            actor,
            target,
            value_wei: "0".into(),
            facts: DexFacts {
                protocol_ids: vec!["uniswap_v3".into()],
                input_tokens: vec![weth()],
                output_tokens: vec![usdc()],
                ..Default::default()
            },
            oracle_requirements: vec![
                OracleRequirement {
                    kind: OracleRequirementKind::Input,
                    token: weth(),
                    raw_amount: "1000000000000000000".into(),
                },
                OracleRequirement {
                    kind: OracleRequirementKind::MinOutput,
                    token: usdc(),
                    raw_amount: "3400000000".into(),
                },
            ],
            trace: DexTrace::default(),
        })
    }

    #[test]
    fn dex_plan_collects_oracle_balance_and_allowance() {
        let actor = addr("0x1111111111111111111111111111111111111111");
        let target = addr("0xE592427A0AEce92De3Edee1F18E0157C05861564"); // V3 SwapRouter
        let action = dex_swap_weth_to_usdc(actor.clone(), target.clone());

        let plan = required_host_facts(&action);

        // Oracle: input + output tokens.
        let oracle_addrs: Vec<_> = plan
            .tokens_for_oracle
            .iter()
            .map(|t| t.address.as_str().to_lowercase())
            .collect();
        assert!(oracle_addrs.contains(&weth().address.as_str().to_lowercase()));
        assert!(oracle_addrs.contains(&usdc().address.as_str().to_lowercase()));

        // Balances: actor for each non-native input token.
        assert_eq!(plan.balances.len(), 1);
        assert_eq!(plan.balances[0].0, actor);
        assert_eq!(plan.balances[0].1.symbol, "WETH");

        // Allowances: actor against target for each non-native input token.
        assert_eq!(plan.allowances.len(), 1);
        assert_eq!(plan.allowances[0].0, actor);
        assert_eq!(plan.allowances[0].1.symbol, "WETH");
        assert_eq!(plan.allowances[0].2, target);

        assert!(!plan.clock_required);
        assert!(plan.sig_oracle_requirements.is_empty());
    }

    #[test]
    fn dex_plan_skips_native_token_for_balance_and_allowance() {
        let actor = addr("0x1111111111111111111111111111111111111111");
        let target = addr("0xE592427A0AEce92De3Edee1F18E0157C05861564");
        let action = Action::Dex(DexAction {
            actor: actor.clone(),
            target: target.clone(),
            value_wei: "1000000000000000000".into(),
            facts: DexFacts {
                protocol_ids: vec!["uniswap_v3".into()],
                input_tokens: vec![native_eth()],
                output_tokens: vec![usdc()],
                ..Default::default()
            },
            oracle_requirements: vec![],
            trace: DexTrace::default(),
        });

        let plan = required_host_facts(&action);

        // Native ETH is not a balanceOf/allowance candidate.
        assert!(plan.balances.is_empty());
        assert!(plan.allowances.is_empty());
    }
}
