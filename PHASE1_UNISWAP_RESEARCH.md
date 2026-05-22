# Phase 1 — Uniswap Tier A/B Research

ScopeBall Uniswap 신규 protocol 추가 Phase 1 조사 결과. 6 router (V2 Router02 / V3 SwapRouter / SwapRouter02 / V3 NonfungiblePositionManager / Permit2 / Universal Router) 의 13 target chain 배포 주소 matrix + user-facing 함수 / overload inventory + Permit2 EIP-712 struct.

- **산출물 1 (배포 주소 matrix)**: `registry/scripts/uniswap-deployments.json`
- 13 target chain: `1 8453 10 42161 137 43114 81457 56 42220 57073 130 480 7777777`
- read-only / pure · admin / governance · internal helper 함수는 제외 (scope 룰)

---

## 1. 함수 / overload inventory

selector = `keccak256(canonicalSignature)` 의 첫 4 byte (본 조사에서 pure-python keccak-256 으로 계산, EIP-55 self-test 통과).

"기존 manifest" 열 = `registry/manifests/uniswap/` 의 49 manifest 존재 여부. manifest 의 `match.selector` 와 대조.

### 1.1 Universal Router (`universal-router/`)

| 함수 | selector | 기존 manifest | 비고 |
|---|---|---|---|
| `execute(bytes,bytes[],uint256)` | `0x3593564c` | O — `execute@1.0.0` | deadline 포함. 대부분 production tx 가 이 overload |
| `execute(bytes,bytes[])` | `0x24856bc3` | **X — 누락** | deadline 없는 overload. `universal_router.rs` 의 `EXECUTE_SELECTOR` 상수로 Tier B 가 인지하지만 declarative manifest 의 `match.selector` 는 `0x3593564c` 하나뿐 |

- `universal-router/execute@1.0.0.json` 의 `match.to` 리스트는 v1.2 + v2 주소만 담음 — **v2.1 신규 배포 주소 (예: mainnet `0xd92A36B0...`, Base `0xF3A4F409...`) 미수록.** Phase 후속에서 `to` 리스트를 `uniswap-deployments.json` 의 `universal-router` 배열로 동기화 필요.
- `execute` 의 내부 opcode stream 은 Tier B `UNISWAP_UR_TABLE` 가 dispatch — opcode 인벤토리는 §1.7 참조.

### 1.2 V2 Router02 (`v2/`) — `UniswapV2Router02`

| 함수 | selector | 기존 manifest |
|---|---|---|
| `swapExactTokensForTokens(uint256,uint256,address[],address,uint256)` | `0x38ed1739` | O |
| `swapTokensForExactTokens(uint256,uint256,address[],address,uint256)` | `0x8803dbee` | O |
| `swapExactETHForTokens(uint256,address[],address,uint256)` | `0x7ff36ab5` | O |
| `swapTokensForExactETH(uint256,uint256,address[],address,uint256)` | `0x4a25d94a` | O |
| `swapExactTokensForETH(uint256,uint256,address[],address,uint256)` | `0x18cbafe5` | O |
| `swapETHForExactTokens(uint256,address[],address,uint256)` | `0xfb3bdb41` | O |
| `swapExactTokensForTokensSupportingFeeOnTransferTokens(uint256,uint256,address[],address,uint256)` | `0x5c11d795` | O |
| `swapExactETHForTokensSupportingFeeOnTransferTokens(uint256,address[],address,uint256)` | `0xb6f9de95` | O |
| `swapExactTokensForETHSupportingFeeOnTransferTokens(uint256,uint256,address[],address,uint256)` | `0x791ac947` | O |
| `addLiquidity(address,address,uint256,uint256,uint256,uint256,address,uint256)` | `0xe8e33700` | O |
| `addLiquidityETH(address,uint256,uint256,uint256,address,uint256)` | `0xf305d719` | O |
| `removeLiquidity(address,address,uint256,uint256,uint256,address,uint256)` | `0xbaa2abde` | O |
| `removeLiquidityETH(address,uint256,uint256,uint256,address,uint256)` | `0x02751cec` | O |
| `removeLiquidityWithPermit(...,bool,uint8,bytes32,bytes32)` | `0x2195995c` | O |
| `removeLiquidityETHWithPermit(...,bool,uint8,bytes32,bytes32)` | `0xded9382a` | O |
| `removeLiquidityETHSupportingFeeOnTransferTokens(address,uint256,uint256,uint256,address,uint256)` | `0xaf2979eb` | O |
| `removeLiquidityETHWithPermitSupportingFeeOnTransferTokens(...,bool,uint8,bytes32,bytes32)` | `0x5b0d5984` | O |

