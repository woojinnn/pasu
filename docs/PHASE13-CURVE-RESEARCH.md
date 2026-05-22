# Phase 13 — Curve Tier A/B Implementation Research

> **Scope**: Router NG multi-chain extension 검증 + 기존 declarative bundle 의 selector / address 전수 검증.
> **성격**: implementation research (read-only). Policy/ Tier-3 schema spec 아님.
> **출처 원칙**: 1차 — Curve 공식 GitHub Vyper + 배포 contract 의 on-chain bytecode/state. commit SHA 명시.
> **검증 도구**: Foundry `cast` 1.5.1-stable (commit `b0a9dd9ceda36f63e2326ce530c10e6916f4b8a2`). selector = `cast keccak '<sig>'` 의 첫 4 byte. on-chain 검증 = `cast call` / `cast code` (public RPC: publicnode / drpc / frax / mantle / xlayer / zksync).

---

## HEADLINE 요약

| 항목 | 결과 |
|---|---|
| Router NG 배포 chain 수 (검증) | **14 chain** (README) — EVM 13 standard + zkSync Era 1 |
| 🔴 `exchange` ABI uniformity | **BROKEN — 균일하지 않음** |
| `uint256[5][5]` 변종 (selector `0xc872a3c5`) | 10 chain: Ethereum, Optimism, BSC, Gnosis, Polygon, Fantom, Base, Arbitrum, Avalanche, Kava |
| `uint256[4][5]` 변종 (selector `0xaad348a2`) | **4 chain: Fraxtal(252), Mantle(5000), X-Layer(196), zkSync Era(324)** |
| selector 검증 FAIL | **1건 — `curve/crvusd/wsteth/selfLiquidate@1.0.0.json`** (selector `0x6cdb5a2a` = `self_liquidate(uint256)`) — 배포 Controller 에 존재하지 않는 phantom function |
| `CURVE_ROUTER_NG_ADDRESSES` Rust table 오류 | chain 목록 불일치: Kava(2222)/X-Layer(196) **누락**, zkSync(324) 주소는 맞으나 4x5 변종 — `0xc872a3c5` bundle 로 mis-decode |

---

## 1. Router NG — chain / address / version 표 + 🔴 ABI-uniformity verdict

### 1.1 Authoritative chain list

출처: `curvefi/curve-router-ng` README.md @ commit `1014d3691bd9df935dc06fc5988484b0614d1fd5` (2025-05-21, master HEAD — 본 commit 은 README 만 갱신).

README 는 **14개 chain** 의 Router NG 주소를 명시. version 은 README 에 per-chain 표기 없음 — 각 배포 contract 의 on-chain `version()` constant 를 직접 조회해 확정.

| chain_id | chain | Router NG 주소 (checksummed) | on-chain `version()` | `exchange` ABI 변종 |
|---|---|---|---|---|
| 1 | Ethereum | `0x45312ea0eFf7E09C83CBE249fa1d7598c4C8cd4e` | `1.2.0` | `uint256[5][5]` |
| 10 | Optimism | `0x0DCDED3545D565bA3B19E683431381007245d983` | `1.1.0` | `uint256[5][5]` |
| 56 | BSC | `0xA72C85C258A81761433B4e8da60505Fe3Dd551CC` | `1.1.0` | `uint256[5][5]` |
| 100 | Gnosis | `0x0DCDED3545D565bA3B19E683431381007245d983` | `1.1.0` | `uint256[5][5]` |
| 137 | Polygon | `0x0DCDED3545D565bA3B19E683431381007245d983` | `1.1.0` | `uint256[5][5]` |
| 250 | Fantom | `0x0DCDED3545D565bA3B19E683431381007245d983` | `1.1.0` | `uint256[5][5]` |
| 252 | Fraxtal | `0x56C526b0159a258887e0d79ec3a80dfb940d0cD7` | `1.1.0` | **`uint256[4][5]` ⚠️** |
| 324 | zkSync Era | `0x7C915390e109CA66934f1eB285854375D1B127FA` | `1.1.0` | **`uint256[4][5]` ⚠️** |
| 2222 | Kava | `0x0DCDED3545D565bA3B19E683431381007245d983` | `1.1.0` | `uint256[5][5]` |
| 8453 | Base | `0x4f37A9d177470499A2dD084621020b023fcffc1F` | `1.1.0` | `uint256[5][5]` |
| 42161 | Arbitrum One | `0x2191718CD32d02B8E60BAdFFeA33E4B5DD9A0A0D` | `1.1.0` | `uint256[5][5]` |
| 43114 | Avalanche | `0x0DCDED3545D565bA3B19E683431381007245d983` | `1.1.0` | `uint256[5][5]` |
| 5000 | Mantle | `0x4f37A9d177470499A2dD084621020b023fcffc1F` | `1.1.0` | **`uint256[4][5]` ⚠️** |
| 196 | X-Layer | `0xBFab8ebc836E1c4D81837798FC076D219C9a1855` | `1.1.0` | **`uint256[4][5]` ⚠️** |

> `version()` 값 주의: Fraxtal/Mantle/X-Layer/zkSync 도 `version()` constant 가 `"1.1.0"` 을 반환하지만, `exchange` 의 실제 ABI 는 `uint256[5][5]` 변종과 다르다 (아래 §1.2 참조). `version()` constant 만으로는 ABI 변종을 구분할 수 없다.

### 1.2 🔴 CRITICAL — `exchange` ABI uniformity verdict: **BROKEN**

**검증 방법** (selector grep 의 Vyper false-negative 우회). Vyper 0.3.10 dispatcher 는 raw 4-byte selector 를 항상 contiguous PUSH4 immediate 로 저장하지 않으므로 `cast code` grep 은 신뢰 불가. 대신 **revert-behavior fingerprint** 사용:

- bogus selector `0xdeadbeef` 호출 → Vyper fallback → `0x` (빈 반환, error 없음)
- 실존 function 을 zero-arg 로 호출 → 내부 로직 진입 후 `execution reverted` (revert)
- 미존재 function selector 호출 → fallback → `0x`

따라서 `0x` ↔ `execution reverted` 의 차이로 function 존재를 판별. 모든 chain 에서 bogus selector 가 `0x` 를 반환함을 baseline 으로 먼저 확인 (fallback 정상 동작 확인).

**결과** — `exchange` 6-arg (`uint256[5][5]`, selector `0xc872a3c5`) vs `exchange` 4-arg (`uint256[4][5]`, selector `0xaad348a2`):

