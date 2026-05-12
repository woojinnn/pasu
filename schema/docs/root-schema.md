# Root Schema 가이드 — `root.json`

이 문서는 **Root Schema 가 무엇이며 왜 필요한가** 부터 시작해, 각 필드가 어떤 의미인지를 사용자/정책 작성자의 관점에서 풀어 설명합니다. 작업자가 처음 본 schema 라도 술술 읽혀야 합니다.

---

## 1. 한 줄로 말하면

> **사용자가 서명할 트랜잭션 한 건을 "정책으로 평가하기 좋게" 정리한 최상위 컨테이너**

지갑이 "이거 서명할래?" 라고 물어보는 순간, 그 트랜잭션 한 건이 본 schema 에 맞는 JSON 으로 변환됩니다. 정책 엔진은 그 JSON 을 보고 *"허용 / 거부 / 경고"* 를 결정합니다.

---

## 2. 왜 이 schema 가 필요한가

지갑에 들어온 raw transaction 은 보통 이렇게 생겼습니다:

```text
to       = 0x68b3...fc45                     ← 무슨 컨트랙트?
value    = 0                                  ← native 안 내고
data     = 0x414bf389...                      ← 64 KB hex blob
```

이 상태로는 **정책을 쓸 수가 없습니다.** "USDC swap 만 허용" 같은 규칙을 0x414bf389... 같은 hex 로 표현할 수는 없으니까요.

그래서 디코더 / 어댑터가 raw transaction 을 **사람과 정책이 다 이해할 수 있는 형태** 로 풀어줍니다. 그 결과의 wire 형식이 본 schema 입니다.

핵심 원칙은 단 한 줄:

> **"이 필드 위에 사용자가 정책 규칙을 쓸 수 있는가?" → Yes 면 schema 에 포함. No 면 제외.**

calldata 의 raw bytes? 정책 못 씁니다. 그래서 schema 에 없습니다. 라우터가 내부적으로 거친 pool 주소 N 개? 정책상 별 의미 없습니다. 그래서 없습니다. **있을 자격은 "정책 작성자가 이 필드를 보고 분기할 만한 데이터" 한 가지** 입니다.

---

## 3. 전체 구조 한눈에

```jsonc
{
  "schemaVersion": "1.0.1",
  "requestKind":   "transaction",        // 또는 "signature" / "userOperation"
  "chainId":       1,                    // 어느 체인인가
  "from":          "0xUser…",            // 누가 서명하는가
  "to":            "0xRouter…",          // 어느 컨트랙트를 부르는가
  "value":         "1000000000000000000", // 얹은 native (wei)
  "selector":      "0x414bf389",         // 어떤 함수인가
  "protocol":      { "name": "uniswap", "version": "v3", "component": "swapRouter" },
  "blockTimestamp": 1715961234,
  "actions": [                            // 의미적으로 N 개 동작으로 분해
    {
      "action":   "swap",                 // 의미 단위 식별자
      "category": "dex",                  // protocol 맥락 태그 (직교 차원)
      "fields":   { … }                   // actions/swap.json 으로 별도 검증
    }
  ]
}
```

상단 7개 (`schemaVersion` ~ `selector`) 는 **공통 메타** — 어떤 카테고리/액션인지에 무관하게 모두 동일하게 필요한 정보. 하단의 `actions[]` 가 의미 단위 분해 결과.

---

## 4. 필드별 의미

### 4.1 메타

| 필드 | 뜻 | 정책에서 어떻게 쓰이는가 |
|---|---|---|
| `schemaVersion` | 본 schema 버전 | 향후 v1.0.2 같이 올리면 정책 엔진이 호환성 확인 |
| `requestKind` | 어떤 형태의 서명 요청인가 | `transaction` = 일반 tx, `signature` = EIP-712, `userOperation` = ERC-4337. 정책 "signature 만 차단" 같은 분기 |

> **신뢰도 (confidence) 가 없는 이유**: 본 schema 는 화이트리스트 기반 — 직접 매핑한 함수만 valid 처리하므로 "이 디코드가 얼마나 믿을만한가" 같은 분기가 의미 없습니다. schema 에 들어왔다 = 이미 verified. 매핑 안 된 함수는 애초에 root 가 생성되지 않거나 별도 unknown 경로로 처리.

### 4.2 체인 / 주체

| 필드 | 뜻 | 정책 예시 |
|---|---|---|
| `chainId` | EVM 체인 식별자 (mainnet=1, base=8453, …) | "mainnet 외 차단", "L2 만 허용" |
| `from` | 서명 주체 — 사용자 본인의 EOA | 다른 모든 분기의 기준점. recipient 와 비교해서 "self vs external" 판단 |
| `blockTimestamp` | 현재 block.timestamp (host 가 채움) | deadline 분석 base. deadline - blockTimestamp 가 너무 짧으면 racing risk |

### 4.3 컨트랙트 / 함수

