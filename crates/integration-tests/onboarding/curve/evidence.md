# Curve Protocol Onboarding Evidence

> Re-onboarding run for Curve. Treat existing Curve artifacts as candidates, not
> proof. Fill each phase row with exact commands, counts, artifacts, and blockers.

## Run Metadata

| field | value |
|---|---|
| protocol | curve |
| branch | feat/curve-onboarding-redo |
| worktree | /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2 |
| date | 2026-06-01 |
| main agent | Codex |
| base commit | e2bb86d2 |

## P0 Research Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Codex current-session research executed | done | Codex fetched Curve public API snapshots with `curl -L -s https://api.curve.fi/api/getPools/{all,ethereum/main,ethereum/factory,ethereum/factory-stable-ng,ethereum/crypto,ethereum/factory-crypto,ethereum/factory-tricrypto,ethereum/factory-twocrypto}` into `/private/tmp/curve-*.json`, compared them with `registryV2/surface/curve/_deployments.json`, and wrote `crates/integration-tests/onboarding/curve/address-universe-summary.json`. |
| Claude Code or sub-agent research executed | done | Sub-agent `019e817a-891d-7d73-819c-40117dde92f0` reviewed contract/pool universe local artifacts; sub-agent `019e817a-a93e-76b1-adb1-8675f11b19db` reviewed token-surface local artifacts. Claude Code CLI was attempted with `claude -p ...` but blocked with `Not logged in · Please run /login`; sub-agents satisfied the framework's required independent second opinion for this run. |
| Claude/sub-agent exact prompt or command recorded | done | Prompts are recorded in this session transcript. Claude command attempted: `claude -p --permission-mode auto --allowedTools Read,Grep,Glob -- "<Curve P0 second-opinion prompt>"`; result `Not logged in`. Sub-agent prompts included cwd, branch, read targets, no-edit guardrail, pool/factory universe focus, token-surface focus, and gap classification output. |
| Codex-only candidates listed | done | `address-universe-summary.json` records Curve API `all` universe: 2,265 pool rows / 2,260 unique pool addresses. Top undeclared candidates by TVL include 3Crv `0xbebc...ff1c7`, PYUSDUSDS `0xa632...fc1f`, steCRV `0xdc24...7022`, RLUSD/USDC `0xd001...a186`, and factory crvUSD pools. |
| Claude/sub-agent-only candidates listed | done | Contract-universe sub-agent identified local-only gaps: `_deployments.json` is a selected concrete subset; factory contracts are excluded without a recorded factory/event/address-provider universe query/count; no `chain_to_addresses_source` resolver for Curve. Token sub-agent identified local-only token-gate gaps and four token files lacking `token_kind` despite having `erc_kind`. |
| dropped-unverified candidates listed with reason | done | No new candidate was added to registry during P0. Existing `_deployments.json` already warns prior LLM cross-check addresses were hallucinated and not accepted. Current Curve API candidates remain undispositioned rather than dropped. |
| final contract inventory verified against first-party sources | blocked | Full Curve inventory is not complete. Current local `_deployments.json` has 47 contracts (41 cover / 6 exclude), while Curve API `all` snapshot has 2,260 unique pool addresses; only 16 API pool rows are declared locally. |
| pool-heavy/factory protocol address universe source/query/count recorded, or explicitly not applicable | done | Re-fetched Curve public API snapshots into `/private/tmp/curve-*-p0.json` and generated `registryV2/surface/curve/_pool_universe.json`. Sources include Ethereum `all/main/factory/factory-stable-ng/crypto/factory-crypto/factory-tricrypto/factory-twocrypto` and Base `all/factory-stable-ng/factory-tricrypto`. Unique candidates: 3,097 pools (Ethereum 2,260, Base 837). |
| pool-heavy/factory universe artifact is machine-readable, nonzero, and committed, or explicitly not applicable | done | `registryV2/surface/curve/_pool_universe.json`: `source_count=3097`, `counts.total=3097`, `cover=17`, `defer=3080`, `exclude=0`. |
| every pool/factory child address in universe dispositioned as cover/exclude/defer with reason and batch boundary | done | `_pool_universe.json` dispositions every candidate: 17 existing concrete supported pools are `cover`; all other Curve API pools are `defer` with family-specific batches such as `curve-ethereum-factory-stable-ng-deferred`, `curve-ethereum-legacy-main-deferred`, and `curve-base-factory-stable-ng-deferred`. |
| concrete manifest vs protocol source resolver/generator strategy decided for pool universe | done | Current batch keeps existing concrete manifests for 17 covered pools and explicitly defers the remaining 3,080 pools pending a protocol source resolver/generator or family-specific batch onboarding. This avoids pretending representative pools equal full Curve support. |
| `npm run check:universe -- --protocol <protocol>` output recorded for pool/factory/vault-heavy protocols, or explicitly not applicable | done | `cd registryV2 && npm run check:universe -- --protocol curve` PASS: `surface/curve/_pool_universe.json: 3097 candidates · 17 cover · 0 exclude · 3080 defer · source_count=3097`. P4-form linkage also PASS after `npm run build`: `npm run check:universe -- --protocol curve --require-cover-linkage`. |
| token-surface inventory completed or explicitly scoped out | blocked | Covered local subset appears registered, but full Curve token surface is not complete because pool universe is not closed. Sub-agent found no machine gate proving LP/share/receipt/underlying completeness across deferred/undeclared Curve pool universe. |
| `registryV2/surface/<protocol>/_deployments.json` updated if applicable | blocked | Not updated in P0 because the required full universe disposition is missing. Current file remains a selected subset: `registryV2/surface/curve/_deployments.json` 47 contracts, 41 cover, 6 exclude. |
| `npm run check:surface` output recorded | done | `cd registryV2 && npm run check:surface` PASS. Curve selected subset lines include crvUSD Controller 3 pools, CryptoSwap-NG 5 pools, LiquidityGauge 8 pools, RouterNG 3 pools, StableSwap-NG mainnet 3 pools + Base 1 pool, Twocrypto-NG 8 pools; I0 `curve: 47 deployed · 41 cover · 6 exclude`. This does not prove full Curve pool universe closure. |

