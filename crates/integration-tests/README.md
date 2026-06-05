# v3 `ActionBody[]` 디코드 하니스 (`policy-engine-integration-tests`)

raw Tx (calldata) / EIP-712 typed-data 를 **production 디코더**로 돌려 `ActionBody[]` 가 구조적으로 정확히 나오는지 자동 검증하는 하니스. 수동 e2e(Uniswap 사이트에서 Tx 만들기 → 익스텐션 catch → SW DevTools console 확인)를 **프로그래밍적 fuzzing + 실거래 replay** 로 대체한다.

> **이 README 는 사람 + 에이전트(Claude Code) 둘 다를 위한 runbook 이다.** §6 "Log → Gap → Develop 루프" 가 핵심 — 다른 팀원의 Claude Code 가 이 파일만 읽고 (1) 3-source 입력 생성 → (2) 하니스 실행 + 로그 수집 → (3) 부족한 부분(gap) 자동 분류 → (4) 어디를 고칠지 판단 → (5) 회귀로 닫기 를 **자율 수행**할 수 있도록 작성했다.
>
> **범위 — framework 진입점 맵**: 이 README 는 **P3-P4(decode 테스트 + fix 루프)** 담당. 새 프로토콜을 **처음부터 온보딩(P0 research → P1 Tier 1/2/3 authoring)** 하려면 먼저 **`PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md`**(프로토콜 독립 completion model + semantic oracle contract)를 읽고, 세부 실행은 **`PROTOCOL_ONBOARDING_AND_TESTING.md`**(4-phase·3-tier·worked example 전체 spine), Tier 3 ActionBody 확장은 **`ACTIONBODY_EXTENSION_GUIDE.md`**, surface 전수성 gate(어댑터 누락 차단)는 **`registryV2/surface/README.md`**(gate 데이터 옆 — `npm run check:surface`)를 본다. **인스트럭션 문서는 전부 tracked 파일이다.**

---

## 0. TL;DR

```bash
cd <repo-root>   # scopeball-registry-v2

# (1) deterministic gate — CI 회귀. install 청결 + 합성 퍼징(고정 seed) + 실거래 corpus.
cargo test -p policy-engine-integration-tests --test v3_decode_harness -- --nocapture

# (2) soak fuzz + JSON 리포트 — gap 탐색.
cargo run -p policy-engine-integration-tests --bin v3-harness -- fuzz --iterations 5000 --json /tmp/v3-report.json

# (3) 커버리지(미커버 목록) + 실거래 corpus replay.
cargo run -p policy-engine-integration-tests --bin v3-harness -- coverage
cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus
cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus --filter <protocol> --require-expect-body
```

네트워크/브라우저/WASM 런타임/GCS **불필요**. 전부 offline + deterministic.

---

## 1. What & Why

- **무엇을 검증하나** — `(chain_id, to, selector, calldata, value)` 또는 EIP-712 typed-data 를 입력하면 디코더가 `ActionBody[]` (정규화된 intent 배열)를 만든다. 하니스는 그 출력이 **구조적으로 정확한지**만 본다. rpc-server 가 미완성이라 `live_inputs.value`(실측 잔액/금액)는 비어 있어도 되고, **ActionBody 의 shape · domain · 디코드 성공 여부**만 검증 대상이다.
- **production 디코더를 plain Rust 로 직접 호출** — `policy_engine_wasm::{declarative_install_v3_json, declarative_route_request_v3_json, declarative_route_typed_data_v3_json}` 는 `pub fn(String) -> String`. `#[wasm_bindgen]` 는 host 타겟에서 inert 이므로 **브라우저에 실리는 것과 동일한 소스**가 그대로 cargo test 에서 돈다.
- **로컬 어댑터** — `registryV2/index/by-callkey/*.json` + `by-typed-data/*.json` 를 읽어 설치한다. GCS Cloud Run 미접촉. (manifest 를 고쳤으면 먼저 `cd registryV2 && npx tsx scripts/build-index.ts` 로 index 재빌드.)

