# `balancer.vault` Extension

Balancer V2 Vault — 모든 풀이 단일 Vault 컨트랙트를 통해 조작되는 *singleton* 모델.

## 진입점

| 컨트랙트 | 주소 (mainnet) |
|---|---|
| Vault | `0xBA12222222228d8Ba445958a75a0704d566BF2C8` |

## 핵심 함수

| 시그니처 | ActionType |
|---|---|
| `swap(SingleSwap, FundManagement, uint256 limit, uint256 deadline)` | Swap |
| `batchSwap(SwapKind, BatchSwapStep[], IAsset[], FundManagement, int256[] limits, uint256 deadline)` | BatchSwap |
| `joinPool(bytes32 poolId, address sender, address recipient, JoinPoolRequest)` | JoinPool |
| `exitPool(bytes32 poolId, address sender, address recipient, ExitPoolRequest)` | ExitPool |
| `flashLoan(IFlashLoanRecipient, IERC20[], uint256[], bytes)` | FlashLoan |

## Extension `data` 필드

```jsonc
{
  "namespace": "balancer.vault",
  "data": {
    "poolId": "0x...32B",        // bytes32 — 앞 20B = pool address, 뒤 12B = nonce + specialization
    "specialization": "GENERAL" | "MINIMAL_SWAP_INFO" | "TWO_TOKEN",
    "fundManagement": {
      "sender": "0x...",
      "fromInternalBalance": false,
      "recipient": "0x...",
      "toInternalBalance": false
    },
    "userData": "0x..."          // JoinKind/ExitKind 인코딩 (decoder 의존)
  }
}
```

## 참고

- **`bytes32 poolId`**: `(pool_address << 96) | (specialization << 80) | nonce`. 풀 주소 도출 = `poolId >> 96`.
- **Internal Balance**: 자산이 Vault 내부에 머물면 `fromInternalBalance/toInternalBalance` true. UI는 명시 노출 권장.
- **`IAsset` sentinel**: `0x0` = 네이티브 ETH (체인별 상이).
- **userData**: `JoinKind`/`ExitKind` opaque bytes — 별도 decoder 없으면 `confidence: medium`.
