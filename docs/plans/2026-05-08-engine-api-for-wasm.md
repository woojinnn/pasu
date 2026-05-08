# Engine API Surface for WASM Bridge — Plan 1

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the policy engine with the public Rust APIs the Chrome extension's WASM bridge needs to drive two-phase evaluation (build action → derive fact plan → snapshot host → evaluate).

**Architecture:** Add a public `Pipeline::build_action_for(&Request)` dispatcher, a new `lowering::host_fact_plan` module exposing `required_host_facts(&Action) → HostFactPlan` and `required_window_keys(&Action, &OracleSnapshot) → WindowKeyPlan`, and a new `SnapshotOracle` impl in `host::oracle` for snapshot-based evaluation. All changes are additive — existing `Pipeline::evaluate*` methods and adapters remain byte-for-byte unchanged.

**Tech Stack:** Rust 2024 edition, existing engine crate (`crates/policy-engine`), `cedar-policy` 4.x. No new dependencies.

**Series:** Plan 1 of a Chrome-extension implementation series. Subsequent plans cover (2) WASM bridge crate via wasm-pack, (3) extension scaffold + provider proxy, (4) RPC + price client fact fetchers, (5) verdict modal UI + orchestrator, (6) marketplace catalog + bundle templating.

**Scope (in this plan):**
- Public Rust API surface — additions only.
- Unit tests for each new function/impl.
- Integration test exercising the full extract-plan-then-evaluate flow against the existing adapter bundle.

**Out of scope (this plan):**
- WASM compilation, JSON serialization layer, JS-side bridge.
- Marketplace, parameter rendering, AST equivalence.
- Any extension-side TypeScript code.

---

## File map

| Path | Action | Responsibility |
|------|--------|----------------|
| `crates/policy-engine/src/host/oracle.rs` | Modify | Add `SnapshotOracle` struct + `Oracle` impl + tests |
| `crates/policy-engine/src/lowering/host_fact_plan.rs` | Create | Define `HostFactPlan`, `WindowKeyPlan`, `required_host_facts`, `required_window_keys` + unit tests per Action variant |
| `crates/policy-engine/src/lowering/mod.rs` | Modify | Re-export new module's public types/functions |
| `crates/policy-engine/src/pipeline.rs` | Modify | Add public `Pipeline::build_action_for(&Request)` dispatcher |
| `crates/integration-tests/tests/host_fact_plan.rs` | Create | End-to-end: adapter resolves → action built → plan extracted → matches enrichment expectations |

---

## Task 1: Add `SnapshotOracle` impl

**Files:**
- Modify: `crates/policy-engine/src/host/oracle.rs` — append `SnapshotOracle` after the existing `MockOracle` block.

The engine's existing `MockOracle` is keyed by `Token::key()` and used in tests. `SnapshotOracle` is the same shape but with explicit semantics for "values were precomputed by an external snapshot builder and frozen for one evaluation pass." Naming separation prevents production code from accidentally instantiating `MockOracle`.

- [ ] **Step 1: Write the failing tests**

Append at the bottom of `crates/policy-engine/src/host/oracle.rs`, *inside* the existing `#[cfg(test)] mod tests` block, just before its closing brace:

```rust
    #[test]
    fn snapshot_returns_recorded_price() {
        let oracle = SnapshotOracle::new().with_price(
            &usdt(),
            UsdValuation {
                value: "1.00".into(),
                as_of_ts: 1_700_000_000,
                sources: vec!["coingecko".into()],
                stale_sec: 30,
            },
        );
        let v = oracle.price(&usdt()).unwrap();
        assert_eq!(v.value, "1.00");
        assert_eq!(v.sources, vec!["coingecko".to_string()]);
        assert_eq!(v.stale_sec, 30);
    }

    #[test]
    fn snapshot_errors_on_unknown_token() {
        let oracle = SnapshotOracle::new();
        let err = oracle.price(&usdt()).unwrap_err();
        assert!(matches!(err, OracleError::NoPrice(_)));
    }

    #[test]
    fn snapshot_independent_per_token() {
        let oracle = SnapshotOracle::new()
            .with_price(
                &usdt(),
                UsdValuation { value: "1.00".into(), as_of_ts: 0, sources: vec![], stale_sec: 0 },
            )
            .with_price(
                &weth(),
                UsdValuation { value: "3500.00".into(), as_of_ts: 0, sources: vec![], stale_sec: 0 },
            );
        assert_eq!(oracle.price(&usdt()).unwrap().value, "1.00");
        assert_eq!(oracle.price(&weth()).unwrap().value, "3500.00");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p policy-engine --lib host::oracle::tests::snapshot 2>&1 | tail -10`

