# default_policies_v2 — 구현 단계(phase)별 분리

scopeball 기본 v2 정책 번들(`<id>/{manifest.json, policy.cedar}`)을 **구현 단계별 디렉터리**로 둔다. 기준 = "정책 추가 시 PASU 익스텐션이 실제 그 검사를 수행하는가"의 준비도.

## 단계 정의 (2026-06-05 라운드3 — origin/main 머지 후 결정론적 재분류)

- **phase1/A** (35) — 추가작업 0 즉시작동. 트리거 action이 v3 디코드(registryV2 `emit.body`)되고, cedar가 읽는 값이 전부 **calldata 디코드 필드 / auto-synced 지갑상태(approval·token·HL-position) / 구현 fact**에서 나옴. external·stub·신규state·live_input·신규표면 없음.
- **phase1/B** (1) — 우리가 stub 하나 채우거나 한 가지 배선만 하면 작동(타 팀 불필요).
- **phase2** (120) — 타 작업자 의존: external/oracle/sanctions 커넥터, **registry v3 디코드 매니페스트 추가**(perp/launchpad 등 미디코드), **live_input/sync 배선**(v3 manifest는 live_inputs를 0개 emit — 모든 enrichment 값은 sync 제공 필요), 신규 state 모델.
- **phase3** (19) — 난도↑이나 경로 존재: 신규 action 표면(x402 `Erc3009`·EIP-7702), 거래/행동 이력 state.
- **phase-not-classified** (7) — main 브랜치 pool_state/current_price lowering 드롭으로 보류(curve/lp).

합계 **182**.

