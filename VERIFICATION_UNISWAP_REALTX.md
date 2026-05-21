# VERIFICATION — 실제 Uniswap Tx 기반 declarative 경로 검증 + finding 수정

> Phase 7B "Uniswap 완벽 지원" 후속. 검증 측정일 2026-05-21, finding 수정 동일.
> 실제 on-chain Uniswap 트랜잭션을 ScopeBall 의 production verdict 경로 (declarative / Tier A) 에 통과시켜 L0~L4 5단계로 검증.
> 하네스 `crates/integration-tests/tests/uniswap_real_tx.rs` · corpus `crates/integration-tests/data/golden/uniswap-real-tx/corpus.json`.

## 결론 — 42 tx 검증 → finding 5건 도출, 전부 수정 완료

합성(synthetic) 단위 테스트는 abi_fragment 와 자기일관적이라 실제 calldata 의 edge 를 놓친다. 실제 tx 검증이 declarative 경로의 finding 5건을 도출했다 — F1(registry 에 등재됐으나 작동 불가한 죽은 manifest)은 검증 단계에서, F2~F5 는 후속 작업에서 수정 완료. 전부 fail-open 아님 — envelope 는 생성되나 권한 표면이 부정확/불완전했던 결함.

| severity | finding | 상태 |
|---|---|---|
| **P1** | F1 — V3 NFPM `collect` 번들 2겹 manifest 버그 | **수정 완료** (`61a0820`, 검증 단계) |
| P3 | F2 — native currency sentinel (`0x0`) → `erc20` 오표기 | **수정 완료** (`7cd6c8a`) |
| P3 | F3 — UR opcode recipient sentinel (`0x..01/02`) 미해석 | **수정 완료** (`da7dd67`) |
| P2 | F4 — V2 ETH-input 함수 native input `amount.value` 누락 | **수정 완료** (`e7d5341`) |
| P3 | F5 — V4 `modifyLiquidities` `decrease_liquidity.outputTokens` 빈 배열 | **수정 완료** (`668f14d`) |

## 검증 매트릭스 — L0~L4

production 라우팅 재현: 실제 tx `(chain_id, to, calldata)` → callkey → `registry/index/by-callkey/<callkey>.json`.

| 단계 | 검증 | 실패 의미 |
|---|---|---|
| L0 라우팅 | byCallKey 인덱스에 callkey 존재 | declarative MISS — static fallback |
| L1 디코딩 | `decode_with_json_abi(bundle.abi, calldata)` 성공 | abi_fragment 불일치 |
| L2 매핑 | `DeclarativeMapper::map` envelope ≥ 1 | silent drop / fault |
| L3 정합성 | action 종류 + 핵심 필드 + sentinel 오표기 없음 | 권한 표면 누락/오표기 |
| L4 lowering | `policy_request_from_envelope` → `Some` | fail-open |

## per-family 판정 (collect 수정 후)

| family | tx | 결과 |
|---|---|---|
| v2 | 7 | 7 pass |
| v3-swap-router (SR01) | 3 | 3 pass |
| swap-router-02 | 7 | 7 pass |
| v3-nfpm | 6 | 6 pass (collect 2건 수정 후 통과) |
| universal-router | 5 | 5 pass |
| permit2 | 8 | 8 pass |
| v4 | 3 | 3 pass (initialize / modifyLiquidities / multicall) |
| excluded | 3 | 3 MISS — `unlock`·`lockdown`·`invalidateNonces` 전부 의도대로 미커버 |
| **계** | **42** | **39 full pass (L0~L4) / 3 정상 MISS** |

## Findings

### [P1 — 수정 완료] F1 — V3 NFPM `collect` 번들 2겹 manifest 버그