Expected: compilation error `cannot find struct SnapshotOracle in this scope`.

- [ ] **Step 3: Implement `SnapshotOracle`**

Append, *immediately above* the `#[cfg(test)] mod tests` line in `crates/policy-engine/src/host/oracle.rs`:

```rust
/// Snapshot-backed oracle for one evaluation pass.
///
/// Constructed by an external host (e.g., the WASM bridge) from precomputed
/// USD valuations. Unlike [`MockOracle`], this is a production type intended
/// for browser/extension/server callers that pre-fetch prices asynchronously
/// and then evaluate synchronously.
///
/// Semantically equivalent to `MockOracle` — both are HashMap-backed lookups
/// keyed on [`Token::key`] — but kept distinct so callers signal intent.
#[derive(Debug, Clone, Default)]
pub struct SnapshotOracle {
    prices: HashMap<String, UsdValuation>,
}

impl SnapshotOracle {
    /// Construct an empty snapshot oracle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a price for a token. Builder-style.
    #[must_use]
    pub fn with_price(mut self, token: &Token, valuation: UsdValuation) -> Self {
        self.prices.insert(token.key(), valuation);
        self
    }

    /// Insert a price for a token in place.
    pub fn insert(&mut self, token: &Token, valuation: UsdValuation) {
        self.prices.insert(token.key(), valuation);
    }

    /// Number of tokens in the snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.prices.len()
    }

    /// Whether the snapshot has any prices.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.prices.is_empty()
    }
}

impl Oracle for SnapshotOracle {
    fn price(&self, token: &Token) -> Result<UsdValuation, OracleError> {
        self.prices
            .get(&token.key())
            .cloned()
            .ok_or_else(|| OracleError::NoPrice(token.key()))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p policy-engine --lib host::oracle::tests::snapshot 2>&1 | tail -10`

Expected: `test result: ok. 3 passed; 0 failed`.

- [ ] **Step 5: Run the full oracle test module to confirm no regression**

Run: `cargo test -p policy-engine --lib host::oracle 2>&1 | tail -5`

Expected: all existing oracle tests still pass plus the three new ones.

- [ ] **Step 6: Commit**

```bash
git add crates/policy-engine/src/host/oracle.rs
git commit -m "$(cat <<'EOF'
feat(engine): add SnapshotOracle for snapshot-driven evaluation

Mirrors MockOracle's HashMap-backed shape but exists as a production
type, so browser-extension and server hosts can populate it with
pre-fetched prices and then evaluate synchronously through the engine's
sync Oracle trait.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Define `HostFactPlan` and `WindowKeyPlan` types

**Files:**
- Create: `crates/policy-engine/src/lowering/host_fact_plan.rs`

The plan types describe what host data the orchestrator must fetch before invoking enrichment. Tier 1 is derivable from a bare Action; Tier 2 (window keys) requires an Oracle snapshot first, because window-stat keys are derived from the USD-stamped action.

- [ ] **Step 1: Create the empty file with type stubs**

Create `crates/policy-engine/src/lowering/host_fact_plan.rs` with:

```rust
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
pub fn required_host_facts(_action: &Action) -> HostFactPlan {
    HostFactPlan::default()
}

/// Tier-2 plan extraction. Pure function over a built Action plus the
/// already-fetched oracle snapshot.
#[must_use]
pub fn required_window_keys(_action: &Action, _oracle: &SnapshotOracle) -> WindowKeyPlan {
    WindowKeyPlan::default()
}
```

- [ ] **Step 2: Wire the new module into `lowering/mod.rs`**

Edit `crates/policy-engine/src/lowering/mod.rs`. Find the existing `pub mod` declarations and the `pub use` re-exports. Add:

```rust
pub mod host_fact_plan;
```

below the existing `pub mod stamping;` line (alphabetical order after `decimal`, `request`, `stamping`), and add:

```rust
pub use host_fact_plan::{
    required_host_facts, required_window_keys, HostFactPlan, WindowKey, WindowKeyPlan,
};
```

right below the existing `pub use stamping::{...};` re-export.

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p policy-engine 2>&1 | tail -5`