| 필드 | 뜻 | 정책 예시 |
|---|---|---|
| `to` | 호출 대상 컨트랙트 (라우터 / 풀 / Vault / Manager) | "whitelisted target 만 허용" — 가장 흔한 정책 |
| `value` | msg.value (wei, 10진 string) | "value > 1 ETH 차단", "native 송금 0 만 허용" |
| `selector` | 호출 함수의 4-byte 식별자 | "이 selector 만 허용" — 보수적 정책의 핵심 |

### 4.4 protocol

```jsonc
"protocol": {
  "name":      "uniswap",     // 어느 프로토콜
  "version":   "v3",          // 어떤 버전
  "component": "swapRouter"   // 그 안에서 어떤 역할 (optional)
}
```

`(chainId, to)` → `protocol` 은 host (지갑/서버) 가 미리 가진 매핑으로 결정합니다. 같은 selector 라도 Uniswap V2 와 PancakeSwap V2 는 ABI 가 동일해서 selector 만 보고는 못 구분 — protocol.name 분기로 해결.

정책 예시: `"protocol.name == 'curve' 차단"` (특정 프로토콜 ban), `"protocol.version == 'v4'`만 허용" (특정 generation 만).

### 4.5 actions[]

```jsonc
"actions": [
  { "action": "wrap",   "category": "misc", "fields": { … } },
  { "action": "swap",   "category": "dex",  "fields": { … } },
  { "action": "unwrap", "category": "misc", "fields": { … } }
]
```

**중요**: 한 트랜잭션이 의미적으로 여러 action 으로 분해될 수 있음.

- Universal Router 의 `execute(...)` 한 건 ≈ wrap + swap + unwrap 3 개
- Uniswap V3 NPM 의 `multicall(bytes[])` ≈ sub-call N 개

정책 평가는 보통 **각 leaf action 별로 따로**, 그리고 **묶음 전체** 두 가지를 모두 봅니다.

#### ★ action 과 category 는 직교 차원 (independent)

이전 설계는 "category 가 action 의 가능한 값을 결정한다" 였습니다. v1.0.1 에서는 **둘이 독립**. 같은 `action` 이름이 여러 `category` 에 등장할 수 있습니다:

| action | 가능한 category 와 예시 |
|---|---|
| `swap` | `dex` (Uniswap/Curve/Balancer), `liquid_staking` (Lido stETH ↔ wstETH), `rwa` (LBTC ↔ WBTC 1:1 mint), `lending` (일부 lending 의 collateral swap routing) |
| `add_liquidity` / `remove_liquidity` | `dex` (V2/Curve/Balancer fungible LP), `yield` (vault deposit/withdraw) |
| `mint_liquidity_nft` / `burn_liquidity_nft` | `dex` (V3/V4 NFT position) |
| `increase_liquidity` / `decrease_liquidity` | `dex` (V3/V4 기존 NFT internal liquidity) |
| `wrap` / `unwrap` | `misc` (WETH9 직접), `dex` (UR 내 wrap/unwrap opcode), 그 외 |
| `approve` | `misc` (단독), `dex` / `lending` / 모든 category (prerequisite 로) |

→ **action = "무엇을 하는가" 의미 단위 / category = "어떤 protocol 맥락인가" 태그**. 직교.

→ 정책은 `action` 단독 분기 ("모든 swap 차단") 또는 `(action, category)` 결합 분기 ("dex swap 만 허용") 모두 가능.

각 entry 의 구조:

| 필드 | 뜻 |
|---|---|
| `action` | 의미 단위 식별자. 정책 1차 분기점. v1.0.1 enum 10 종 — `swap` / `add_liquidity` / `remove_liquidity` / `mint_liquidity_nft` / `burn_liquidity_nft` / `increase_liquidity` / `decrease_liquidity` / `wrap` / `unwrap` / `approve`. host validator 가 이 값을 보고 actions/<action>.json schema 를 골라 fields 를 추가 검증. 새 action 추가 시 schema minor version bump. |
| `category` | protocol 맥락 태그. 8 종 enum — `dex` / `lending` / `rwa` / `liquid_staking` / `restaking` / `yield` / `misc` / `unknown`. Defillama 카테고리와 align. **action 의 유효성을 제한하지 않음**. |
| `fields` | action 별 schema (`actions/<action>.json`) 로 추가 검증되는 객체. shape 의 선택은 위 action 값으로 결정. |

`fields` 의 구체적 형식은 별도 문서:
- `category=misc` 의 action 들: `misc-actions.md`
- `category=dex` 의 action 들: `dex-actions.md`

---

## 5. 무엇이 schema 에 *없는가* (그리고 왜 없는가)