V2 Router02 의 user-facing swap / liquidity 함수 17종 전부 manifest 존재. (`quote` / `getAmountsOut` 등은 pure/view — scope 제외.)

### 1.3 V3 SwapRouter (SwapRouter01) — `registry/manifests/uniswap/v3/` 의 swap 계열이 이 contract 를 가리킴

`v3/` 디렉토리 manifest 중 swap 계열 (`exactInputSingle` 등) 의 `match.to` 는 **`0xE592427A0AEce92De3Edee1F18E0157C05861564`** — 즉 V3 **SwapRouter01** (deadline 포함 시그니처). manifest 이름의 "v3" 는 SwapRouter01 을 의미.

| 함수 | selector | 기존 manifest |
|---|---|---|
| `exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))` | `0x414bf389` | O — `v3/exactInputSingle` |
| `exactInput((bytes,address,uint256,uint256,uint256))` | `0xc04b8d59` | O — `v3/exactInput` |
| `exactOutputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))` | `0xdb3e2198` | O — `v3/exactOutputSingle` |
| `exactOutput((bytes,address,uint256,uint256,uint256))` | `0xf28c0498` | O — `v3/exactOutput` |
| `multicall(bytes[])` | `0xac9650d8` | O — `v3/multicall` |
| `unwrapWETH9(uint256,address)` | `0x49404b7c` | O — `v3/unwrapWETH9` |
| `selfPermit(address,uint256,uint256,uint8,bytes32,bytes32)` | `0xf3995c67` | O — `v3/selfPermit` |
| `refundETH()` | `0x12210e8a` | X (state 변경 미미 — 잔여 ETH 환불, multicall 보조) |

SwapRouter01 시그니처는 SwapRouter02 와 다름: SR01 의 `exactInputSingle` params 에 `deadline` + `sqrtPriceLimitX96` 가 모두 들어 9-field tuple, SR02 는 `deadline` 제거된 7-field tuple. selector 가 완전히 다르다 (`0x414bf389` vs `0x04e45aaf`).

### 1.4 SwapRouter02 (`swap-router-02/`)

| 함수 | selector | 기존 manifest | 비고 |
|---|---|---|---|
| `exactInputSingle((address,address,uint24,address,uint256,uint256,uint160))` | `0x04e45aaf` | O | SR02 (deadline 없는 7-field tuple) |
| `exactInput((bytes,address,uint256,uint256))` | `0xb858183f` | O | |
| `exactOutputSingle((address,address,uint24,address,uint256,uint256,uint160))` | `0x5023b4df` | O | |
| `exactOutput((bytes,address,uint256,uint256))` | `0x09b81346` | O | |
| `swapExactTokensForTokens(uint256,uint256,address[],address)` | `0x472b43f3` | O | SR02 의 V2-style 함수. 인자 4개 (deadline 없음) |
| `swapTokensForExactTokens(uint256,uint256,address[],address)` | `0x42712a67` | O | |
| `multicall(uint256,bytes[])` | `0x5ae401dc` | O — `multicall@1.0.0` | manifest 의 selector = `0x5ae401dc` (deadline overload) |
| `multicall(bytes[])` | `0xac9650d8` | **X — 누락 overload** | base overload. SR02 에 실재 |
| `multicall(bytes32,bytes[])` | `0x1f0464d1` | **X — 누락 overload** | `previousBlockhash` 가드 overload. SR02 에 실재 |
| `unwrapWETH9(uint256,address)` | `0x49404b7c` | O — `unwrapWETH9@1.0.0` | manifest selector 확인 필요 (아래 overload) |
| `unwrapWETH9(uint256)` | `0x49616997` | 미확인 overload | SR02 에 둘 다 존재 |
| `wrapETH(uint256)` | `0x1c58db4f` | O — `wrapETH@1.0.0` | |
| `sweepToken(address,uint256,address)` | `0xdf2ab5bb` | X | multicall 내부 보조 |
| `selfPermit(address,uint256,uint256,uint8,bytes32,bytes32)` | `0xf3995c67` | X | |
| `selfPermitAllowed(address,uint256,uint256,uint8,bytes32,bytes32)` | `0x4659a494` | X | |
| `refundETH()` | `0x12210e8a` | X | |