## P1 Authoring Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| every COVER selector mapped to existing ActionBody or Tier3 requirement | done | Current Curve selected-support batch has 13 coverage files with 91 COVER selector decisions and 232 existing manifests. Manifest bodies map to existing `ActionBody` domains: `amm=140`, `lending=70`, `staking=19`, `permission=3`; no new domain/Tier3 schema was introduced in this run. `npm run check:surface` confirms every COVER selector has manifests at each covered address. |
| permission/fund-movement/red-flag selector review recorded | done | `registryV2/surface/curve/*.coverage.json` records all external mutating selectors as cover/exclude. Red-flag permission selectors are covered either through protocol-specific `permission` manifests (`llamalend approve`, `minter toggle_approve_mint`) or excluded only when they are standard ERC20 pool/LP token functions covered by standard token auto-enumeration. Fund movement selectors map to AMM swap/add/remove liquidity, lending borrow/repay/collateral, staking deposit/withdraw/claim/veCRV lock, or explicit defer/exclude reasons. |
| manifest files added/changed listed | done | No Curve manifests were added or changed in this P1 pass. Existing manifest set remains `registryV2/manifests/curve/**` with 232 files across crvUSD controller, CryptoSwap, FeeDistributor, Gauge, GaugeController, LlamaLend, Minter, RouterNG, StableSwap-NG, Twocrypto-NG, and veCRV. New P0 artifact is `registryV2/surface/curve/_pool_universe.json`; it does not add support for deferred pools. |
| enrichment/live_field decision recorded for every COVER action | done | Existing Curve manifests carry decode-faithful fields plus declarative `live_inputs` for the actions that need runtime context. Summary: 70 manifests include non-empty live input data, 162 have none. Examples: StableSwap-NG swap has `onchain_view get_balances()`, derived expected output/price impact, and gas oracle; crvUSD controller borrow has controller/user state and derived debt/rate/liquidity fields. |
| required remote policy-RPC/live/enrichment methods have local handler, configured endpoint test, or explicit blocker | done | Existing Curve manifests declare no `requires.host_capabilities` and no policy-RPC method requirements; live/enrichment data is declared through manifest `live_inputs` sources (`onchain_view`, `derived_from`, `oracle_feed`) or absent for decode-faithful actions. No new remote method was introduced in this run. |
| Tier3 not needed or full Tier3 downstream contract completed | done | No new Tier3 action/domain was needed for this P1 pass. Existing Curve manifests target already-defined domains/actions in `crates/simulation/reducer/src/action/{amm,lending,staking,permission}` and existing `policy-engine/src/lowering_v2/{amm,lending,staking,permission}` lowering paths. |
| Tier3 files listed if applicable: ActionBody/effect/view/sync/lowering_v2/cedarschema/schema registration/conformance test | done | Applicable existing downstream files: `crates/simulation/reducer/src/action/amm`, `crates/simulation/reducer/src/action/lending`, `crates/simulation/reducer/src/action/staking`, `crates/simulation/reducer/src/action/permission`, `crates/policy-engine/src/lowering_v2/{amm,lending,staking,permission}`, and `schema/policy-schema/actions/{amm,lending,staking,permission}`. No new files changed for Tier3 in this run. |
| `npm run check:manifest` or protocol-filtered validate output recorded | done | `cd registryV2 && npm run check:manifest` PASS: build-index wrote 1,626 callkeys + 82 typed-data entries across 481 manifests; Rust v3 harness `validate (all): 1436 single_emit manifest(s) OK, 0 structural errors [iters/manifest=24]`. Protocol-filtered command also PASS: `cargo run -p policy-engine-integration-tests --bin v3-harness -- validate --filter curve --iterations 24` => `328 single_emit manifest(s) OK, 0 structural errors`. |

