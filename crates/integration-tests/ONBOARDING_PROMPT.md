<!-- 새 세션에서 새 프로토콜을 온보딩할 때 복붙하는 kickoff 프롬프트.
     `<PROTOCOL>` 만 실제 이름으로 바꿔 입력. 방법론 본문 = PROTOCOL_ONBOARDING_AND_TESTING.md.
     gitignore `!crates/integration-tests/*.md` 로 tracked. -->

# 새 프로토콜 온보딩 kickoff 프롬프트

아래 블록을 새 세션에 그대로 붙여넣고 `<PROTOCOL>` 만 바꾼다.

```
ScopeBall 의 V3 ActionBody 디코드 경로에 <PROTOCOL> 를 온보딩하라.
repo woojinnn/scopeball, cwd /Users/jhy/Desktop/ScopeBall/scopeball-registry-v2.

[작업 워크플로 — 먼저 셋업]
 · 온보딩은 양이 크고 다단계다. **새 worktree + 브랜치**에서 작업하라:
   git worktree add -b feat/<PROTOCOL>-onboarding ../scopeball-<PROTOCOL> <base>
   (base = 현 온보딩 base 브랜치. 그 브랜치가 다른 worktree 에 점유/dirty 면 그 worktree 비접촉.)
   완료·검증 후 base 로 머지(FF 가능하면 FF).
 · **각 phase(P0/P1/P2/P3/P4, 또는 더 잘게 컨트랙트·함수군별)가 끝나면 explicit-stage 커밋**
   (git add <파일>, git add -A 금지; 메시지 말미 Co-Authored-By). 중간 유실 방지 + 회귀 지점.
 · **sub-agent 적극 활용** — 한 세션에 다 못 담는다. fan-out 가능한 작업은 분할:
   P0 컨트랙트별 research/discovery · P1 함수(selector)별 manifest · P2 소스별 corpus pull ·
   surface snapshot per-contract. 메인 세션 = 종합·검증·게이트·커밋.
 · **sub-agent 프롬프트는 self-contained·디테일하게** (sub-agent 는 이 세션 컨텍스트 없음):
   repo/branch/cwd/worktree 경로 + 읽을 문서 + 정확한 대상 파일·심볼·좌표 + 미러할 기존 선례
   + 정확한 산출물·게이트 + 가드레일을 전부 embed. 면밀할수록 rework 가 준다.

[먼저 읽어라 — 인스트럭션, 방법론 1차 source-of-truth. 전부 crates/integration-tests/]
 1. PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK.md — protocol-agnostic completion model,
                                         semantic oracle contract, strict audit skeleton
 2. README.md                          — 하니스 runbook (CLI·3 입력소스·Log→Gap→Develop 루프)
 3. PROTOCOL_ONBOARDING_AND_TESTING.md — spine. P0~P4 전체 + 문서맵·§2.1 워크플로·♻️재진입·
                                         §3.1 LLM discovery panel·§4d enrichment·§5d 소스별 하한·
                                         §8.6 self-check·§9 worked example
 4. ACTIONBODY_EXTENSION_GUIDE.md      — Tier3 확장(새 domain/action/live_field)
 5. registryV2/surface/README.md       — surface gate(I0/I1) + _deployments.json
 읽고 큰 틀 파악 후 스스로 판단해 자율 실행(매 단계 confirm 요청 X).
 새 domain 같은 큰 설계만 ExitPlanMode 로 plan 1회 받고 자율 진행.

[진행 P0→P4]
 P0 1차출처로 컨트랙트 인벤토리 전수 → surface/<PROTOCOL>/_deployments.json(I0) +
    <contract>.{abi,coverage}.json(I1~I3) → npm run check:surface PASS.
    (선택 §3.1: gemini/codex CLI 로 contract discovery 폭 보강 — candidate-only, 1차 verify 필수.)
 P1 함수마다 schema(§4a)→manifest(§4b)→engine(§4c)→enrich(§4d: 추상 단위면 환산 live_field).
    npm run check:manifest.
 P2 synthetic fuzz + Etherscan(bulk 10k) + Dune(Base/OP·cross-chain pinpoint, free 엔진 +
    partition WHERE) — §5d 소스별 하한 준수. semantic-critical 필드는
    PROTOCOL_AGNOSTIC_ONBOARDING_FRAMEWORK 기준으로 expect_body/projection/field-level golden 중 하나로 pin.
 P3 gap 분류→manifest/decoder/harness 처치→회귀(§6).
 P4 build-index → check:manifest → check:surface → cargo test --workspace 0 fail →
    wasm-build → clippy/fmt(변경 crate) → explicit-stage 커밋.

[♻️ <PROTOCOL> 가 이미 온보딩돼 있으면] greenfield 아님 — 재검증:
 현 _deployments/coverage/manifest/corpus 를 1차출처와 diff → 틀린 곳 수정,
 빠진 selector/컨트랙트/live_field 보충, 회귀. 신규·재검증이 같은 게이트로 수렴.

[가드레일 — 절대]
 explicit-stage(git add <파일>, git add -A 금지) · 무관 churn·.env(ETHERSCAN_API_KEY 로컬) 비접촉
 · 주소/ABI 는 1차 출처(Etherscan/Sourcify/공식 GitHub verified)만, 추측·블로그 금지
 · cargo fmt --all 후 내가 안 건드린 파일 재포맷되면 stage 하지 말고, 실제 revert 는 명확히 내 변경 파일이거나 사용자 승인 받은 경우만
 · 출력 한국어(기술용어 영어), 정직한 한계, 작업/결정에 sequential-thinking.

산출: <PROTOCOL> manifest + (필요시)Tier3 + surface gate PASS + corpus/golden + workspace green.
 phase 별 커밋 + 완료 후 worktree 머지.
```