| chain | `0xc872a3c5` (5x5) | `0xaad348a2` (4x5) | verdict |
|---|---|---|---|
| Ethereum 1 | EXISTS | absent | `uint256[5][5]` |
| Optimism 10 | EXISTS | absent | `uint256[5][5]` |
| BSC 56 | EXISTS | absent | `uint256[5][5]` |
| Gnosis 100 | EXISTS | absent | `uint256[5][5]` |
| Polygon 137 | EXISTS | absent | `uint256[5][5]` |
| Fantom 250 | EXISTS | absent | `uint256[5][5]` |
| **Fraxtal 252** | **absent** | **EXISTS** | **`uint256[4][5]` 🔴** |
| Base 8453 | EXISTS | absent | `uint256[5][5]` |
| Arbitrum 42161 | EXISTS | absent | `uint256[5][5]` |
| Avalanche 43114 | EXISTS | absent | `uint256[5][5]` |
| Kava 2222 | EXISTS | absent | `uint256[5][5]` |
| **Mantle 5000** | **absent** | **EXISTS** | **`uint256[4][5]` 🔴** |
| **X-Layer 196** | **absent** | **EXISTS** | **`uint256[4][5]` 🔴** |
| **zkSync Era 324** | **absent** | **EXISTS** | **`uint256[4][5]` 🔴** |

**근거 fingerprint 예시** (Ethereum vs Fraxtal):

```
Ethereum:
  bogus 0xdeadbeef   -> 0x                                  (fallback)
  exchange 5x5 6-arg -> execution reverted, data: "0x"       (function EXISTS)
  exchange 4x5 4-arg -> 0x                                   (function ABSENT)
Fraxtal:
  bogus 0xdeadbeef   -> 0x                                  (fallback)
  exchange 5x5 6-arg -> 0x                                   (function ABSENT)
  exchange 4x5 4-arg -> execution reverted                   (function EXISTS)
```

추가 교차 확인 — Fraxtal bytecode 에서 selector `7b5e2c7b` 가 검출되어 4byte-directory 로 역조회 결과 **`get_dx(address[11],uint256[4][5],uint256)`** 로 확정. 이는 Fraxtal Router NG 의 view function 이 `uint256[4][5]` 를 사용함을 독립적으로 입증한다 (`exchange` 의 swap_params 차원과 일치).

#### Verdict

`exchange` 의 ABI 는 **chain 간 균일하지 않다.** `_swap_params` argument 의 차원이 두 가지:

- **`uint256[5][5]`** — per-hop 5 element `[i, j, swap_type, pool_type, n_coins]`. selector `0xc872a3c5` (6-arg). **10 chain.**
- **`uint256[4][5]`** — per-hop 4 element (구버전 CurveRouter v1.0, `n_coins` 미포함 또는 packing 상이). selector `0xaad348a2` (4-arg). **4 chain — Fraxtal / Mantle / X-Layer / zkSync.**

> **함의 (구현):** 현재 bundle `registry/manifests/curve/router-ng/exchange@1.0.0.json` 은 `chain_ids:[1]` 1개 chain 만, selector `0xc872a3c5`. 이를 단순히 `chain_ids` 에 14개를 나열한 단일 multi-chain bundle 로 확장하면 **Fraxtal / Mantle / X-Layer / zkSync 4 chain 에서 callkey `<chain>__<to>__0xc872a3c5` 가 그 chain 의 실제 selector(`0xaad348a2`)와 불일치하여 registry index miss** 가 된다. 더 위험한 시나리오: 만약 어댑터가 selector 무관하게 ABI 만으로 decode 하면 `uint256[4][5]` calldata 를 `uint256[5][5]` ABI 로 잘못 decode 하여 route/amount offset 이 어긋난다. **두 ABI 변종은 별도 bundle 로 분리**해야 한다 (5x5 bundle: 10 chain / 4x5 bundle: 4 chain).
>
> zkSync Era(324) 는 추가로 zkEVM bytecode 라 `cast code` selector grep 자체가 무의미 — `cast call` revert-behavior 로만 검증 가능했고, 결과는 `uint256[4][5]`.

### 1.3 `CURVE_ROUTER_NG_ADDRESSES` (Rust Tier B table) 교차 검증

파일: `crates/adapters/abi-resolver/src/subdecode/protocols/curve.rs` line 118-179.

| Rust table chain_id | Rust table 주소 | README 주소 | 판정 |
|---|---|---|---|
| 1 | `0x45312ea0...cd4e` | `0x45312ea0...cd4e` | OK |
| 10 | `0x0DCDED35...d983` | `0x0DCDED35...d983` | OK |
| 56 | `0xA72C85C2...51CC` | `0xA72C85C2...51CC` | OK |
| 100 | `0x0DCDED35...d983` | `0x0DCDED35...d983` | OK |
| 137 | `0x0DCDED35...d983` | `0x0DCDED35...d983` | OK |
| 250 | `0x0DCDED35...d983` | `0x0DCDED35...d983` | OK |
| 252 | `0x56C526b0...0cD7` | `0x56C526b0...0cD7` | OK (단 ABI = 4x5) |
| 324 | `0x7C915390...27FA` | `0x7C915390...27FA` | OK (단 ABI = 4x5, zkEVM) |
| 8453 | `0x4f37A9d1...fc1F` | `0x4f37A9d1...fc1F` | OK |
| 42161 | `0x2191718C...0A0D` | `0x2191718C...0A0D` | OK |
| 43114 | `0x0DCDED35...d983` | `0x0DCDED35...d983` | OK |
| 5000 | `0x4f37A9d1...fc1F` | `0x4f37A9d1...fc1F` | OK (단 ABI = 4x5) |

**주소 자체는 12개 모두 정확.** 그러나:

1. **누락 chain 2개** — Rust table 에 **Kava(2222)** 와 **X-Layer(196)** 가 없다. README 는 14 chain, Rust 는 12 chain. Kava 는 5x5 변종, X-Layer 는 4x5 변종.
2. **ABI 변종 미반영** — Rust table 은 (chain_id, address) tuple 만 보유하고 `exchange` ABI 변종을 구분하지 않는다. chain 252/324/5000 (+ 누락 196) 은 `uint256[4][5]` 인데, 이 table 을 단일 `0xc872a3c5` 디코딩 경로에 연결하면 4 chain 이 mis-route 된다.
3. 주석 (line 110-114) 은 "`README.md @ master`" 출처를 명시하나 commit SHA 가 없다 — `1014d3691bd9df935dc06fc5988484b0614d1fd5` 로 pin 권장.

> **deploy 이후 신규 chain**: README @ `1014d369` 기준 14 chain 이 전부. Sonic(146) / Taiko / Hyperliquid / Corn 등은 `curve-router-ng` README 에 **미등재** (출처 미확인 — 본 repo 의 1차 출처에 없음). curve-router-ng repo 의 release tag 도 `bsc / fraxtal / xlayer / mantle / zksync / v1.1` 6개로, 이후 신규 chain release 없음.

---

## 2. Router NG `exchange` overload selector 표