- **위치**: `registry/manifests/uniswap/v3/collect@1.0.0.json` + per-chain split 9개 = 10 manifest. 발견 tx: `0x4A3895FC...` (단독 collect), `0x9FD307BD...` (`multicall(decreaseLiquidity + collect)`).
- **버그 1 — emit 경로 불일치**: emit 이 `$.args.params.tokenId`·`$.args.params.recipient` (nested) 를 읽음. 그러나 `decode_with_json_abi` → `convert_legacy_call` 이 단일 tuple 인자 `params` 를 **flatten** — 실제 경로는 `$.args.tokenId`·`$.args.recipient` (top-level). → `MapperError::MissingArgument`.
- **버그 2 — emit category 오류**: emit `category:"dex"`. 그러나 `claim_rewards` action 은 `misc` category. `("dex","claim_rewards")` arm 부재 → `MapperError::Unsupported`. (버그 1 을 고치자 이 단계가 드러남 — 2겹.)
- **수정** (`61a0820`): 10 manifest emit 의 (1) `$.args.params.` → `$.args.`, (2) `category:"dex"` → `"misc"`. `npm run build` 로 collect 13 callkey 인덱스 갱신. 재검증 — collect 2 tx 모두 L0~L4 통과.

### [P3 — 수정 완료] F2 — native currency sentinel (`0x0`) → `erc20` 오표기

- **결함**: UR `TRANSFER` (`0xC1E0...` — token `0x0`), V4 `initialize` (`0xE051...` — `PoolKey.currency0` `0x0`) 의 토큰이 native ETH 인데 declarative manifest 가 `kind` 를 `{"literal":"erc20"}` 로 하드코딩 — envelope 가 `{kind:erc20, address:0x000…000}` 로 오표기. UR `Constants.ETH` / v4-core `Currency` 컨벤션상 token/currency `0x0` = native.
- **수정** (`7cd6c8a`): `single_emit.rs` `read_asset`/`read_asset_inline` (모든 declarative asset 의 단일 chokepoint) 에 `normalize_native_sentinel` — `erc20 @ 0x0 → native`. manifest 0건 수정. static UR 경로의 `protocols/universal_router/common.rs::token_asset_ref` 가 이미 같은 `0x0→native` 규칙 보유 — declarative 만 누락분이었다.

### [P3 — 수정 완료] F3 — UR opcode recipient sentinel 미해석

- **결함**: UR `execute` 의 opcode recipient 가 `$.args.recipient` 를 raw 추출 — recipient 가 `0x..01`(`MSG_SENDER`) / `0x..02`(`ADDRESS_THIS`, v4-periphery `ActionConstants`) sentinel 일 때 envelope 가 sentinel literal 을 그대로 표기 (검증 특정: `WRAP_ETH` `0x6D0A...`). static UR mapper 의 `map_recipient` 가 이미 해석하나 declarative 경로만 누락.
- **수정** (`da7dd67`): 신규 DSL builtin `BuiltinFn::MapRecipient` — Tier B `common::map_recipient` (`0x..01→ctx.from`, `0x..02→ctx.to`) 를 wrap. `common` 모듈 `pub(crate)` 노출 + `bundle-schema.ts` allowlist 등록. UR `execute`/`execute-no-deadline` 의 opcode recipient 12종 전부를 `{"fn":"map_recipient"}` 로 wrap (검증 특정 `WRAP_ETH` 외 swap/transfer 도 동일 결함 → 일괄 수정).

### [P2 — 수정 완료] F4 — V2 ETH-input 함수 native input `amount.value` 누락

- **결함**: `swapExactETHForTokens` (`0x9658...`), `addLiquidityETH` (`0xC990...`) 등의 native input envelope 가 `amount:{kind}` — `value` 누락. native input 수량 = `msg.value` 로 calldata 인자에 없어 bundle emit 에 해당 필드 부재. input **수량**만 비가시.
- **수정** (`e7d5341`): V2 ETH-input manifest 28개 (`swapExactETHForTokens`/`swapETHForExactTokens`/`*SupportingFeeOnTransferTokens`/`addLiquidityETH` × 7 per-chain) emit 에 `amount.value ← $.tx.value_wei` 추가. `eval.rs` 가 `$.tx.value_wei` 를 이미 지원, `aerodrome/v2` 가 동일 패턴 보유 (in-repo 선례). `npm run build` 로 callkey 36개 갱신.

### [P3 — 수정 완료] F5 — V4 `modifyLiquidities` `decrease_liquidity.outputTokens` 빈 배열

