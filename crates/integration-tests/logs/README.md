# Harness run logs (프로토콜별)

v3 디코드 하니스 실행 결과를 **프로토콜별로 기록**하는 곳. `README.md` §6 "Log → Gap → Develop 루프" 의 Step 1 산출물이 여기 쌓인다 — 다음 실행/에이전트가 직전 결과와 diff 해 진행도(고친 gap, 새 gap)를 추적한다.

## 레이아웃

```
logs/
├─ README.md                         ← 이 파일 (포맷 + 인덱스)
└─ <protocol>/
   └─ YYYY-MM-DD-<source>.json       ← 한 번의 실행 기록 (날짜+소스별)
```

`<source>` = `synthetic` | `etherscan` | `dune` | `corpus` | `mixed`.

## 추적 정책 (gitignore)

- per-run 로그 파일(`<protocol>/*.json`)은 **gitignore — 로컬 scratch** (clone 에 안 따라옴). 이 `README.md` 만 추적된다 (포맷 가이드 + 아래 큐레이션 인덱스/findings).
- 로그는 `.json` 으로 쓴다: 머신 판독(auto-improve 루프 파싱·diff) + 사람 판독(`summary` 필드) 둘 다. 로컬에서 직접 diff 로 실행 간 변화 확인.
- 의미 있는 스냅샷·findings 는 본 README 의 인덱스 표 + 라운드 결과에 **요약해 커밋**한다(원시 로그 대신).

## 기록 포맷 (한 실행 = 파일 1개)

```jsonc
{
  "protocol": "uniswap",
  "date": "2026-05-30",
  "source": "etherscan",                 // synthetic | etherscan | dune | corpus | mixed
  "scope": "어떤 컨트랙트/selector, 몇 건",
  "command": "재현 커맨드 (그대로 복붙되게)",
  "totals": { "total": 90, "pass": 50, "soft": 20, "hard": 20, "panic": 0 },
  "gaps": {
    "coverage_soft": [                   // selector 미등록 → manifest 추가로 해결 (registry)
      { "selector": "0x..", "name": "...", "contract": "...", "count": 9, "kind": "no_declarative_v3_mapper" }
    ],
    "decoder_hard": [                     // 등록됐지만 디코드 실패 → decoder/manifest 수정
      { "selector": "0x..", "name": "...", "count": 10, "kind": "build_multicall_failed", "detail": "근본원인" }
    ]
  },
  "sample_failing_txs": ["0x..", "0x.."], // 재현용 대표 해시 (replay/조사)
  "summary": "사람이 읽는 한 단락 요약 — 무엇이/왜",
  "baseline": { "corpus": "23/23", "synthetic": "uniswap 9200/0 hard" }  // 동시 sanity (선택)
}
```

## 생성 방법

```bash
# 합성 fuzz (전 프로토콜 한 번에 — report 안에 per_protocol 분리됨)
cargo run -p policy-engine-integration-tests --bin v3-harness -- \
  fuzz --iterations 5000 --json logs/_synthetic/2026-05-30-synthetic.json

# 실거래 (프로토콜별): Etherscan/Dune pull → import → corpus 실행 → 결과를 위 포맷으로 기록.
#   README.md §3.B/§3.C 로 corpus 만들고, `corpus` 출력의 got 분포를 집계해 gaps 에 정리.
```

> 모든 per-run 로그는 gitignore (로컬 only). 커밋 대상은 본 README 의 인덱스/findings 요약뿐.

## 인덱스 (프로토콜별 최신)

| protocol | 최신 로그 | total | pass | soft(커버리지) | hard(디코더) |
|---|---|---|---|---|---|
| uniswap | 2026-06-01 Etherscan keyed covered-selector scratch + Dune probe | 4,471 | 4,471 | 0 | 0 |
| aave | `aave/2026-05-30-etherscan.json` | 300 | * | L2Pool packed (31% Arb) | 0 |
| balancer | `balancer/2026-06-01-real-tx-research.json` | 535 | 155 | 360 (batchSwap/join/exit + V3 liquidity/initialize) | 20 (`permitBatchAndCall` overcoverage corrected to soft exclude) |
| hyperliquid | `hyperliquid/2026-05-30-etherscan.json` | 160 | 5+ | 2 (infra, out-of-scope) | 0 |
| layerzero | `layerzero/2026-05-30-etherscan.json` | ~640 | * | ZRO ERC20(no token file) + claim overloads | 0 |
| uniswapx | `uniswapx/2026-05-30-etherscan.json` | 160 | 0 | 3 (reactor execute → Tier B) | 0 |
| registry-v2 | `registry-v2/2026-05-31-progress.json` | mixed | mixed | Scenario B/C/D kickoff + skipped ActionBody list | 0 fresh hard |

