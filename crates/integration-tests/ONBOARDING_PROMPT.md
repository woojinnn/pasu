<!-- 새 세션에서 새 프로토콜을 온보딩할 때 복붙하는 kickoff 프롬프트.
     `<PROTOCOL>` 만 실제 이름으로 바꿔 입력. 방법론 본문 = PROTOCOL_ONBOARDING_AND_TESTING.md.
     gitignore `!crates/integration-tests/*.md` 로 tracked. -->

# 새 프로토콜 온보딩 kickoff 프롬프트

아래 블록을 새 세션에 그대로 붙여넣고 `<PROTOCOL>` 만 바꾼다.

```
ScopeBall 의 V3 ActionBody 디코드 경로에 <PROTOCOL> 를 온보딩하라.
repo woojinnn/scopeball, cwd /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2.

[작업 워크플로 — 먼저 셋업]
 · **fresh state 면 먼저 한 번**: `cd registryV2 && npm install && npm run build` — `registryV2/index/` 는
   gitignored 생성물이라 없으면 `v3_decode_harness` 가 즉시 panic. + 데이터 lane 확인(ETHERSCAN_API_KEY =
   crates/integration-tests/.env, 로컬·commit 금지 / Dune MCP).
 · 온보딩은 양이 크고 다단계다. **요청받은 worktree 에서 전용 브랜치를 먼저 만들고** 작업하라.
   사용자가 온보딩용 worktree/cwd 를 지정했으면 그 안에서:
   git switch -c feat/<PROTOCOL>-onboarding
   (이미 있으면 git switch feat/<PROTOCOL>-onboarding)
   사용자가 별도 worktree 를 지정하지 않았을 때만 새 worktree + 브랜치를 만든다:
   git worktree add -b feat/<PROTOCOL>-onboarding ../scopeball-<PROTOCOL> <base>
   (base = 현 온보딩 base 브랜치. 그 브랜치가 다른 worktree 에 점유/dirty 면 그 worktree 비접촉.)
   완료·검증 후에도 base/worktree 머지는 사용자가 명시적으로 요청할 때만 진행.
 · **각 phase(P0/P1/P2/P3/P4, 또는 더 잘게 컨트랙트·함수군별)가 끝나면 explicit-stage 커밋**
   (git add <파일>, git add -A 금지; 메시지 말미 Co-Authored-By). 중간 유실 방지 + 회귀 지점.
 · **한 큐로 진행** — 브랜치·외부 데이터 lane 이 준비되면 P0→P4 를 phase 경계 확인 요청 없이 이어서 수행.
   phase 커밋은 체크포인트일 뿐 멈춤 지점이 아니다. 커밋 후 곧바로 다음 phase 로 진행.
   멈춤은 merge/push/destructive action, Etherscan/Dune/auth 부재, 1차출처로도 안 풀리는 스코프 모호성,
   같은 blocker 3회 이상 반복처럼 사용자 입력 없이는 진행 불가능할 때만.
 · **증거 없으면 완료 아님** — `crates/integration-tests/ONBOARDING_EVIDENCE_TEMPLATE.md` 를
   `crates/integration-tests/onboarding/<PROTOCOL>/evidence.md` 로 복사해 채운다. P0/P1/P2/P3/P4 각 행이
   `done` 또는 구체적 `blocked` 가 아니면 phase/온보딩 완료 선언 금지. ⚠️ status 는 **정확히 `done`/`blocked`**
   여야 한다(파서 엄격 — `done (n/a)`·`not applicable` 같은 변형은 게이트 fail). n/a 면 status=`done`, 사유는 artifact 칸에.
   blocked 면 Blockers 표에 행 1개 이상 필수.
   phase 완료 선언 전 `cargo run -p policy-engine-integration-tests --bin check-onboarding-evidence -- <PROTOCOL> --phase <p0|p1|p2|p3|p4|all>`
   를 실행하고, 실패하면 incomplete 로 남긴다.
   시작 시 completion scope 를 evidence 에 먼저 적는다: primary chain(s), `wallet-facing` vs `full-surface`
   vs `full factory-child universe`, multichain expansion 포함 여부. 그냥 "온보딩 완료"라고 말하지 말고
   마지막 claim 에도 같은 label 을 붙인다.
   사용자가 나중에 "Claude Code에 시켰냐?", "Etherscan/Dune real tx 돌렸냐?"라고 물었을 때
   evidence.md 의 명령·결과·카운트로 답할 수 있어야 한다. 못 했으면 사과하지 말고 incomplete/blocked 로 남기고 계속 처리.
 · P2 real-tx 시작 전 외부 데이터 lane 연결 확인:
   Etherscan API/MCP(`ETHERSCAN_API_KEY`, `https://mcp.etherscan.io/mcp`) +
   Dune MCP/API(`https://api.dune.com/mcp/v1`). 키는 로컬 설정만, repo commit 금지.
 · **sub-agent / Claude Code 적극 활용** — 한 세션에 다 못 담는다. fan-out 가능한 작업은 분할:
   P0 컨트랙트별 research/discovery · P0 token-surface research · P1 함수(selector)별 manifest ·
   P1 Tier3/lowering/cedarschema review · P2 synthetic edge matrix · P2 소스별 corpus pull/verdict ·
   P3 gap triage · surface snapshot per-contract. 메인 세션 = 종합·검증·게이트·커밋.
   P0 외에도 새 ActionBody/Tier3, permission/fund-move selector, synthetic edge, real-tx verdict,
   hard decoder gap 은 Claude Code 2nd-opinion 을 받아 union/diff 후 검증.
 · **sub-agent 프롬프트는 self-contained·디테일하게** (sub-agent 는 이 세션 컨텍스트 없음):
   repo/branch/cwd/worktree 경로 + phase/목표/non-goal + 읽을 문서 + 정확한 대상 파일·심볼·좌표
   + 미러할 기존 선례 + 정확한 산출물·출력 포맷·게이트 + 가드레일(1차출처, 무관 churn 금지,
   commit 금지, 불확실 항목은 unverified 표기)을 전부 embed. 결과는 candidate-only —
   메인 세션이 실제 코드/1차출처/gate 로 검증 후 반영.

