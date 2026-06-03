# default_policies_v2 — 구현 단계(phase)별 분리

scopeball 기본 v2 정책 번들(`<id>/{manifest.json, policy.cedar}`)을 **구현 단계별 디렉터리**로 나눠 둔다.

## 단계 정의

- **phase1 (1차 구현)** — `18-policy-protocol-action.html`의 `functional=true` 정책. 다시 둘로 나뉜다:
  - **phase1/A (1차-A)** = `team/state-validation/policy-datasource-IMPLEMENTATION.html` 기준 **"지금 작동"(순수 G0)** **36개**. 순수 액션필드 비교거나, 구현된 fact가 이미 sync되는 state(approvals/tokens/HL-position)·액션필드만 읽음 → **추가 데이터소스 없이 현 구현으로 발화/테스트 가능**.
  - **phase1/B (1차-B)** = 같은 기준 **"추가 구현 필요"** **45개**. 로직(Cedar)은 1차 범위지만 State/registry/external/live-input 보강이 있어야 동작. 포함: `ammlp-cancel-target-missing-warn`(열린 주문 목록 조회 불가), `alloc-bucket-overweight-warn`(portfolio-*와 동일하게 `portfolio.group_pct basis=state2` 의존 → State₂ reducer 필요).
- **phase2 (2차 구현)** — 18-HTML 분류에 아직 없는 **신규** 10개(anti-scam/phishing 배치). 대부분 external policy-rpc 호스트 또는 sim-server fact 본문 작성 후 동작(전부 optional fail-open이라 미완성 상태로도 무해).
- **phase3 (3차 구현)** — 18-HTML `functional=false`("미지원") 41개.

합계 132개 = (A 36 + B 45) + 10 + 41.

분류 진실의 원천: `agentBase/team/cedar-manifest/18-policy-protocol-action.html`(1차/3차 = functional 플래그)와 `agentBase/team/state-validation/policy-datasource-IMPLEMENTATION.html`(1차-A/B = "지금 작동" vs "추가 구현 필요").

## 로더 규약 (중요)

이 디렉터리를 읽는 모든 소비처는 **임의 깊이 재귀**한다: 어떤 디렉터리가 `manifest.json`을 **직접** 가지면 *번들*(더 안 내려감), 아니면 *grouping dir*(`phaseN/`, `phase1/A/` 등)로 보고 그 하위를 재귀한다. 평면 `<id>/`, phased `<phaseN>/<id>/`, 중첩 `<phaseN>/<sub>/<id>/` 레이아웃을 **모두** 지원하며, 새 그룹/하위그룹을 추가해도 자동 포함된다. 번들 dir 이름 == `manifest.id` 불변식 유지.

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

