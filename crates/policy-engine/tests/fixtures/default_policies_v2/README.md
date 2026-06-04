# default_policies_v2 — 구현 단계(phase)별 분리

scopeball 기본 v2 정책 번들(`<id>/{manifest.json, policy.cedar}`)을 **구현 단계별 디렉터리**로 나눠 둔다. 분류 = "정책을 시스템에 추가했을 때 PASU 익스텐션이 실제로 그 검사를 수행할 수 있는가"의 준비도.

## 단계 정의 (2026-06-05 정제 — facts 수렴 반영)

- **phase1/A** — 지금 Integrate하면 **추가 작업 0으로 익스텐션이 ≥95% 정확히 검사**. 순수 디코드-action 비교거나, 필요한 fact가 전부 *구현*되어 있고 action 필드 / lowering이 이미 내리는 `live_inputs` 스냅샷 / 자동 sync되는 지갑상태(approvals·tokens·HL-position)만 읽음. external/stub/신규 state/신규 lowering 없음.
- **phase1/B** — **우리가 한 가지만 보강하면** 작동. stub fact 1개 본문 작성(데이터는 이미 가용), 쉽게 받아올 정보 1개(예: 단일 eth_getCode, 인라인 allowlist), 또는 *기존* state 필드 sync. 타 팀 불필요.
- **phase2** — **타 작업자 의존**. `external.*`/oracle/sanctions/reputation/floor/metadata 커넥터(그들의 IO), 미구현 registry 서버, 또는 그들이 채울 신규 state/DB 필드. cedar/action 표현은 OK, 블로커가 단일 hand-off로 남의 소관.
- **phase3** — 현실적으로 난도 높음·다단계지만 **판단을 가능케 할 구체적 경로 존재**(신규 action 표면 x402, 거래이력/행동 state, 다단계 sim, 비자명 신규 fact 네임스페이스). 경로를 조사·문서화.
- **phase-not-classified** — **main 브랜치 lowering이 데이터를 드롭**해서 보류(Curve pool_state, LP remove current_price). 또는 아직 phase 미부여 신작.
- **폐기 `discarded/`** — 장기적으로도 신뢰할 경로 없음. 극히 드물게만(경로 있으면 phase3). *삭제 아님*: id+파일 보존(리뷰 키잉). 현재 0건.

합계 **180개** = A 46 + B 41 + phase2 35 + phase3 51 + 미분류 7.

> **2026-06-05 라운드2 재분류** (facts 수렴 반영, 58 이동): lending HF/LTV가 action 스냅샷-우선(`position.health_factor_after`/`borrow_fraction_bps`는 `userStateBefore`+`reserveState`를 `param_action`으로 읽음)임이 확정돼 aave-hf-floor·aave-borrow-fraction 등이 1A로 승격. perp action-only fact(order_leverage·funding_adverse_rate·stop_trigger_misplaced·market slippageBp)는 1A, position-reading fact(crosses_zero·leverage_increase 등)는 라이브 포지션 sync 필요로 1B. curve/lp-remove는 main lowering 드롭으로 미분류. x402 10건은 신규 action 표면 필요로 미분류→phase3. 분류는 17 도메인 배치 fan-out + 적대적 검증(skeptic refute)으로 산출. 상세 근거: `agentBase/team/state-validation/reclassify-2026-06-05.md`.

## 로더 규약 (중요)

이 디렉터리를 읽는 모든 소비처는 **임의 깊이 재귀**한다: 어떤 디렉터리가 `manifest.json`을 **직접** 가지면 *번들*(더 안 내려감), 아니면 *grouping dir*(`phaseN/`, `phase1/A/` 등)로 보고 그 하위를 재귀한다. 평면 `<id>/`, phased `<phaseN>/<id>/`, 중첩 `<phaseN>/<sub>/<id>/` 레이아웃을 **모두** 지원하며, 새 그룹/하위그룹을 추가해도 자동 포함된다. 번들 dir 이름 == `manifest.id` 불변식 유지.

**phase 재분류 = `<id>/` 디렉터리를 phase 폴더 사이로 `git mv`** 하는 것. id·내용 불변(도구 리뷰 레이어가 id로 키잉되므로 id를 바꾸거나 삭제하면 리뷰 의견이 고아가 된다). 폐기도 삭제가 아니라 `discarded/` 이동.

**BLOCKED-BY-ACTION skip (중요)**: `policy.cedar` 첫머리에 `// BLOCKED-BY-ACTION` 배너가 있는 번들은 *현 스키마에 없는 action 표면*(예: x402 `Token::Erc3009TransferWithAuth`)을 참조하므로 **모든 소비처가 건너뛴다** — 스키마 컴파일(default_policies_v2)·method 카탈로그 검사(catalog_conformance)·extension 출하(copy-default-policies.js) 모두에서 제외. 트리에 *staged* 되지만 **inert**다. 표면이 착지하고 배너를 지우면 자동으로 활성·검증 대상이 된다. **x402 10건은 phase3으로 재분류됐으나 여전히 BLOCKED-BY-ACTION 배너 보유 → 게이트 skip 유지**(표면+sign매퍼 착지 전까지 inert).