SR02 의 `multicall` 은 Solidity 에서 3 overload (`bytes[]` / `uint256,bytes[]` / `bytes32,bytes[]`). 기존 manifest 는 `0x5ae401dc` (`uint256,bytes[]`) 하나만 — **`0xac9650d8`, `0x1f0464d1` 누락**.

### 1.5 V3 NonfungiblePositionManager (`v3/` 의 NFT 계열) — `NonfungiblePositionManager`

`v3/` 디렉토리의 NFT 계열 manifest. `v3/nfpm-multicall` 의 `match.to` 는 `0xC36442b4a4522E871399CD717aBDD847Ab11FE88` (NFPM).

| 함수 | selector | 기존 manifest |
|---|---|---|
| `mint((address,address,uint24,int24,int24,uint256,uint256,uint256,uint256,address,uint256))` | `0x88316456` | O — `v3/mint` |
| `increaseLiquidity((uint256,uint256,uint256,uint256,uint256,uint256))` | `0x219f5d17` | O — `v3/increaseLiquidity` |
| `decreaseLiquidity((uint256,uint128,uint256,uint256,uint256))` | `0x0c49ccbe` | O — `v3/decreaseLiquidity` |
| `collect((uint256,address,uint128,uint128))` | `0xfc6f7865` | O — `v3/collect` |
| `burn(uint256)` | `0x42966c68` | O — `v3/burn` |
| `createAndInitializePoolIfNecessary(address,address,uint24,uint160)` | `0x13ead562` | O — `v3/createAndInitializePoolIfNecessary` |
| `multicall(bytes[])` | `0xac9650d8` | O — `v3/multicall` + `v3/nfpm-multicall` |
| `permit(address,uint256,uint256,uint8,bytes32,bytes32)` | `0x7ac2ff7b` | O — `v3/permit` |
| `selfPermit(address,uint256,uint256,uint8,bytes32,bytes32)` | `0xf3995c67` | O — `v3/selfPermit` |
| `safeTransferFrom(address,address,uint256)` | `0x42842e0e` | O — `v3/safeTransferFrom` |
| `safeTransferFrom(address,address,uint256,bytes)` | `0xb88d4fde` | O — `v3/safeTransferFromWithData` |
| `transferFrom(address,address,uint256)` | `0x23b872dd` | O — `v3/transferFrom` |
| `approve(address,uint256)` | `0x095ea7b3` | O — `v3/approve` |
| `setApprovalForAll(address,bool)` | `0xa22cb465` | O — `v3/setApprovalForAll` |
| `unwrapWETH9(uint256,address)` | `0x49404b7c` | O — `v3/unwrapWETH9` |

NFPM 의 user-facing 함수 전부 manifest 존재.

> 주: 본 조사는 selector 만 대조함. `v3/multicall` selector(`0xac9650d8`) 는 SwapRouter01 과 NFPM 양쪽에 동일 — manifest 의 `match.to` 가 어느 contract 인지로 구분됨. `v3/multicall@1.0.0.json` 의 `to` 는 SwapRouter01(`0xE592...`), `v3/nfpm-multicall@1.0.0.json` 의 `to` 는 NFPM(`0xC364...`).

### 1.6 Permit2 (`permit2/`) — calldata 함수