**최신 synthetic sweep:** `_synthetic/2026-05-31-synthetic.json` — 2,410,000 probes, 2,183,381 pass, 226,619 soft, **0 fail / 0 panic**. `soft`는 synthesis/model-limit 분류이며, fresh hard decoder regression은 없었다.

새 로그를 추가하면 이 표 한 줄을 갱신한다.

## 2026-06-01 Balancer V2/V3 실거래 재검증 — 정정 결과

**외부 데이터 lane 확인**: Claude Code headless(`claude -p`) 2차 검토 + Etherscan v2 txlist + Dune MCP를 사용했다. Etherscan은 Free API에서 mainnet/arbitrum/polygon V2 Vault와 mainnet V3 Router를 각 10,000 tx까지 가져왔고, Base/OP/BNB/Avalanche Etherscan pulls는 Free API chain coverage 제한으로 실패해 Dune Base query로 보정했다.

**selector 분포 요약**:
- V2 Vault mainnet: `batchSwap 0x945bcec9` 3,827; `swap 0x52bbbe29` 2,317; `exitPool 0x8bdb3913` 2,128; `joinPool 0xb95cac28` 689; `setRelayerApproval 0xfa6e671d` 377.
- V2 Vault arbitrum: `manageUserBalance 0x5c38449e` 4,078; `exitPool` 2,561; `swap` 1,793; `batchSwap` 828; `joinPool` 324; `setRelayerApproval` 303.
- V2 Vault polygon: `batchSwap` 9,001; `exitPool` 434; `swap` 304; `joinPool` 143; `setRelayerApproval` 25.
- V3 Router mainnet: `permitBatchAndCall 0x19c6989f` 9,097; `swapSingleTokenExactIn 0x750283bc` 283; `initialize 0x026b3d95` 233; `removeLiquidityProportional 0x51682750` 228; `addLiquidityProportional 0x724dba33` 87; `addLiquidityUnbalanced 0xc08bc851` 56; `swapSingleTokenExactOut 0x94e86ef8` 15.

**대량 재생 결과**: `/private/tmp/balancer-bulk-replay/balancer/corpus.json` 에서 535건을 샘플링해 `v3-harness corpus` 실행. 초기 기대값 기준 `515/535` matched; 20개 mismatch는 전부 `permitBatchAndCall 0x19c6989f`를 pass로 기대했지만 `build_multicall_failed: no inner leg resolved to an installed mapper`로 실패했다. 대표 tx `0x4a9a6c961047086d6cee5cc227385447388619a1f3019ac9efea3600767051f3` 는 내부 child가 deferred liquidity selector(`0x724dba33`)라서 plain multicall 재귀만으로는 충실하게 표현할 수 없다.

**처치**: `balancer/v3/router-permit-batch-and-call@1.0.0` 매니페스트 제거, mainnet/base coverage에서 `permitBatchAndCall`을 `exclude`로 정정, 같은 real tx를 Balancer golden corpus에 `expect:error`로 pin. 현재 Balancer V3 Router covered scope는 direct single-token swaps + plain `multicall(bytes[])` wrapper다. `permitBatchAndCall`은 child liquidity mapping + Permit2 batch side-effect modeling이 들어오기 전까지 fail-closed gap으로 둔다.

## 2026-06-01 Uniswap P2 real-tx 보강

**요약:** earlier green state was not enough for P2 real-tx completeness: it used
official deployment docs, npm ABIs, Sourcify/local ABI cache, and the existing
golden corpus, but did not run a fresh Etherscan+Dune real-tx lane. Re-run now:

- **Etherscan MCP:** mainnet `txlist` samples for V2 Router02 and SwapRouter02,
  then public RPC hydration to exact `to/value/input`. Initial scratch replay:
  `cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus --root /tmp/scopeball-uniswap-etherscan`
  -> `10/10` pass.
- **Etherscan API v2 keyed:** loaded `ETHERSCAN_API_KEY` from the original
  integration-test `.env` without committing or printing the secret. Queried the
  33 Uniswap cover contracts from `_deployments.json` at offset 300. Raw
  expected-pass scratch found 4,792 unique txs across mainnet+Arbitrum:
  `4,692/4,792` matched. The 100 misses classified to 99 no-index/excluded
  selectors (mostly Permit2 `transferFrom` and low-level V4 PoolManager
  flash-accounting surface) plus 1 failed malformed on-chain tx. Filtering to
  successful txs whose callkey exists in `registryV2/index/by-callkey` produced
  `/tmp/scopeball-uniswap-etherscan-keyed-covered`, replayed at `4,471/4,471`
  pass. Optimism/Base Etherscan API calls still returned plan-limit
  `Free API access is not supported for this chain`, so L2 coverage there came
  from the Dune+RPC lane below.