- **결함**: V4 PositionManager `modifyLiquidities` (`0x2B0A...`) 의 `decrease_liquidity` envelope `outputTokens:[]`. V4 action stream 의 `TAKE`/`TAKE_PAIR` opcode (출금 토큰·목적지)가 dispatcher 에서 skip — `AUDIT_UNISWAP_PHASE7B.md` §P3 이 정적 분석기 한계로 "의도된 scope" 판정했던 항목.
- **수정** (`668f14d`): `opcode_stream.rs` `dispatch_v4_pm_steps` 2-pass 재구성 — TAKE-family opcode(`0x0e TAKE`/`0x0f TAKE_ALL`/`0x10 TAKE_PORTION`/`0x11 TAKE_PAIR`/`0x14 SWEEP`)를 `V4TakeOutput` 으로 수집 → `decrease_liquidity` envelope 의 `outputs`+`recipient` 에 attach. currency 는 `token_asset_ref` (F2 정합), recipient 는 `map_recipient` (F3 정합) 재사용. V4 flash-accounting 상 per-position 정적 귀속 불가 — stream-level output set 부여 (의도적 근사).

## 수정 검증 — 영구 회귀 가드

`uniswap_real_tx.rs` 에 `fixed_findings_f2_f5_regression` 테스트 추가 — finding 4건의 L3 정정을 영구 lock:
- **F2** — 42 tx 전체 envelope 에 `erc20 @ 0x0` 자산 0건 (corpus-wide invariant)
- **F3** — 42 tx 전체 envelope recipient 에 `0x..01`/`0x..02` sentinel 0건 (corpus-wide invariant)
- **F4** — `swapExactETHForTokens`/`addLiquidityETH` 의 native input `amount.value` 채워짐
- **F5** — `modifyLiquidities` 의 `decrease_liquidity.outputTokens` 비어있지 않음

`corpus_verification` (L0/L1/L2/L4 strict) 는 무변경 — 이후 mapper/manifest 변경이 declarative 경로를 회귀시키면 CI 에서 즉시 검출.

## 검증 수치

- corpus: **42 tx** — mainnet 36 + L2 Base 3 + Arbitrum 3, 8 family. 전부 실제 on-chain tx (tx_hash + explorer_url).
- 검증 결과: **39 full pass (L0~L4) / 0 partial / 3 정상 MISS**.
- `cargo test --workspace`: **882 passed / 0 failed / 6 ignored** — 검증 단계 baseline 881 + 신규 regression 가드 1 (F1~F5 수정 회귀 0).
- `cargo test … uniswap_real_tx`: **3 passed** — `harness_self_check` + `corpus_verification` + `fixed_findings_f2_f5_regression`.
- `vitest` (browser-extension): **356 passed** · `typecheck` 0.
- WASM `policy_engine_wasm_bg.wasm`: **5.57 MiB** (5,843,458 bytes) — plan 6 MiB 예산 내 (수정 전 5.54).
- registry: F4 callkey 36개 + F3 UR callkey 754개 갱신, collision 0.

## 출처

- corpus tx: Dune query 7551293 (mainnet `ethereum.transactions`) · 7551379 (`base`/`arbitrum.transactions`). 각 tx 의 txhash + explorer_url 은 `corpus.json` 에 명시.
- 수정 commit: F1 `61a0820` · F4 `e7d5341` · F2 `7cd6c8a` · F3 `da7dd67` · F5 `668f14d`.
- 코드 1차 확인: `single_emit.rs` (`read_asset`·`read_asset_inline`·`build_field_tree`) · `eval.rs` (`evaluate_transform`·`evaluate_tx_field`) · `opcode_stream.rs` (`dispatch_v4_pm_steps`) · `protocols/universal_router/common.rs` (`map_recipient`·`token_asset_ref`) · `abi-resolver/.../v4_router.rs` (TAKE-family 시그니처).
- UR/V4 컨벤션: Uniswap v4-periphery `ActionConstants.sol` (`MSG_SENDER`/`ADDRESS_THIS`) · v4-core `Currency.sol` (native `0x0`) · v4-periphery `Actions.sol` (TAKE/TAKE_PAIR/SWEEP) · UR `Constants.sol` (`ETH = address(0)`).
- 계획서: `~/.claude-web3/plans/scopeball-uniswap-zany-neumann.md`.