소비처(이 규약을 따름):
- `crates/policy-engine/tests/default_policies_v2.rs` (`collect_bundles`, 재귀 walk)
- `crates/policy-engine/tests/catalog_conformance.rs` (`collect_bundles`, 재귀 walk)
- `crates/policy-engine-wasm/tests/hl_exchange_deny_e2e.rs` (`seed_bundle`)
- `browser-extension/scripts/copy-default-policies.js` (`collectBundles`)

`policies-loader-v2.ts`는 빌드 산출물(`policy-set-v2.json`)만 fetch하므로 무수정.


## phase1/A — 즉시 작동 — 46개

- `aave-borrow-fraction-warn`
- `aave-hf-floor-warn`
- `air-delegatee-not-self-deny`
- `air-merkle-without-proof-warn`
- `air-permit-on-held-token-deny`
- `air-recipient-not-self-deny`
- `ammlp-collect-recipient-not-self-deny`
- `ammlp-remove-recipient-not-self-deny`
- `bridge-recipient-not-self-deny`
- `bridge-refund-not-self-warn`
- `bridge-target-not-allowlisted-deny`
- `bridge-unlimited-approval-deny`
- `gas-cost-ratio-warn`
- `hl-confirm-approve-agent`
- `hl-confirm-high-leverage`
- `hl-confirm-usd-send`
- `hl-confirm-withdraw`
- `hl-no-short-perp`
- `holding-pct-outflow-warn`
- `increase-allowance-cap-warn`
- `lp-claim-recipient-self-warn`
- `lp-commit-platform-allowlist-deny`
- `multicall-hidden-approval-warn`
- `nft-bid-weth-unlimited-warn`
- `nft-far-expiry-order-warn`
- `nft-setapprovalforall-conduit-warn`
- `nft-transfer-burn-recipient-deny`
- `nft-untrusted-blur-root-deny`
- `permit-allowance-horizon-warn`
- `permit2-sign-allowance-far-expiry-warn`
- `perp-funding-adverse-warn`
- `perp-leverage-cap-deny`
- `perp-market-slippage-warn`
- `perp-self-leverage-ceiling-deny`
- `perp-stop-trigger-misplaced-warn`
- `reapprove-already-granted-warn`
- `send-first-time-or-burn-recipient-warn`
- `setapprovalforall-operator-warning`
- `signature-chain-mismatch-permit-warn`
- `swap-price-impact-warn`
- `swap-recipient-not-self-deny`
- `swap-slippage-high-warn`
- `transfer-to-token-own-contract-deny`
- `unknown-blind-sign-warning`
- `unlimited-approval-deny`
- `values-recipient-denylist-deny`


## phase1/B — 한 가지 보강 후 작동 — 41개

- `aave-cap-nearly-full-warn`
- `aave-delegate-borrow-allowlist-deny`
- `aave-emode-leverage-warn`
- `aave-frozen-paused-supply-deny`
- `aave-hf-band-volatile-warn`
- `aave-utilization-high-warn`
- `aave-withdraw-hf-floor-deny`
- `air-claim-locks-received-warn`
- `air-source-contract-mismatch-warn`
- `ammlp-cancel-target-missing-warn`
- `ammlp-intent-cap-over-balance-warn`
- `ammlp-intent-duplicate-warn`
- `ammlp-uni-v3v4-out-of-range-warn`
- `approve-spender-eoa-warn`
- `cftc-retail-leverage-cap-warn`
- `eu-esma-retail-perp-leverage-cap-deny`
- `gov-delegatee-allowlist-deny`
- `gov-redelegate-large-power-warn`
- `intent-validity-horizon-warn`
- `jp-perp-leverage-cap-2x-deny`
- `large-swap-usd-warning`
- `lp-claim-target-sale-mismatch-deny`
- `lp-commit-cumulative-cap-warn`
- `permit-unlimited-allowance-warn`
- `perp-adding-to-loser-warn`
- `perp-averaging-down-warn`
- `perp-concentration-warn`
- `perp-cross-exposure-cap-warn`
- `perp-isolated-to-cross-warn`
- `perp-leverage-increase-warn`
- `perp-liq-distance-thin-warn`
- `perp-reduce-only-flip-deny`
- `portfolio-category-concentration-cap-warn`
- `portfolio-fiat-peg-exposure-cap-warn`
- `portfolio-stable-reserve-floor-warn`
- `portfolio-token-concentration-cap-warn`
- `stk-lst-concentration-warn`
- `swap-out-token-honeypot-warn`
- `swap-special-token-fot-rebasing-warn`
- `transfer-outflow-usd-cap`
- `values-interest-bearing-exclude-warn`


