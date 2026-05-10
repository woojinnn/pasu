# `uniswap.universalRouter` Extension

Uniswap의 멀티-프로토콜 메타 라우터. 단일 `execute(commands, inputs)` 진입점에서 V2/V3/V4 swap, Permit2 서명, wrap/unwrap, sweep 등을 한 트랜잭션에 묶음.

## 진입점

| 컨트랙트 | 주소 (mainnet) |
|---|---|
| UniversalRouter | `0x66a9893cC07D91D95644AEDD05D03f95e1dBA8Af` (v1.1) |

## 진입 selector

| selector | 시그니처 |
|---|---|
| `0x3593564c` | `execute(bytes commands, bytes[] inputs, uint256 deadline)` |
| `0x24856bc3` | `execute(bytes commands, bytes[] inputs)` (deadline 없음) |

## opcode 마스킹

`commands: bytes`의 각 byte가 한 명령. **마스킹 규칙**:
- `command_type = byte & 0x7f`
- `byte & 0x80 == 0x80`이면 `FLAG_ALLOW_REVERT` (해당 명령이 실패해도 트랜잭션은 진행)

## 주요 opcode

| opcode (mask 후) | 이름 | 자식 ActionType |
|---|---|---|
| `0x00` | V3_SWAP_EXACT_IN | `Swap` |
| `0x01` | V3_SWAP_EXACT_OUT | `Swap` |
| `0x02` | PERMIT2_TRANSFER_FROM | (Sign 자식 없음 — Permit2 사전 권한 사용) |
| `0x03` | PERMIT2_PERMIT_BATCH | `SignPermit2Approve` (인라인) |
| `0x04` | SWEEP | (Extension에만 기록) |
| `0x05` | TRANSFER | (Extension) |
| `0x06` | PAY_PORTION | (Extension) |
| `0x08` | V2_SWAP_EXACT_IN | `Swap` |
| `0x09` | V2_SWAP_EXACT_OUT | `Swap` |
| `0x0a` | PERMIT2_PERMIT | `SignPermit2Approve` (인라인) |
| `0x0b` | WRAP_ETH | (Extension) |
| `0x0c` | UNWRAP_WETH | (Extension) |
| `0x0e` | BALANCE_CHECK_ERC20 | (Extension) |
| `0x10` | V4_SWAP | `Swap` (자식 V4 Action 디코드) |
| `0x21` | EXECUTE_SUB_PLAN | (재귀 RouterPlan) |

## Extension `data` 필드 (부모 RouterPlan)

```jsonc
{
  "namespace": "uniswap.universalRouter",
  "data": {
    "commandsHex": "0x0a00",                 // 원본 commands 바이트
    "mask": 127,                             // 0x7f
    "deadline": "1762500000",                // execute 인자 (없는 경우 null)
    "commands": [                            // 디코드된 자식 명령 리스트
      { "opcode": 10, "name": "PERMIT2_PERMIT", "allowRevert": false, "promote": true },
      { "opcode": 0, "name": "V3_SWAP_EXACT_IN", "allowRevert": false, "promote": true }
    ]
  }
}
```

## 참고사항

- 자식 Action은 `parent_action_id`로 부모 RouterPlan을 참조.
- `WRAP_ETH` / `UNWRAP_WETH` / `SWEEP` / `PAY_PORTION` 은 정산 보조 — 자식 Action으로 promote 안 함, Extension 안의 `commands[]`에만 기록.
- `EXECUTE_SUB_PLAN` (0x21)은 재귀 RouterPlan — 자식 RouterPlan이 또 자식 swap을 가질 수 있음.