---

## 2. Architecture

```
입력 (3 source)                     엔진 (src/harness/)                     검증
──────────────                     ───────────────────                     ────
A. synthetic fuzz   ─┐             adapters  : index 로드 + install(dedup)
   (머신 생성 + 손수)  │             prng      : SplitMix64 (replayable)
B. Etherscan 실거래  ─┼─ calldata ─► encode    : abi → DynSolValue → calldata ─► route ─► oracle ─► report
C. Dune 실거래       ─┘  / td        fuzz/*    : 전략별 합성 (4종)              │              (layered)   (히스토그램
                                    corpus    : 실거래 JSON replay            │                          + repro)
                                    route     : declarative_*_v3_json 호출 ◄──┘
```

- **2 front-end** — `tests/v3_decode_harness.rs` (deterministic CI gate = **4 structural + protocol 별 field-level golden 다수**; 총수는 늘어남 → 측정 `grep "test result"`) + `src/bin/v3_harness.rs` (CLI, 무제한 fuzz + 리포트).
- **layered oracle** (`src/harness/oracle.rs`) — L1 envelope(`ok`) → L2 typed round-trip(`Vec<policy_transition::action::Action>` 역직렬화 = serde-shape 회귀 검출, 최강) → L3 domain validity(`VALID_DOMAINS`; 현재 목록은 oracle 코드에서 직접 확인) → L4 soft/hard error class.
- **⚠️ R1 (필독)** — WASM v3 install state 는 **thread-local**. install 과 route 는 **반드시 동일 OS 스레드**에서. 각 test fn 이 스스로 install 한다. 새 헬퍼를 만들 때 install→route 를 같은 함수 안에서 호출할 것.

---

## 3. 3 Input Sources

세 소스 모두 결국 `route_calldata`/`route_typed_data` → oracle 로 수렴한다. 실거래(B/C)와 손수 만든 edge(A)는 **동일한 source-agnostic corpus 포맷**(§5)으로 들어오며, 출처는 디렉토리로만 구분한다: `data/golden/v3-decode/<protocol>/corpus.json`.

### A. Synthetic fuzz (머신 생성 + 손수 edge)

**A-1. 머신 생성 (기본 fuzzing).** manifest 의 `abi_inputs` 를 읽어 ABI-aware 랜덤 값을 만든다. seed = `fnv1a64(callkey) ^ global_seed ^ iter` (position-stable, 재현 가능). 전략 4종을 모두 sweep:

| 전략 | 대상 | callkey 수 |
|---|---|---|
| `single_emit` | flat ABI args (대부분) | 측정: `v3-harness coverage` |
| `opcode_stream_dispatch` | UniversalRouter `(bytes commands, bytes[] inputs)` | (per-strategy 분포) |
| `array_emit` | Permit2 batch 등 배열 | ↑ |
| `tagged_dispatch` | HyperLiquid CoreWriter | ↑ |

> callkey 수는 manifest 가 늘수록 drift 한다 — 하드코딩 대신 `v3-harness coverage` 로 측정(strategy 별 분포 출력).

각 callkey 의 첫 `EDGE_ITERS=4` 회는 boundary 값(0 / max / empty / single-element)을 주입한다.

```bash
# 전 surface fuzz. --iterations = callkey 당 반복 횟수.
cargo run -p policy-engine-integration-tests --bin v3-harness -- fuzz --iterations 5000 --seed 42 --json /tmp/r.json

# 단일 single_emit 케이스 재현 (seed 고정 → 동일 calldata 출력).
cargo run -p policy-engine-integration-tests --bin v3-harness -- replay \
  --callkey 1__0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48__0x095ea7b3 --seed 42
```

**A-2. 손수 조립한 edge case.** corpus 는 source-agnostic 이므로 — **랜덤 sweep 이 deterministic 하게 맞히기 어려운 경계 입력**(infinite-allowance, zero-amount, truncated calldata 등)은 손으로 calldata 를 조립해 `data/golden/v3-decode/_edge-cases/corpus.json` 에 `expect` 와 함께 적어둔다. 실제 예시가 그 파일에 있다(max-uint approve = `pass`, zero-amount transfer = `pass`, 인자 없는 approve = `error/decode_failed`). 추가 절차:

1. 노릴 callkey 의 selector/주소를 `coverage` 또는 `registryV2/index/by-callkey/` 에서 확인.
2. calldata 를 손으로 만든다 (`selector + abi.encode(args)`).
3. **먼저 probe** — 임시 파일에 `expect:"pass"` 로 넣고 `v3-harness corpus --root /tmp/probe` 실행 → 실제 동작(`got=...`)을 관찰. Landing 전에는 `v3-harness corpus --filter <protocol> --require-expect-body` 로 해당 프로토콜 pass corpus 가 field-level semantic assertion 을 갖는지 확인한다.
4. 관찰된 결과로 `expect`/`expect_domain`/`expect_error` 를 정확히 pin → 최종 파일에 기록.

### B. Etherscan (실거래)

실제 사용자 Tx 를 explorer 에서 받아 corpus 로 변환한다.

```bash
# 키 설정 — .env 에 보관(커밋 안 됨). 최초 1회: 샘플 복사 후 키 채우기.
cp crates/integration-tests/.env.sample crates/integration-tests/.env   # 그리고 ETHERSCAN_API_KEY 채우기
set -a; source crates/integration-tests/.env; set +a                    # 셸에 로드
# (키 발급: https://etherscan.io/myapikey — v2 는 키 1개로 전 체인)

# (권장) 주소별 최근 Tx 목록 — txlist 는 value 를 10진수로 준다.
curl -s "https://api.etherscan.io/v2/api?chainid=1&module=account&action=txlist\
&address=0x7a250d5630b4cf539739df2c5dacb4c659f2488d&startblock=0&endblock=99999999&page=1&offset=50&sort=desc\
&apikey=$ETHERSCAN_API_KEY" > /tmp/es.json

# 또는 특정 해시 1건 (proxy — value 가 16진수로 옴, 아래 주의 참고).
curl -s "https://api.etherscan.io/v2/api?chainid=1&module=proxy&action=eth_getTransactionByHash\
&txhash=0x9658...&apikey=$ETHERSCAN_API_KEY" > /tmp/es_one.json

# corpus 로 변환 (parse-only, 네트워크 X).
cargo run -p policy-engine-integration-tests --bin v3-harness -- import-etherscan /tmp/es.json --chain 1 \
  --out crates/integration-tests/data/golden/v3-decode/uniswap-v2-router/corpus.json
```

- **어떤 주소를 노릴까** — `coverage` 의 surface(by-callkey) 에 있는 컨트랙트 주소. 미커버/저커버 전략부터.
- **변환이 인식하는 shape** — `txlist`(`{status,message,result:[...]}`) 와 `eth_getTransactionByHash`(`{result:{...}}`) 모두. 컬럼(`to`/`input`/`value`/`hash`)은 자동 매핑.
- **주의** — proxy(`eth_getTransactionByHash`)는 `value` 를 16진수(`0x0`)로 준다. corpus 의 `value` 는 **10진수 wei** 여야 하므로, proxy 경로를 쓰면 변환 후 `value` 를 10진수로 고쳐라. `txlist` 는 이미 10진수다(권장 이유).
- 변환 후 각 entry 에 `expect`/`expect_domain` 을 손으로 단다(§5). 기본값은 `"pass"`.

### C. Dune (실거래)

Dune 쿼리로 surface 기준 실거래를 대량 pull → 변환한다.

```sql
-- surface(by-callkey)의 (to, selector) 기준. 프로토콜별로 to 주소·selector 를 바꿔 사용.
SELECT  hash AS tx_hash,  "to",  value,  data,  1 AS chain_id
FROM    ethereum.transactions
WHERE   "to" = 0x7a250d5630b4cf539739df2c5dacb4c659f2488d        -- UniswapV2Router02
  AND   substring(data, 1, 4) = 0x38ed1739                       -- swapExactTokensForTokens
  AND   block_time > now() - interval '30' day
LIMIT   50
```