Permit2 는 두 sub-interface (`IAllowanceTransfer`, `ISignatureTransfer`) 의 함수를 한 contract 에서 노출. canonical 주소 `0x000000000022D473030F116dDEE9F6B43aC78BA3` (13 chain 공통).

| 함수 | selector | 기존 manifest | 비고 |
|---|---|---|---|
| `permit(address,((address,uint160,uint48,uint48),address,uint256),bytes)` | `0x2b67b570` | O — `permit2/permit` | `IAllowanceTransfer.permit` (PermitSingle) |
| `permit(address,((address,uint160,uint48,uint48)[],address,uint256),bytes)` | `0x2a2d80d1` | **X — 누락 overload** | `IAllowanceTransfer.permit` (PermitBatch) |
| `transferFrom(address,address,uint160,address)` | `0x36c78516` | O — `permit2/transferFrom` | `IAllowanceTransfer.transferFrom` (single) |
| `transferFrom((address,address,uint160,address)[])` | `0x0d58b1db` | **X — 누락 overload** | `IAllowanceTransfer.transferFrom` (batch, `AllowanceTransferDetails[]`) |
| `approve(address,address,uint160,uint48)` | `0x87517c45` | X | `IAllowanceTransfer.approve` — calldata 기반 allowance 설정 (서명 없이) |
| `permitTransferFrom(((address,uint256),uint256,uint256),(address,uint256),address,bytes)` | `0x30f28b7a` | X | `ISignatureTransfer.permitTransferFrom` (single) |
| `permitTransferFrom(((address,uint256)[],uint256,uint256),(address,uint256)[],address,bytes)` | `0xedd9444b` | X | `ISignatureTransfer.permitTransferFrom` (batch) |
| `lockdown((address,address)[])` | `0xcc53287f` | X | `TokenSpenderPair[]` allowance 일괄 회수 |
| `invalidateNonces(address,address,uint48)` | `0x65d9723c` | X | AllowanceTransfer nonce 무효화 |
| `invalidateUnorderedNonces(uint256,uint256)` | `0x3ff9dcb1` | X | SignatureTransfer unordered nonce 무효화 |

`permit2/permit` manifest 는 single (`0x2b67b570`) 만 — **batch overload `0x2a2d80d1` 누락**. `permit2/transferFrom` 도 single (`0x36c78516`) 만 — **batch overload `0x0d58b1db` 누락**.

### 1.7 Universal Router 내부 opcode (참고 — 함수 아님)

`execute` 의 `commands` byte stream 은 Tier B `universal_router.rs` 의 `UNISWAP_UR_TABLE` 가 dispatch. opcode 자체는 4-byte selector 가 없음. 현재 table 등재: `0x00` V3_SWAP_EXACT_IN, `0x01` V3_SWAP_EXACT_OUT, `0x02` PERMIT2_TRANSFER_FROM, `0x03` PERMIT2_PERMIT_BATCH, `0x04` SWEEP, `0x05` TRANSFER, `0x06` PAY_PORTION, `0x07` PAY_PORTION_FULL_PRECISION, `0x08` V2_SWAP_EXACT_IN, `0x09` V2_SWAP_EXACT_OUT, `0x0a` PERMIT2_PERMIT, `0x0b` WRAP_ETH, `0x0c` UNWRAP_WETH, `0x0d` PERMIT2_TRANSFER_FROM_BATCH, `0x0e` BALANCE_CHECK_ERC20, `0x10` V4_SWAP, `0x11`-`0x12` V3_POSITION_MANAGER_*, `0x13` V4_INITIALIZE_POOL, `0x14` V4_POSITION_MANAGER_CALL, `0x21` EXECUTE_SUB_PLAN, `0x40` ACROSS_V4_DEPOSIT_V3.

---

## 2. Permit2 EIP-712 typed data struct

출처: `Uniswap/permit2` GitHub `src/interfaces/IAllowanceTransfer.sol` + `src/interfaces/ISignatureTransfer.sol` (verbatim).

Permit2 는 두 종류의 서명 표면을 가진다. 사용자 wallet 이 `eth_signTypedData_v4` 로 서명하는 typed struct.

