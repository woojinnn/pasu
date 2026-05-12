# wrap / unwrap / approve Action 가이드

본 문서는 보통 `category=misc` 로 라벨링되는 3 가지 action — `wrap`, `unwrap`, `approve` — 의 schema 를 사용자/정책 작성자 관점에서 풀어 설명합니다.

---

## 0. 시작하기 전에 — action 과 category 는 직교 차원

**중요한 design 변경**: v1.0.1 에서는 action 이 category 에 종속되지 않습니다. wrap/unwrap/approve 가 무조건 `category=misc` 인 것은 *아님*:

| action | 보통의 category | 다른 category 에 등장하는 케이스 |
|---|---|---|
| `wrap` | `misc` (WETH9.deposit 직접) | `dex` — Universal Router 의 WRAP_ETH opcode (swap 의 일부) |
| `unwrap` | `misc` (WETH9.withdraw 직접) | `dex` — UR UNWRAP_WETH, V3 SwapRouter.unwrapWETH9 (swap 의 결제 단계) |
| `approve` | `misc` (사용자가 단독 approve) | `dex` / `lending` / `rwa` / 모든 category — 거의 모든 action 의 prerequisite |

즉 본 문서는 *세 action 의 schema 정의* 를 다루고, category 는 실제 발생한 protocol 맥락에 따라 root level 에서 결정됩니다.

---

## 1. 세 action 의 공통 성격

- protocol-agnostic 표면 — 어느 DEX/Lending/LST 든 동일 shape
- 단독으로 행위의 "목적" 은 가지지 않음 — 보통 다른 action 의 사전/사후 단계
- 그러나 **정책상 무시할 수 없음** — phishing 의 1차 신호가 여기서 나옴 (unlimited approve, 외부 EOA 로 unwrap 등)

v1.0.1 에 포함:
- `wrap` — native (ETH) → wrapped (WETH) 변환
- `unwrap` — wrapped (WETH) → native (ETH) 변환
- `approve` — ERC-20/721/1155/Permit2 approval

`permit` (EIP-2612, Permit2 PermitSingle) 과 `transfer` (단순 ERC-20.transfer) 도 자리 reserve 가능 — 필요 시 추가.

### 1.1 AmountConstraint 미리 알기

세 action 모두 amount 필드를 가집니다. **숫자 자체보다 "어떤 의미의 숫자인가" 가 더 중요**:

```jsonc
{ "kind": "exact" | "min" | "max" | "unlimited" | "estimated" | "unknown",
  "value": "1000000" }    // 10진 string
```

| kind | 뜻 |
|---|---|
| `exact` | 정확히 이만큼 |
| `min` | 최소 이만큼은 받아야 (슬리피지 하한) |
| `max` | 최대 이만큼만 낼 의향 (슬리피지 상한) |
| `unlimited` | uint256.max — **무한 허용** (approve 위험 신호) |
| `estimated` | 라우터가 quote 한 추정치 (확정 아님) |
| `unknown` | 디코드 불가 |

---

## 2. `wrap` — native → wrapped 변환

### 2.1 어떤 함수가 이걸 트리거하나

| 함수 | 어디서 |
|---|---|
| `WETH9.deposit()` (payable, no args) | 사용자가 WETH 컨트랙트 직접 호출 |
| `WETH9.deposit{value:X}()` | 동상, value 만 다름 |
| UR opcode `0x0b WRAP_ETH (recipient, amount)` | Universal Router execute 안 |
| V4 Router action `0x15 WRAP (currency, amount)` | V4 router 내부 |

→ 정책 관점에서는 "ETH 를 WETH 로 바꾼다" 라는 의미 단위가 같으므로 **모두 본 schema 1개** 로 표현.

### 2.2 필드

```jsonc
{
  "nativeAsset":  { "kind": "native", "symbol": "ETH" },
  "wrappedAsset": { "kind": "erc20",  "address": "0xC02a…", "symbol": "WETH", "decimals": 18 },
  "amount":       { "kind": "exact", "value": "1000000000000000000" },  // 1 ETH
  "recipient":    "0xUser…"
}
```

| 필드 | 정책 관점 |
|---|---|
| `nativeAsset` / `wrappedAsset` | 어떤 native/wrapped pair 인가 (ETH/WETH, BNB/WBNB, MATIC/WMATIC). 정책 "WBNB 차단" 같은 분기 |
| `amount` | wrap 양. kind=exact 가 정상 (msg.value 그대로). 정책 "max 10 ETH per wrap" |
| `recipient` | wrapped asset 수령자. **`root.from` 과 다르면 의심** — wrap 후 다른 EOA 로 WETH 가 가는 패턴 (정상은 거의 없음). 정책 "recipient must equal from" 강력히 권장 |

