# `uniswap.v2` Extension

Uniswap V2 Router02 — 가장 단순한 constant-product (x·y=k) AMM. 고정 0.3% fee, transparent `address[] path`.

## 진입점

| 컨트랙트 | 주소 (mainnet) | 역할 |
|---|---|---|
| Router02 | `0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D` | 사용자 진입점, transferFrom + WETH wrap/unwrap + pair 라우팅 |
| Factory | `0x5C69bEe701ef814a2B6a3EDD4B1652CB9cc5aA6f` | `pairFor(factory, tokenA, tokenB)` 도출용 |

## 다루는 함수

| selector | 시그니처 | mode |
|---|---|---|
| `0x38ed1739` | `swapExactTokensForTokens(uint256,uint256,address[],address,uint256)` | ExactIn |
| `0x8803dbee` | `swapTokensForExactTokens(uint256,uint256,address[],address,uint256)` | ExactOut |
| `0x7ff36ab5` | `swapExactETHForTokens(uint256,address[],address,uint256)` payable | ExactIn (입력=ETH) |
| `0x4a25d94a` | `swapTokensForExactETH(uint256,uint256,address[],address,uint256)` | ExactOut (출력=ETH) |
| `0x18cbafe5` | `swapExactTokensForETH(uint256,uint256,address[],address,uint256)` | ExactIn (출력=ETH) |
| `0xfb3bdb41` | `swapETHForExactTokens(uint256,address[],address,uint256)` payable | ExactOut (입력=ETH) |
| `0x5c11d795` | `swapExactTokensForTokensSupportingFeeOnTransferTokens` | ExactIn (FOT) |
| `0xb6f9de95` | `swapExactETHForTokensSupportingFeeOnTransferTokens` | ExactIn (FOT) |
| `0x791ac947` | `swapExactTokensForETHSupportingFeeOnTransferTokens` | ExactIn (FOT) |

## Extension `data` 필드

```jsonc
{
  "namespace": "uniswap.v2",
  "data": {
    "path": ["0x...", "0x..."],            // address[] 그대로 보존
    "supportingFeeOnTransfer": false       // FOT 변형 진입 여부
  }
}
```

## 참고사항

- Pair 주소는 calldata에 *없음*. `pairFor(factory, tokenA, tokenB)` (CREATE2)로 도출. v0.1에서는 별도 등록 안 함 — confidence note에 기록.
- FOT 변형 진입 시 `amount_out.kind = Min`의 의미가 약화 (router는 도달 입력을 정확히 알 수 없음) → confidence ceiling = `medium`.
- WETH wrap/unwrap이 *내부적으로* 일어나는 변형(`*ETH*`)은 `tx.value` 또는 router의 WETH 사용을 통해 처리.
