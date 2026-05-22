# AUDIT — Uniswap 완벽 지원 (P1+P2 함수 보강 + V4)

> Phase 5 security audit. QuillAudits BSA framing. read-only.
> Scope: 이번 작업 변경분 (HEAD `9653047` 대비). 측정일 2026-05-21.
> Auditor: `solidity-auditor` agent + main agent 검증.

## 결론 — P0 0건 (gate 충족)

fail-open 수정 (`Action::Approve`/`SetApprovalForAll` dispatch arm) 이 정확히 적용 — Permit2 approve declarative 지원 + 기존 ERC-20 approve / NFPM setApprovalForAll 정적 path silent-pass 를 동시 차단. 통합 test 가 forbid verdict 발화까지 입증. selector/address 1차 출처 대조 불일치 0, index collision 0, `cargo test` 0 regression.

| severity | 건수 | 상태 |
|---|---|---|
| P0 Critical | 0 | — |
| P1 High | 0 | — |
| P2 Medium | 1 | **수정 완료** |
| P3 Low | 1 | 문서화 (PoC scope 수용) |
| Info | 2 | 문서화 |

## Findings

### [P2 — 수정 완료] V4 swap recipient — `ActionConstants` sentinel 미해석

- **위치**: `crates/adapters/mappers/src/protocols/universal_router/v4_swap_builder.rs`.
- **문제**: V4-periphery `ActionConstants.sol` 의 `MSG_SENDER=address(1)`·`ADDRESS_THIS=address(2)` sentinel 을 해석하지 않아 V4 swap envelope 의 `recipient` 가 `0x00…01` literal 로 오표기. declarative `execute_v4_swap_step` 이 verdict driver 이므로 recipient allowlist / `recipient != myWallet` 정책이 오평가됨. fail-open 아님 (오표기). legacy `v4_swap.rs` 에 원래 있던 버그가 declarative emit 활성화로 verdict-affecting 으로 승격.
- **수정**: Pass 2 에서 `swap.recipient` 설정 직전 `common::map_recipient(ctx, raw)` 적용 (`0x..01→ctx.from`, `0x..02→ctx.to`, 그 외 raw 보존 — 기존 helper 재사용). sentinel regression test 2개 추가. `cargo test --workspace` 877→879 (0 regression).

### [P3 — 문서화] V4 PM `modifyLiquidities` — settle/take/sweep step under-report

- **위치**: `registry/manifests/uniswap/v4/modifyLiquidities@1.0.0.json` 외. `per_opcode_emit` 키 = `0x00-0x03` (LP intent) 만, `unknown_opcode_policy:"ignore_step"`.
- **문제**: V4 PM action stream 의 `0x0e TAKE`/`0x11 TAKE_PAIR`/`0x14 SWEEP`/`0x0b-0x0d SETTLE*` 가 silent drop — 출금 토큰 목적지 비가시.
- **판정 — fail-open 아님**: 사용자의 권한-핵심 intent (mint/increase/decrease/burn LP) 0x00-0x03 은 정상 emit·verdict 생산. TAKE/SWEEP recipient 는 fund-flow 차원 (secondary). ScopeBall 은 simulation 없는 정적 권한 분석기 — V4 research §6.4 가 swap↔TAKE 결합을 사용자 위임 open question 으로 분류, modifyLiquidities 도 같은 범주. 의도된 scope.
- **권고**: CLAUDE.md "한계" 1줄. 향후 0x0e/0x11/0x14 → `transfer` per_opcode_emit rule.

### [Info] V4 `permitForAll` → `set_approval_for_all` 매핑 — 서명 deadline 소실

`permitForAll` (EIP-712 서명 relay) 을 `set_approval_for_all` action 으로 매핑 — `SetApprovalForAllContext` 에 `validity` 없어 `deadline` 소실. 권한 표면 (operator 컬렉션 위임) 은 surface 됨 — 정밀도 문제. `PermitKind::Erc721PermitForAll` variant 가 이미 존재하므로 `permit` action 매핑이 의미상 더 정확 — 후속 검토.

### [Info] ERC-721 `approve` 가 `approvalKind:"erc20"` 차용

`ApprovalKind` enum 에 ERC-721 single-token approve variant 없어 `erc20` 차용 (`token.kind:erc721`·`tokenId` 로 NFT 표시). 권한 표면 정상 surface — enum coverage gap 의 차선. 정밀하게는 `ApprovalKind::Erc721` 신설 (Action 정의 변경 — 별도 트랙).

## 1차 출처 대조 — 확정 버그 0

| 항목 | 대조 | 결과 |
|---|---|---|
| Permit2 7 selector | `cast sig` vs manifest vs `/tmp/uniswap-fn-inventory.md` | 7/7 일치 |
| V4 8 selector | `cast sig` vs `/tmp/uniswap-v4-research.md` | 8/8 일치 |
| ERC-721 5 + multicall | `cast sig` | 6/6 일치 |
| selfPermit 4 variant | `cast sig` | 4/4 일치 |
| payment 9종 | `cast sig` | 9/9 일치 |
| V4 PoolManager/PositionManager 13체인 주소 | `uniswap-deployments.json` vs research matrix | 26/26 일치 (Arbitrum+Ink PoolManager CREATE2 colocation 정상) |
| on-chain `cast code` | chain1 6컨트랙트 + Base 2 | 8/8 배포 확인 |
| index callkey collision | `uniq -d` | 0건 |

## fail-open 차단 확인 (이번 작업 핵심 gate)

`lowering/dispatch.rs` 에 `Action::Approve`/`Action::SetApprovalForAll` arm 이 `Ok(Some(_))` 반환 — catch-all `_ => Ok(None)` 위에 위치, silent-drop 없음. 통합 test (`p0_1_action_lowering.rs`, `e2e_new_pipeline.rs`) 가 envelope → lower → 실제 forbid verdict 발화 + 정적 erc20 mapper 산출 envelope 가 `Verdict::Fail` 도달 입증 (기존 ERC-20 approve 정적 path silent-pass P0 동시 수정).

`array_emit` — `MAX_ARRAY_ELEMENTS=64` + per-bundle `max_elements` 이중 cap, parallel array 길이 강제, recurse 없음 (DoS 무관). 9 TDD 통과.

## 검증 수치

- `cargo test --workspace`: **879 passed / 0 failed / 6 ignored** (baseline 836 → +43, 0 regression)
- `vitest`: 45 suites / 356 tests
- `tsc --noEmit`: 0 error
- WASM `policy_engine_wasm_bg.wasm`: 5.57 MiB (< 6 MiB)
- registry: 1922 callkey, 0 error, collision 0

## 출처

- `/tmp/uniswap-fn-inventory.md` (Permit2/SR01/SR02/NFPM 함수 1차 출처) · `/tmp/uniswap-v4-research.md` (V4)
- V4-periphery `ActionConstants.sol`·`Actions.sol` (main HEAD, WebFetch) · `cast sig`/`cast code` on-chain
- 계획서 `~/.claude-web3/plans/scopeball-uniswap-zany-neumann.md`