출처: `curvefi/curve-router-ng/contracts/Router.vy` — master HEAD `1014d3691bd9df935dc06fc5988484b0614d1fd5` (`# @version 0.3.10`, `version = "1.2.0"`, `@title CurveRouter v1.2`). v1.1 비교: tag `v1.1` = commit `727ed48d550b26a068208babfd0b6f0f1206b4f6` (`version = "1.1.0"`).

Vyper 의 default argument 는 ABI overload 를 생성한다. `exchange` 의 Vyper signature (v1.1 / v1.2 **동일**):

```vyper
def exchange(
    _route: address[11],
    _swap_params: uint256[5][5],
    _amount: uint256,
    _min_dy: uint256,
    _pools: address[5]=empty(address[5]),
    _receiver: address=msg.sender
) -> uint256:
```

→ default arg 2개 (`_pools`, `_receiver`) 가 3개의 ABI overload 를 생성:

| # | signature | selector (`cast keccak` 첫 4B) | user-facing | 비고 |
|---|---|---|---|---|
| 4-arg | `exchange(address[11],uint256[5][5],uint256,uint256)` | `0x371dc447` | yes | `_pools`/`_receiver` 둘 다 default |
| 5-arg | `exchange(address[11],uint256[5][5],uint256,uint256,address[5])` | `0x5c9c18e2` | yes | `_receiver` default (`msg.sender`) |
| 6-arg | `exchange(address[11],uint256[5][5],uint256,uint256,address[5],address)` | `0xc872a3c5` | yes | 전체 명시 — 현재 bundle 대상 |

세 selector 모두 Ethereum(v1.2) + Optimism(v1.1) 배포 bytecode 에 PRESENT 임을 grep 으로 확인 (5x5 chain 에 한해 grep 이 일치 — 해당 contract 가 contiguous PUSH4 로 저장). bundle `exchange@1.0.0.json` 은 6-arg `0xc872a3c5` 만 target. 4-arg / 5-arg 도 user-facing 이므로 별도 bundle 필요 (현재 미커버 — gap).

**`uint256[4][5]` 변종 (Fraxtal/Mantle/X-Layer/zkSync) 의 대응 overload** — 참고용:

| # | signature | selector |
|---|---|---|
| 4-arg | `exchange(address[11],uint256[4][5],uint256,uint256)` | `0xaad348a2` |
| 5-arg | `exchange(address[11],uint256[4][5],uint256,uint256,address[5])` | `0x83cf75c8` |
| 6-arg | `exchange(address[11],uint256[4][5],uint256,uint256,address[5],address)` | `0xf0edc80e` |

> 주의: `curve-router-ng` 의 `fraxtal` release tag (`fdd5c6c73374bdf2c6b0736d02456f9f1fdbb0cf`, `@title CurveRouter v1.0`, `# @version 0.3.9`) 의 `Router.vy` source 는 `_swap_params: uint256[5][5]` 로 적혀 있어, **GitHub tag source 와 Fraxtal 실제 배포 contract 가 불일치**한다. 배포 contract 는 on-chain 검증상 `uint256[4][5]` — tag source 보다 더 오래된/다른 빌드. 따라서 `uint256[4][5]` 변종의 정확한 per-hop semantics 는 GitHub source 로 확정 불가 (출처 미확인 — 배포 bytecode 의 `aad348a2`/`7b5e2c7b` selector 와 fingerprint 만 확정). 4x5 변종 bundle 작성 시 해당 chain 의 verified Etherscan-equivalent source 를 별도 확보 필요.

---

## 3. `Router.vy::exchange` early-break 루프 (verbatim 인용)

출처: `curvefi/curve-router-ng/contracts/Router.vy` @ `1014d3691bd9df935dc06fc5988484b0614d1fd5`, line 222-345 (`# @version 0.3.10`, v1.2.0).

루프 본문 (5-round iteration + zero-pool early-break):

```vyper
    for i in range(5):
        # 5 rounds of iteration to perform up to 5 swaps
        swap: address = _route[i * 2 + 1]
        pool: address = _pools[i]  # Only for Polygon meta-factories underlying swap (swap_type == 6)
        output_token = _route[(i + 1) * 2]
        params: uint256[5] = _swap_params[i]  # i, j, swap_type, pool_type, n_coins

        # store the initial balance of the output_token
        output_token_initial_balance: uint256 = self.balance
        if output_token != ETH_ADDRESS:
            output_token_initial_balance = ERC20(output_token).balanceOf(self)

        if not self.is_approved[input_token][swap]:
            assert ERC20(input_token).approve(swap, max_value(uint256), default_return_value=True, skip_contract_check=True)
            self.is_approved[input_token][swap] = True

        ...
        # [swap branches: params[2] == 1..9, else "Bad swap type"]
        ...

        # update the amount received
        if output_token == ETH_ADDRESS:
            amount = self.balance
        else:
            amount = ERC20(output_token).balanceOf(self)

        # sanity check, if the routing data is incorrect we will have a 0 balance change and that is bad
        assert amount - output_token_initial_balance != 0, "Received nothing"

        # check if this was the last swap
        if i == 4 or _route[i * 2 + 3] == empty(address):
            break
        # if there is another swap, the output token becomes the input for the next round
        input_token = output_token
```

루프 종료 후:

```vyper
    amount -= 1  # Change non-zero -> non-zero costs less gas than zero -> non-zero
    assert amount >= _min_dy, "Slippage"
```

**핵심 (P1-5 — Tier B 버그 수정 anchor):**

- `_route` 는 `[token0, swap1, token1, swap2, token2, ...]` 의 **interleaved** 11-slot 배열. index 0,2,4,6,8,10 = token, index 1,3,5,7,9 = pool/zap.
- break 조건은 **`i == 4` OR `_route[i*2+3] == empty(address)`**. 즉 i 번째 hop 직후, **다음 hop 의 pool slot** `_route[i*2+3]` 이 zero 면 더 이상 swap 안 함.
- i=0 의 다음 pool slot = `_route[3]`, i=1 → `_route[5]`, i=2 → `_route[7]`, i=3 → `_route[9]`.
- 최종 output token 은 break 시점의 `output_token = _route[(i+1)*2]`. 즉 마지막으로 실행된 hop 의 output index.
- docstring: *"The array is iterated until a pool address of 0x00, then the last given token is transferred to `_receiver`"*.