> **USD 환산값은 schema 표면에 없습니다.** Oracle 데이터는 schema 인스턴스가 만들어진 *이후* 별도 enrichment 단계에서 attach 됩니다. USD 기반 정책 (`max wrap $1000`) 은 그 단계 이후의 policy DSL 에서 다뤄집니다.

### 2.3 정책 예시

```text
// 1. 단순 안전판
recipient == from
amount.value <= 100000000000000000000   // 100 ETH (10진 string 비교)

// 2. native 자산 종류 제한
nativeAsset.symbol == "ETH"             // BNB/MATIC wrap 차단
```

---

## 3. `unwrap` — wrapped → native 변환

### 3.1 어떤 함수가

| 함수 | 어디서 |
|---|---|
| `WETH9.withdraw(uint256 wad)` | WETH 컨트랙트 직접 |
| `unwrapWETH9(uint256 amountMin, address recipient)` | Uniswap V3 SwapRouter |
| `unwrapWETH9WithFee(...)` | 동상, frontend fee 있는 변형 |
| UR opcode `0x0c UNWRAP_WETH (recipient, amountMin)` | Universal Router |
| V4 Router action `0x16 UNWRAP (currency, amount)` | V4 router |

### 3.2 필드

```jsonc
{
  "wrappedAsset": { "kind": "erc20",  "address": "0xC02a…", "symbol": "WETH" },
  "nativeAsset":  { "kind": "native", "symbol": "ETH" },
  "amount":       { "kind": "min", "value": "500000000000000000" },   // 최소 0.5 ETH
  "recipient":    "0xAttacker…"    // ← 정책 trigger
}
```

| 필드 | 정책 관점 |
|---|---|
| `amount.kind` | UR 의 UNWRAP_WETH 는 `amountMin` (kind=min) — 최소 보장. WETH9.withdraw 는 정확한 양 (kind=exact) |
| `recipient` | **★ 가장 중요한 필드.** wrap 보다 더 위험 — unwrap 후 native ETH 는 자유롭게 송금됨. `recipient ≠ from` 이면 **사용자 모르게 ETH 빼가는 패턴**. 정책 강력히 권장: `recipient == from` |

### 3.3 wrap 과의 비대칭

- wrap: 사용자 본인이 native 를 wrapped 로 변환 — 잘못 가도 wrapped 는 ERC-20 이라 복구 가능성 있음
- unwrap: native 가 외부로 가면 **즉시 사용자 통제 외**. 추적/복구 거의 불가능

따라서 unwrap 의 recipient 정책이 wrap 보다 *엄격* 해야 합니다.

---

## 4. `approve` — ERC-20/721/1155/Permit2 권한 부여

### 4.1 가장 위험한 misc action

approve 자체로 자산이 빠져나가지는 않지만, **나중에 spender 가 마음대로 끌어다 쓸 수 있는 권한** 을 부여합니다. unlimited approval + unknown spender 조합은 사실상 "사용자 자산 위탁" 과 같음.

### 4.2 어떤 함수가

| 함수 | selector | 어디서 |
|---|---|---|
| `approve(address spender, uint256 amount)` | `0x095ea7b3` | ERC-20 표준 |
| `increaseAllowance(address spender, uint256 added)` | `0x39509351` | OpenZeppelin 안전 변형 |
| `decreaseAllowance(address spender, uint256 sub)` | `0xa457c2d7` | 동상 |
| `setApprovalForAll(address operator, bool approved)` | `0xa22cb465` | ERC-721/1155 |
| `approve(address to, uint256 tokenId)` | `0x095ea7b3` | ERC-721 (selector 충돌은 ABI 컨텍스트로 분기) |
| Permit2 `approve(address token, address spender, uint160 amount, uint48 expiration)` | (Permit2-specific) | Uniswap Permit2 |

### 4.3 필드

```jsonc
{
  "token":         { "kind": "erc20", "address": "0xA0b86…", "symbol": "USDC", "decimals": 6 },
  "spender":       "0x68b3…fc45",
  "spenderLabel":  "Uniswap V3 SwapRouter02",        // ← host 보강
  "amount":        { "kind": "unlimited" },           // ← 위험
  "approvalKind":  "erc20",
  "currentAllowance": "0",                            // ← host:onchain 보강
  "validity": {                                        // ← Permit2 한정
    "expiresAt": "1716566034",
    "source": "grant-expiration"
  }
}
```