### 2.1 IAllowanceTransfer — 영속 allowance (시간 제한 + nonce)

```solidity
struct PermitDetails {
    address token;
    uint160 amount;
    uint48 expiration;
    uint48 nonce;
}

struct PermitSingle {
    PermitDetails details;
    address spender;
    uint256 sigDeadline;
}

struct PermitBatch {
    PermitDetails[] details;
    address spender;
    uint256 sigDeadline;
}

struct AllowanceTransferDetails {  // 서명 대상 아님 — transferFrom calldata 인자
    address from;
    address to;
    uint160 amount;
    address token;
}

struct TokenSpenderPair {          // 서명 대상 아님 — lockdown calldata 인자
    address token;
    address spender;
}
```

서명 후 `permit(address owner, PermitSingle permitSingle, bytes signature)` (selector `0x2b67b570`) 또는 PermitBatch overload (`0x2a2d80d1`) 에 제출. Universal Router opcode `0x0a` (PERMIT2_PERMIT) / `0x03` (PERMIT2_PERMIT_BATCH) 도 동일 struct 를 inline 으로 받는다.

### 2.2 ISignatureTransfer — 일회성 transfer (서명 1회 = 1 transfer)

```solidity
struct TokenPermissions {
    address token;
    uint256 amount;
}

struct PermitTransferFrom {
    TokenPermissions permitted;
    uint256 nonce;
    uint256 deadline;
}

struct PermitBatchTransferFrom {
    TokenPermissions[] permitted;
    uint256 nonce;
    uint256 deadline;
}

struct SignatureTransferDetails {  // 서명 대상 아님 — permitTransferFrom calldata 인자
    address to;
    uint256 requestedAmount;
}
```

함수 시그니처:
```solidity
function permitTransferFrom(
    PermitTransferFrom memory permit,
    SignatureTransferDetails calldata transferDetails,
    address owner,
    bytes calldata signature
) external;                       // selector 0x30f28b7a

function permitTransferFrom(
    PermitBatchTransferFrom memory permit,
    SignatureTransferDetails[] calldata transferDetails,
    address owner,
    bytes calldata signature
) external;                       // selector 0xedd9444b
```

### 2.3 ERC-2612 `permit` (EIP-712, Permit2 와 무관)

Uniswap V2 Router02 의 `removeLiquidityWithPermit` / `removeLiquidityETHWithPermit*` 와 V3 NFPM / SwapRouter01 / SR02 의 `selfPermit` 은 ERC-2612 `permit` 서명 (`v,r,s` split) 을 받는다. ERC-2612 는 Permit2 와 별개 표준 — typed struct 는 `Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)`. NFPM 의 `permit(address,uint256,uint256,uint8,bytes32,bytes32)` (selector `0x7ac2ff7b`) 은 ERC-721 형 permit 으로 `tokenId` 기반 (시그니처 인자 순서가 ERC-2612 와 다름).

---

## 3. 13 chain 배포 주소 matrix 출처

산출물 `registry/scripts/uniswap-deployments.json` 의 각 주소 출처. 모든 주소는 EIP-55 checksum 으로 정규화 (본 조사 keccak-256 self-test 통과, 80/80 주소 checksum 일치).

### 3.1 1차 출처

- **`Uniswap/contracts` repo `deployments/<chainId>.md`** — chain 별 Summary table + Deployment History. 6 router 전부 + Permit2 를 한 파일에서 제공하는 가장 권위있는 1차 출처. 본 조사에서 13 chain md 전부 fetch.
  - URL: `https://github.com/Uniswap/contracts/tree/main/deployments` (raw: `https://raw.githubusercontent.com/Uniswap/contracts/main/deployments/<chainId>.md`)
- **ScopeBall Tier B 코드** — `crates/adapters/abi-resolver/src/subdecode/protocols/universal_router.rs`:
  - `UNISWAP_UR_ADDRESSES` (행 ~339-479) — UR 주소 21 entry. byte literal 을 본 조사에서 hex 디코드.
  - `V3_NPM_ADDRESSES` (행 ~506-598) — V3 NFPM 주소 13 entry. 출처 주석: `https://github.com/Uniswap/contracts/tree/main/deployments`.