> **현재 bundle 의 `curve_route_last_token` BuiltinFn 영향**: bundle `exchange@1.0.0.json` 의 `outputToken.asset.address` 는 `curve_route_last_token(_route)` 로 last token 을 도출한다. 정확한 last token 은 **"`_route[i*2+1]` 이 zero 가 되기 직전의 가장 큰 짝수 index token"** = pool slot (홀수 index 1,3,5,7,9) 중 첫 zero 의 직전 token slot. 단순히 `_route` 의 마지막 non-zero 주소를 취하면 안 된다 (route 배열 끝쪽이 zero-padding 이므로 우연히 맞을 수 있으나, **token 과 pool 이 interleave 되어 마지막 non-zero 가 token 일 수도 pool 일 수도 있음**). 정확한 규칙: pool slot (`idx % 2 == 1`) 을 1,3,5,7,9 순으로 스캔, 첫 zero pool slot `p` 발견 시 output token = `_route[p-1]`; 9까지 모두 non-zero 면 output token = `_route[10]`. 이 규칙이 Vyper 의 `output_token = _route[(i+1)*2]` + `break on _route[i*2+3]==0` 와 정확히 등가다. Tier B `curve_route_last_token` 가 이 규칙으로 구현되었는지 별도 검토 필요.

---

## 4. crvUSD / Gauge / GaugeController / veCRV selector 검증 표

`cast keccak` 으로 계산한 selector vs bundle JSON 의 `match.selector`. crvUSD 는 wstETH market (`0x100daa78...`) bundle 15개 전수.

| Contract | function signature | `cast keccak` selector | bundle selector | 판정 |
|---|---|---|---|---|
| crvUSD Controller | `create_loan(uint256,uint256,uint256)` | `0x23cfed03` | `0x23cfed03` | **PASS** |
| crvUSD Controller | `create_loan_extended(uint256,uint256,uint256,address,uint256[])` | `0xbc61ea23` | `0xbc61ea23` | **PASS** |
| crvUSD Controller | `borrow_more(uint256,uint256)` | `0xdd171e7c` | `0xdd171e7c` | **PASS** |
| crvUSD Controller | `add_collateral(uint256)` | `0x6f972f12` | `0x6f972f12` | **PASS** |
| crvUSD Controller | `add_collateral(uint256,address)` | `0x24049e57` | `0x24049e57` | **PASS** |
| crvUSD Controller | `remove_collateral(uint256)` | `0xd14ff5b6` | `0xd14ff5b6` | **PASS** |
| crvUSD Controller | `remove_collateral(uint256,bool)` | `0x2e4af52a` | `0x2e4af52a` | **PASS** |
| crvUSD Controller | `repay(uint256)` | `0x371fd8e6` | `0x371fd8e6` | **PASS** |
| crvUSD Controller | `repay(uint256,address,int256)` | `0xb4440df4` | `0xb4440df4` | **PASS** |
| crvUSD Controller | `repay(uint256,address,int256,bool)` | `0x37671f93` | `0x37671f93` | **PASS** |
| crvUSD Controller | `repay_extended(address,uint256[])` | `0x152f65cb` | `0x152f65cb` | **PASS** |
| crvUSD Controller | `liquidate(address,uint256)` | `0xbcbaf487` | `0xbcbaf487` | **PASS** |
| crvUSD Controller | `liquidate(address,uint256,bool)` | `0x3ecdb828` | `0x3ecdb828` | **PASS** |
| crvUSD Controller | `liquidate_extended(address,uint256,uint256,bool,address,uint256[])` | `0x036aed88` | `0x036aed88` | **PASS** |
| crvUSD Controller | `self_liquidate(uint256)` | `0x6cdb5a2a` | `0x6cdb5a2a` | **🔴 FAIL** — 아래 §4.1 |
| LiquidityGauge | `deposit(uint256)` | `0xb6b55f25` | `0xb6b55f25` | **PASS** |
| LiquidityGauge | `deposit(uint256,address)` | `0x6e553f65` | `0x6e553f65` | **PASS** |
| LiquidityGauge | `withdraw(uint256)` | `0x2e1a7d4d` | `0x2e1a7d4d` | **PASS** |
| LiquidityGauge | `claim_rewards()` | `0xe6f1daf2` | `0xe6f1daf2` | **PASS** |
| LiquidityGauge | `claim_rewards(address)` | `0x84e9bd7e` | `0x84e9bd7e` | **PASS** |
| GaugeController | `vote_for_gauge_weights(address,uint256)` | `0xd7136328` | `0xd7136328` | **PASS** |
| veCRV VotingEscrow | `create_lock(uint256,uint256)` | `0x65fc3873` | `0x65fc3873` | **PASS** |
| veCRV VotingEscrow | `deposit_for(address,uint256)` | `0x3a46273e` | `0x3a46273e` | **PASS** |
| veCRV VotingEscrow | `increase_amount(uint256)` | `0x4957677c` | `0x4957677c` | **PASS** |
| veCRV VotingEscrow | `increase_unlock_time(uint256)` | `0xeff7a612` | `0xeff7a612` | **PASS** |
| veCRV VotingEscrow | `withdraw()` | `0x3ccfd60b` | `0x3ccfd60b` | **PASS** |

**합계: 25 PASS / 1 FAIL.** selector 산술 자체는 25건 모두 bundle 과 일치 — `cast keccak` ↔ `match.selector` 의 hex 가 동일. FAIL 1건은 selector 산술 오류가 아니라 **해당 function 이 배포 contract 에 존재하지 않는** 문제 (§4.1).

### 4.1 🔴 FAIL — `selfLiquidate@1.0.0.json` 은 phantom function

bundle `curve/crvusd/wsteth/selfLiquidate@1.0.0.json` 은 `self_liquidate(uint256)` (selector `0x6cdb5a2a`) 를 정의. 그러나:

- 배포 wstETH Controller `0x100daa78fc509db39ef7d04de0c1abd299f4c6ce` 의 bytecode 에 `0x6cdb5a2a` **부재** (selector grep). sanity check: random `0xdeadbeef` 부재 + 실존 `0x23cfed03` (`create_loan`) 존재로 grep 신뢰성 확인.
- `self_liquidate(uint256)` / `self_liquidate(uint256,uint256)` / `self_liquidate(uint256,bool)` / `self_liquidate()` 4개 변종 모두 부재.
- crvUSD Controller source (`curve-stablecoin` tag `v1` = `edbb5ef5bf421d4222f4571f1884f7c8e6c6fc7c`, `contracts/Controller.vy`, `# @version 0.3.10`) 에 `self_liquidate` 라는 standalone function 자체가 **없다**. `grep self_liquidate` 결과 = `liquidate` / `liquidate_extended` 의 docstring 안 *"Perform a bad liquidation (or self-liquidation) of user"* 문구뿐.

**crvUSD 의 self-liquidation 은 별도 function 이 아니다.** 사용자가 자기 포지션을 청산하려면 `liquidate(user, min_x)` 를 `user == msg.sender` 로 호출한다 (`liquidate` 내부에서 `_check_approval(user)` 통과 시 discount 0 적용). 즉 self-liquidation = `liquidate` 의 actor 케이스이지 별도 entry 가 아니다.

