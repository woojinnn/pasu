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
            // Oracle: derive from `dex.oracle_requirements` so the plan matches
            // exactly what enrichment will consult (lowering/stamping/dex.rs:64,
            // :164). Falling back to `input_tokens`/`output_tokens` would miss
            // tokens when adapter populated only `oracle_requirements`.
            let mut seen = std::collections::HashSet::new();
            for req in &dex.oracle_requirements {
                if seen.insert(req.token.key()) {
                    plan.tokens_for_oracle.push(req.token.clone());
                }
            }
            // Defensive: also surface any input/output tokens that didn't make
            // it into `oracle_requirements` (shouldn't happen for well-formed
            // adapters, but cheap to guard).
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

            // Balance + allowance: actor against each non-native Input token
            // from `oracle_requirements`. Using the same source of truth keeps
            // plan/enrichment aligned.
            let mut bal_seen = std::collections::HashSet::new();
            for req in &dex.oracle_requirements {
                if !matches!(req.kind, crate::core::OracleRequirementKind::Input) {
                    continue;
                }
                if req.token.is_native {
                    continue;
                }
                if !bal_seen.insert(req.token.key()) {
                    continue;
                }
                plan.balances.push((dex.actor.clone(), req.token.clone()));
                plan.allowances
                    .push((dex.actor.clone(), req.token.clone(), dex.target.clone()));
            }
            // Fallback: if no Input requirements, use facts.input_tokens.
            if plan.balances.is_empty() {
                for token in &dex.facts.input_tokens {
                    if token.is_native {
                        continue;
                    }
                    plan.balances.push((dex.actor.clone(), token.clone()));
                    plan.allowances
                        .push((dex.actor.clone(), token.clone(), dex.target.clone()));
                }
            }
        }
        Action::Permit2(p) => {
            let mut seen = std::collections::HashSet::new();
            for approval in &p.approvals {
                if seen.insert(approval.token.key()) {
                    plan.tokens_for_oracle.push(approval.token.clone());
                }
                plan.sig_oracle_requirements.push(OracleRequirement {
                    kind: crate::core::OracleRequirementKind::Input,
                    token: approval.token.clone(),
                    raw_amount: approval.amount.clone(),
                });
            }
            // Fallback when `approvals` is empty (some Permit2 variants
            // pre-decode): surface the representative `p.token`/`p.amount`
            // so USD policies still get an oracle hint.
            if p.approvals.is_empty() {
                plan.tokens_for_oracle.push(p.token.clone());
                plan.sig_oracle_requirements.push(OracleRequirement {
                    kind: crate::core::OracleRequirementKind::Input,
                    token: p.token.clone(),
                    raw_amount: p.amount.clone(),
                });
            }
            plan.clock_required = true;
        }
        Action::Eip2612(p) => {
            plan.tokens_for_oracle.push(p.token.clone());
            plan.sig_oracle_requirements.push(OracleRequirement {
                kind: crate::core::OracleRequirementKind::Input,
                token: p.token.clone(),
                raw_amount: p.value.clone(),
            });
            plan.clock_required = true;
        }
        Action::Eip712Other(_) => {
            plan.clock_required = true;
        }
        Action::Other(_) => {
            // No host facts needed; user policies decide based on calldata + selector.
        }
    }
    plan
}

