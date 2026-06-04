# default_policies_v2 — 구현 단계(phase)별 분리

scopeball 기본 v2 정책 번들(`<id>/{manifest.json, policy.cedar}`)을 **구현 단계별 디렉터리**로 나눠 둔다.

## 단계 정의

- **phase1 (1차 구현)** — `18-policy-protocol-action.html`의 `functional=true` 정책. 다시 둘로 나뉜다:
  - **phase1/A (1차-A)** = `team/state-validation/policy-datasource-IMPLEMENTATION.html` 기준 **"지금 작동"(순수 G0)** **36개**. 순수 액션필드 비교거나, 구현된 fact가 이미 sync되는 state(approvals/tokens/HL-position)·액션필드만 읽음 → **추가 데이터소스 없이 현 구현으로 발화/테스트 가능**.
  - **phase1/B (1차-B)** = 같은 기준 **"추가 구현 필요"** **45개**. 로직(Cedar)은 1차 범위지만 State/registry/external/live-input 보강이 있어야 동작. 포함: `ammlp-cancel-target-missing-warn`(열린 주문 목록 조회 불가), `alloc-bucket-overweight-warn`(portfolio-*와 동일하게 `portfolio.group_pct basis=state2` 의존 → State₂ reducer 필요).
- **phase2 (2차 구현)** — **26개** = anti-scam/phishing 10 + **policy-factory draft 승격 16**(2026-06-04). 공통 성격: action·표현은 OK, **서버 fact(facts/ aggregator) 하나만 정의되면 바로 동작**(optional fail-open). 단 honeypot-buy-block-presign 1건은 manifest `Record` 타입이라 `// BLOCKED-BY-ACTION`.
- **phase3 (3차 구현)** — **57개** = 18-HTML `functional=false`("미지원") 41 + **draft 승격 16**(2026-06-04). 승격분은 신규 action/state 표면·관할 config·거래이력 등 **복잡·다단계**라 전부 `// BLOCKED-BY-ACTION`(staged, 게이트 skip).
- **phase-not-classified (미분류)** — 18-HTML 분류 체계 **밖에서 새로 작성**된 정책(수요조사→정책화 산출). phase1/2/3과 **동일 차원의 최상위 그룹**. phase 분류(functional/작동여부)는 아직 부여 안 됨. 두 층:
  - **게이트 통과** 2개(Lido: `lido-rebasing-steth-to-contract-warn`, `lido-rebasing-steth-as-lp-warn`). 산출 노트: `team/state-validation/lido-policy-proposal.md`.
  - **BLOCKED-BY-ACTION** 10개(x402 / EIP-3009). 모두 신규 action 표면 `Token::Erc3009TransferWithAuth`(tag `erc3009_transfer_with_auth`)를 참조 — 현 스키마에 **없으므로 표면 + sign매퍼 + 신규 method 3종이 착지하기 전까지는 게이트가 통과하지 않는다**(각 `policy.cedar` 상단 `// BLOCKED-BY-ACTION` 박제). Lido와 달리 "지금 통과"가 아니라 "표면 도입 후 통과". 산출: `agentBase/policy-factory/reports/cycle-2026-06-03-x402.md` · 보완사항: `agentBase/policy-factory/reports/x402-gap-spec.md`.

합계 **180개** = (A 36 + B 45) + 26 + 57 + 16(not-classified). 2026-06-04 policy-factory **미분류 draft 49건을 분류**: phase2 16 + phase3 16(BLOCKED) + **폐기 16**(게이트 밖 `agentBase/policy-factory/discarded/`로 이동). 분류 근거: `team/state-validation/draft-promotion-2026-06-04.md`.

분류 진실의 원천: `agentBase/team/cedar-manifest/18-policy-protocol-action.html`(1차/3차 = functional 플래그)와 `agentBase/team/state-validation/policy-datasource-IMPLEMENTATION.html`(1차-A/B = "지금 작동" vs "추가 구현 필요").

## 로더 규약 (중요)

이 디렉터리를 읽는 모든 소비처는 **임의 깊이 재귀**한다: 어떤 디렉터리가 `manifest.json`을 **직접** 가지면 *번들*(더 안 내려감), 아니면 *grouping dir*(`phaseN/`, `phase1/A/` 등)로 보고 그 하위를 재귀한다. 평면 `<id>/`, phased `<phaseN>/<id>/`, 중첩 `<phaseN>/<sub>/<id>/` 레이아웃을 **모두** 지원하며, 새 그룹/하위그룹을 추가해도 자동 포함된다. 번들 dir 이름 == `manifest.id` 불변식 유지.