Expected: `Finished` with no errors. (Warnings about unused parameters are expected; we'll fill in real bodies in tasks 3-5.)

- [ ] **Step 4: Commit**

```bash
git add crates/policy-engine/src/lowering/host_fact_plan.rs crates/policy-engine/src/lowering/mod.rs
git commit -m "$(cat <<'EOF'
feat(engine): scaffold host_fact_plan module

Stubs HostFactPlan, WindowKeyPlan, WindowKey, required_host_facts,
required_window_keys. Bodies are filled in subsequent tasks per
Action variant.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `required_host_facts` for `Action::Dex`

**Files:**
- Modify: `crates/policy-engine/src/lowering/host_fact_plan.rs`

DEX needs:
- Oracle prices for input/output tokens (already enumerated as `oracle_requirements`).
- `balanceOf(actor)` for each non-native input token.
- `allowance(actor, target)` for each non-native input token.
- No clock.

- [ ] **Step 1: Write the failing test**

Append to `crates/policy-engine/src/lowering/host_fact_plan.rs` (create the test module if missing):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        Address, ChainId, DexAction, DexFacts, DexTrace, OracleRequirement, OracleRequirementKind,
        Token,
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
        let oracle_addrs: Vec<_> = plan.tokens_for_oracle.iter().map(|t| t.address.as_str().to_lowercase()).collect();
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p policy-engine --lib lowering::host_fact_plan 2>&1 | tail -15`

Expected: `dex_plan_collects_oracle_balance_and_allowance ... FAILED` with `assertion failed: oracle_addrs.contains(...)` (because the stub returns `HostFactPlan::default()`).

- [ ] **Step 3: Implement DEX plan extraction**

Replace the `required_host_facts` function body in `crates/policy-engine/src/lowering/host_fact_plan.rs`:

```rust
#[must_use]
pub fn required_host_facts(action: &Action) -> HostFactPlan {
    let mut plan = HostFactPlan::default();
    match action {
        Action::Dex(dex) => {
            // Oracle: union of input + output tokens (deduped by chain-qualified key).
            let mut seen = std::collections::HashSet::new();
            for token in dex.facts.input_tokens.iter().chain(dex.facts.output_tokens.iter()) {
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p policy-engine --lib lowering::host_fact_plan 2>&1 | tail -10`

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/policy-engine/src/lowering/host_fact_plan.rs
git commit -m "$(cat <<'EOF'
feat(engine): required_host_facts for Action::Dex

Tokens for oracle = union of input + output (deduped by Token::key()).
Balances + allowances enumerate non-native input tokens against the
calldata target. Native ETH is correctly excluded.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `required_host_facts` for signature actions

**Files:**
- Modify: `crates/policy-engine/src/lowering/host_fact_plan.rs`

Permit2 / EIP-2612 / EIP-712-other:
- Oracle: each token whose USD value the policy may want.
- Balances/allowances: empty — sig evaluation does not need on-chain reads.
- Clock: required for deadline checks (`sigDeadline`, `deadline`, `nowTs`).
- `sig_oracle_requirements`: structured `OracleRequirement` list, kind `Input`, populated for Permit2 (per-approval) and EIP-2612 (single).

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module in `host_fact_plan.rs` (inside the same `mod tests {}` block):

```rust
    use crate::core::{
        ChainId as CId, Eip2612Action, Eip712OtherAction, Permit2Action, Permit2Approval,
        Permit2PermitKind, UsdValuation,
    };

    fn permit2_action_two_tokens() -> Action {
        let signer = addr("0x2222222222222222222222222222222222222222");
        let spender = addr("0x3333333333333333333333333333333333333333");
        let permit2 = addr("0x000000000022D473030F116dDEE9F6B43aC78BA3");
        Action::Permit2(Permit2Action {
            signer: signer.clone(),
            chain_id: 1 as CId,
            domain_chain_id: 1 as CId,
            verifying_contract: permit2,
            primary_type: "PermitBatch".into(),
            permit_kind: Permit2PermitKind::PermitBatch,
            spender: spender.clone(),
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

        let oracle_addrs: Vec<_> =
            plan.tokens_for_oracle.iter().map(|t| t.address.as_str().to_lowercase()).collect();
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
            owner: signer.clone(),
            chain_id: 1 as CId,
            domain_chain_id: 1 as CId,
            verifying_contract: usdc().address.clone(),
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p policy-engine --lib lowering::host_fact_plan 2>&1 | tail -20`

Expected: 3 new tests fail (oracle list empty, clock_required false, etc.).

- [ ] **Step 3: Implement signature plan extraction**

Replace the `Action::Other | Permit2 | Eip2612 | Eip712Other` arm in `required_host_facts` with explicit per-variant logic:

```rust
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
```

The match in `required_host_facts` should now read:

```rust
match action {
    Action::Dex(dex) => { /* ...existing dex logic... */ }
    Action::Permit2(p) => { /* ...new permit2 logic... */ }
    Action::Eip2612(p) => { /* ...new eip2612 logic... */ }
    Action::Eip712Other(_) => { plan.clock_required = true; }
    Action::Other(_) => { }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p policy-engine --lib lowering::host_fact_plan 2>&1 | tail -10`

Expected: 5 passed (2 dex + 3 sig).

- [ ] **Step 5: Commit**

```bash
git add crates/policy-engine/src/lowering/host_fact_plan.rs
git commit -m "$(cat <<'EOF'
feat(engine): required_host_facts for signature variants

Permit2 enumerates each approval token; EIP-2612 single token; both
signal clock_required=true so the orchestrator knows to attach
HostCapabilities::with_clock. Eip712Other only requires the clock.
sig_oracle_requirements mirrors DEX oracle_requirements shape so
extension code can use one fact-fetch path.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: `required_window_keys` for `Action::Dex`

**Files:**
- Modify: `crates/policy-engine/src/lowering/host_fact_plan.rs`

Window keys are per-actor. Today `compute_dex_window_deltas` in `lowering/stamping/dex.rs` projects two stat keys, both exposed as canonical constants in `host::stat_windows::StatKey`:

| Constant | Wire string |
|----------|-------------|
| `StatKey::SWAP_VOLUME_USD_24H` | `"swapVolumeUsd24h"` |
| `StatKey::SWAP_COUNT_24H` | `"swapCount24h"` |

Use the constants directly — never hardcode the wire strings. The host's `StatWindows::snapshot(&Address, &[StatKey])` reads keys in one batch. **Volume is conditionally projected** (only when `dex.facts.total_input_usd` is `Some` after enrichment); **count is unconditional**. The window-key plan returns the *full* set of keys an actor's snapshot must cover for any DEX policy to evaluate, so it includes both — the storage-read fetcher prefetches both regardless of whether the current request's volume delta is present. (A reservation-style planner would condition on USD presence; that's deliberately out of scope per Plan 5's pending-deltas strategy.)

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `host_fact_plan.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p policy-engine --lib lowering::host_fact_plan 2>&1 | tail -10`

Expected: `dex_window_keys_extract_swap_volume_and_count ... FAILED`.

- [ ] **Step 3: Implement window-key extraction**

`WindowKey` was already redefined in Task 2 stub as `{actor: Address, key: StatKey}`. Replace the body of `required_window_keys`:

```rust
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
```

> Note 1: keys are returned in canonical form so the host's `StatWindows::snapshot(&Address, &[StatKey])` reads them in one call. The wire string (`"swapVolumeUsd24h"` / `"swapCount24h"`) is `StatKey::as_str()` — the WASM bridge serializes that for the TS side.
>
> Note 2: the `_oracle` parameter is intentional. Window deltas projected by `compute_dex_window_deltas` depend on `total_input_usd` (an oracle-stamped field), but the *keys* themselves are static per actor for storage-read planning — we always read both volume and count snapshots, even on a request whose oracle missed and won't actually project a volume delta. Reservation-style planning that conditions on USD presence is deferred to v1.1 (Plan 5 uses inline projection only).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p policy-engine --lib lowering::host_fact_plan 2>&1 | tail -10`

Expected: 7 passed (5 prior + 2 new).

- [ ] **Step 5: Commit**

```bash
git add crates/policy-engine/src/lowering/host_fact_plan.rs
git commit -m "$(cat <<'EOF'
feat(engine): required_window_keys for DEX windowing

Returns the two stat-window names DEX evaluation reads:
swap_volume_usd_24h and swap_count_24h, both keyed on the actor.
Non-DEX variants return an empty plan. Oracle snapshot parameter is
preserved in the signature for future window types that read USD
context, even though current windows derive keys statically.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: `Pipeline::build_action_for(&Request)` public dispatcher

**Files:**
- Modify: `crates/policy-engine/src/pipeline.rs`

Today both internal builders (`build_action(&TransactionRequest)` and `build_signature_action(&SignatureRequest)`) are private. Expose a unified public entry point that accepts a `&Request` and returns the bare `Action` without enrichment.

- [ ] **Step 1: Write the failing test**

Create the `tests` module at the bottom of `crates/policy-engine/src/pipeline.rs` (or extend an existing one). For this test to be meaningful, it needs an `AdapterRegistry` and a built `HostCapabilities`. The simplest fixture uses the in-tree `MockAdapterRegistry` and `MockOracle`. Append:

```rust
#[cfg(test)]
mod build_action_for_tests {
    use super::*;
    use crate::core::{Address, Request, SignatureRequest, TransactionRequest};
    use crate::host::{HostCapabilities, oracle::MockOracle};
    use crate::policy::PolicyEngine;
    use crate::registry::MockAdapterRegistry;

    fn empty_pipeline_fixture() -> (MockAdapterRegistry, MockOracle, PolicyEngine) {
        let registry = MockAdapterRegistry::default();
        let oracle = MockOracle::new();
        // PolicyEngine has no Default impl — it's always built via the
        // builder. An empty (no policies, no schema) build is fine for
        // pipeline-only tests that never call evaluate().
        let engine = PolicyEngine::builder()
            .build()
            .expect("empty PolicyEngine builds");
        (registry, oracle, engine)
    }

    #[test]
    fn build_action_for_tx_returns_other_when_no_adapter_matches() {
        let (registry, oracle, policies) = empty_pipeline_fixture();
        let host = HostCapabilities::new(&oracle);
        let pipeline = Pipeline::new(&registry, host, &policies);

        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x1111111111111111111111111111111111111111").unwrap(),
            to: Address::new("0x2222222222222222222222222222222222222222").unwrap(),
            value_wei: "0".into(),
            data: vec![0xde, 0xad, 0xbe, 0xef],
            gas: None,
            nonce: None,
        };
        let action = pipeline.build_action_for(&Request::Tx(tx)).unwrap();
        assert!(matches!(action, Action::Other(_)));
    }

    #[test]
    fn build_action_for_sig_returns_eip712_other_when_no_adapter_matches() {
        let (registry, oracle, policies) = empty_pipeline_fixture();
        let host = HostCapabilities::new(&oracle);
        let pipeline = Pipeline::new(&registry, host, &policies);

        // Construct a minimal SignatureRequest that matches no signature
        // adapter. Step 1a adds the test helper.
        let sig = SignatureRequest::test_minimal_eip712_other();
        let action = pipeline.build_action_for(&Request::Sig(sig)).unwrap();
        assert!(matches!(action, Action::Eip712Other(_)));
    }
}
```

> The plan formerly used `PolicyEngine::default()` — that does not exist; the engine has `PolicyEngineBuilder::default()` and `PolicyEngine::builder()`/`PolicyEngine::from_sources()`. The `gas`/`nonce` fields on `TransactionRequest` (`core.rs:486,489`) are required (not `#[serde(default)]`); omit them at your peril. `SignatureRequest::test_minimal_eip712_other` is added in Step 1a.