## phase2 — 타 작업자(external 커넥터·registry·신규 state) 의존 — 35개

- `aave-isolation-debt-ceiling-warn`
- `aave-siloed-borrow-warn`
- `air-recipient-is-contract-warn`
- `air-unknown-token-warn`
- `alloc-bucket-overweight-warn`
- `approve-first-seen-spender-warn`
- `approve-fresh-domain-airdrop-context-deny`
- `approve-spender-unknown-contract-warn`
- `behav-fomo-pump-chase-warn`
- `buy-hidden-mint-proxy-power-warn`
- `buy-rug-risk-lp-owner-power-warn`
- `eu-mica-nonauthorized-emt-acquire-deny`
- `eu-mica-nonauthorized-emt-acquire-warn`
- `eu-sanctions-listed-recipient-deny`
- `fefta-designated-party-payment-deny`
- `gas-cost-usd-cap-deny`
- `honeypot-buy-block-presign`
- `jp-fefta-sanctioned-recipient-deny`
- `kr-terror-financing-designated-recipient-deny`
- `lido-rebasing-steth-to-contract-warn`
- `lp-commit-pay-token-mismatch-deny`
- `morpho-approve-to-bundler-core-warn`
- `morpho-blue-unrecognized-market-supply-warn`
- `morpho-withdraw-illiquid-or-paused-warn`
- `nft-transfer-blocklisted-recipient-deny`
- `ofac-sanctioned-mixer-receipt-deny`
- `ofac-sdn-sanctioned-address-deny`
- `ofac-sdn-sanctioned-recipient-deny`
- `permit2-sign-allowance-phishing-trigger-fanout-warn`
- `send-wrong-network-mismatch-deny`
- `stk-unstake-cooldown-warn`
- `swap-native-gas-starvation-warn`
- `swap-out-token-symbol-spoof-warn`
- `transfer-blocklisted-recipient-deny`
- `transfer-recipient-is-contract-warn`


## phase3 — 난도 높음, 판단 경로 존재 — 51개

- `aave-lst-emode-divergence-warn`
- `aave-oracle-stale-borrow-warn`
- `aave-repay-swap-slippage-deny`
- `ammlp-intent-isolated-fill-warn`
- `ap-doppelganger-recipient-guard`
- `approve-dormant-deprecated-contract-warn`
- `behav-overtrading-daily-count-warn`
- `bridge-cctp-recipient-unreceivable-deny`
- `bridge-dest-chain-unsupported-warn`
- `bridge-min-out-haircut-warn`
- `bridge-permission-change-deny`
- `bridge-relayer-fee-band-warn`
- `cooling-off-lock-deny`
- `daily-loss-limit-lockout-deny`
- `eip7702-delegate-sweeper-deny`
- `intent-dutch-decay-warn`
- `kr-darkcoin-privacy-token-swap-warn`
- `lp-bonding-curve-premium-deny`
- `morpho-set-authorization-operator-warn`
- `morpho-vault-unrecognized-deposit-warn`
- `multicall-outflow-cap-deny`
- `nft-low-floor-listing-warn`
- `nft-low-offer-accept-warn`
- `nft-seaport-wildcard-zone-deny`
- `nft-seaport-zero-consideration-sign-deny`
- `nft-zero-price-sale-deny`
- `permit2-unknown-spender-full-balance-warn`
- `perp-revenge-reentry-cooldown-warn`
- `perp-revenge-trade-cooldown-warn`
- `privacy-coin-delisted-acquire-warn`
- `self-exclusion-block-deny`
- `stk-lst-depeg-sell-warn`
- `suitability-leveraged-token-buy-warn`
- `swap-exactout-maxin-vs-fair-warn`
- `swap-floor-vs-fair-warn`
- `swap-permit2-spender-not-router-deny`
- `swap-shallow-route-warn`
- `swap-uni-v3v4-effective-fee-warn`
- `transfer-first-time-recipient-warn`
- `transfer-recipient-lookalike-poisoning-deny`
- `transfer-unknown-recipient-warn`
- `x402-agentic-cumulative-spend-cap-warn`
- `x402-auth-far-future-expiry-warn`
- `x402-first-time-pay-to-warn`
- `x402-from-not-connected-wallet-warn`
- `x402-future-validafter-dormant-warn`
- `x402-micro-payment-value-cap-warn`
- `x402-near-full-usdc-balance-warn`
- `x402-pay-to-blocklisted-deny`
- `x402-pay-to-is-contract-frontrun-warn`
- `x402-unknown-blind-sign-replaces-warn`


## phase-not-classified — main lowering 보류 / 미부여 — 7개

- `air-upfront-payment-warn`
- `ammlp-remove-exit-asymmetry-warn`
- `curve-depeg-pool-add-warn`
- `curve-imbalanced-add-skew-warn`
- `curve-metapool-base-depeg-warn`
- `curve-one-coin-withdraw-penalty-warn`
- `lido-rebasing-steth-as-lp-warn`