**BLOCKED-BY-ACTION skip (중요)**: `policy.cedar` 첫머리에 `// BLOCKED-BY-ACTION` 배너가 있는 번들은 *현 스키마에 없는 action 표면*(예: x402 `Token::Erc3009TransferWithAuth`)을 참조하므로 **모든 소비처가 건너뛴다** — 스키마 컴파일(default_policies_v2)·method 카탈로그 검사(catalog_conformance)·extension 출하(copy-default-policies.js) 모두에서 제외. 트리에 *staged* 되지만 **inert**다. 표면이 착지하고 배너를 지우면 자동으로 활성·검증 대상이 된다(별도 등록 불필요). 구현: 두 게이트의 `is_blocked_by_action()` + `copy-default-policies.js`의 `blocked` 가드. 현재 x402 10건이 이 상태(`team`/`policy-factory` 산출).

소비처(이 규약을 따르도록 수정됨):
- `crates/policy-engine/tests/default_policies_v2.rs` (`collect_bundles`, 재귀 walk)
- `crates/policy-engine/tests/catalog_conformance.rs` (`collect_bundles`, 재귀 walk)
- `crates/policy-engine-wasm/tests/hl_exchange_deny_e2e.rs` (`seed_bundle` — id를 임의 깊이 재귀 탐색)
- `browser-extension/scripts/copy-default-policies.js` (`collectBundles`, 재귀 walk → 평면 `policy-set-v2.json` 그대로 출력)

`policies-loader-v2.ts`는 빌드 산출물(`policy-set-v2.json`)만 fetch하므로 무수정.

## phase1/A — 1차-A (지금 작동, 추가 구현 불필요) — 36개

순수 액션필드 또는 구현된 fact가 이미-sync state만 읽음. 현 구현으로 발화/테스트 가능.

- `air-permit-on-held-token-deny`
- `air-recipient-not-self-deny`
- `ammlp-remove-recipient-not-self-deny`
- `bridge-recipient-not-self-deny`
- `bridge-refund-not-self-warn`
- `bridge-target-not-allowlisted-deny`
- `bridge-unlimited-approval-deny`
- `gas-cost-ratio-warn`
- `gas-cost-usd-cap-deny`
- `hl-confirm-approve-agent`
- `hl-confirm-high-leverage`
- `hl-confirm-usd-send`
- `hl-confirm-withdraw`
- `hl-no-short-perp`
- `holding-pct-outflow-warn`
- `increase-allowance-cap-warn`
- `lp-commit-platform-allowlist-deny`
- `multicall-hidden-approval-warn`
- `nft-bid-weth-unlimited-warn`
- `nft-far-expiry-order-warn`
- `nft-setapprovalforall-conduit-warn`
- `nft-untrusted-blur-root-deny`
- `permit-allowance-horizon-warn`
- `perp-leverage-cap-deny`
- `perp-leverage-increase-warn`
- `perp-market-slippage-warn`
- `perp-reduce-only-flip-deny`
- `reapprove-already-granted-warn`
- `send-first-time-or-burn-recipient-warn`
- `setapprovalforall-operator-warning`
- `signature-chain-mismatch-permit-warn`
- `swap-recipient-not-self-deny`
- `swap-slippage-high-warn`
- `unknown-blind-sign-warning`
- `unlimited-approval-deny`
- `values-recipient-denylist-deny`

## phase1/B — 1차-B (1차 범위, 추가 구현 필요) — 45개

로직은 1차이나 데이터소스(State/registry/external/live-input) 보강 필요. 버킷 A~F는 IMPLEMENTATION.html 참고.

- `aave-borrow-fraction-warn`
- `aave-cap-nearly-full-warn`
- `aave-delegate-borrow-allowlist-deny`
- `aave-emode-leverage-warn`
- `aave-frozen-paused-supply-deny`
- `aave-hf-band-volatile-warn`
- `aave-hf-floor-warn`
- `aave-utilization-high-warn`
- `aave-withdraw-hf-floor-deny`
- `air-claim-locks-received-warn`
- `air-delegatee-not-self-deny`
- `air-merkle-without-proof-warn`
- `air-source-contract-mismatch-warn`
- `air-unknown-token-warn`
- `alloc-bucket-overweight-warn`
- `ammlp-cancel-target-missing-warn`
- `ammlp-collect-recipient-not-self-deny`
- `ammlp-intent-cap-over-balance-warn`
- `ammlp-intent-duplicate-warn`
- `ammlp-uni-v3v4-out-of-range-warn`
- `gov-delegatee-allowlist-deny`
- `gov-redelegate-large-power-warn`
- `large-swap-usd-warning`
- `lp-claim-recipient-self-warn`
- `lp-claim-target-sale-mismatch-deny`
- `lp-commit-cumulative-cap-warn`
- `perp-adding-to-loser-warn`
- `perp-averaging-down-warn`
- `perp-concentration-warn`
- `perp-cross-exposure-cap-warn`
- `perp-funding-adverse-warn`
- `perp-isolated-to-cross-warn`
- `perp-liq-distance-thin-warn`
- `perp-stop-trigger-misplaced-warn`
- `portfolio-category-concentration-cap-warn`
- `portfolio-fiat-peg-exposure-cap-warn`
- `portfolio-stable-reserve-floor-warn`
- `portfolio-token-concentration-cap-warn`
- `stk-lst-concentration-warn`
- `stk-unstake-cooldown-warn`
- `swap-out-token-honeypot-warn`
- `swap-price-impact-warn`
- `swap-special-token-fot-rebasing-warn`
- `transfer-outflow-usd-cap`
- `values-interest-bearing-exclude-warn`