| 빠진 것 | 왜 |
|---|---|
| `calldata` 원본 hex bytes | 정책으로 다룰 수 없는 raw blob. 디코드 결과만 schema 에. |
| `decodedCalls[]` (V3 multicall 의 sub-call 트리, UR opcode 시퀀스 등) | wire 추적용 메타. 정책은 leaf action 만 보면 충분. |
| `route.hops[]` (swap 의 경유 pool 리스트) | 통과 경로 자체는 정책 가치가 없음. "USDC 만 받음" 같은 정책은 tokenOut 만 봐도 됨. |
| `targets[]` / `ContractTarget` 분류 (entrypoint / router / pool / vault / token / …) | 디코더 내부 정보. 정책은 protocol.name + to 만 봄. |
| `rawRoute` / `encodedPath` (V3 packed path) | 디버깅 정보. 정책 무용. |
| `confidence` 모든 자리에서 | **화이트리스트 기반** — 직접 매핑한 함수만 schema 에 들어옴. "이게 얼마나 믿을만한가" 는 의미 없음 ("schema 에 있다 = 믿을만하다"). v260511 의 8-stage confidence 컨테이너 + per-object confidence 전부 제거. |
| **USD 환산값** (`valueInUsd`, `minValueOutUsd`, `totalInputUsd`, ...) | **별도 enrichment 단계의 영역.** schema 인스턴스가 만들어진 *이후* Oracle 데이터를 호스트가 별도 파이프라인으로 attach. schema 본 표면에는 부재. `UsdValuation` 타입도 제거됨. |
| User portfolio 정보 (잔고, allowance 누적, position 보유 NFT 등) | 동상 — 별도 enrichment 단계. (`approve.currentAllowance` 는 단일 transient lookup 이라 예외로 schema 에 자리 보존.) |

이게 v1.0.1 의 핵심 design: **schema 의 표면을 정책-relevant 한 데이터로만 채운다.**

---

## 6. 자주 묻는 질문

**Q. `actions[]` 가 1개면 한 트랜잭션이 1 action 이라는 뜻인가?**
A. 네. 단순 `Uniswap V2 swapExactTokensForTokens` 한 건 → `actions = [{action: swap, category: dex, fields: …}]` 한 entry.

**Q. UR `execute(...)` 가 wrap + swap + unwrap 3 가지를 하면?**
A. `actions` 가 3 entry. 각각 wrap/misc, swap/dex, unwrap/misc. 정책은 entry 단위로도, 전체 묶음으로도 평가 가능.

**Q. `swap` 이 DEX 외 다른 category 에 등장한다는 게 무슨 뜻?**
A. 의미적으로 "토큰 A → 토큰 B 교환" 인 모든 동작은 `action=swap`. 발생한 protocol 맥락이 무엇이냐에 따라 category 가 달라짐:
- Lido `wstETH.wrap(stETH)` → `action=swap, category=liquid_staking` (rebasing 처리)
- Lombard LBTC mint (WBTC → LBTC) → `action=swap, category=rwa`
- 일부 lending protocol 의 collateral swap → `action=swap, category=lending`
정책은 `action=swap and category=dex` 같이 결합 분기 가능.

**Q. `category=misc` 가 너무 자루 안에 잡다하지 않나?**
A. 자루 맞음. 어떤 특정 DeFi protocol class 와도 직접 결합되지 않는 "토큰 movement 보조 동작" 의 default. 현재 v1.0.1 의 misc 후보: `wrap`, `unwrap`, `approve`. `permit`, `transfer` 도 자리만 reserve (필요 시 추가). 단, **wrap/unwrap/approve 가 무조건 misc** 인 것은 아님 — UR 내 wrap opcode 는 swap 의 일부니까 `category=dex` 일 수 있음.

**Q. `unknown` category 는 언제?**
A. host 가 protocol 식별에 실패했거나, schema 에 정의된 category 어디에도 안 맞는 경우. 정책은 보통 unknown 을 보수적으로 차단. (단, action 자체가 매핑된 함수면 schema 에는 들어옴 — whitelist 통과 의미.)

**Q. `protocol.name` 의 enum 이 따로 없는데?**
A. 일부러 free string. Defillama 식별자에 맞춰 host 가 채움. enum 으로 폐쇄하면 새 protocol 추가마다 schema 수정 필요 — 운영 비용 큼. policy 측이 직접 string 비교 (`protocol.name == 'curve'`).

**Q. confidence 가 다 빠진 이유?**
A. 본 schema 는 **화이트리스트 기반** — 매핑된 함수만 schema 인스턴스를 생성합니다. 즉 schema 에 들어왔다는 사실 자체가 "verified" 의 의미. 별도 신뢰도 등급은 필요 없습니다. 매핑 안 된 함수는 처음부터 root 인스턴스가 안 만들어지거나 별도 unknown 경로로 처리.

---

## 7. 관련 파일

- `schema/root.json` — 본 문서의 대상
- `schema/common/_common.json` — Address / Hex / DecimalString / IntDecimalString / AssetRef / AmountConstraint / Validity primitive
- `schema/actions/*.json` — action 별 fields shape
- `docs/misc-actions.md` — wrap / unwrap / approve 설명
- `docs/dex-actions.md` — swap / add_liquidity / remove_liquidity 설명
