# Protocol Onboarding Evidence Template

> Copy this file to `crates/integration-tests/onboarding/<protocol>/evidence.md` for each protocol onboarding run.
> This is a completion gate, not a nice-to-have note. If any mandatory row is missing, the phase is incomplete.
>
> **SSOT:** this template is the single source of truth for *per-phase evidence requirements* — `check-onboarding-evidence` parses it and cross-checks every mandatory row. The spine's §2.1b table, §3.1 P0 step, and §8.6 self-check summarize it; on conflict, this file wins. (The definition of "onboarded" itself lives in `PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md` §6.)

## Run Metadata

| field | value |
|---|---|
| protocol | aave |
| branch | test/aave-fw-dogfood |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-morpho |
| date | 2026-06-03 |
| main agent | Claude Opus 4.8 (1M context) — framework dogfood (♻️ re-entry + governance axis-1) |
| base commit | d06ea467 |

## Scope Classification

Use this section to make the final claim precise. This table is narrative
evidence; the phase tables below are the mandatory gate.

| field | value |
|---|---|
| representative chain (SINGLE — multichain = separate framework, deferred) | mainnet (Ethereum, chainId 1). L2 (Optimism/Base/Arbitrum) weth-gateway/Pool surface+manifests are PRE-EXISTING (prior multichain default) and retained non-destructively, but OUT of this run's representative-chain scope = multichain expansion deferred. |
| completion target | `wallet-facing` (Aave v3 Pool + WrappedTokenGatewayV3 + variable-debt credit delegation [re-validated] + AAVE governance-power delegation [governance domain, axis-1, NEW]) |
| covered real-usage coverage-share (P2-measured: % of recent P0-universe txs the covered set decodes) | PENDING (P2 SCOPE ORACLE — Etherscan/Dune on Pool 0x87870bca…) |
| user-facing DEFERs, each with its 1st-party usage-share (%/count) | PENDING (P2): GHO/GSM/SGHO, stkAAVE/stkGHO/Umbrella staking, Governance proposal+voting (VotingMachine), aAAVE/stkAAVE delegation (same surface as AAVE). flashLoan/flashLoanSimple = EXCLUDE (keeper, receiver is a contract not EOA — 1st-party confirmed, not defer). |
| direct factory-child calls | not applicable (Aave is not factory-pool-heavy; aTokens/debtTokens are per-reserve standard/registry tokens, not user-called child pools) |
| final claim label (MUST NOT over-claim the measured coverage-share above) | PENDING (set at P4 from P2 coverage-share) |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| completion scope declared: primary chain(s), wallet-facing vs full-surface/full-universe target, and multichain status | done | representative chain = mainnet(1); target = primary-chain wallet-facing; multichain (L2) deferred. See Scope Classification table + plan aave-fw-dogfood.md + _deployments.json note. |
| Codex current-session research executed | done | main session (Opus 4.8): base code-state diagnosis (lending domain has supply/withdraw/borrow/repay/buy_collateral/delegate_borrow/liquidate/set_authorization/set_collateral/set_emode/swap_rate_mode; LendingVenue::AaveV3 exists; 40 existing aave/v3 manifests + 9 surface snapshots) + 1st-party Etherscan getabi verify of AAVE impl 0x5d4aa78b delegation selectors. |
| Claude Code or sub-agent research executed | done | 2 general-purpose sub-agents, both 1st-party (bgd-labs/aave-address-book + Etherscan getabi + on-chain EIP-1967 impl-slot reads): (A) lending+periphery, (B) governance V3. |
| Claude/sub-agent exact prompt or command recorded | done | agentId a95b182df4aa75c2f (lending/periphery), a66ec553ee47745dd (governance). Prompts embedded scope/1st-party sources/output format (full text in session transcript). |
| Codex-only candidates listed | done | main-session reconciliation: swapBorrowRateMode mainnet-absence root cause (confirmed Aave v3.4 stable-rate removal via impl ABI, NOT a manifest gap — existing manifest correctly omits chain 1). |
| Claude/sub-agent-only candidates listed | done | agent A: SGHO (Savings GHO ERC4626), Umbrella + stkWaToken set, stkGHO, configureEModeCategoryIsolated (v3.4 new). agent B: META_DELEGATE_HELPER, representative voting, GovernancePowerType enum order, over-modeling critique of crank fns. |
| dropped-unverified candidates listed with reason | done | SGHO/Umbrella ABIs not pulled (deferred surface, not covered this run). configureEModeCategoryIsolated selector flagged "not pre-computed" by agent A — exclude-class governance, not load-bearing this run; the existing pool-mainnet snapshot predates it (I1 PASS today), noted for ♻️ snapshot refresh follow-up. |
| final contract inventory verified against first-party sources | done | registryV2/surface/aave/_deployments.json: 37 contracts vs bgd-labs/aave-address-book (AaveV3Ethereum/GovernanceV3Ethereum/AaveSafetyModule/GhoEthereum/UmbrellaEthereum) + Etherscan-verified ABIs. All cover addresses Etherscan-confirmed. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | not applicable — Aave is not factory-pool-heavy; aTokens/debtTokens are per-reserve standard/registry tokens routed via lending Pool + token adapters, not a user-called child-pool universe. |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | not applicable (see above). |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | not applicable (see above). |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | not applicable (see above). |
| direct factory-child calls are covered, source-materialized, or explicitly deferred separately from router/live-input discovery | done | not applicable (see above). |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | not applicable — Aave not pool/factory/vault-heavy. |
| token-surface inventory completed or explicitly scoped out | done | AAVE governance base token registered (tokens/1/0x7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9.json — pre-existing, kind=base/governance, verified). Variable-debt-token underlyings + aTokens routed via existing lending/standard erc20 adapters. stkAAVE/aAAVE/GHO/SGHO tokens deferred with safety-module/governance surface. |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | done | registryV2/surface/aave/_deployments.json CREATED — I0 now enforced (was "NOT enforced" WARN before). 37 deployed · 9 cover · 28 exclude. |
| `npm run check:surface` output recorded | done | `[I0] aave: 37 deployed · 9 cover · 28 exclude (enforced vs bgd-labs/aave-address-book + Etherscan-verified ABIs)`. AaveTokenV3 I1 PASS (11 surface · 4 cover · 7 exclude). EXPECTED partial: I2/S2 FAIL ×6 (AAVE delegate/delegateByType/metaDelegate/metaDelegateByType + Delegate/DelegateByType typed-data) = P1 manifest to-do (test-as-loop-engine). I0' WARN ×3 = L2 weth-gateway gated-but-unlisted = multichain deferred (expected, non-destructive). |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | pending | |
| permission/fund-movement/red-flag selector review recorded | pending | |
| manifest files added/changed listed | pending | |
| enrichment/live_field decision recorded for every COVER action | pending | |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | pending | |
| Tier3 not needed or full Tier3 downstream contract completed | pending | |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | pending | |
| `npm run check:manifest` or protocol-filtered validate output recorded | pending | |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | pending | |
| iterations >= 5000 or justified lower bound | pending | |
| fixed edge-case matrix recorded | pending | |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | pending | |
| representative pass/error corpus entries committed or justified | pending | |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | pending | |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | pending | |
| external tx pull target address count is nonzero and recorded | pending | |
| Etherscan `api_calls_used` recorded | pending | |
| Etherscan `raw_txs_seen` recorded | pending | |
| Etherscan `unique_selectors_seen` recorded | pending | |
| Etherscan real tx coverage per COVER selector recorded | pending | |
| wallet-facing target sweep executed or explicitly not applicable, with target count, per-target floor, raw/matched tx counts, and target file | pending | |
| unmatched Etherscan txs classified as actionable/non-actionable with disposition counts | pending | |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | pending | |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | pending | |
| typed-data signing corpus/golden executed for every in-scope EIP-712 primaryType/witnessType, or explicitly not applicable | pending | |
| Dune MCP/API availability checked | pending | |
| Dune usage baseline recorded | pending | |
| Dune calibration/query executed with partition WHERE or explicitly blocked | pending | |
| Dune `executionCostCredits` / usage delta recorded | pending | |
| Dune rows returned / selected tx hashes recorded | pending | |
| representative real-tx corpus/golden entries committed or justified | pending | |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | pending | |
| SCOPE ORACLE — covered-surface real-usage coverage-share measured on the P0 universe (1st-party Etherscan/Dune: % of recent txs the covered (chain,to,selector) set decodes), and each user-facing DEFER's usage-share recorded; completion label must not over-claim it | pending | |