/// Tier-2 plan extraction. Pure function over a built Action plus the
/// already-fetched oracle snapshot.
#[must_use]
pub fn required_window_keys(action: &Action, _oracle: &SnapshotOracle) -> WindowKeyPlan {
    let mut plan = WindowKeyPlan::default();
    if let Action::Dex(dex) = action {
        plan.keys.push(WindowKey {
            actor: dex.actor.clone(),
            key: StatKey::SWAP_VOLUME_USD_24H,
        });
        plan.keys.push(WindowKey {
            actor: dex.actor.clone(),
            key: StatKey::SWAP_COUNT_24H,
        });
    }
    plan
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

    use crate::core::{
        ChainId as CId, Eip2612Action, Eip712OtherAction, Permit2Action, Permit2Approval,
        Permit2PermitKind,
    };

    fn permit2_action_two_tokens() -> Action {
        let signer = addr("0x2222222222222222222222222222222222222222");
        let spender = addr("0x3333333333333333333333333333333333333333");
        let permit2 = addr("0x000000000022D473030F116dDEE9F6B43aC78BA3");
        Action::Permit2(Permit2Action {
            signer,
            chain_id: 1 as CId,
            domain_chain_id: 1 as CId,
            verifying_contract: permit2,
            primary_type: "PermitBatch".into(),
            permit_kind: Permit2PermitKind::PermitBatch,
            spender,
            token: weth(),
            amount: "1000000000000000000".into(),
            expiration: 1_700_000_000,
            sig_deadline: 1_700_000_000,
            nonce: "0".into(),
            approvals: vec![
                Permit2Approval {
                    token: weth(),
                    amount: "1000000000000000000".into(),
                    expiration: 1_700_000_000,
                    nonce: "0".into(),
                },
                Permit2Approval {
                    token: usdc(),
                    amount: "1000000000".into(),
                    expiration: 1_700_000_000,
                    nonce: "1".into(),
                },
            ],
            is_unlimited: false,
            nonce_valid: true,
            witness_present: false,
            total_approved_usd: None,
        })
    }

    #[test]
    fn permit2_plan_collects_per_approval_oracle_and_clock() {
        let action = permit2_action_two_tokens();
        let plan = required_host_facts(&action);

        let oracle_addrs: Vec<_> = plan
            .tokens_for_oracle
            .iter()
            .map(|t| t.address.as_str().to_lowercase())
            .collect();
        assert!(oracle_addrs.contains(&weth().address.as_str().to_lowercase()));
        assert!(oracle_addrs.contains(&usdc().address.as_str().to_lowercase()));

        // No on-chain reads for sig evaluation.
        assert!(plan.balances.is_empty());
        assert!(plan.allowances.is_empty());

        // Clock is needed for deadline.
        assert!(plan.clock_required);

        // sig_oracle_requirements: one per approval.
        assert_eq!(plan.sig_oracle_requirements.len(), 2);
        assert!(plan
            .sig_oracle_requirements
            .iter()
            .all(|r| matches!(r.kind, OracleRequirementKind::Input)));
    }

    #[test]
    fn eip2612_plan_collects_single_token_and_clock() {
        let signer = addr("0x4444444444444444444444444444444444444444");
        let action = Action::Eip2612(Eip2612Action {
            signer: signer.clone(),
            owner: signer,
            chain_id: 1 as CId,
            domain_chain_id: 1 as CId,
            verifying_contract: usdc().address,
            primary_type: "Permit".into(),
            spender: addr("0x5555555555555555555555555555555555555555"),
            token: usdc(),
            is_unlimited: false,
            nonce_valid: true,
            value: "100000000".into(),
            deadline: 1_700_000_000,
            nonce: "0".into(),
            total_approved_usd: None,
        });
        let plan = required_host_facts(&action);

        assert_eq!(plan.tokens_for_oracle.len(), 1);
        assert_eq!(plan.tokens_for_oracle[0].symbol, "USDC");
        assert!(plan.balances.is_empty());
        assert!(plan.allowances.is_empty());
        assert!(plan.clock_required);
        assert_eq!(plan.sig_oracle_requirements.len(), 1);
    }

    #[test]
    fn eip712_other_plan_only_clock() {
        let signer = addr("0x6666666666666666666666666666666666666666");
        let action = Action::Eip712Other(Eip712OtherAction {
            signer,
            chain_id: 1 as CId,
            domain_chain_id: 1 as CId,
            verifying_contract: addr("0x7777777777777777777777777777777777777777"),
            primary_type: "Mail".into(),
            domain_name: None,
            domain_version: None,
            domain_salt: None,
            types_json: "{}".into(),
            message_json: "{}".into(),
        });
        let plan = required_host_facts(&action);

        assert!(plan.tokens_for_oracle.is_empty());
        assert!(plan.balances.is_empty());
        assert!(plan.allowances.is_empty());
        assert!(plan.clock_required);
        assert!(plan.sig_oracle_requirements.is_empty());
    }

    #[test]
    fn permit2_with_empty_approvals_falls_back_to_representative() {
        let signer = addr("0x2222222222222222222222222222222222222222");
        let permit2 = addr("0x000000000022D473030F116dDEE9F6B43aC78BA3");
        let action = Action::Permit2(Permit2Action {
            signer,
            chain_id: 1 as CId,
            domain_chain_id: 1 as CId,
            verifying_contract: permit2,
            primary_type: "PermitTransferFrom".into(),
            permit_kind: Permit2PermitKind::PermitTransferFrom,
            spender: addr("0x3333333333333333333333333333333333333333"),
            token: weth(),
            amount: "1000000000000000000".into(),
            expiration: 0,
            sig_deadline: 1_700_000_000,
            nonce: "0".into(),
            approvals: vec![], // empty — exercise the fallback
            is_unlimited: false,
            nonce_valid: true,
            witness_present: false,
            total_approved_usd: None,
        });
        let plan = required_host_facts(&action);
        assert_eq!(plan.tokens_for_oracle.len(), 1);
        assert_eq!(plan.tokens_for_oracle[0].symbol, "WETH");
        assert_eq!(plan.sig_oracle_requirements.len(), 1);
        assert_eq!(
            plan.sig_oracle_requirements[0].raw_amount,
            "1000000000000000000"
        );
        assert!(plan.clock_required);
    }

    #[test]
    fn dex_plan_uses_oracle_requirements_when_input_tokens_empty() {
        // Regression: earlier draft underplanned facts when an adapter
        // populated `oracle_requirements` but left `input_tokens` empty.
        let actor = addr("0x1111111111111111111111111111111111111111");
        let target = addr("0xE592427A0AEce92De3Edee1F18E0157C05861564");
        let action = Action::Dex(DexAction {
            actor: actor.clone(),
            target: target.clone(),
            value_wei: "0".into(),
            facts: DexFacts {
                protocol_ids: vec!["uniswap_v3".into()],
                input_tokens: vec![], // empty
                output_tokens: vec![],
                ..Default::default()
            },
            oracle_requirements: vec![OracleRequirement {
                kind: OracleRequirementKind::Input,
                token: weth(),
                raw_amount: "1000000000000000000".into(),
            }],
            trace: DexTrace::default(),
        });
        let plan = required_host_facts(&action);
        assert_eq!(
            plan.tokens_for_oracle.len(),
            1,
            "WETH must be planned via oracle_requirements"
        );
        assert_eq!(plan.tokens_for_oracle[0].symbol, "WETH");
        assert_eq!(
            plan.balances.len(),
            1,
            "balance must derive from Input requirements"
        );
        assert_eq!(plan.balances[0].0, actor);
        assert_eq!(plan.allowances.len(), 1);
        assert_eq!(plan.allowances[0].2, target);
    }

    #[test]
    fn dex_window_keys_extract_swap_volume_and_count() {
        use crate::host::stat_windows::StatKey;
        let actor = addr("0x1111111111111111111111111111111111111111");
        let target = addr("0xE592427A0AEce92De3Edee1F18E0157C05861564");
        let action = dex_swap_weth_to_usdc(actor.clone(), target);

        // Snapshot oracle is accepted as a parameter to preserve the two-tier
        // API contract even though DEX storage-read planning derives the key
        // set statically.
        let oracle = SnapshotOracle::new();
        let plan = required_window_keys(&action, &oracle);

        let stat_keys: Vec<_> = plan.keys.iter().map(|k| k.key).collect();
        assert!(stat_keys.contains(&StatKey::SWAP_VOLUME_USD_24H));
        assert!(stat_keys.contains(&StatKey::SWAP_COUNT_24H));
        assert!(plan.keys.iter().all(|k| k.actor == actor));
    }

    #[test]
    fn non_dex_window_keys_empty() {
        let action = Action::Eip712Other(Eip712OtherAction {
            signer: addr("0x6666666666666666666666666666666666666666"),
            chain_id: 1 as CId,
            domain_chain_id: 1 as CId,
            verifying_contract: addr("0x7777777777777777777777777777777777777777"),
            primary_type: "Mail".into(),
            domain_name: None,
            domain_version: None,
            domain_salt: None,
            types_json: "{}".into(),
            message_json: "{}".into(),
        });
        let oracle = SnapshotOracle::new();
        assert!(required_window_keys(&action, &oracle).keys.is_empty());
    }

    #[test]
    fn dex_plan_skips_native_token_for_balance_and_allowance() {
        let actor = addr("0x1111111111111111111111111111111111111111");
        let target = addr("0xE592427A0AEce92De3Edee1F18E0157C05861564");
        let action = Action::Dex(DexAction {
            actor,
            target,
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
