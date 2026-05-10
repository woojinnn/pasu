# Signature Extensions: `permit2`, `eip2612`, `eip712`

EIP-712 서명 흐름 3종을 통합 설명. 각각 별도 namespace.

---

## `permit2` — Uniswap Permit2 통합 권한위임

Canonical Permit2 컨트랙트: `0x000000000022D473030F116dDEE9F6B43aC78BA3` (모든 EVM 체인 동일).

### 6 primaryType

| primaryType | ActionType | 의미 |
|---|---|---|
| `PermitSingle` | SignPermit2Approve | 단일 토큰 권한 부여 |
| `PermitBatch` | SignPermit2Approve | 다중 토큰 권한 부여 |
| `PermitTransferFrom` | SignPermit2TransferFrom | 단일 토큰 즉시 이체 |
| `PermitBatchTransferFrom` | SignPermit2TransferFrom | 다중 즉시 이체 |
| `PermitWitnessTransferFrom` | SignPermit2TransferFrom | + witness 바이트 |
| `PermitBatchWitnessTransferFrom` | SignPermit2TransferFrom | 다중 + witness |

### Extension `data` 필드

```jsonc
{
  "namespace": "permit2",
  "data": {
    "component": "single" | "batch" | "transferFrom" | "batchTransferFrom" | "witness" | "batchWitness",
    "witnessTypeString": "..."          // *Witness* 변형만
  }
}
```

### 매핑

- `signer = sig.message.owner`
- `domain.name = "Permit2"`, `version = "1"`, `verifyingContract = canonical Permit2`
- `semantic = Permit2Approve { spender, tokens, nonce }` 또는 `Permit2TransferFrom { spender, transfers, nonce, witness? }`
- `deadlines.deadline = sig.message.sigDeadline` (Approve) 또는 `sig.message.deadline` (TransferFrom)

### Unlimited 패턴

`PermitSingle.details.amount = type(uint160).max`이면 `AmountKind::Unlimited`.

---

## `eip2612` — ERC20 표준 EIP-712 permit

ERC20 토큰 컨트랙트 자체에 서명하는 표준 (`Permit` primaryType).

### 매핑

- `signer = sig.message.owner`
- `domain.verifyingContract = token address` (canonical Permit2 아님)
- `domain.name = token.name()`, `version = token.version()` (보통 `"1"`)
- `primary_type = "Permit"`
- `semantic = Eip2612Permit { token, owner, spender, value: AmountSpec, nonce }`
- `value`가 `type(uint256).max`이면 `AmountKind::Unlimited`
- `deadlines.deadline = sig.message.deadline`

### Extension `data` 필드

```jsonc
{
  "namespace": "eip2612",
  "data": {}                   // 추가 필드 없음 — semantic에 다 들어감
}
```

---

## `eip712` — Catch-all

인식되지 않은 EIP-712 서명. 도메인·메시지를 *원본 그대로* 보존.

### 매핑

- `primary_type` = (varies)
- `semantic = Other { types_json, message_json }`
- `deadlines = DeadlineFields { deadline: None, deadline_horizon_seconds: None }` (대부분 deadline 없음)

### Extension `data` 필드

```jsonc
{
  "namespace": "eip712",
  "data": {
    "typesJson": { ... },           // 원본 EIP-712 types 정의
    "messageJson": { ... }          // 원본 메시지
  }
}
```

### Confidence ceiling

`SignEip712Other`는 `confidence: low` ceiling — 메시지 의미를 정적으로 해석하지 못함.

---

## 통합 흐름

`PrimaryType` DispatchKey가 `(verifying_contract, primary_type)` → 위 ActionType 룩업:

| 조건 | ActionType | namespace |
|---|---|---|
| `verifying_contract == Permit2 canonical && primary_type ∈ {PermitSingle, PermitBatch}` | SignPermit2Approve | `permit2` |
| `verifying_contract == Permit2 canonical && primary_type ∈ {Permit*TransferFrom*}` | SignPermit2TransferFrom | `permit2` |
| `primary_type == "Permit"` (token contract) | SignEip2612Permit | `eip2612` |
| 그 외 | SignEip712Other | `eip712` |