> **구현 권고**: `selfLiquidate@1.0.0.json` bundle 은 **삭제** 하거나, "self-liquidate" 의도를 `liquidate` (`0xbcbaf487`) bundle 에 흡수해야 한다 (actor = self 케이스). 현 상태로 두면 callkey index 에 실제로 매칭될 calldata 가 영원히 없는 dead bundle 이다 (registry 오염). 15개 wstETH bundle 중 14개만 유효.

> **✅ 2026-05-21 해소 (R1-F1)** — 3 market (wstETH/sfrxETH/WBTC) Controller 전수 재검증: `cast code` bytecode 에 `0x6cdb5a2a` 부재 (sanity: `0x23cfed03`·`0xbcbaf487`·`0x3ecdb828` 존재 / `0xdeadbeef` 부재 — grep 신뢰 재확인). `cast call 0x6cdb5a2a` 는 bogus selector 와 **byte 단위 동일**하게 `execution reverted, data: "0x"` 반환 — Controller 가 미존재 selector 에 revert 하므로 revert-probe 는 present/absent 를 구분 못 한다. (작업 중간 plan 의 revert-probe "PRESENT" 판정은 이 false-positive 였음 — 본 §4.1 의 grep 판정이 옳다.) → `crvusd/{wsteth,sfrxeth,wbtc}/selfLiquidate@1.0.0.json` 3 bundle + callkey 삭제. self-liquidation 은 `liquidate@1.0.0.json` (`borrower`←`$.args.user`) 가 이미 커버 — 커버리지 손실 없음.

### 4.2 P2-1 — `repay` 의 `_max_active_band` parameter 타입

**확정: `int256`.**

- crvUSD Controller source (`v1` tag, `Controller.vy` line 928): `def repay(_d_debt: uint256, _for: address = msg.sender, max_active_band: int256 = 2**255-1):` — Vyper signature 가 명시적으로 `int256`.
- bundle `repay-3arg@1.0.0.json` / `repay@1.0.0.json` 도 `max_active_band` 를 `int256` 으로 ABI 정의 — source 와 일치.
- selector 교차 검증: `repay(uint256,address,int256)` = `0xb4440df4` (배포 contract bytecode 에 PRESENT), `repay(uint256,address,uint256)` = `0x1f3a1272` (부재). 배포 contract 는 `int256` 변종을 노출.
- default 값 `2**255-1` = `int256` max → 사실상 "제한 없음" sentinel. front-run 방지용 ceiling. parameter 가 `int256` 인 이유: LLAMMA band index 는 음수 가능 (가격축 좌우로 음/양 band).

bundle 3개 (`repay-3arg`, `repay`) 모두 `int256` 사용 — **PASS**.

### 4.3 crvUSD Controller 주소 검증 (3 market)

`Controller.collateral_token()` 을 Ethereum mainnet 에서 `cast call` 로 직접 조회 (publicnode RPC):

| market | Controller 주소 (bundle `match.to`) | on-chain `collateral_token()` | 기대 collateral | 판정 |
|---|---|---|---|---|
| wstETH | `0x100daa78fc509db39ef7d04de0c1abd299f4c6ce` | `0x7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0` | wstETH | **PASS** |
| sfrxETH | `0xEC0820EfafC41D8943EE8dE495fC9Ba8495B15cf` | `0xac3E018457B222d93114458476f3E3416Abbe38F` | sfrxETH | **PASS** |
| WBTC | `0x4e59541306910ad6dc1dac0ac9dfb29bd9f15c67` | `0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599` | WBTC | **PASS** |

3개 Controller 주소 모두 on-chain `collateral_token()` 이 기대 collateral 토큰과 일치. `crates/.../curve.rs` 의 `CRVUSD_CONTROLLERS` table 도 동일 3 entry — 정확. (단 WBTC market 은 sfrxETH 와 동일하게 v1 controller 로, sfrxETH/WBTC 는 다른 2 market 과 signature 를 공유 — 별도 selector 검증 불필요.)

### 4.4 검증된 부속 contract 주소 (mainnet)

| contract | bundle `match.to` / Rust const | on-chain 검증 |
|---|---|---|
| veCRV VotingEscrow | `0x5f3b5DfeB7B28CDbD7FaBa78963EE202a494e2A2` | `symbol()` = `"veCRV"` — PASS |
| GaugeController | `0x2F50D538606Fa9EDD2B11E2446BEb18C9D5846bb` | `admin()` = `0x40907540...9968` (Curve Ownership Admin) — PASS |
| Gauge: 3pool/3CRV | `0xbFcF63294aD7105dEa65aA58F8AE5BE2D9D0952A` | `lp_token()` = `0x6c3F90f0...E490` (3CRV) — PASS |
| Gauge: stETH | `0x182B723a58739a9c974cFDB385ceaDb237453c28` | `lp_token()` = `0x06325440...f14E` (steCRV) — PASS |
| Gauge: frxETH | `0x2932a86df44Fe8D2A706d8e9c5d51c24883423F5` | `lp_token()` = `0xf4321193...6C7A` (frxETHCRV) — PASS |

---

## 5. crvUSD Controller — 전체 overload 카탈로그

출처: `curvefi/curve-stablecoin/contracts/Controller.vy` @ tag `v1` (`edbb5ef5bf421d4222f4571f1884f7c8e6c6fc7c`, `# @version 0.3.10`, `@title crvUSD Controller`).

Controller Vyper signature (default arg 포함) + on-chain bytecode 검증 (wstETH Controller `0x100daa78...`). Vyper default arg 가 ABI overload 를 생성:

### create_loan
Vyper: `def create_loan(collateral: uint256, debt: uint256, N: uint256, _for: address = msg.sender)`
| ABI overload | selector | bytecode 검증 | bundle |
|---|---|---|---|
| `create_loan(uint256,uint256,uint256)` | `0x23cfed03` | PRESENT | `createLoan@1.0.0.json` |
| `create_loan(uint256,uint256,uint256,address)` | (계산 생략) | (미검증) | bundle 없음 — `_for` 명시 변종 |

### create_loan_extended
Vyper: `def create_loan_extended(collateral: uint256, debt: uint256, N: uint256, callbacker: address, callback_args: DynArray[uint256,5], callback_bytes: Bytes[10**4] = b"", _for: address = msg.sender)`
| ABI overload | selector | bundle |
|---|---|---|
| `create_loan_extended(uint256,uint256,uint256,address,uint256[])` | `0xbc61ea23` | `createLoanExtended@1.0.0.json` |
| `create_loan_extended(uint256,uint256,uint256,address,uint256[],bytes)` | (생략) | bundle 없음 — `callback_bytes` 명시 |
| `create_loan_extended(uint256,uint256,uint256,address,uint256[],bytes,address)` | (생략) | bundle 없음 — `callback_bytes`+`_for` 명시 |