- Dune MCP 가 붙어 있으면 `mcp__dune__createAndExecuteQuery`(`performance:"free"`) → `mcp__dune__getExecutionResults` 로 실행하고 결과 JSON 을 파일로 저장. 아니면 Dune UI 의 "Export → JSON".
- **비용·할당** (community 2,500 credit/월; `mcp__dune__getUsage` 확인): 실측 pruned 쿼리(위처럼 **`block_time` 파티션 WHERE** + free 엔진) = **~0.007 credit/execution** → pinpoint 은 사실상 무제한. **단 partition 필터 빼면 풀스캔 → 수백 credit 즉사**. 프로토콜당 ~수 credit, tripwire 25. **소스별 하한·조건은 `PROTOCOL_ONBOARDING_AND_TESTING.md §5d`** (Dune = Base/OP·cross-chain·decoded 조건부, bulk 는 Etherscan).
- 변환:

```bash
cargo run -p policy-engine-integration-tests --bin v3-harness -- import-dune /tmp/dune.json --chain 1 \
  --out crates/integration-tests/data/golden/v3-decode/<protocol>/corpus.json
```

- 인식 shape — bare array `[...]`, `{rows:[...]}`, `{result:{rows:[...]}}`. 컬럼(`tx_hash`/`to_address`/`value`/`data`/`chain_id`) 자동 매핑. row 에 `chain_id` 가 있으면 그게 `--chain` 보다 우선.
- 변환 후 `expect`/`expect_domain` annotate.

> `import-dune` / `import-etherscan` / `import` 은 **완전히 동일한 변환**이다(소스는 wrapper shape 만 다름). 아무거나 써도 된다.

---

## 4. 빠른 시작 → 결과 읽기

`fuzz` 의 summary 출력(실제 형식):

```
total=85000 pass=76698 soft=8302 fail=0 panicked=0 skipped=8

per-protocol:
  aave           total=12000 pass=8257  soft=3743 fail=0   panic=0
  ...
domain histogram (unknown=16.4%):
  token        39600
  amm          13707
  unknown      12600
  ...
error histogram:
  build_action_body_failed                 4836
  opcode_synthesis_limited                 1466
  typed_data_synthesis_limited             2000
```

`--json` 으로 덤프하면 동일 데이터가 구조화되어 나온다(필드: `total/pass/soft/fail/panicked/skipped`, `domain_hist`, `error_hist`, `per_protocol`, `failures[]`). 각 신호의 의미는 §6 분류표 참조.

---

## 5. Corpus 포맷 (source-agnostic)

`data/golden/v3-decode/<protocol>/corpus.json`:

```jsonc
{
  "_comment": "출처 메모 (Dune query id / Etherscan / hand 등)",
  "transactions": [
    // calldata entry
    { "intent": "swapExactTokensForTokens",       // 사람용 라벨 (선택)
      "expect": "pass",                            // "pass" | "excluded" | "error"
      "expect_domain": "amm",                      // (선택) top-level body.domain
      "expect_body": [                             // (선택) field-level semantic assertions
        { "path": "$.data.actions[0].body.domain", "op": "equals", "value": "amm" }
      ],
      "expect_error": "decode_failed",             // (expect=="error" 일 때만)
      "tx_hash": "0x..",  "chain_id": 1,           // tx_hash 선택
      "rpc": { "params": [ { "to": "0x..", "value": "0", "data": "0x.." } ] } },

    // EIP-712 typed-data entry (rpc 대신)
    { "expect": "pass", "expect_domain": "token", "chain_id": 1,
      "typed_data": { "verifying_contract": "0x..", "primary_type": "PermitSingle",
                      "witness_type": null, "domain_name": "Permit2", "message": { } } }
  ]
}
```

`expect` 의미 (`src/harness/corpus.rs` `check_expect`):