| 필드 | 정책 관점 |
|---|---|
| `token` | 어느 토큰의 approval 인가. 정책 "stablecoin approve 만 허용" |
| **`spender`** | **★ 가장 중요한 필드.** 누구에게 권한을 주는가. 알려진 라우터만 허용해야 함 |
| `spenderLabel` | host 보강 — 알려진 라우터의 사람 친화적 라벨. "Unknown contract" 면 위험 신호 |
| **`amount.kind`** | `unlimited` 면 무한 위임 — 정책상 거의 항상 차단 권장. 정책 DSL 은 `amount.kind === "unlimited"` 비교로 직접 분기 |
| `approvalKind` | 어떤 표준의 approve 인가 |
| `currentAllowance` | 이미 얼마나 줘있는지 (host:onchain 단일 lookup). 증가 폭 정책 ("기존의 10% 이상 증가 금지") |
| `validity` | Permit2 한정 — `source="grant-expiration"`, 부여된 allowance 가 `expiresAt` 까지 유지. 일반 ERC-20/721/1155 approve 는 만료 개념 없음 — omit. 정책 "expiresAt - blockTimestamp > 1년 차단" |

> **USD 환산값은 schema 표면에 없습니다.** Oracle 데이터는 별도 enrichment 단계에서 attach 됩니다. "USD 환산으로 $10,000 이상 approve 차단" 같은 정책은 그 단계 이후의 policy DSL 에서 다뤄집니다.

### 4.4 정책 예시

```text
// 1. 보수적 (대부분 권장)
spenderLabel is not null        // 알려진 spender 만
amount.kind != "unlimited"      // 무한 차단

// 2. Permit2 grant 수명 제한 (validity 활용)
validity == null || (
  validity.source == "grant-expiration" &&
  (validity.expiresAt - root.blockTimestamp) <= 604800   // 7 days
)

// 3. NFT 보호
approvalKind != "erc721" || token.address in whitelisted_collections

// 4. 증가 폭 제한 (currentAllowance 활용)
amount.value <= currentAllowance × 1.1
```

---

## 5. 무엇이 schema 에 *없는가*

| 빠진 것 | 왜 |
|---|---|
| approve 의 signature (v/r/s) | EIP-2612 permit 은 별도 `permit` action 으로 reserve 예정. 본 approve 는 일반 ERC-20.approve 한정 |
| wrap/unwrap 시점의 사전 잔고 / 사후 잔고 | simulation 영역. v1.0.1 에서는 미포함 |
| UR sentinel (0x…0001 = MSG_SENDER, 0x…0002 = ADDRESS_THIS) 의 raw 값 | recipient 가 sentinel 이어도 schema 에 들어올 때는 이미 host 가 해석 후 실제 주소로 치환. 정책은 sentinel 인지 모름 |
| approve 의 `nonce` | EIP-2612 permit 의 nonce 는 의미 있음 (permit action 으로 갈 것). 일반 approve 에는 nonce 없음 |

---

## 6. 자주 묻는 질문

**Q. wrap 과 unwrap 가 swap 의 사전/사후 단계로 같이 나오면 정책은 어떻게?**
A. UR `execute(...)` 가 wrap → swap → unwrap 3 entry 의 `actions[]` 로 분해됨. 정책은:
- 각 leaf 별 평가 (wrap.recipient == from, swap.recipient == from, unwrap.recipient == from)
- 묶음 전체 평가 (Σ wrap.amount = swap.amountIn, unwrap.amount = swap.amountOut, 등 consistency 검증)
두 가지를 모두 검토.

**Q. approve 의 `currentAllowance` 가 비어있으면?**
A. host:onchain 조회 실패 — host 가 chain RPC 못 부르거나 시간 초과. 정책 측은 "currentAllowance 없으면 보수적으로 차단" 같은 fallback 권장.

**Q. `spenderLabel` 이 비어있으면 무조건 차단?**
A. 권장은 차단 but, 사용자가 명시적으로 새 dApp 을 쓰는 케이스도 있음. 차단 + warning + manual override 의 3단계 UX 가 일반적.

**Q. ERC-721 setApprovalForAll 은 approvalKind = ?**
A. `erc721` + `amount.kind = unlimited` 로 표현 (operator 권한은 의미상 unlimited 와 같음).

---

## 7. 관련 파일

- `schema/actions/wrap.json` / `unwrap.json` / `approve.json`
- `schema/common/_common.json` (AssetRef, AmountConstraint, Validity)
- `docs/root-schema.md` (이들이 어떻게 root.actions[] 에 묶이는가)
- `docs/dex-actions.md` (정책상 자주 묶이는 dex action 들)