> bundle `createLoanExtended` 의 ABI 는 `callback_args: uint256[]` 까지 5-arg. source 의 `callback_args` 는 `DynArray[uint256,5]` (최대 5 element 동적배열) → ABI canonical 로는 `uint256[]`. 일치.

### borrow_more
Vyper: `def borrow_more(collateral: uint256, debt: uint256, _for: address = msg.sender)`
| ABI overload | selector | bundle |
|---|---|---|
| `borrow_more(uint256,uint256)` | `0xdd171e7c` | `borrowMore@1.0.0.json` |
| `borrow_more(uint256,uint256,address)` | (생략) | bundle 없음 — `_for` 명시 |

### borrow_more_extended
Vyper: `def borrow_more_extended(collateral: uint256, debt: uint256, callbacker: address, callback_args: DynArray[uint256,5], callback_bytes: Bytes[10**4] = b"", _for: address = msg.sender)`
| ABI overload | selector | bundle |
|---|---|---|
| `borrow_more_extended(uint256,uint256,address,uint256[])` | `0x36b7dbb7` | **bundle 없음** — 커버 gap |
| (+`bytes`, +`bytes,address` 변종) | (생략) | bundle 없음 |

### add_collateral
Vyper: `def add_collateral(collateral: uint256, _for: address = msg.sender)`
| ABI overload | selector | bytecode | bundle |
|---|---|---|---|
| `add_collateral(uint256)` | `0x6f972f12` | PRESENT | `addCollateral@1.0.0.json` |
| `add_collateral(uint256,address)` | `0x24049e57` | PRESENT | `addCollateral-for@1.0.0.json` |

### remove_collateral
Vyper (`v1` tag): `def remove_collateral(collateral: uint256, _for: address = msg.sender)` — **NOTE: `v1` tag 는 2번째 arg 가 `address`**
배포 wstETH Controller bytecode 검증:
| ABI overload | selector | bytecode (wstETH) | bundle |
|---|---|---|---|
| `remove_collateral(uint256)` | `0xd14ff5b6` | PRESENT | `removeCollateral@1.0.0.json` |
| `remove_collateral(uint256,bool)` | `0x2e4af52a` | **PRESENT** | `removeCollateral-useEth@1.0.0.json` |
| `remove_collateral(uint256,address)` | `0xe4f9df10` | **ABSENT** | bundle 없음 |

> 🔴 **버전 불일치 발견**: `v1` GitHub tag 의 `Controller.vy` 는 `remove_collateral(collateral, _for: address)` — 2번째 arg 가 `address`. 그러나 **배포된 wstETH Controller bytecode 에는 `remove_collateral(uint256,bool)` (`use_eth`, `0x2e4af52a`) 가 PRESENT 이고 `remove_collateral(uint256,address)` (`0xe4f9df10`) 는 ABSENT**. 즉 배포 contract 는 `v1` tag 보다 **더 오래된** Controller 빌드로, 2번째 arg 가 `bool use_eth` 다. bundle `removeCollateral-useEth@1.0.0.json` (`uint256,bool`) 는 **배포 contract 와 일치 — 정확**. bundle 이 옳고 GitHub `v1` tag source 가 배포본보다 신버전.

### repay
배포 wstETH Controller bytecode 검증 (`v1` tag source: `repay(_d_debt, _for: address, max_active_band: int256)` 3-arg):
| ABI overload | selector | bytecode (wstETH) | bundle |
|---|---|---|---|
| `repay(uint256)` | `0x371fd8e6` | PRESENT | `repay-1arg@1.0.0.json` |
| `repay(uint256,address)` | `0xacb70815` | (미검증) | bundle 없음 |
| `repay(uint256,address,int256)` | `0xb4440df4` | PRESENT | `repay-3arg@1.0.0.json` |
| `repay(uint256,address,int256,bool)` | `0x37671f93` | **PRESENT** | `repay@1.0.0.json` |

> `v1` tag source 의 `repay` 는 3-arg 까지 (`use_eth` 없음). 그러나 배포 wstETH Controller 는 `repay(uint256,address,int256,bool)` 4-arg (`use_eth`, `0x37671f93`) 가 PRESENT — 다시 한번 배포본이 `v1` tag 보다 구버전(`use_eth` arg 보유). bundle `repay@1.0.0.json` (4-arg) 는 배포본과 일치 — 정확.

### repay_extended
Vyper (`v1` tag): `def repay_extended(callbacker: address, callback_args: DynArray[uint256,5], callback_bytes: Bytes[10**4] = b"", _for: address = msg.sender)`
| ABI overload | selector | bytecode | bundle |
|---|---|---|---|
| `repay_extended(address,uint256[])` | `0x152f65cb` | PRESENT | `repayExtended@1.0.0.json` |
| (+`bytes`, +`bytes,address`) | (생략) | (미검증) | bundle 없음 |

### liquidate
배포 wstETH Controller bytecode 검증:
| ABI overload | selector | bytecode (wstETH) | bundle |
|---|---|---|---|
| `liquidate(address,uint256)` | `0xbcbaf487` | PRESENT | `liquidate@1.0.0.json` |
| `liquidate(address,uint256,bool)` | `0x3ecdb828` | **PRESENT** | `liquidate-useEth@1.0.0.json` |

> `v1` tag source 의 `liquidate` 는 2-arg `liquidate(user, min_x)` 만 (`use_eth` 없음). 배포본은 `liquidate(address,uint256,bool)` 3-arg (`0x3ecdb828`) PRESENT — bundle `liquidate-useEth@1.0.0.json` 와 일치. 정확.

### liquidate_extended
Vyper (`v1` tag): `def liquidate_extended(user: address, min_x: uint256, frac: uint256, callbacker: address, callback_args: DynArray[uint256,5], callback_bytes: Bytes[10**4] = b"")`
| ABI overload | selector | bytecode | bundle |
|---|---|---|---|
| `liquidate_extended(address,uint256,uint256,bool,address,uint256[])` | `0x036aed88` | PRESENT | `liquidateExtended@1.0.0.json` |

> bundle ABI = `(address user, uint256 min_x, uint256 frac, bool use_eth, address callbacker, uint256[] callback_args)`. `v1` tag source 는 `(user, min_x, frac, callbacker, callback_args, callback_bytes)` — **arg 순서/구성이 다름** (배포본은 `use_eth` bool 이 4번째, source 는 callbacker 가 4번째). 배포 contract 에 `0x036aed88` 이 PRESENT 라는 사실은 배포본이 `use_eth` 포함 변종임을 입증. bundle 이 배포본과 일치. 정확.

### self_liquidate
**존재하지 않음.** `v1` tag source 에 standalone `self_liquidate` def 없음. 배포 contract bytecode 에 `0x6cdb5a2a` 부재. bundle `selfLiquidate@1.0.0.json` = phantom (§4.1).