- **Dune MCP:** `dex.trades` partitioned query `7625591`, filtered to covered
  Uniswap router/manager `tx_to` across Ethereum/Base/Arbitrum/Optimism. Cost:
  `0.41` credit, 40 rows, 39 unique hydrated calldata rows. Scratch replay:
  `cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus --root /tmp/scopeball-uniswap-realtx`
  -> `39/39` pass.
- **Committed representative rows:** 12 Dune-derived real txs were appended to
  `data/golden/v3-decode/uniswap/corpus.json`, covering Optimism V2, Arbitrum
  SwapRouter02 exactInput path, Base SwapRouter02 multicall/FoT/UniversalRouter,
  Ethereum UniversalRouter 2.1.1/v2, and V3/V4 position NFT transfer paths.
- **Claude Code 2nd-opinion:** headless audit ran candidate-only with hooks
  disabled. Valid findings left as follow-up: Universal Router opcode branch
  breadth, direct Permit2 transfer/signature-transfer surfaces, more V3
  `exactOutput`/alt multicall real txs, and deeper V4 PositionManager/PoolManager
  action-stream coverage.

Remaining known gaps are corpus breadth gaps, not fresh hard decoder failures in
the sampled real-tx lane.

## 2026-05-30 커버리지 확장 라운드 — 처치 결과

진단(위 6 로그) → 어댑터 추가. corpus 의 해당 `expect:error` 는 `expect:pass` 로 flip(회귀 baseline).

**처치 완료 (declarative, commit):**
- `uniswap` V2 fee-on-transfer swap 3종 (0xb6f9de95 / 0x791ac947 / 0x5c11d795) — 기존 V2 swap manifest 복제. 4 chain. commit `4bad8bb`.
- `layerzero` ZRO erc20 (transfer/approve/transferFrom) — `tokens/<chain>/zro.json`(1/10/8453/42161) 추가로 erc20 auto-enumerate. commit `5e9b842`.

**Tier B 로 재분류 (declarative 불가 — defer, `expect:error` baseline 유지):**
- `balancer` joinPool/exitPool/batchSwap — `assets[]`/`amounts[]` 가 **동적 길이 배열**(tuple 내부). single_emit field-path 는 정적 인덱스만 → array 전략/Tier B 필요. (단순 swap 만 declarative 로 커버됨.)
- `aave` L2Pool packed-args (withdraw/repay/setCollateral `bytes32`) — reserve **index**(주소 아님) + bit-field 패킹 → bit-slice 전략/onchain reserve→address 필요. Arbitrum 31%.
- `uniswap` Permit2 named-tuple (permitSingle.details.token / direct permit 0x2b67b570 + UR-embedded) — **정밀 root-cause(2026-05-30):** manifest 의 dotted path `$args.permitSingle.details.token` 는 eval.rs 가 미지원(`$.args.x.y` 불가, eval.rs:469-471). chained-numeric `$args.permitSingle[0][0]` 로 바꾸면 path 는 resolve 되나, 그 다음 nested-tuple element coercion 버그로 막힘 — abi_fragment 가 bare `"type":"tuple"`+`components` 라 nested 접근 시 per-component abi type 이 유실되어 uint48(expiration) 가 JSON **string** 으로 나옴 → `expires_at` u64 역직렬화 실패(`build_action_body_failed: invalid type string, expected u64`, eval.rs:65-68 documented limitation). **Fix = eval.rs 가 nested tuple `[i][j]` 접근 시 component 타입을 threading 해 uint≤64 를 number 로 coerce.** (manifest-only 불가 확인 — 시도 후 revert, baseline 유지.)
- `uniswap` UR V4 nested action-stream(pool_id/currencyIn, 0x3593564c) — opcode_stream/action_builder. 최고가치 hard.
- `uniswapx` reactor execute((bytes,bytes)) — SignedOrder inner-bytes 를 reactor family 별 재디코드 → Tier B.
- `aave` flashLoan/flashLoanSimple — declarative 가능하나 `flash_loan` lending action 스키마 추가 선행 필요. 샘플 트래픽 0.
- `layerzero` ClaimContract overload 3종 — declarative 가능하나 시그니처 미확정(4byte 부재, Sourcify 확인 필요). ~11 tx.