> **라운드3 방법(결정론적)**: ①디코드 셋 = registryV2 1011 manifest `emit.body.<domain>.action` 전수(90 action)+HL. ②method 109 구현상태 = facts/*.rs grep. ③v3는 live_inputs 0 emit → live_input 읽는 deny/정책은 sync 전까지 phase2. ④단정 버킷(1A/1B/미분류) 머지 트리서 적대적 재검증(5건 정정). 디코드 진실의 원천은 배포 GCS(repo registryV2는 시드). 상세: `agentBase/team/state-validation/`. fail-closed 게이트(`no_default_deny_policy_depends_only_on_optional_enrichment`)는 출하분 phase1/A만 검사(staged phase2/3 deny는 enrichment 배선 전 fail-open이 정상).

## 로더 규약
소비처는 **임의 깊이 재귀**(dir이 manifest.json 직접 보유=번들, 아니면 grouping). 재분류 = `<id>/`를 phase 폴더 사이 `git mv`(id·내용 불변). `// BLOCKED-BY-ACTION` 번들은 전 소비처 skip.

## phase1/A — 즉시 작동 (추가작업 0) — 35개

- `aave-delegate-borrow-allowlist-deny`
- `air-claim-locks-received-warn`
- `air-delegatee-not-self-deny`
- `air-merkle-without-proof-warn`
- `air-recipient-not-self-deny`
- `ammlp-collect-recipient-not-self-deny`
- `ammlp-remove-recipient-not-self-deny`
- `bridge-recipient-not-self-deny`
- `bridge-refund-not-self-warn`
- `bridge-target-not-allowlisted-deny`
- `bridge-unlimited-approval-deny`
- `gov-delegatee-allowlist-deny`
- `hl-confirm-approve-agent`
- `hl-confirm-high-leverage`
- `hl-confirm-unknown`
- `hl-confirm-usd-send`
- `hl-confirm-withdraw`
- `hl-no-short-perp`
- `holding-pct-outflow-warn`
- `increase-allowance-cap-warn`
- `multicall-hidden-approval-warn`
- `nft-bid-weth-unlimited-warn`
- `nft-setapprovalforall-conduit-warn`
- `nft-transfer-burn-recipient-deny`
- `permit2-sign-allowance-confirm`
- `permit2-sign-allowance-far-expiry-warn`
- `reapprove-already-granted-warn`
- `send-first-time-or-burn-recipient-warn`
- `setapprovalforall-operator-warning`
- `signature-chain-mismatch-permit-warn`
- `swap-recipient-not-self-deny`
- `transfer-to-token-own-contract-deny`
- `unknown-blind-sign-warning`
- `unlimited-approval-deny`
- `values-recipient-denylist-deny`

## phase1/B — 우리가 한 가지 보강 — 1개

- `air-source-contract-mismatch-warn`

## phase2 — 타 작업자 의존 (external 커넥터·registry v3 디코드·live_input/sync·신규state) — 120개

- `aave-borrow-fraction-warn`
- `aave-cap-nearly-full-warn`
- `aave-emode-leverage-warn`
- `aave-frozen-paused-supply-deny`
- `aave-hf-band-volatile-warn`
- `aave-hf-floor-warn`
- `aave-isolation-debt-ceiling-warn`
- `aave-lst-emode-divergence-warn`
- `aave-oracle-stale-borrow-warn`
- `aave-repay-swap-slippage-deny`
- `aave-siloed-borrow-warn`
- `aave-utilization-high-warn`
- `aave-withdraw-hf-floor-deny`
- `air-recipient-is-contract-warn`
- `air-unknown-token-warn`
- `alloc-bucket-overweight-warn`
- `ammlp-cancel-target-missing-warn`
- `ammlp-intent-cap-over-balance-warn`
- `ammlp-intent-duplicate-warn`
- `ammlp-intent-isolated-fill-warn`
- `ammlp-uni-v3v4-out-of-range-warn`
- `approve-dormant-deprecated-contract-warn`
- `approve-first-seen-spender-warn`
- `approve-fresh-domain-airdrop-context-deny`
- `approve-spender-eoa-warn`
- `approve-spender-unknown-contract-warn`
- `behav-fomo-pump-chase-warn`
- `bridge-cctp-recipient-unreceivable-deny`
- `bridge-dest-chain-unsupported-warn`
- `bridge-min-out-haircut-warn`
- `bridge-permission-change-deny`
- `bridge-relayer-fee-band-warn`
- `buy-hidden-mint-proxy-power-warn`
- `buy-rug-risk-lp-owner-power-warn`
- `cftc-retail-leverage-cap-warn`
- `eu-esma-retail-perp-leverage-cap-deny`
- `eu-mica-nonauthorized-emt-acquire-deny`
- `eu-mica-nonauthorized-emt-acquire-warn`
- `eu-sanctions-listed-recipient-deny`
- `fefta-designated-party-payment-deny`
- `gas-cost-ratio-warn`
- `gas-cost-usd-cap-deny`
- `gov-redelegate-large-power-warn`
- `honeypot-buy-block-presign`
- `intent-dutch-decay-warn`
- `intent-validity-horizon-warn`
- `jp-fefta-sanctioned-recipient-deny`
- `jp-perp-leverage-cap-2x-deny`
- `kr-darkcoin-privacy-token-swap-warn`
- `kr-terror-financing-designated-recipient-deny`
- `large-swap-usd-warning`
- `lido-rebasing-steth-as-lp-warn`
- `lido-rebasing-steth-to-contract-warn`
- `lp-bonding-curve-premium-deny`
- `lp-claim-recipient-self-warn`
- `lp-claim-target-sale-mismatch-deny`
- `lp-commit-cumulative-cap-warn`
- `lp-commit-pay-token-mismatch-deny`
- `lp-commit-platform-allowlist-deny`
- `morpho-approve-to-bundler-core-warn`
- `morpho-blue-unrecognized-market-supply-warn`
- `morpho-set-authorization-operator-warn`
- `morpho-vault-unrecognized-deposit-warn`
- `morpho-withdraw-illiquid-or-paused-warn`
- `multicall-outflow-cap-deny`
- `nft-far-expiry-order-warn`
- `nft-low-floor-listing-warn`
- `nft-low-offer-accept-warn`
- `nft-seaport-wildcard-zone-deny`
- `nft-seaport-zero-consideration-sign-deny`
- `nft-transfer-blocklisted-recipient-deny`
- `nft-untrusted-blur-root-deny`
- `nft-zero-price-sale-deny`
- `ofac-sanctioned-mixer-receipt-deny`
- `ofac-sdn-sanctioned-address-deny`
- `ofac-sdn-sanctioned-recipient-deny`
- `permit-allowance-horizon-warn`
- `permit-unlimited-allowance-warn`
- `permit2-sign-allowance-phishing-trigger-fanout-warn`
- `permit2-unknown-spender-full-balance-warn`
- `perp-adding-to-loser-warn`
- `perp-averaging-down-warn`
- `perp-concentration-warn`
- `perp-cross-exposure-cap-warn`
- `perp-funding-adverse-warn`
- `perp-isolated-to-cross-warn`
- `perp-leverage-cap-deny`
- `perp-leverage-increase-warn`
- `perp-liq-distance-thin-warn`
- `perp-market-slippage-warn`
- `perp-reduce-only-flip-deny`
- `perp-self-leverage-ceiling-deny`
- `perp-stop-trigger-misplaced-warn`
- `portfolio-category-concentration-cap-warn`
- `portfolio-fiat-peg-exposure-cap-warn`
- `portfolio-stable-reserve-floor-warn`
- `portfolio-token-concentration-cap-warn`
- `privacy-coin-delisted-acquire-warn`
- `send-wrong-network-mismatch-deny`
- `stk-lst-concentration-warn`
- `stk-lst-depeg-sell-warn`
- `stk-unstake-cooldown-warn`
- `suitability-leveraged-token-buy-warn`
- `swap-exactout-maxin-vs-fair-warn`
- `swap-floor-vs-fair-warn`
- `swap-native-gas-starvation-warn`
- `swap-out-token-honeypot-warn`
- `swap-out-token-symbol-spoof-warn`
- `swap-permit2-spender-not-router-deny`
- `swap-price-impact-warn`
- `swap-shallow-route-warn`
- `swap-slippage-high-warn`
- `swap-special-token-fot-rebasing-warn`
- `swap-uni-v3v4-effective-fee-warn`
- `transfer-blocklisted-recipient-deny`
- `transfer-first-time-recipient-warn`
- `transfer-outflow-usd-cap`
- `transfer-recipient-is-contract-warn`
- `transfer-unknown-recipient-warn`
- `values-interest-bearing-exclude-warn`

## phase3 — 난도↑·경로 존재 (신규 action 표면·행동이력) — 19개

- `ap-doppelganger-recipient-guard`
- `behav-overtrading-daily-count-warn`
- `cooling-off-lock-deny`
- `daily-loss-limit-lockout-deny`
- `eip7702-delegate-sweeper-deny`
- `perp-revenge-reentry-cooldown-warn`
- `perp-revenge-trade-cooldown-warn`
- `self-exclusion-block-deny`
- `transfer-recipient-lookalike-poisoning-deny`
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

## phase-not-classified — main pool_state/current_price lowering 보류 — 7개

- `air-permit-on-held-token-deny`
- `air-upfront-payment-warn`
- `ammlp-remove-exit-asymmetry-warn`
- `curve-depeg-pool-add-warn`
- `curve-imbalanced-add-skew-warn`
- `curve-metapool-base-depeg-warn`
- `curve-one-coin-withdraw-penalty-warn`