## P2 Synthetic Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| fuzz command with seed recorded | blocked | Required P2 synthetic fuzz was not run as a completion gate. Only the lightweight structural validation was run: `cargo run -p policy-engine-integration-tests --bin v3-harness -- validate --filter curve --iterations 24`, which is not a substitute for the required seeded 5,000-iteration fuzz/edge matrix. |
| iterations >= 5000 or justified lower bound | blocked | Lower bound is not justified for completion. `_pool_universe.json` now closes the address universe, but existing Curve corpus still lacks semantic pins and the selected concrete support batch remains only 17 pool addresses; a 5,000-iteration run would validate only selected subset shape, not full Curve support. |
| fixed edge-case matrix recorded | blocked | No complete edge-case matrix recorded for the new Curve run. Existing corpus remains useful but legacy-valid only because pass entries lack `expect_body`. |
| permission/value/nested/array/opcode/deadline/path edge coverage recorded | blocked | Not complete. Current strict corpus output shows no field-level semantic pins for Curve pass entries; nested/router/path/controller semantic coverage must be added before completion. |
| representative pass/error corpus entries committed or justified | blocked | Existing Curve corpus entries are present, but strict semantic replay shows 10 pass entries without `expect_body`; this is a hard blocker, not a completion justification. |

## P2 Real-Tx Evidence