| `expect` | 통과 조건 |
|---|---|
| `pass` | `ok:true` + (지정 시) top domain == `expect_domain` + (지정 시) `expect_body` 전부 통과 |
| `excluded` | `ok:true` + top domain == `unknown` (의도적 out-of-scope / off-chain 정상 출력의 양성 검증) |
| `error` | verdict 가 Fail/Soft + (지정 시) `error.kind` == `expect_error` |

`expect_body` 는 JSON Pointer(`/...`), `$` dotted/index path(`$.data.actions[0]`), recursive field path(`$..address`)를 지원한다. op 는 `exists`, `absent`, `equals`, `not_equals`, `one_of`, `contains`, `len`, `nonzero_address`, `hex_eq`, `u256_hex_eq`.

`value` 는 corpus 내부에서 **10진수 wei** 문자열이어야 한다. `v3-harness import-*` 는 Etherscan `eth_getTransactionByHash` 같은 `0x` quantity 입력을 10진수로 정규화한다. `corpus` 명령이 root 와 1-depth 하위 디렉토리의 모든 `corpus.json` 을 walk 한다.

---

## 6. ★ Log → Gap 자동 파악 → Develop 루프

**이 절이 "부족한 부분을 자동으로 파악하고 리팩토링/디벨롭" 의 구현이다.** 신규 자동화 코드는 없다 — 기존 `fuzz`/`coverage`/`corpus`/`replay` 출력을 아래 규칙으로 해석하면 닫힌 개선 루프가 된다.

### Step 1 — RUN (로그 수집)

```bash
cargo run -p policy-engine-integration-tests --bin v3-harness -- fuzz --iterations 5000 --json logs/_synthetic/$(date +%F)-synthetic.json
cargo run -p policy-engine-integration-tests --bin v3-harness -- coverage
cargo run -p policy-engine-integration-tests --bin v3-harness -- corpus
```

결과는 **`logs/<protocol>/`** 에 프로토콜별로 기록한다 (포맷·인덱스 = [`logs/README.md`](logs/README.md)). 실거래(Etherscan/Dune) 실행도 같은 포맷의 `logs/<protocol>/YYYY-MM-DD-<source>.json` 으로 남겨, 다음 실행/에이전트가 직전 로그와 diff 해 진행도(고친 gap, 새 gap)를 추적한다. 실제 예시(스냅샷): `logs/uniswap/2026-05-30-etherscan-fresh.json` — 그 배치의 실거래 일부가 V2 fee-on-transfer / NFPM multicall 미등록 + UR V4·Permit2 디코더 gap 으로 디코드 공백 분류됨 (건수는 당시 측정값; 현 수치는 재측정).

### Step 2 — CLASSIFY (report 신호 → gap 종류)

| report 신호 | gap 종류 | 우선순위 | 다음 행동 |
|---|---|---|---|
| `fail > 0` 또는 `panicked > 0` (→ `failures[]` 에 layer/seed/input) | **디코더 버그 (hard)** | 최우선 | Step 3 로 재현 → Step 4 진단 |
| `error_hist` 의 `typed_data_synthesis_limited` / `opcode_synthesis_limited` 多 | **하니스 합성 한계** (디코더 아님) | 중 | fuzzer 정밀화 OR 해당 케이스를 corpus(실서명/실거래)로 이전 |
| `error_hist` 의 `build_*_failed` 인데 `fail == 0` | **soft shape-artifact** (랜덤이 비현실적 shape 생성) | 하 | 보통 무시 가능. 잦으면 `fuzz/values.rs` 생성 규칙 보정 |
| `domain histogram` 의 `unknown%` 높음 | **mapper 커버리지 gap** | 중 | manifest mapper 보강. 단 off-chain 정상(HyperLiquid 등)이면 corpus `expect:excluded` 로 pin |
| `coverage` 의 미커버 목록(witness / V4 nested / sentinel) 비어있지 않음 | **completeness gap** | 중 | 실거래 corpus 항목 추가(합성 불가 영역) |
| `corpus` 에 `expect:error` entry 존재 | **확정 실거래 gap (최고가치)** | 높 | 실제 사용자 Tx 가 디코드 안 되는 것. 고치면 Step 4 에서 `expect:pass` 로 flip |