- **Permit2 canonical 주소** — `0x000000000022D473030F116dDEE9F6B43aC78BA3`, CREATE2 결정적 배포. 13 chain md 전부에서 등장 확인.

### 3.2 router 별 chain coverage

| router | 배포 chain (key 존재) | 미배포 (key 생략) |
|---|---|---|
| `v2-router02` | 1, 8453, 10, 42161, 137, 56, 81457, 130, 480 (9) | 43114, 42220, 57073, 7777777 |
| `v3-swap-router` (SwapRouter01) | 1, 10, 42161, 137 (4) | 8453, 43114, 81457, 56, 42220, 57073, 130, 480, 7777777 |
| `swap-router-02` | 1, 8453, 10, 42161, 137, 43114, 56, 42220, 130 (9) | 81457, 57073, 480, 7777777 |
| `v3-nfpm` | 13 chain 전부 | — |
| `permit2` | 13 chain 전부 | — |
| `universal-router` | 13 chain 전부 | — |

미배포 판정 근거:
- **57073 (Ink) / 7777777 (Zora)** — `deployments/57073.md` / `7777777.md` Summary 가 V4 (PoolManager / PositionManager / V4Quoter / StateView) + UniversalRouter 만 — V2/V3 contract heading 자체 부재. V4 네이티브 신규 chain.
- **480 (World Chain)** — `### Uniswap V2 Router02` + `### Nonfungible Position Manager` heading 은 존재하나 `### Swap Router02` / `### Swap Router` heading 부재 → SwapRouter02 / SwapRouter01 미배포.
- **V3 SwapRouter01** 은 mainnet + 초기 4 chain (1/10/42161/137) 에만 — 이후 chain 은 SR01 을 건너뛰고 SR02 부터 배포.

### 3.3 universal-router 배열 구성

`universal-router` 만 chain 당 주소 배열. 배열은 다음 union:
- Tier B `UNISWAP_UR_ADDRESSES` 의 v1.2 (`UniversalRouterV1_2_V2Support`) + v2 (`UniversalRouterV2`) 주소.
- `Uniswap/contracts` md 의 최신 v2.1 (또는 chain 별 Summary 의 현 권장) 주소.

본 조사의 cross-check: Tier B 의 26 UR 주소 (13 chain × v1.2/v2) 중 **25 개가 `Uniswap/contracts` md body 에 실재** — 강한 일치. 1 개 불일치는 §4 참조.

`Uniswap/contracts` md 의 버전 라벨링은 chain 마다 다르다 — 8453 (Base) 만 `### Universal Router (v2.1)/(v2.0)/(v1.2)` 처럼 버전별 분리 섹션을 노출, 나머지 12 chain 의 md 는 `### Universal Router` 단일 섹션 (현 권장 1개) 만. 과거 버전 주소는 Tier B table 이 보유.

---

## 4. Tier B table ↔ 1차 출처 불일치

### 4.1 [확정 버그] World Chain (480) UR v1.2 주소 오류

`universal_router.rs` 의 `UNISWAP_UR_ADDRESSES` World Chain (chain 480) `UniversalRouterV1_2_V2Support` entry:

```rust
// World Chain
(
    480,
    Address::new(
        *b"\x7a\x25\x0d\x56\x30\xb4\xcf\x53\x97\x39\xdf\x2c\x5d\xac\xb4\xc6\x59\xf2\x48\x8d",
    ),
), // UniversalRouterV1_2_V2Support
```

이 byte literal 을 디코드하면 `0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D` 인데, 이는 **Ethereum mainnet 의 UniswapV2Router02 주소** (`deployments/1.md` 의 `<td>UniswapV2Router02</td>` 행에서 확인 — Etherscan 등재). World Chain 의 `deployments/480.md` 에는 Summary + 6 개 Deployment History 날짜 섹션을 통틀어 UniversalRouter 주소가 `0x03c4F6B55733CdF3CAA07C01E5b83DdEe3381F60` 한 개뿐 — v1.2 분리 배포가 없다.

