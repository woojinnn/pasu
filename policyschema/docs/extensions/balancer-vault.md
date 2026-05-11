# `balancer.vault` Extension (V2)

Balancer V2 Vault — 모든 풀이 단일 Vault 컨트랙트를 통해 조작되는 *singleton* 모델.

## 진입점

| 컨트랙트 | 주소 (mainnet) |
|---|---|
| Vault | `0xBA12222222228d8Ba445958a75a0704d566BF2C8` |

## 핵심 함수 (5종)

| selector | 시그니처 | ActionType |
|---|---|---|
| `0x52bbbe29` | `swap(SingleSwap, FundManagement, uint256 limit, uint256 deadline)` | Swap |
| `0x945bcec9` | `batchSwap(SwapKind, BatchSwapStep[], IAsset[], FundManagement, int256[] limits, uint256 deadline)` | **BatchSwap** ⭐ |
| `0xb95cac28` | `joinPool(bytes32 poolId, address sender, address recipient, JoinPoolRequest)` | JoinPool |
| `0x8bdb3913` | `exitPool(bytes32 poolId, address sender, address recipient, ExitPoolRequest)` | ExitPool |
| `0x5c38449e` | `flashLoan(IFlashLoanRecipient, IERC20[], uint256[], bytes)` | FlashLoan |

## `bytes32 poolId` 구조

```
poolId (32 bytes) = (pool_address << 96) | (specialization << 80) | (nonce)
                     |─────20 bytes────| |── 2 bytes ──| |─── 10 bytes ──|
```

- 앞 20B = pool 컨트랙트 주소
- 다음 2B = `PoolSpecialization` (0=GENERAL, 1=MINIMAL_SWAP_INFO, 2=TWO_TOKEN)
- 뒤 10B = 풀 등록 nonce

`pool_id >> 96` 으로 주소 추출 (세미-어댑터 `pool_id_to_address()`).

## batchSwap 디테일 ⭐

배치 스왑이 핵심 — Uniswap의 1차원 path와 달리 **임의 그래프 traverse**:

### BatchSwapStep 구조

```solidity
struct BatchSwapStep {
    bytes32 poolId;
    uint256 assetInIndex;   // assets[] 배열 인덱스
    uint256 assetOutIndex;  // 동상
    uint256 amount;          // 0이면 이전 step의 출력 사용 (체이닝)
    bytes userData;          // JoinKind/ExitKind 등 인코딩
}
```

### 매핑 규칙

- `kind = 0 (GIVEN_IN)` → `SwapMode::ExactIn`, 첫 step의 `amount` = exact input
- `kind = 1 (GIVEN_OUT)` → `SwapMode::ExactOut`, 마지막 step의 `amount` = exact output
- `int256[] limits`:
  - 양수 → 사용자가 풀에 *지출* 한도 → `input_tokens`에 추가
  - 음수 → 사용자가 풀에서 *수령* 보장 → `output_tokens`에 추가 (절대값)
- 각 step → `HopRef` 매핑, `pool = "t#pool-{address}"`
- `route = SwapRoute::Batch { steps: Vec<HopRef> }`

### 예시 — USDC → WETH → WBTC

```jsonc
{
  "kind": 0,              // GIVEN_IN
  "swaps": [
    { "poolId": "0xa6f5…0044", "assetInIndex": 0, "assetOutIndex": 1, "amount": "1000000000", "userData": "0x" },  // USDC → WETH
    { "poolId": "0xbf96…0087", "assetInIndex": 1, "assetOutIndex": 2, "amount": "0", "userData": "0x" }            // WETH → WBTC (chain)
  ],
  "assets": ["USDC", "WETH", "WBTC"],
  "limits": ["1000000000", "0", "-3000000"],     // USDC out, WETH 통과, WBTC in
  "funds": { /* sender/recipient/internal flags */ },
  "deadline": "..."
}
```

→ `SwapRoute::Batch { steps: [HopRef(USDC→WETH), HopRef(WETH→WBTC)] }`,
`input_tokens=[USDC]`, `output_tokens=[WBTC]`.

## FundManagement

```jsonc
{
  "sender": "0x...",
  "fromInternalBalance": false,    // Vault internal balance에서 지출?
  "recipient": "0x...",
  "toInternalBalance": false        // Vault internal balance에 수령?
}
```

`fromInternalBalance` / `toInternalBalance` — 자산이 Vault 안에 머무르면 ERC-20 transfer 안 일어남. UI는 명시 노출 권장.

## IAsset sentinel

- `0x0000...0000` = 네이티브 ETH (체인별 상이)
- 그 외 = ERC-20 주소

## userData (opaque)

`JoinKind` / `ExitKind` 등 풀 종류별 인코딩. 별도 decoder 없으면 `confidence: medium`. 현재 세미-어댑터는 *raw bytes 보존*만.

## Extension `data` 필드

```jsonc
{
  "namespace": "balancer.vault",
  "data": {
    "specialization": "GENERAL" | "MINIMAL_SWAP_INFO" | "TWO_TOKEN",
    "userData": "0x..."
  }
}
```