> **핵심 구분** — hard 여부는 오직 `fail`/`panicked`(=`failures[]`)로 판단한다. `error_hist` 에는 soft 로 강등된 kind 도 들어간다(위 표에서 `error_hist` 합 == `soft` 인 이유). `failures[]` 가 비어 있으면 디코더 버그는 0.

### Step 3 — REPRODUCE

- `single_emit` 실패 → `replay --callkey <K> --seed <S>` (`failures[]` 의 key/seed 사용) → raw route envelope 출력.
- corpus / typed-data / 기타 전략 → `corpus` 의 `MISS ...` 줄 또는 `fuzz` summary 의 `failures` 블록(seed + input 포함).

### Step 4 — DEVELOP (fix 위치 판단)

| 증상 | 고칠 곳 |
|---|---|
| 특정 selector 가 `no_declarative_v3_mapper` / `unknown` | **manifest** (`registryV2/manifests/<protocol>/<contract>/`) — emit/abi_fragment 보강 → `cd registryV2 && npx tsx scripts/build-index.ts` 재실행 |
| `build_*_failed` / `decode_failed` 가 실거래(corpus)에서 hard | **decoder** (`crates/adapters/mappers/src/declarative/`, `crates/policy-engine-wasm/src/declarative_exports.rs`) — 근본 원인 |
| `*_synthesis_limited` 만 많고 corpus 는 통과 | **harness** (`src/harness/fuzz/`) — 합성 fidelity. 무리면 corpus 로 이전 |

디코더 fix 가 착지하면 대응되는 corpus `expect:error` → `expect:pass`(+`expect_domain`)로 바꾼다 → 이후 회귀가 자동으로 보장한다.

### Step 5 — REGRESS (닫기)

```bash
cargo test -p policy-engine-integration-tests --test v3_decode_harness    # gate 4종 green 유지
cargo test --workspace                                                    # 전체 회귀
```

### FOR CLAUDE CODE AGENTS — 자율 루프

> 다른 팀원의 Claude Code 가 무인 실행할 절차:
>
> 1. Step 1 실행. `/tmp/r.json` 의 `failures[]` 와 `error_hist`, `coverage` 의 미커버 목록, `corpus` 의 `expect:error` 목록을 수집.
> 2. Step 2 표로 각 gap 을 분류. **hard(`fail`/`panicked`>0) 를 최우선**으로 처리.
> 3. 각 hard 건마다 Step 3 로 재현 → Step 4 로 manifest/decoder/harness 중 한 곳을 수정.
> 4. Step 5 로 회귀. gate 가 green 이고 새 hard 가 없으면 다음 gap.
> 5. **완료 조건** — `fail==0 && panicked==0` 이고, `coverage` 신규 미커버 0, 남은 `expect:error` 는 전부 `_note` 로 사유가 문서화됨(예: "V4 nested — deferred").
> 6. 커밋은 explicit-stage(`git add <파일>`)만. `git add -A` 금지. `registryV2/index` 생성물·무관 `registryV2/tokens` curation churn 은 절대 건드리지 말 것. 단, 현재 프로토콜의 token-surface 보강은 `TOKEN_INVENTORY_GUIDE.md` 기준으로 명시적 stage 한다.

---

## 7. Directory Layout

```
crates/integration-tests/
├─ README.md                          ← 이 파일
├─ src/
│  ├─ bin/v3_harness.rs               CLI (fuzz/coverage/replay/corpus/import-*)
│  └─ harness/
│     ├─ adapters.rs                  index 로드 + install + RoutableSurface
│     ├─ prng.rs  encode.rs  route.rs
│     ├─ oracle.rs                    layered 판정 (L1~L4)
│     ├─ report.rs                    히스토그램 + 재현 가능한 failures[]
│     ├─ corpus.rs                    실거래 corpus 로더 + expect 검증
│     └─ fuzz/{mod,values,single_emit,opcode_stream,tagged_dispatch,typed_data}.rs
├─ tests/v3_decode_harness.rs         deterministic CI gate (4 structural + field-level golden 다수)
└─ data/golden/v3-decode/
   ├─ uniswap/corpus.json             실거래 (Dune-sourced)
   └─ _edge-cases/corpus.json         손수 조립한 boundary 케이스
```