> **요약 — 배포본 vs GitHub `v1` tag 의 체계적 불일치**: 배포된 wstETH/sfrxETH/WBTC mint-market Controller (2023 배포) 는 `remove_collateral` / `repay` / `liquidate` 에 **`use_eth: bool` argument 를 추가로 가진 구버전** Controller 다. `curve-stablecoin` repo 의 유일한 tag `v1` (`edbb5ef5`) 의 `Controller.vy` 는 이 `use_eth` arg 가 제거된 **신버전** source 라 배포본과 다르다. 또한 master HEAD (`8a98f2043d3f4f2b0eb14c24e89d21df32e4bba6`) 의 `curve_stablecoin/controller.vy` 는 **Vyper 0.4.3** 의 완전히 재작성된 버전 (path 도 `curve_stablecoin/controller.vy` 로 이동, lending/ 디렉토리 신설). 본 PoC bundle 의 선택 (`use_eth` 변종) 은 **on-chain 배포 bytecode 검증 결과 정확** — bundle 작성자가 GitHub source 가 아니라 실제 배포 contract 의 ABI 를 따랐음. bundle 의 1차 출처는 "배포 contract bytecode" 로 명시하는 것이 정직하다 (GitHub tag 가 아님).

---

## 6. P1-6 — frxETH/ETH pool 및 LP token 주소

**frxETH/ETH pool contract (Ethereum mainnet): `0xa1F8A6807c402E4A15ef4EBa36528A3FED24E577`**

on-chain 검증 (`cast call`, publicnode RPC):
- `coins(0)` = `0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE` (ETH sentinel)
- `coins(1)` = `0x5E8422345238F34275888049021821E8E08CAa1f` (frxETH 토큰)
- → ETH/frxETH 2-coin pool 확정. bundle `stableswap/frxeth/*` 의 `match.to` 와 일치.

**LP token 은 pool 과 별개 contract.**

- pool `0xa1F8A6807c...` 에 `name()` / `symbol()` / `totalSupply()` 호출 → **모두 `execution reverted`**. 즉 이 pool 은 ERC-20 interface 를 노출하지 않는다 — pool ≠ LP token.
- 이 pool 은 **StableSwap factory pool** 이라 pool/LP 가 분리되어 있다 (legacy non-factory Curve pool 은 pool 자신이 LP — 3pool/steth 가 그 케이스: 단, 3pool 도 실제로는 LP `0x6c3F90f0...` 가 별도. legacy "pool = LP" 패턴은 더 오래된 일부 pool 한정).
- **frxETH/ETH LP token: `0xf43211935C781D5ca1a41d2041F397B8A7366C7A`**
  - `name()` = `"Curve.fi ETH/frxETH"`, `symbol()` = `"frxETHCRV"`
  - `minter()` = `0xa1F8A6807c402E4A15ef4EBa36528A3FED24E577` (= pool) — LP↔pool 양방향 확정.
  - 교차 검증: frxETH gauge `0x2932a86df44Fe8D2A706d8e9c5d51c24883423F5` 의 `lp_token()` 이 정확히 `0xf43211935C...` 를 반환.

> **정정**: 작업 지시문의 "For legacy Curve pools the pool contract IS the LP token" 전제는 frxETH/ETH pool 에는 **해당하지 않는다**. frxETH/ETH pool 은 factory pool 이라 pool(`0xa1F8A6807c...`) 과 LP token(`0xf43211935C...`) 이 별도다. (pool=LP 패턴은 3pool 같은 더 오래된 일부 비-factory pool 에만 적용되며, 그조차 3pool 의 LP 3CRV `0x6c3F90f0...` 는 별도 contract다 — "pool=LP" 는 stETH steCRV `0x06325440...`, frxETHCRV 등에는 적용 안 됨.) stableswap bundle 이 `add_liquidity` / `remove_liquidity` 의 `match.to` 로 **pool 주소** 를 쓰는 것은 옳다 (해당 함수는 pool 에 있음). 단 LP token 잔액/approve 를 평가하려면 LP token 주소 `0xf43211935C...` 가 별도로 필요하다 — host:onchain enrichment 또는 token registry 에서 보강해야 한다.

---

## 7. EIP-712 typed-data 표면

작업 지시대로 thin 검증만 (deep-dive 아님).

| 대상 contract | EIP-2612 `permit` | DOMAIN_SEPARATOR | 비고 |
|---|---|---|---|
| **crvUSD 토큰** `0xf939e0A03fB07f59A73314E73794Be0E57ac1b4e` | **있음** | `0x7906987b...a7f0` | `version()` = `"v1.0.0"`. standard `permit(address,address,uint256,uint256,uint8,bytes32,bytes32)` (`0xd505accf`) — bytecode 에 PRESENT |
| Stableswap-NG LP (예: crvUSD/USDC pool `0x4DEcE678ceceb27446b35C672dC7d61F30bAD69E`) | 있음 (deployed `v6.0.1`) | `0x00c585b7...8e8c` | `nonces()` 노출. `permit` 시그니처는 `permit(address,address,uint256,uint256,uint8,bytes32,bytes32)` — master source(`stableswap-ng` `CurveStableSwapNG.vy`, `v7.0.0`) 기준 동일. `cast call permit(...)` 가 `execution reverted` (function 존재, 내부 서명 검증 revert) 로 확인. `cast code` selector grep 은 Vyper 0.3.10 dispatcher 특성상 false-negative — grep 신뢰 불가, behavioral test 로만 확정 |
| 레거시 StableSwap LP (예: frxETHCRV `0xf43211935C781D5ca1a41d2041F397B8A7366C7A`) | **없음** | revert | `CurveTokenV3` 계열 — `permit` 미지원. `0xd505accf` 부재 + `DOMAIN_SEPARATOR()` revert |
| crvUSD Controller / Gauge / GaugeController / veCRV | 없음 | — | 이들은 토큰이 아님. 서명 표면 없음. (단 veCRV 는 transfer 불가 토큰 — permit 무의미) |

**EIP-712 요약**: Curve 의 사용자 wallet 서명 표면은 **thin**. crvUSD 토큰과 Stableswap-NG LP 토큰이 standard EIP-2612 `permit` (`0xd505accf`) 을 노출 — 둘 다 `(owner, spender, value, deadline, v, r, s)` 형태로, **기존 generic eip2612 adapter (`crates/adapters/sign-resolver/src/adapters/eip2612`) 로 커버 가능**. 레거시 StableSwap LP (3CRV / steCRV / frxETHCRV) 는 permit 미지원. Curve-specific 한 별도 typed-data (Permit2 류, delegation 서명 등) 는 발견되지 않음. EIP-712 영역은 Phase 13 에서 신규 adapter 작업 불필요 — 기존 eip2612 adapter 의 chain/contract coverage 에 crvUSD·Stableswap-NG LP 가 포함되는지만 확인하면 됨.

