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

## 왜 `.json` 인가

- root `.gitignore` 가 `*.md` 를 무시(`README.md` 만 예외) → 로그를 `.md` 로 쓰면 추적 안 됨. `.json` 은 추적된다.
- 머신 판독(auto-improve 루프가 파싱·diff) + 사람 판독(`summary` 필드) 둘 다 가능.
- `git diff` 로 실행 간 변화가 그대로 보인다.

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

> ad-hoc/스크래치 실행은 `logs/scratch/` (gitignore 됨) 에. 커밋하는 로그는 **의미 있는 스냅샷**만.

## 인덱스 (프로토콜별 최신)

| protocol | 최신 로그 | total | pass | soft(커버리지) | hard(디코더) |
|---|---|---|---|---|---|
| uniswap | `uniswap/2026-05-30-coverage.json` | 700 | 302 | 325 (stale-index + FoT/V4) | 73 (Permit2/V4 nested) |
| aave | `aave/2026-05-30-etherscan.json` | 300 | * | L2Pool packed (31% Arb) | 0 |
| balancer | `balancer/2026-05-30-etherscan.json` | 300 | 94 | 204 (batchSwap/join/exit) | 0 |
| hyperliquid | `hyperliquid/2026-05-30-etherscan.json` | 160 | 5+ | 2 (infra, out-of-scope) | 0 |
| layerzero | `layerzero/2026-05-30-etherscan.json` | ~640 | * | ZRO ERC20(no token file) + claim overloads | 0 |
| uniswapx | `uniswapx/2026-05-30-etherscan.json` | 160 | 0 | 3 (reactor execute → Tier B) | 0 |

새 로그를 추가하면 이 표 한 줄을 갱신한다.