[SCOPE CONTRACT — 빌드(P1) 전 확정, 비협상. 게이트가 못 잡는 유일한 층이라 여기서 못박는다]
 게이트(surface/universe/corpus/evidence)는 **내부 정합성**만 본다 — "all-green ≠ scope 정답". scope 정합성은
 별도로 못박지 않으면 silent 하게 틀린다(실측: morpho 가 멀티체인+Bundler3 가정으로 틀린 scope 를 짓고 전 게이트 통과). §9.x.
 · **대표 체인 1개.** 온보딩은 한 프로토콜의 **대표 체인 하나**만(보통 mainnet, 또는 그 프로토콜 최대-활동 체인).
   Etherscan+Dune 도 그 한 체인만. **멀티체인 = 별도 프레임워크** → 타체인 variant 는 명시 defer(절대 임의 확장 금지).
 · **scope = COVER/DEFER 경계를 P0 직후·P1 전에 명시 선언**(evidence 'Scope Classification'). scope 는
   게이트로 검증 안 되는 **사용자 고유 결정**이다. 사용자가 지정했으면 그대로(pre-authorized); 모호하면 P0 surface triage +
   제안 cover/defer 경계를 **1회 제시**하고 진행. 자율 진행은 이 contract '안'에서만(그 외 phase confirm 은 여전히 X).
   **경계는 추측이 아니라 아래 SCOPE ORACLE ①(결정 전 cross-entry 거래량 분포 + cover 후보 wrapper 의
   child resolution rate)을 사전입력으로 받아 정한다** — 거래량 지배 entry/실디코드되는 child 를 먼저 cover.
 · **DEFER 는 데이터-게이트.** user-facing surface(유저가 직접 서명하는 컨트랙트/selector)를 defer 하려면
   **1차 usage-share(%/count, Etherscan/Dune 실측)** 를 붙인다 — "큰 공수"같은 산문 사유만으로 defer 금지.
   ⚠️ "비중 클 것 같다"고 *추정*하면서 defer 하면 자기모순 — **defer 하기 전에 측정**한다.
   (infra/governance/keeper EXCLUDE 는 정의상 user-facing 아니라 면제.)
 · **SCOPE ORACLE = coverage 측정**(§9.4 field-level projection 의 **scope-level 버전**). **두 시점**에서 측정한다:
   **① 결정 전(P0직후·P1전, H1)** — cover/defer 경계를 정하기 *전에* (a) 전 user-facing entry 의
   **cross-entry 거래량 분포**(어느 entry 가 지배하는지)와 (b) cover 후보가 wrapper/router selector 면 그
   **child resolution rate** 를 Etherscan/Dune 으로 실측해 경계 결정의 **사전입력**으로 쓴다. "비중 클 것
   같다"는 가정으로 cover/defer 를 정하지 말 것 — 측정이 cover 우선순위를 정한다.
   **② 빌드 후(P2)** — covered 집합이 P0 universe 실거래의 몇 %를 잡는지 측정하되 **반드시 프로토콜-레벨
   거래량 가중(H2)**: Σ covered top-level tx / Σ all top-level tx 를 **전 entry 합산**으로 — 한 entry 의
   per-contract selector-share 만 보고 "대부분 커버"라 하지 말 것(거래량 지배 entry 를 놓친다).
   ⚠️ **wrapper-effective(H3): wrapper/router selector(multicall_recurse·opcode_stream·tagged_dispatch·
   permitBatchAndCall 류)는 manifest 존재 = covered 가 아니다.** effective coverage = 그 selector 실거래
   child 중 covered child 비율(**child resolution rate**)로 센다. (Balancer dogfood 실측: permitBatchAndCall
   이 V3 selector 의 91.7% 지만 child 95% 가 deferred proportional/unbalanced liquidity → effective 4.9%.
   V2 Vault 98.9% 인데 V3 Router-v2 가 거래량 6.7배·실커버 3.5% → 프로토콜-레벨 ~14.3% 인데 전 게이트 green.)
   **completion label 은 이 측정치를 초과주장 못 한다** — 14% 잡으면 "full/dominant surface" 못 쓰고
   "14% subset, uncovered majority=<X>" 로 정직 라벨. 이 측정 없이 "full/dominant surface" 선언 금지.