## phase2 — 2차 (신규 anti-scam/phishing) — 10개

18-HTML DATA에 아직 없는 신규 정책. external 커넥터/sim-server fact 본문 작성 후 동작.

- `approve-spender-unknown-contract-warn`
- `nft-seaport-zero-consideration-sign-deny`
- `nft-transfer-blocklisted-recipient-deny`
- `nft-transfer-burn-recipient-deny`
- `permit2-sign-allowance-far-expiry-warn`
- `permit2-sign-allowance-phishing-trigger-fanout-warn`
- `swap-native-gas-starvation-warn`
- `swap-out-token-symbol-spoof-warn`
- `transfer-recipient-is-contract-warn`
- `transfer-to-token-own-contract-deny`

## phase3 — 3차 (미지원) — 41개

18-HTML functional=false. 데이터소스 추가 구현 이후 단계.

- `aave-isolation-debt-ceiling-warn`
- `aave-lst-emode-divergence-warn`
- `aave-oracle-stale-borrow-warn`
- `aave-repay-swap-slippage-deny`
- `aave-siloed-borrow-warn`
- `air-recipient-is-contract-warn`
- `air-upfront-payment-warn`
- `ammlp-intent-isolated-fill-warn`
- `ammlp-remove-exit-asymmetry-warn`
- `approve-spender-eoa-warn`
- `behav-fomo-pump-chase-warn`
- `behav-overtrading-daily-count-warn`
- `bridge-cctp-recipient-unreceivable-deny`
- `bridge-dest-chain-unsupported-warn`
- `bridge-min-out-haircut-warn`
- `bridge-permission-change-deny`
- `bridge-relayer-fee-band-warn`
- `curve-depeg-pool-add-warn`
- `curve-imbalanced-add-skew-warn`
- `curve-metapool-base-depeg-warn`
- `curve-one-coin-withdraw-penalty-warn`
- `intent-dutch-decay-warn`
- `intent-validity-horizon-warn`
- `lp-bonding-curve-premium-deny`
- `lp-commit-pay-token-mismatch-deny`
- `multicall-outflow-cap-deny`
- `nft-low-floor-listing-warn`
- `nft-low-offer-accept-warn`
- `nft-seaport-wildcard-zone-deny`
- `nft-zero-price-sale-deny`
- `permit2-unknown-spender-full-balance-warn`
- `stk-lst-depeg-sell-warn`
- `suitability-leveraged-token-buy-warn`
- `swap-exactout-maxin-vs-fair-warn`
- `swap-floor-vs-fair-warn`
- `swap-permit2-spender-not-router-deny`
- `swap-shallow-route-warn`
- `swap-uni-v3v4-effective-fee-warn`
- `transfer-blocklisted-recipient-deny`
- `transfer-first-time-recipient-warn`
- `transfer-recipient-lookalike-poisoning-deny`

## phase-not-classified — 미분류 (수요조사→정책화 산출) — 12개

18-HTML 분류 밖에서 새로 작성. phase1/2/3과 동일 차원.

### 게이트 통과 (2개 · Lido)
- `lido-rebasing-steth-to-contract-warn`
- `lido-rebasing-steth-as-lp-warn`

### BLOCKED-BY-ACTION (10개 · x402 / EIP-3009)

신규 action 표면 `Token::Erc3009TransferWithAuth`(tag `erc3009_transfer_with_auth`) 부재로 **현 스키마에서 컴파일 불가 → 게이트 보류**. 표면 + sign-resolver 매퍼 + 신규 method 3종(`auth.validity_horizon_sec`, `intent.start_horizon_sec`, `x402_budget.window_spend_state`) 착지 시 활성. 9개는 cedar/method/external 재사용으로 즉시 표현 가능, `x402-agentic-cumulative-spend-cap-warn`만 추가로 needs-new-state. 보완사항: `agentBase/policy-factory/reports/x402-gap-spec.md`.

- `x402-pay-to-blocklisted-deny` (deny)
- `x402-auth-far-future-expiry-warn`
- `x402-micro-payment-value-cap-warn`
- `x402-near-full-usdc-balance-warn`
- `x402-from-not-connected-wallet-warn`
- `x402-first-time-pay-to-warn`
- `x402-pay-to-is-contract-frontrun-warn`
- `x402-future-validafter-dormant-warn`
- `x402-unknown-blind-sign-replaces-warn`
- `x402-agentic-cumulative-spend-cap-warn` (needs-new-state)