- [ ] **Step 1a: Add `SignatureRequest::test_minimal_eip712_other()` (helper does not yet exist)**

Confirmed: `core.rs:504` has only `impl SignatureRequest { fn primary_type ... }`. Append a `#[cfg(test)]` impl block in the same file:

```rust
#[cfg(test)]
impl SignatureRequest {
    /// Minimal EIP-712 signature request that no adapter will match.
    #[must_use]
    pub fn test_minimal_eip712_other() -> Self {
        use serde_json::json;
        Self {
            chain_id: 1,
            signer: Address::new("0x6666666666666666666666666666666666666666").unwrap(),
            typed_data: Eip712TypedData {
                domain: Eip712Domain {
                    chain_id: 1,
                    verifying_contract: Address::new(
                        "0x7777777777777777777777777777777777777777",
                    )
                    .unwrap(),
                    name: None,
                    version: None,
                    salt: None,
                },
                primary_type: "Mail".into(),
                types: json!({
                    "EIP712Domain": [{"name": "chainId", "type": "uint256"},
                                      {"name": "verifyingContract", "type": "address"}],
                    "Mail": [{"name": "from", "type": "address"},
                              {"name": "to", "type": "address"}]
                }),
                message: json!({
                    "from": "0x6666666666666666666666666666666666666666",
                    "to":   "0x8888888888888888888888888888888888888888"
                }),
            },
        }
    }
}
```