## 2026-05-30 Uniswap 전수 해결 라운드 — 처치 결과

**오진 정정**: 위 라운드가 "stale index (build-index 재실행 필요)"로 본 uniswap soft 갭은 실제로 **registry→registryV2 마이그레이션 누락**이었다. repo 에 트리가 둘 다 git-tracked: `registry/`(레거시 schema v2, uniswap 92 manifest) + `registryV2/`(현행 v3, 하니스가 로드하는 유일 트리, `crates/integration-tests/src/harness/adapters.rs:142`). swap-router-02/pool-manager/position-manager 등은 `registry/` 에만 있었음 → schema v2→v3 변환(type adapter_function→adapter_action, schema_version 2→3) 후 포팅 필요. "재빌드"가 아니라 "포팅".

**또 하나의 정정**: production 디코더는 `crates/policy-engine-wasm/src/declarative_exports.rs` 이고 mappers crate(`eval.rs`/`single_emit.rs`)를 LIVE 재사용한다. strategy dispatch arm 추가/타입 fix 는 이 파일에서.

**처치 완료 (commit):**
- **multicall_recurse** (`feat(b3) 7f89b71`) — v3 경로에 strategy arm 자체가 없었음(unsupported_strategy + install allow-list drop). manifest 4 포팅(v3-nfpm/multicall, v4-position-manager/multicall, swap-router-02/multicall + multicall-bytes) + `declarative_exports.rs` 에 `build_multicall_recurse_body`(leg 마다 public entry 재진입→ActionBody::Multicall, 미매핑 helper leg skip) + install SUPPORTED_STRATEGIES 5개로. NFPM/PosM(nested modifyLiquidities) multicall pass.
- **Permit2 named-tuple** (`feat(b3) 3f2a9c80`) — (1) dotted path 가 positional array 에 named 접근 실패 → numeric index `$args.permitSingle[0][0]` (Balancer/V2 `[N]` 선례). (2) sig_deadline/expires_at 가 `Time`(u64)인데 uint256 sigDeadline 은 decimal string 으로 렌더 → `time_from_str_or_num` 추가(number|string→Time, typed-data sign flow 도 커버). permitSingle/permitBatch pass.
- **SwapRouter02 v3 swap** (`feat(b3) 3f2a9c80`) — exactInputSingle(0x04e45aaf)/exactOutputSingle(0x5023b4df) v2→v3 포팅. 단일 tuple flatten → `$args.tokenIn` 평탄화. live_inputs 4필드(route/expected_amount_out/price_impact_bp/gas_estimate) 완성(SwapLiveInputs 필수). SR02 multicall(inner exactInputSingle) 도 pass.
- **per-chain UR 주소** (`feat(b3)`) — Arbitrum/Optimism UR 추가. V1_2(no-V4)→execute-v1 계열(Arb 0x5E32/OP 0xCb13), V4-aware→execute-v2 계열(Arb 0xa51a/OP 0x851116). 1차출처: github Uniswap/universal-router deploy-addresses + docs.uniswap.org v4 deployments.

**defer (문서화):**
- **UR V4 nested action-stream**(0x10 V4_SWAP, `docs(b3)`) — 두 비-contained 갭: (1) inner SWAP_EXACT_IN/OUT(0x07/0x09) inputs_abi 의 PathKey[] nested-tuple-array 가 `decode_inputs_abi_tuple` 의 `Function::parse` synthetic-signature 를 막아 inner action 전체 Null → `$inputs.currencyIn` 누락. fix=`DynSolType::parse` per-component(shared helper, 전 opcode-stream manifest 회귀). (2) SWAP_*_SINGLE(0x06/0x08) pool_id=keccak256(PoolKey) nested 주입(`maybe_inject_v4_pool_id` declarative_exports.rs:1699, 현재 modifyLiquidities MINT 만). corpus 0x36F4F2FF/0xEB9775B6/0x321da671 baseline 유지.
- **PoolManager initialize**(0x6276cbbe) — ("amm","initialize_pool") v3 single_emit action 변종 부재(amm domain = swap/add·remove_liquidity/collect_fees/sign·cancel_intent_order). 변종 추가 선행 필요.
- **PositionManager erc721 approve**(0x095ea7b3, ~1 tx) — tokens erc721 enumerate curation 영향, 저트래픽이라 보류.
- **SR02 exactInput/exactOutput**(packed path) — v3 body 가 `$derived.v3_path_first_token`(deriver) 필요, 오프라인 하니스 미충족.
