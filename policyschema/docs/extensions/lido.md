# `lido` Extension (통합)

Ethereum 최대 LST. **3 component 통합 namespace**:

| `data.component` | 컨트랙트 | 역할 |
|---|---|---|
| `stETH` | `0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84` | ETH→stETH 스테이킹 (`submit`) |
| `wstETH` | `0x7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0` | stETH ↔ wstETH wrap/unwrap |
| `withdrawalQueue` | `0x889edC2eDab5f40e902b864aD4d7AdE8E412F9B1` | unstake 요청·청구 (NFT) |

## v0.1이 다루는 함수

### Component `stETH`

| 시그니처 | ActionType | 카테고리 |
|---|---|---|
| `submit(address _referral)` payable | Stake | LiquidStaking |

```jsonc
{ "component": "stETH", "referral": "0x..." }   // address(0) → null
```

### Component `wstETH`

| 시그니처 | ActionType | 카테고리 |
|---|---|---|
| `wrap(uint256 _stETHAmount)` | Wrap | **Swap** (1:1 토큰 변환) |
| `unwrap(uint256 _wstETHAmount)` | Unwrap | **Swap** |

```jsonc
{ "component": "wstETH" }   // 추가 필드 없음
```

`SwapFields` 사용:
- `mode = ExactIn`, `amount_in.kind = Exact`, `amount_out.kind = Exact` (1:1)
- `route = SingleHop { hop.protocol = "lido" }`
- `max_fee_bps = Some(0)`

### Component `withdrawalQueue`

| 시그니처 | ActionType | 카테고리 |
|---|---|---|
| `requestWithdrawals(uint256[] _amounts, address _owner)` | RequestWithdrawal | LiquidStaking |
| `claimWithdrawal(uint256 _requestId)` | ClaimWithdrawal | LiquidStaking |

```jsonc
// requestWithdrawals
{
  "component": "withdrawalQueue",
  "amounts": ["1000000000000000000", "2000000000000000000"],   // uint256 decimal strings
  "requestIds": []                                              // event-derived (calldata 아님)
}

// claimWithdrawal
{
  "component": "withdrawalQueue",
  "requestId": "12345"                                          // calldata 인자
}
```

## 의미 매핑

### `submit` (Stake)
- `actor = msg.sender` (mint 받는 주체)
- `asset_in = ETH (native)` — `tx.value` 사용
- `asset_out = stETH`
- `amount.raw = tx.value`, kind = `Exact`
- `recipients.recipient = Actor` (msg.sender에 mint)
- `referral` → Extension data (`address(0)`이면 `None`)

### `requestWithdrawals` (RequestWithdrawal)
- `asset_in = stETH`
- `asset_out = None` (대신 NFT 발급 — schema 외)
- `amount.raw = sum(amounts)`, kind = `Exact`
- `recipients.recipient = _owner` (NFT 받는 주체)

### `claimWithdrawal` (ClaimWithdrawal)
- `asset_in` = stETH (잠긴 자산, placeholder Token)
- `asset_out = ETH (native)`
- `amount.raw = ?` — calldata에 amount가 *없음* (NFT lookup 필요) → `Unspecified`
- `recipients.recipient = Actor`

## 참고사항

- **wrap/unwrap의 카테고리**: stETH와 wstETH는 1:1 환산이지만 ratio가 시간에 따라 다름 (rebasing token vs non-rebasing). 그래도 1:1 *swap*으로 분류.
- **recipient 정책**: `submit`/`claim`은 항상 `msg.sender`에 mint/transfer. 외부 recipient 지정 불가 → `recipient_equals_actor = true` 항상.
- **확장 v0.2**: Lido Dual Governance, gas-station 등은 v0.1 범위 외.