[먼저 읽어라 — 인스트럭션, 방법론 1차 source-of-truth. 전부 crates/integration-tests/]
 1. PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md — protocol-agnostic completion model,
                                         semantic oracle contract, strict audit skeleton
 2. README.md                          — 하니스 runbook (CLI·3 입력소스·Log→Gap→Develop 루프)
 3. PROTOCOL_ONBOARDING_AND_TESTING.md — spine. P0~P4 전체 + 문서맵·§2.1 워크플로·♻️재진입·
                                         §3.1 LLM discovery panel·§4d enrichment·§5d 소스별 하한·
                                         §8.6 self-check·§9 worked example
 4. ACTIONBODY_EXTENSION_GUIDE.md      — Tier3 확장(새 domain/action/live_field)
 5. registryV2/surface/README.md       — surface gate(I0/I1) + _deployments.json
 6. TOKEN_INVENTORY_GUIDE.md           — protocol token-surface / registryV2/tokens 작성
 7. ONBOARDING_EVIDENCE_TEMPLATE.md    — phase 완료 증거 ledger
 읽고 큰 틀 파악 후 스스로 판단해 자율 실행(매 단계 confirm 요청 X).
 새 domain 같은 큰 설계만 ExitPlanMode 로 plan 1회 받고 자율 진행.
 단 **SCOPE CONTRACT(대표 체인 + cover/defer 경계)만은 P1 전 명시·합의**(pre-authorized 거나 1회 제시) — scope 는
 게이트가 못 잡고 사용자 고유 결정이라 유일한 예외. 그 외 모든 phase 는 confirm 없이 자율.