`is_uniswap_universal_router(480, 0x7a250d...)` 가 `true` 를 반환하면 World Chain 에서 mainnet V2 Router02 주소를 Universal Router 로 오인 → 잘못된 opcode dispatch 위험. **Tier B 의 단일 진실 위반.**

- 본 조사 산출물 `uniswap-deployments.json` 의 `universal-router["480"]` 는 이 오류 주소를 **제외** — Tier B v2 (`0x8ac7bEE9...`) + md 최신 (`0x03c4F6B5...`) 만 수록.
- 후속 조치: `universal_router.rs` 의 480 v1.2 entry 제거 검토 필요 (本 조사는 read-only — 파일 미수정).

### 4.2 그 외

Tier B 의 나머지 25 UR 주소 + 13 NFPM 주소 (`V3_NPM_ADDRESSES`) 는 본 조사 범위 내에서 `Uniswap/contracts` 1차 출처와 모두 일치 — 추가 불일치 없음.

---

## 5. 기존 manifest 누락 요약 (Phase 후속 입력)

본 조사가 발견한 manifest 누락 / 동기화 필요 항목:

1. **`universal-router/execute`** — `execute(bytes,bytes[])` (`0x24856bc3`, no-deadline overload) manifest 없음. `match.selector` 가 `0x3593564c` 만.
2. **`universal-router/execute`** — `match.to` 리스트가 v1.2 + v2 주소만 — v2.1 신규 주소 미수록.
3. **`swap-router-02/multicall`** — overload 3종 중 `0x5ae401dc` (`uint256,bytes[]`) 만. `0xac9650d8` (`bytes[]`), `0x1f0464d1` (`bytes32,bytes[]`) 누락.
4. **`permit2/permit`** — PermitBatch overload (`0x2a2d80d1`) 누락. single (`0x2b67b570`) 만.
5. **`permit2/transferFrom`** — batch overload (`0x0d58b1db`, `AllowanceTransferDetails[]`) 누락. single (`0x36c78516`) 만.
6. **Permit2 `ISignatureTransfer`** — `permitTransferFrom` (single `0x30f28b7a` / batch `0xedd9444b`) manifest 전무. Uniswap dApp 의 일회성 서명 전송 경로.
7. **모든 Uniswap manifest 의 `match` coverage** — 대부분 manifest 가 5 chain (`1,8453,10,42161,137`) + `to` 1개만 — 13 target chain 미커버. `uniswap-deployments.json` 으로 `chain_ids` + `to` 확장 필요.

---

## 6. 출처

- Uniswap 배포 주소 (1차) — `Uniswap/contracts` repo `deployments/<chainId>.md`: <https://github.com/Uniswap/contracts/tree/main/deployments>
  - 본 조사 fetch chain: 1, 8453, 10, 42161, 137, 43114, 81457, 56, 42220, 57073, 130, 480, 7777777 의 `.md` (raw.githubusercontent.com)
- Permit2 EIP-712 struct (1차) — `Uniswap/permit2` repo:
  - `src/interfaces/IAllowanceTransfer.sol`: <https://github.com/Uniswap/permit2/blob/main/src/interfaces/IAllowanceTransfer.sol>
  - `src/interfaces/ISignatureTransfer.sol`: <https://github.com/Uniswap/permit2/blob/main/src/interfaces/ISignatureTransfer.sol>
- Uniswap v3 deployments docs (보강) — <https://developers.uniswap.org/contracts/v3/reference/deployments>
- ScopeBall Tier B UR / NFPM 주소 table — `crates/adapters/abi-resolver/src/subdecode/protocols/universal_router.rs` (`UNISWAP_UR_ADDRESSES` 행 ~339-479, `V3_NPM_ADDRESSES` 행 ~506-598)
- selector — `keccak256(canonicalSignature)[:4]`, 본 조사 pure-python keccak-256 으로 계산 (Permit2 canonical 주소 EIP-55 self-test 통과)