## P3 Develop Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| all P2 hard/soft/misdecoded/unknown_protocol_address/excluded gaps bucketed | pending | |
| each fix tied to a gap id, selector, tx hash, or synthetic seed | pending | |
| manifest/decoder/Tier3/harness change list recorded | pending | |
| P2 rerun after fixes recorded | pending | |
| corpus `expect` flips or exclusions justified | pending | |
| remaining gaps have explicit defer/blocker disposition | pending | |

## P4 Land Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| `registryV2 npm run build` output recorded | pending | |
| registryV2 build-index vitest output recorded | pending | |
| `npm run check:manifest` output recorded | pending | |
| `npm run check:surface` output recorded | pending | |
| `npm run check:universe -- --protocol <protocol> --require-cover-linkage` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | pending | |
| v3-harness coverage/fuzz/corpus outputs recorded | pending | |
| protocol-filtered strict corpus output recorded: `v3-harness corpus --filter <protocol> --require-expect-body` | pending | |
| `cargo test --workspace` output recorded | pending | |
| wasm build output recorded if runtime/wasm/schema changed | pending | |
| fmt/clippy/typecheck output recorded for changed crates/packages | pending | |
| exact staged files and commit hash recorded | pending | |
| remaining WARNs/deferred selectors/actions listed with reason | pending | |
| final completion label recorded without overclaiming wallet-facing/full-universe/multichain scope | pending | |
| no base/worktree merge performed unless user explicitly requested it | pending | |

## Blockers

If a mandatory item cannot be completed, write `blocked` rather than `done`.

| blocker | source | next action |
|---|---|---|
| | | |

## Final Completion Claim

Do not write "onboarding complete" unless every mandatory P0/P1/P2/P3/P4 row is `done` or has a concrete, user-visible `blocked` disposition and this command passes:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- <protocol> --phase all
```