| required evidence | status | artifact / exact command / summary |
|---|---|---|
| Etherscan MCP/API availability checked | done | Etherscan REST API v2 reachable with local `.env` key. Initial run exposed a framework/dogfood script bug (`status` vs `_deployments.json` `decision` field produced 0 addresses); corrected run succeeded. |
| Etherscan txlist pull executed adapter-blind by P0 cover addresses | done | `crates/integration-tests/onboarding/curve/etherscan-cover-tx-summary.json`: queried every current local Curve `decision=cover`, `chainId=1` address from `_deployments.json` using `account.txlist`, `offset=10000`, `sort=desc`; 40 cover addresses queried. |
| external tx pull target address count is nonzero and recorded | done | `etherscan-cover-tx-summary.json`: `coverAddressesQueried=40`. The initial zero-target run was discarded and recorded as a dogfood failure (`status` vs `decision` field mismatch), not accepted as evidence. |
| Etherscan `api_calls_used` recorded | done | `etherscan-cover-tx-summary.json`: `apiCalls=40`. |
| Etherscan `raw_txs_seen` recorded | done | `etherscan-cover-tx-summary.json`: `rawTxs=125253`. This exceeds the framework's 10,000 tx/protocol target for the currently selected cover set, but not for the full Curve pool universe. |
| Etherscan `unique_selectors_seen` recorded | done | `etherscan-cover-tx-summary.json`: `uniqueSelectors=112`; top selectors include `0x5c9c18e2`, `0x1e83409a`, `0xdd171e7c`, `0x6a627842`, `0xd7136328`. |
| Etherscan real tx coverage per COVER selector recorded | done | `etherscan-cover-tx-summary.json` records per-address selector counts plus `indexed` flags; `matchedCallkeys=183`. High-volume unmatched address/selectors are also listed, e.g. `0x4ebd...4a14/0x65b2489b` and crvUSD/LlamaLend controller selectors. |
| pool-heavy/factory protocols swept candidate/universe addresses, not only selected cover addresses, or explicitly not applicable | blocked | Not complete. `_pool_universe.json` now provides the candidate universe, but the Etherscan/Dune real-tx sweep has not yet been rerun over the 3,097 candidate pool addresses; previous Etherscan sweep covered only 40 local cover/deployment addresses. |
| unknown to-addresses with known protocol selectors bucketed as P0/P2 hard gaps | blocked | Current real-tx summary buckets unknown selectors on known covered addresses only. Need a fresh P2 sweep against `_pool_universe.json` to bucket candidate addresses as `cover`/`defer` and identify any non-universe addresses with known Curve selectors. |
| Dune MCP/API availability checked | done | `mcp__dune.getUsage` succeeded before and after query. Plan `community_fluid_engine_v2`, quota 2,500 credits. |
| Dune usage baseline recorded | done | `crates/integration-tests/onboarding/curve/dune-calibration-summary.json`: before query `creditsUsed=377.867`, after query `creditsUsed=378.16`. |
| Dune calibration/query executed with partition WHERE or explicitly blocked | done | Dune query `7626385` (`https://dune.com/queries/7626385`) executed on free engine with `ethereum.transactions WHERE block_date >= current_date - interval '7' day`, scoped to current local Curve cover addresses. |
| Dune `executionCostCredits` / usage delta recorded | done | `dune-calibration-summary.json`: `executionCostCredits=0.293`, usage delta `0.293`. |
| Dune rows returned / selected tx hashes recorded | done | `dune-calibration-summary.json`: 58 total address/selector rows, 20 preview rows, selected sample hashes recorded for top rows. This is calibration/selector stats, not full Curve universe completion. |
| representative real-tx corpus/golden entries committed or justified | blocked | New Etherscan/Dune summaries identify candidates and samples, but no new representative Curve corpus entries with `expect_body` were committed in this run. Existing real-tx corpus is blocked by the strict semantic pin failure. |
| protocol-filtered corpus replay executed with semantic pin gate: `v3-harness corpus --filter <protocol> --require-expect-body` | blocked | Command executed and correctly failed: `cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus --filter curve --require-expect-body` => `10/10 matched`, but `semantic expect_body: 0/10 pass entries pinned`; 10 pass entries lack field-level `expect_body` assertions. |

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
| no base/worktree merge performed unless user explicitly requested it | pending | |

## Blockers

If a mandatory item cannot be completed, write `blocked` rather than `done`.

| blocker | source | next action |
|---|---|---|
| 3,080 Curve API pool candidates are explicitly deferred, not supported | `registryV2/surface/curve/_pool_universe.json` | Implement a Curve protocol source resolver/generator or family-specific onboarding batches to move deferred pools into `cover`; until then the universe is closed but full Curve pool support is not complete. |
| full Curve token surface not machine-verified | token sub-agent review + missing `check:tokens` gate | Add protocol-agnostic token inventory checker; ensure covered pools/gauges/controllers imply required LP/share/receipt/underlying token JSON or explicit defer. |
| Claude Code unavailable | `claude -p ...` returned `Not logged in · Please run /login` | Use Codex sub-agents for this run; authenticate Claude Code before a future required Claude-specific review. |
| Curve strict corpus semantic pin gate fails | `v3-harness corpus --filter curve --require-expect-body` | Add `expect_body` assertions to existing Curve corpus pass entries before any Curve onboarding completion claim. |
| required P2 synthetic fuzz/edge matrix not run | existing corpus semantic pin failure + selected concrete support batch | Run seeded fuzz at the required iteration floor or record a defensible lower bound, then commit representative pass/error corpus entries. |

## Final Completion Claim

Do not write "onboarding complete" unless every mandatory P0/P1/P2/P3/P4 row is
`done` or has a concrete, user-visible `blocked` disposition and this command
passes:

```bash
cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- curve --phase all
```