(Field names — `Eip712TypedData`, `Eip712Domain` — must match the existing type definitions. Verify with `grep -n "pub struct Eip712" crates/policy-engine/src/core.rs` first; if the field set differs, adjust accordingly.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p policy-engine --lib build_action_for 2>&1 | tail -15`

Expected: compilation error `no method named build_action_for found for struct Pipeline`.

- [ ] **Step 3: Implement the public dispatcher**

In `crates/policy-engine/src/pipeline.rs`, locate the existing `impl<'a, R: AdapterRegistry + ?Sized> Pipeline<'a, R> { ... }` block. The private functions `build_action(&TransactionRequest)` and `build_signature_action(&SignatureRequest)` already live there. Add a new public method **above** them in the same impl block:

```rust
    /// Build the semantic [`Action`] for a request without enrichment, lowering,
    /// or evaluation.
    ///
    /// Wraps the existing private TX and signature builders behind the unified
    /// [`Request`] surface. Used by external orchestrators (notably the Chrome
    /// extension's WASM bridge) that want to derive a [`HostFactPlan`] from
    /// the bare action and prefetch host data before evaluation.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Ambiguous`] if multiple adapters resolve, and
    /// [`PipelineError::AdapterBuild`] if the matched adapter fails to build.
    pub fn build_action_for(&self, request: &Request) -> Result<Action, PipelineError> {
        match request {
            Request::Tx(tx) => self.build_action(tx),
            Request::Sig(sig) => self.build_signature_action(sig),
        }
    }
```

`build_action` and `build_signature_action` remain private — only the unified entry point is public.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p policy-engine --lib build_action_for 2>&1 | tail -10`

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/policy-engine/src/pipeline.rs crates/policy-engine/src/core.rs
git commit -m "$(cat <<'EOF'
feat(engine): public Pipeline::build_action_for(&Request)

Unified public entry that wraps the existing private build_action /
build_signature_action dispatchers. Used by the Chrome extension's
WASM bridge to derive HostFactPlan before evaluation. Existing
private builders are unchanged.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Integration test — full extract-then-evaluate flow

**Files:**
- Create: `crates/integration-tests/tests/host_fact_plan.rs`

This test exercises the full external-orchestrator workflow: build the action via the new public API, derive Tier-1 plan, populate a `SnapshotOracle` from that plan, run `Pipeline::evaluate` with the snapshot host, and confirm the verdict matches what `Pipeline::evaluate` would return when given an oracle directly. The two paths must produce identical verdicts because `SnapshotOracle` is just `MockOracle` with a different name.

- [ ] **Step 1: Write the integration test**

Create `crates/integration-tests/tests/host_fact_plan.rs`:

```rust
//! Integration test: external-orchestrator workflow.
//!
//! Validates that:
//! 1. `Pipeline::build_action_for(&Request::Tx(tx))` returns the same action
//!    `Pipeline::evaluate_tx` would build internally.
//! 2. `required_host_facts(&action)` enumerates exactly the tokens the engine
//!    consults during enrichment.
//! 3. Evaluating with a `SnapshotOracle` populated from the plan yields the
//!    same verdict as evaluating with a `MockOracle` populated identically.

use policy_engine::core::{Action, Address, Request, TransactionRequest, UsdValuation};
use policy_engine::host::oracle::{MockOracle, SnapshotOracle};
use policy_engine::host::HostCapabilities;
use policy_engine::lowering::required_host_facts;
use policy_engine::policy::{PolicyEngine, Verdict};
use policy_engine::Pipeline;
use policy_engine_adapters_bundle::default_registry;

const ACTOR: &str = "0x1111111111111111111111111111111111111111";
const V3_SWAP_ROUTER: &str = "0xE592427A0AEce92De3Edee1F18E0157C05861564";

fn weth_swap_calldata_v3_exact_input_single() -> Vec<u8> {
    // Calldata for a Uniswap V3 exactInputSingle WETH->USDC swap.
    // Selector 0x414bf389 then 8 × 32-byte words (matches
    // crates/adapters/uniswap-v3/src/exact_input_single.rs:99 decode shape).
    // We use the workspace's existing `hex` dep — `hex-literal` is *not*
    // a workspace member, so do not introduce it here.
    let raw = concat!(
        "414bf389",
        "000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // tokenIn = WETH
        "000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // tokenOut = USDC
        "00000000000000000000000000000000000000000000000000000000000001f4", // fee = 500
        "0000000000000000000000001111111111111111111111111111111111111111", // recipient
        "00000000000000000000000000000000000000000000000000000000ffffffff", // deadline
        "0000000000000000000000000000000000000000000000000de0b6b3a7640000", // amountIn = 1 WETH
        "00000000000000000000000000000000000000000000000000000000b2d05e00", // amountOutMin = 3000 USDC
        "0000000000000000000000000000000000000000000000000000000000000000", // sqrtPriceLimitX96 = 0
    );
    hex::decode(raw).expect("static hex literal decodes")
}

#[test]
fn extract_plan_then_evaluate_matches_direct_path() {
    let registry = default_registry();
    let policies = PolicyEngine::builder()
        .build()
        .expect("empty PolicyEngine builds");

    let tx = TransactionRequest {
        chain_id: 1,
        from: Address::new(ACTOR).unwrap(),
        to: Address::new(V3_SWAP_ROUTER).unwrap(),
        value_wei: "0".into(),
        data: weth_swap_calldata_v3_exact_input_single(),
        gas: None,
        nonce: None,
    };

    // Path A: extract action via public API, derive plan, populate snapshot.
    let oracle_for_extract = SnapshotOracle::new();
    let host_a = HostCapabilities::new(&oracle_for_extract);
    let pipeline_a = Pipeline::new(&registry, host_a, &policies);
    let action = pipeline_a.build_action_for(&Request::Tx(tx.clone())).unwrap();
    assert!(matches!(action, Action::Dex(_)));

    let plan = required_host_facts(&action);
    // The plan must enumerate WETH (input) + USDC (output) for oracle.
    let oracle_addrs: Vec<_> = plan
        .tokens_for_oracle
        .iter()
        .map(|t| t.address.as_str().to_lowercase())
        .collect();
    assert!(oracle_addrs.iter().any(|a| a.contains("c02aaa")), "plan must include WETH");
    assert!(oracle_addrs.iter().any(|a| a.contains("a0b869")), "plan must include USDC");

    // Build the SnapshotOracle the orchestrator would build.
    let mut snapshot = SnapshotOracle::new();
    for token in &plan.tokens_for_oracle {
        let usd = if token.symbol == "WETH" { "3500.00" } else { "1.00" };
        snapshot.insert(
            token,
            UsdValuation {
                value: usd.into(),
                as_of_ts: 1_700_000_000,
                sources: vec!["test-snapshot".into()],
                stale_sec: 30,
            },
        );
    }

    let host_b = HostCapabilities::new(&snapshot);
    let pipeline_b = Pipeline::new(&registry, host_b, &policies);
    let verdict_b = pipeline_b.evaluate(&Request::Tx(tx.clone())).unwrap();

    // Path C: direct evaluation with a MockOracle populated identically.
    let mock = MockOracle::new()
        .with_simple_price(&plan.tokens_for_oracle[0].clone(), "3500.00", 30)
        .with_simple_price(&plan.tokens_for_oracle[1].clone(), "1.00", 30);
    // Pin token order: ensure WETH is first so the with_simple_price values map correctly.
    // If the plan happens to dedupe-ordered USDC-first, the assertion above would have
    // read different addresses; we'd need to swap. Defensive build:
    let mock = match plan.tokens_for_oracle[0].symbol.as_str() {
        "WETH" => mock,
        _ => MockOracle::new()
            .with_simple_price(&plan.tokens_for_oracle[1].clone(), "3500.00", 30)
            .with_simple_price(&plan.tokens_for_oracle[0].clone(), "1.00", 30),
    };
    let host_c = HostCapabilities::new(&mock);
    let pipeline_c = Pipeline::new(&registry, host_c, &policies);
    let verdict_c = pipeline_c.evaluate(&Request::Tx(tx)).unwrap();

    // SnapshotOracle and MockOracle must produce identical verdicts when populated identically.
    match (&verdict_b, &verdict_c) {
        (Verdict::Pass, Verdict::Pass) => {}
        (Verdict::Warn(a), Verdict::Warn(b)) | (Verdict::Fail(a), Verdict::Fail(b)) => {
            let ids_a: Vec<_> = a.iter().map(|m| &m.policy_id).collect();
            let ids_b: Vec<_> = b.iter().map(|m| &m.policy_id).collect();
            assert_eq!(ids_a, ids_b, "matched policies differ between snapshot and mock paths");
        }
        _ => panic!("verdict variants differ: snapshot={:?} mock={:?}", verdict_b, verdict_c),
    }
}
```

- [ ] **Step 2: Confirm `hex` crate is in scope (already a workspace member)**

The integration-tests crate already declares `hex` (`crates/integration-tests/Cargo.toml:19`). The integration test uses `hex::decode(...)` rather than `hex-literal::hex!` — **no new dependency needed**.

- [ ] **Step 3: Run the integration test to verify it passes**

Run: `cargo test -p policy-engine-integration-tests --test host_fact_plan 2>&1 | tail -15`

Expected: `test extract_plan_then_evaluate_matches_direct_path ... ok`.

If the existing engine without USD-cap user policies returns `Verdict::Pass` on the WETH→USDC swap path, that's expected — the test asserts *equivalence between paths*, not a specific decision. Both paths must agree.

- [ ] **Step 4: Run the full integration suite to confirm zero regressions**

Run: `cargo test -p policy-engine-integration-tests 2>&1 | tail -5`

Expected: all existing integration tests still pass plus the new one.

- [ ] **Step 5: Commit**

```bash
git add crates/integration-tests/tests/host_fact_plan.rs crates/integration-tests/Cargo.toml
git commit -m "$(cat <<'EOF'
test: integration coverage for build_action_for + required_host_facts

Validates the full external-orchestrator path: build action via the
new public Pipeline::build_action_for, derive HostFactPlan via
required_host_facts, populate a SnapshotOracle from the plan, and
confirm the verdict matches the direct MockOracle path.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Run full workspace test + lint sweep

- [ ] **Step 1: Full test pass**

Run: `cargo test --workspace 2>&1 | tail -10`

Expected: every test passes, count is `prior_total + 8` (3 SnapshotOracle + 5 fact-plan + 2 build-action-for + 1 integration; SignatureRequest helper added under `#[cfg(test)]` doesn't count).

If any pre-existing test fails, *do not patch it as part of this task*. Investigate with `cargo test <name>` and surface as a separate fix commit.

- [ ] **Step 2: Clippy clean**

Run: `cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -10`

Expected: `Finished` with no warnings. Common issue to watch: `clippy::needless_borrow` on `&token.key()` calls — fix in place if flagged.

- [ ] **Step 3: Format check**

Run: `cargo fmt --all -- --check 2>&1 | tail -5`

Expected: no output (clean).

If diffs are reported, run `cargo fmt --all` and commit:

```bash
git add -u
git commit -m "style: cargo fmt"
```

- [ ] **Step 4: Final summary commit if anything else needs cleanup**

If after the previous commits the repo has stray uncommitted changes (only fmt — no logic), they're absorbed by Step 3. Otherwise no commit needed.

---

## Self-review summary

**Spec coverage** (against `docs/specs/2026-05-08-chrome-extension-design.md` §3.3):

- ✅ `Pipeline::build_action_for(&Request)` public — Task 6
- ✅ `lowering::required_host_facts(&Action) -> HostFactPlan` — Tasks 3, 4
- ✅ `lowering::required_window_keys(&Action, &OracleSnapshot) -> WindowKeyPlan` — Task 5 (parameter type is `SnapshotOracle`, the design's "OracleSnapshot" — name aligned to existing engine convention)
- ✅ `HostFactPlan { tokens_for_oracle, balances, allowances, clock_required, sig_oracle_requirements }` — Task 2 + 3 + 4
- ✅ `SnapshotOracle` impl — Task 1
- ✅ Integration test exercising full pipeline — Task 7
- ✅ Full workspace test + lint pass — Task 8

**Out of scope (correctly deferred to subsequent plans):** WASM bridge crate, JSON-string boundary, extension-side TS code, marketplace, parameterization.

**Type consistency check:** All references (`HostFactPlan`, `WindowKeyPlan`, `WindowKey`, `SnapshotOracle`, `Token::key`, `OracleRequirement`, `Action::Permit2 | Eip2612 | Dex | Other | Eip712Other`) use names that already exist in the engine or are introduced in earlier tasks of this plan. Reviewed for cross-task drift.
