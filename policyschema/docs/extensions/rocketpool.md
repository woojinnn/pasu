# `rocketPool` Extension

Rocket Pool — 분산형 ETH 액화 스테이킹 (rETH).

## 진입점

| 컨트랙트 | 주소 (mainnet) |
|---|---|
| RocketDepositPool | `0xDD3f50F8A6CafbE9b31a427582963f465E745AF8` |
| rETH | `0xae78736Cd615f374D3085123A210448E74Fc6393` |

## 핵심 함수

```solidity
function deposit() external payable;        // ETH → rETH (RocketDepositPool)
function burn(uint256 _rethAmount) external;  // rETH → ETH (rETH 컨트랙트)
```

## Extension `data` 필드

```jsonc
{ "namespace": "rocketPool", "data": { "minipoolCount": 0 } }
```

v0.1 *세미-어댑터 미구현*.