[진행 P0→P4]
 P0 1차출처로 컨트랙트 인벤토리 + token-surface 전수. 현 Codex 세션 리서치 + Claude Code headless
    (`claude -p ... --add-dir <repo>`) 에 같은 P0 discovery prompt 병렬 실행 →
    union/diff 통합 → 1차출처 verify. surface/<PROTOCOL>/_deployments.json(I0) +
    <contract>.{abi,coverage}.json(I1~I3) → npm run check:surface PASS.
    LLM 결과는 candidate-only, 1차 verify 필수. 프로토콜이 LP/share/receipt/debt/governance/base token 을
    만들거나 직접 다루면 TOKEN_INVENTORY_GUIDE.md 기준으로
    registryV2/tokens/<chain>/<addr>.json 등록/보강. Curve 같은 pool-heavy 프로토콜은
    covered pool 의 LP token + underlyings 를 포함하고, long-tail 제외분은 P0 로그에 명시.
    factory/pool-heavy 프로토콜은 먼저 공식 pool list/factory/registry/Dune decoded stats 로
    address universe 를 만들고 source/query/count 를 기록한다. universe 의 모든 pool/factory child 주소는
    cover/exclude/defer 로 disposition 해야 하며, 일부만 concrete 로 커버할 경우 batch boundary 와
    concrete manifest vs protocol source resolver/generator 결정을 evidence.md 에 남긴다.
    wallet-facing only run 이면 direct pool/pair/vault 호출은 full-universe follow-up 으로 명시 defer 한다.
    router tx 의 pool metadata live_input/RPC lookup 을 direct child callkey coverage 로 세지 않는다.
    pool-heavy/factory 는 machine-readable universe artifact 를 남긴다. source count 가 0 이거나
    tx pull target address count 가 0 이면 성공으로 치지 말고 필터/schema 버그로 보고 즉시 수정한다.
    pool/factory/vault-heavy 프로토콜은 `registryV2/surface/<PROTOCOL>/_address_universe.json`
    또는 `_pool_universe.json` 을 작성하고
    `cd registryV2 && npm run check:universe -- --protocol <PROTOCOL>` 를 실행한다.
    P0 를 완료했다고 말하기 전 evidence.md 에 Claude/sub-agent 명령, 결과 요약, Codex-only/Claude-only/dropped
    후보, 1차출처 검증 disposition, pool universe disposition(해당 시), check:surface 출력이 기록되어 있어야 한다.
 P1 함수마다 schema(§4a)→manifest(§4b)→engine(§4c)→enrich(§4d: 추상 단위면 환산 live_field).
    Tier3 필요 시 ActionBody + effect/view/sync + lowering_v2 + cedarschema +
    schema registration + conformance test 를 먼저 완성한 뒤 manifest 작성.
    npm run check:manifest(CI-safe representative index + source-ref representative). Source-materialized 변경이면
    `cargo run --bin v3-harness -- validate --filter <PROTOCOL> --representative-source-refs` 를 병행하고,
    feasible 하면 `npm run check:manifest:full` 도 실행. full 이 resource/OOM 으로 막히면 evidence.md 에 blocker 기록.
    P1 완료 전 evidence.md 에 COVER selector→ActionBody/Tier3 mapping, permission/fund-move red-flag review,
    manifest 파일 목록, live_field/enrichment 결정, required remote policy-RPC/live/enrichment method 의
    local handler/configured endpoint test/blocker, Tier3 downstream 산출물(해당 시), check:manifest 출력 기록.
 P2 synthetic fuzz(random 5000+ fixed seed) + hand edge synthesis(permission/value/nested/array/opcode)
    + Etherscan API/MCP bulk 최소 10,000 tx/protocol(10,000 API call 아님; 현재 txlist 최대 10k tx/call,
      2026-07-01 이후 Free tier 는 1k/request 예정이라 현재 docs 재확인)
    + Dune MCP/API calibration(**대표 체인 1개**; free 엔진 + partition WHERE). (멀티체인 pinpoint = 별도 프레임워크.)
    + **SCOPE ORACLE — coverage 측정(필수)**: covered (chain,to,selector) 집합이 P0 universe(P0 의
      cover+defer 주소 전체)의 최근 실거래 중 **몇 %를 잡는지** Etherscan/Dune 으로 측정·기록. 이게 §9.4 field-level
      projection 의 scope-level 버전 — completion label 이 측정치를 초과주장 못 하게 P4 에서 대조. user-facing DEFER
      마다 그 surface 의 usage-share 도 1차 실측. **(H1) 이 측정은 P2 가 처음이 아니다 — cover/defer 경계 결정 전
      (P0직후·P1전)에 cross-entry 거래량 분포 + cover 후보 wrapper 의 child resolution rate 를 먼저 실측해 경계의
      사전입력으로 쓰고**(`Scope Classification` 에 기록), P2 는 빌드 후 실커버를 *검증*하는 자리다.
      **(H2) coverage-share 는 프로토콜-레벨 거래량 가중**: Σ covered top-level tx / Σ all top-level tx 를 **전 user-facing
      entry 합산**으로 센다 — 한 entry 의 selector-share 만 보면 거래량 지배 entry 를 놓친다(아래 ⚠️ top-level vs internal 과 함께).
      **(H3) wrapper/router selector(multicall_recurse·opcode_stream·tagged_dispatch·permitBatchAndCall 류)는
      manifest 존재 = covered 가 아니라 effective coverage = child resolution rate**(실거래 child 중 covered child 비율)
      로 센다 — selector-presence 로 세면 child 가 전부 deferred 여도 "covered" 로 과대평가된다(Balancer permitBatchAndCall:
      selector 91.7% 커버처럼 보이나 child 95% deferred → effective 4.9%).
      ⚠️ **측정 단위 = 유저가 *서명하는* top-level tx (tx.to)** — internal call 총합이 아니다. router-heavy 는
      direct(to=target) vs router 경유(to=router, target 은 internal trace 에서 hit)를 `ethereum.traces` 의
      top-level(`cardinality(trace_address)=0`) vs internal 로 분리해 센다. **"total 호출"을 "direct"로 세면 direct
      coverage 과대평가**(실제 사고: morpho 가 "direct 95%/Bundler3 5%"(3483 vs 182) 로 오판 → 그 3483 은 total 호출을
      direct 로 오라벨한 것; 재측정 시 진짜 direct = 전 entrypoint 의 ~2.7% / Morpho-native 의 ~16%, 나머지 ~97% router 경유. §9.x).
    router/manager/aggregator-heavy 프로토콜은 canonical wallet-facing target file 을 만들고
    router/manager/permission/signature/settlement 주소별 최근 tx 를 별도 sweep 한다(대표 프로토콜 기본값:
    target 별 약 5,000 tx). `etherscan-bulk-sweep.mjs` 사용 시 `--surface-root registryV2/surface/<PROTOCOL>`
    를 명시하고, unmatched 는 actionable/non-actionable 으로 disposition 한다.
    EIP-712 typed-data signing 은 Etherscan tx input 으로 검증되지 않는다. 모든 in-scope
    primaryType/witnessType 은 typed-data corpus 또는 field golden 을 만들고
    `v3-harness corpus --filter <PROTOCOL> --require-expect-body` 로 `route_typed_data` 경로를 검증한다.
    off-chain signing 과 on-chain settlement/reactor execution 이 둘 다 있으면 둘을 별도 surface 로 증명한다.
    Etherscan/Dune 연결 없으면 P2 real-tx complete 선언 금지 — blocked_external_data 와 재실행 대상 기록.
    P2 real-tx 를 완료했다고 말하기 전 evidence.md 에 Etherscan api_calls/raw_txs/unique_selectors/selector coverage,
    wallet-facing target sweep 결과/actionable unmatched, typed-data corpus 결과,
    Dune usage baseline/query/rows/credit delta/selected tx hashes 가 기록되어 있어야 한다.
    외부 tx pull 은 target address count 를 반드시 기록한다. 0이면 no-op 이므로 done 금지.
    pool-heavy/factory 프로토콜은 selected cover 주소만이 아니라 P0 candidate/universe 주소도 sweep 한다.
    known protocol selector 로 보이는데 to-address 가 registry/surface 에 없으면 P0/P2 hard gap 으로 버킷팅한다.
    §5d 소스별 하한 준수. semantic-critical 필드는
    PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK 기준으로 expect_body 또는 field-level golden 으로 pin
    (projection 은 하니스 구현 후 사용).
 P3 gap 분류(`unknown_protocol_address` 포함)→manifest/decoder/harness/P0 universe 처치→회귀(§6).
 P4 build-index → registryV2 build-index vitest → check:manifest(CI-safe representative index + source-ref representative) → check:surface →
    pool/factory/vault-heavy 라면 `npm run check:universe -- --protocol <PROTOCOL> --require-cover-linkage` →
    v3-harness coverage/fuzz/corpus → check:manifest:full 또는 resource blocker 기록 → cargo test --workspace 0 fail →
    wasm-build → clippy/fmt(변경 crate) → check-onboarding-evidence --phase all →
    explicit-stage 커밋.
    P3/P4 완료 전 evidence.md 에 gap bucket, fix↔gap mapping, rerun 결과, corpus expect flip/disposition,
    모든 land gate 출력, staged file list, commit hash, 남은 WARN/defer 를 기록.