> Stableswap-NG `permit` 의 EIP712 domain 은 `EIP712Domain(string name,string version,uint256 chainId,address verifyingContract,bytes32 salt)` — **`salt` field 포함 5-field domain** (`CurveStableSwapNG.vy` line 225). 일반적 EIP-2612 의 4-field domain (`name,version,chainId,verifyingContract`) 과 다르다. generic eip2612 adapter 가 domain 을 재구성해 검증한다면 이 `salt` field 를 반영해야 한다 (단 ScopeBall 은 정적분석 — domain hash 재계산이 아니라 서명 의도 추출이면 영향 적음).

---

## 8. 출처

모든 사실은 아래 1차 출처 (Curve 공식 GitHub Vyper + 배포 contract on-chain 상태) 기반. selector = `cast keccak` (Foundry 1.5.1-stable, commit `b0a9dd9ceda36f63e2326ce530c10e6916f4b8a2`).

### GitHub repository + commit SHA

| repo | commit / tag | 사용 파일 |
|---|---|---|
| `github.com/curvefi/curve-router-ng` | master HEAD `1014d3691bd9df935dc06fc5988484b0614d1fd5` (2025-05-21) | `README.md`, `contracts/Router.vy` (`# @version 0.3.10`, `CurveRouter v1.2`, `version="1.2.0"`) |
| `github.com/curvefi/curve-router-ng` | tag `v1.1` = `727ed48d550b26a068208babfd0b6f0f1206b4f6` | `contracts/Router.vy` (`version="1.1.0"`) — v1.1↔v1.2 `exchange` ABI 동일 확인 |
| `github.com/curvefi/curve-router-ng` | tag `fraxtal` = `fdd5c6c73374bdf2c6b0736d02456f9f1fdbb0cf` | `contracts/Router.vy` (`CurveRouter v1.0`, `# @version 0.3.9`) — 단 배포본과 불일치 (§2 주석) |
| `github.com/curvefi/curve-stablecoin` | tag `v1` = `edbb5ef5bf421d4222f4571f1884f7c8e6c6fc7c` | `contracts/Controller.vy` (`# @version 0.3.10`, `crvUSD Controller`) |
| `github.com/curvefi/curve-stablecoin` | master HEAD `8a98f2043d3f4f2b0eb14c24e89d21df32e4bba6` (2026-05-18) | `curve_stablecoin/controller.vy` (`# pragma version 0.4.3`) — 신버전, 배포본과 다름 |
| `github.com/curvefi/stableswap-ng` | master/main HEAD (조회일 2026-05-21) | `contracts/main/CurveStableSwapNG.vy` (`# pragma version 0.3.10`, `version="v7.0.0"`) — EIP-2612 permit 시그니처 |

### release tag 목록 (curve-router-ng)

`bsc` (`fd06cd05`), `fraxtal` (`fdd5c6c7`), `xlayer` (`5e835a86`), `mantle` (`30ffd14c`), `zksync` (`a970119c`), `v1.1` (`727ed48d`) — 6개. v1.1 이후 추가 release 없음.

### on-chain 검증 RPC

- Ethereum mainnet: `https://ethereum-rpc.publicnode.com`
- L2/sidechain: `optimism-rpc.publicnode.com`, `bsc-rpc.publicnode.com`, `gnosis-rpc.publicnode.com`, `polygon-bor-rpc.publicnode.com`, `rpcapi.fantom.network`, `rpc.frax.com`, `base-rpc.publicnode.com`, `arbitrum-one-rpc.publicnode.com`, `avalanche-c-chain-rpc.publicnode.com`, `rpc.mantle.xyz`, `kava-evm-rpc.publicnode.com`, `rpc.xlayer.tech`, `mainnet.era.zksync.io`

### 검증 대상 ScopeBall 파일

- `crates/adapters/abi-resolver/src/subdecode/protocols/curve.rs` — `CURVE_ROUTER_NG_ADDRESSES` (12 entry), `CRVUSD_CONTROLLERS` (3 entry), 부속 주소 const
- `registry/manifests/curve/router-ng/exchange@1.0.0.json` — `chain_ids:[1]`, selector `0xc872a3c5`
- `registry/manifests/curve/crvusd/wsteth/*.json` (15개) — crvUSD Controller bundle
- `registry/manifests/curve/gauge/{3pool,steth,frxeth}/*.json` — LiquidityGauge bundle
- `registry/manifests/curve/gauge-controller/voteForGaugeWeights@1.0.0.json`
- `registry/manifests/curve/vecrv/*.json` (5개) — VotingEscrow bundle
- `registry/manifests/curve/stableswap/{3pool,steth,frxeth}/*.json` — legacy StableSwap pool bundle

### 검증 한계 (정직한 명시)

1. **Etherscan API key 부재** — Etherscan V2 `getabi` 미사용. 배포 contract 의 ABI 는 verified source 가 아니라 **bytecode selector probe + `cast call` revert-behavior fingerprint** 로 확정. 이 방법은 selector 존재/부재 판별에는 충분히 신뢰 가능 (bogus selector baseline 으로 fallback 동작 확인 + grep sanity check) 하나, function 의 정확한 arg 이름/내부 동작은 GitHub source 에 의존.
2. **Vyper selector grep false-negative** — Vyper 0.3.10+ dispatcher 가 raw 4-byte selector 를 항상 contiguous PUSH4 immediate 로 저장하지 않음. `cast code | grep` 은 5x5 chain 의 Router (구 Vyper) 및 crvUSD Controller 에는 일치했으나, 신 dispatcher (Fraxtal Router, Stableswap-NG `v6.0.1`) 에는 false-negative 가 발생. 따라서 ABI uniformity verdict (§1.2) 와 Stableswap-NG permit (§7) 은 grep 이 아니라 **`cast call` revert-behavior fingerprint** 로 확정 (이 방법은 false-negative 없음 — function 존재 시 반드시 revert-with-data, 부재 시 fallback `0x`).
3. **`uint256[4][5]` 변종의 per-hop semantics 미확정** — Fraxtal/Mantle/X-Layer/zkSync 배포 Router 가 GitHub `fraxtal` tag source 와 불일치하여, 4x5 변종의 정확한 `_swap_params` 4-element 구성 (`[i,j,swap_type,pool_type]`? `n_coins` 위치?) 은 1차 출처로 확정 불가 — "출처 미확인". 4x5 bundle 작성 시 해당 chain block explorer 의 verified source 별도 확보 필요.
4. zkSync Era (324) 는 zkEVM bytecode — `cast code` 무의미. `cast call` revert-behavior 로만 검증 (4x5 변종 확정).