---

## 8. Reference

### CLI 명령

| 명령 | 용도 |
|---|---|
| `fuzz [--iterations N] [--seed S] [--json PATH]` | 전 strategy 합성 sweep + 리포트 (hard fail 시 exit 1) |
| `coverage` | surface(strategy 별 callkey 수) + corpus-deferred 목록 |
| `replay --callkey <K> [--seed S]` | single_emit 케이스 1건 재현 → raw envelope |
| `corpus [--root DIR]` | 실거래 corpus replay + expect 검증 |
| `import-dune \| import-etherscan \| import <export.json> [--chain N] [--out PATH]` | Dune/Etherscan export → corpus JSON (parse-only) |

### error-kind 카탈로그 (oracle L4)

- **soft (tolerated)** — `no_declarative_v3_mapper`, `unsupported_strategy_for_typed_data`, `no_typed_data_mapper`, 그리고 하니스가 강등한 `typed_data_synthesis_limited` / `opcode_synthesis_limited`, shape-artifact 인 `build_*_failed`(out-of-bounds / value-map miss).
- **hard (finding)** — 실거래에서의 `build_action_body_failed` / `build_multicall_failed` / `build_array_emit_failed` / `decode_failed` / `invalid_*`, 그리고 catch 된 panic. 카탈로그 밖의 새 kind 자체도 finding.

### oracle 계층 / domain

- L1 envelope(`ok`) → L2 typed round-trip(`Vec<Action>`) → L3 domain validity → L4 error class.
- domain (`VALID_DOMAINS` — 측정 `grep -n VALID_DOMAINS src/harness/oracle.rs`): `token` / `amm` / `lending` / `airdrop` / `launchpad` / `liquid_staking` / `perp` / `permission` / `yield` / `restaking` / `staking` / `hyperliquid_core` / `multicall` / `unknown`. `unknown` 은 **실패가 아니라 metric**(off-chain 등 정상 출력 포함).

### 환경변수

- `ETHERSCAN_API_KEY` — Etherscan pull 용 (B 소스).
- `V3_HARNESS_MAX_FAILURES` — summary 가 출력할 실패 건수 상한(기본 20).

---

## 9. 정직한 한계

- **typed-data EIP-712 합성은 hard 검증 불가** — calldata 는 canonical ABI 바이트라 "type-valid → 무조건 디코드" 를 hard 로 걸 수 있지만, typed-data 는 JSON message 객체라 표현이 모호하다(uint string-vs-number coercion, 재귀 type-graph). 합성 실패는 `typed_data_synthesis_limited` soft 로 강등하고, **실서명 corpus 로 양성 검증**한다.
- **합성 cut 영역** — UniswapX witness order, UniversalRouter V4 deep-nested(0x10/0x21 modifyLiquidities), native-transfer sentinel `0x00000000` 은 합성하지 않고 **corpus-only**.
- **live pull 은 키/네트워크 필요** — gate test 자체는 commit 된 corpus 만 쓰는 offline 경로다. Etherscan 은 BYO 키, Dune 은 MCP/UI export. import-* 는 parse-only(네트워크 X).
- **현재 corpus** — `data/golden/v3-decode/<protocol>/corpus.json` (측정: `ls data/golden/v3-decode/`; 작성 시점 = aave · aave-origin · balancer · compound-v3 · hyperliquid · layerzero · lido · morpho · uniswap · uniswapx + `_edge-cases`). 실거래 비중은 protocol 마다 다르다(uniswap·morpho 가 두꺼움). 부족분은 §3 B/C 로 실거래 추가, domain 별 미커버는 `coverage` 로 확인.