[♻️ <PROTOCOL> 가 이미 온보딩돼 있으면] greenfield 아님 — 재검증:
 현 _deployments/coverage/manifest/corpus 를 1차출처와 diff → 틀린 곳 수정,
 빠진 selector/컨트랙트/live_field 보충, 회귀. 신규·재검증이 같은 게이트로 수렴.

[가드레일 — 절대]
 explicit-stage(git add <파일>, git add -A 금지) · 무관 churn·.env(ETHERSCAN_API_KEY 로컬) 비접촉
 · 주소/ABI **및 usage/dominance/'대부분 유저가 X 한다' 주장**은 1차 출처만(주소/ABI=Etherscan/Sourcify/공식 GitHub
   verified; **usage/비중=Etherscan/Dune 실측**), 추측·블로그·기억 금지. scope/defer 판단을 추정 usage 로 내리지 말 것
 · **대표 체인 1개**(멀티체인=별도 프레임워크). scope=COVER/DEFER 경계 P1 전 명시(결정 전 cross-entry 거래량 분포로
   사전측정·H1), user-facing DEFER 은 1차 usage-share 첨부, coverage-share 는 **프로토콜-레벨 거래량 가중(H2)** + **wrapper 는
   child resolution rate 로 셈(H3, manifest 존재≠covered)**, completion label 은 측정치 초과주장 금지
 · cargo fmt --all 후 내가 안 건드린 파일 재포맷되면 stage 하지 말고, 실제 revert 는 명확히 내 변경 파일이거나 사용자 승인 받은 경우만
 · 출력 한국어(기술용어 영어), 정직한 한계, 작업/결정에 sequential-thinking.

산출: <PROTOCOL> manifest + (필요시)Tier3 + surface gate PASS + corpus/golden + workspace green.
 phase 별 커밋. 완료 후 worktree/base 머지는 사용자가 명시적으로 요청할 때만 진행.
```
