# 프로토콜 × 필드 매트릭스

이 문서는 `policyschema`가 다루는 모든 프로토콜에 대해, **각 ActionFields의 필드가 calldata 어디서 오는지**를 표로 정리한 *정책 작성자용 reference*입니다.

두 종류 표를 둡니다:
- **§1 ~ §4 — 필드 단위 표 (`x-adapter-mapping`)**: 각 ActionFields의 필드가 프로토콜별로 어디서 오는지 한눈에.
- **§5 ~ §8 — 함수 단위 표**: 각 함수(또는 selector)가 어떤 ActionType을 trigger 하고 어떤 Extension data가 붙는지.

---

## 1. SwapFields × 프로토콜 (필드 단위)

| Field | uniswap.v2 | uniswap.v3 | uniswap.v4 | uniswap.universalRouter | pancakeswap | aerodrome.v1 | aerodrome.slipstream |
|---|---|---|---|---|---|---|---|
| `actor` | `tx.from` | `tx.from` | `tx.from` | `tx.from` | `tx.from` | `tx.from` | `tx.from` |
| `protocol_ids` | `["uniswap.v2"]` | `["uniswap.v3"]` | `["uniswap.v4"]` | child opcode들의 namespace 합집합 | `["pancakeswap"]` (+ `data.component`) | `["aerodrome.v1"]` | `["aerodrome.slipstream"]` |
| `input_tokens[0]` | `path[0]` | `params.tokenIn` 또는 path bytes 첫 20B | `PoolKey.currency0/1` (zeroForOne) | (자식 swap에서 도출) | (component별, 위 V2/V3 패턴) | `path[0].from` | V3와 동일 |
| `output_tokens[-1]` | `path[len-1]` | `params.tokenOut` 또는 path bytes 마지막 20B | `PoolKey.currency0/1` 반대편 | (자식 swap) | (component별) | `path[-1].to` | V3와 동일 |
| `mode` | function name | function name | `params.amountSpecified` 부호 (음수=in, 양수=out) | (자식 opcode) | (component별) | function name | function name |
| `amount_in.raw` | `amountIn` (exact-in) / `amountInMax` (exact-out) | `params.amountIn` / `amountInMaximum` | `params.amountSpecified` (음수일 때) | (자식) | (component별) | `amountIn` | V3와 동일 |
| `amount_out.raw` | `amountOutMin` / `amountOut` | `params.amountOutMinimum` / `amountOut` | `params.amountSpecified` (양수일 때) | (자식) | (component별) | `amountOutMin` | V3와 동일 |
| `route` | `MultiHop(path)` | path 길이로 SingleHop / MultiHop | SingleHop (V4 swap당) | router_plan (자식들이 SingleHop/MultiHop) | (component별) | `MultiHop(Solidly path)` | V3와 동일 |
| `recipients.recipient` | `to` 인자 | `params.recipient` | settlement actions의 `TAKE` 대상 | (자식) | (component별) | `to` 인자 | `params.recipient` |
| `deadlines.deadline` | `deadline` 인자 | `params.deadline` | execute 인자 | execute 인자 | (component별) | `deadline` | `params.deadline` |
| `max_fee_bps` | `Some(30)` 고정 | `feeTier` (path 3B) | `PoolKey.fee` (`& ~0x800000`) | (자식별 합산) | (component별) | stable=`1`, 그외=`30` | tickSpacing → fee 추론 |
| `has_zero_min_output` | `amountOutMin == 0` | `amountOutMinimum == 0` | `amountSpecified` 부호 + min check | 자식 OR | (component별) | `amountOutMin == 0` | V3와 동일 |

### Wrap / Unwrap (SwapFields 재사용)

| Field | lido (stETH↔wstETH wrap) | weth (ETH↔WETH wrap) |
|---|---|---|
| `mode` | `ExactIn` | `ExactIn` |
| `input_tokens` | `[stETH]` (wrap) / `[wstETH]` (unwrap) | `[ETH(native)]` / `[WETH]` |
| `output_tokens` | `[wstETH]` / `[stETH]` | `[WETH]` / `[ETH(native)]` |
| `amount_in.kind` | `Exact` | `Exact` |
| `amount_out.kind` | `Exact` (1:1) | `Exact` (1:1) |
| `max_fee_bps` | `Some(0)` | `Some(0)` |
| `route` | `SingleHop { hop.protocol = "lido" }` | `SingleHop { hop.protocol = "weth" }` |

---

## 2. LendingFields × 프로토콜 (필드 단위)

| Field | aave.v3 | morpho.blue |
|---|---|---|
| `actor` | `tx.from` | `tx.from` |
| `protocol_ids` | `["aave.v3"]` | `["morpho.blue"]` |
| `asset` | `asset` 인자 (Pool 함수 1번 인자) | `marketParams.loanToken` (또는 `collateralToken`) |
| `amount.raw` | `amount` 인자 | `assets` 또는 `shares` (둘 중 하나만 nonzero — 둘 다 nonzero면 revert) |
| `amount.kind` | `Exact` (repay에서 `type(uint256).max`이면 `Unlimited`) | `Exact` (repay에서 max-shares 패턴이면 `Unlimited`) |
| `on_behalf_of` | `onBehalfOf` 인자 | `onBehalf` 인자 |
| `interest_rate_mode` | borrow: `interestRateMode` enum / repay: `rateMode` / supply·withdraw: `None` | 항상 `None` (variable 단일) |
| `recipients.recipient` | withdraw: `to` 인자 / borrow: `onBehalfOf` (또는 actor) / supply·repay: `Actor` | withdraw·borrow: `receiver` / supply·repay: `Actor` |

---

## 3. StakingFields × 프로토콜 (필드 단위, Lido)

> Wrap / Unwrap은 SwapFields 사용이므로 위 §1 마지막 표 참조.

| Field | lido (`stETH.submit`) | lido (`withdrawalQueue.requestWithdrawals`) | lido (`withdrawalQueue.claimWithdrawal`) |
|---|---|---|---|
| `actor` | `tx.from` | `tx.from` | `tx.from` |
| `protocol_ids` | `["lido"]` | `["lido"]` | `["lido"]` |
| `asset_in` | ETH (`Token { is_native: true, … }`) | stETH (curated registry) | stETH (NFT가 잠긴 자산 — placeholder Token) |
| `asset_out` | `Some(stETH)` | `None` (대신 NFT 발급, schema 외) | `Some(ETH)` |
| `amount.raw` | `tx.value` (msg.value) | `sum(amounts[])` | NFT가 잠긴 amount — calldata에 없으므로 `Unspecified` |
| `referral` | `_referral` 인자 (`address(0)` → `None` 정규화) | N/A | N/A |
| `withdrawal_request_id` | `None` | `None` (event-derived) | `_requestId` 인자 |
| `recipients.recipient` | `Actor` (msg.sender에 mint) | `_owner` 인자 | `Actor` |

---

## 4. SignFields × primaryType (필드 단위)

| Field | permit2 PermitSingle | permit2 PermitTransferFrom | eip2612 Permit | eip712 Other |
|---|---|---|---|---|
| `signer` | `sig.message.owner` | `sig.message.owner` | `sig.message.owner` | `sig.message.{owner|signer|...}` (varies) |
| `chain_id` | `sig.domain.chainId` | `sig.domain.chainId` | `sig.domain.chainId` | `sig.domain.chainId` |
| `domain.name` | `"Permit2"` | `"Permit2"` | token contract `name()` | (varies) |
| `domain.version` | `"1"` | `"1"` | token contract `version()` (보통 `"1"`) | (varies) |
| `domain.verifyingContract` | Permit2 canonical (`0x000000000022D473030F116dDEE9F6B43aC78BA3`) | 동일 | token contract address | (varies) |
| `primary_type` | `"PermitSingle"` | `"PermitTransferFrom"` 또는 `"PermitWitnessTransferFrom"` | `"Permit"` | (varies) |
| `semantic.spender` | `sig.message.spender` | `sig.message.spender` | `sig.message.spender` | N/A (`Other`) |
| `semantic.tokens[]` 또는 `transfers[]` | `sig.message.details` (단일) / `details[]` (Batch) | `sig.message.permitted` (단일) / `permitted[]` (Batch) | `sig.message.value` (`AmountSpec`, `Unlimited` if `type(uint256).max`) | N/A |
| `semantic.nonce` | `sig.message.details.nonce` (uint48 → decimal string) | `sig.message.nonce` (uint256 → decimal string) | `sig.message.nonce` (uint256 → decimal string) | (varies) |
| `semantic.witness` | N/A | `sig.message.witness` (`*WitnessTransferFrom`만) + `witnessTypeString` | N/A | N/A |
| `deadlines.deadline` | `sig.message.sigDeadline` | `sig.message.deadline` | `sig.message.deadline` | (custom or none) |

---

## 5. Swap 함수 매트릭스

| Protocol | Function (selector) | mode | route | Extension |
|---|---|---|---|---|
| Uniswap V2 | `swapExactTokensForTokens` (`0x38ed1739`) | ExactIn | MultiHop(path) | `uniswap.v2.{path, supporting_fee_on_transfer:false}` |
| Uniswap V2 | `swapTokensForExactTokens` (`0x8803dbee`) | ExactOut | MultiHop(path) | 동상 |
| Uniswap V2 | `swapExactETHForTokens` (`0x7ff36ab5`) | ExactIn | MultiHop | 동상 |
| Uniswap V2 | `*SupportingFeeOnTransferTokens` (3종) | ExactIn | MultiHop | `supporting_fee_on_transfer:true` (confidence=medium) |
| Uniswap V3 | `exactInputSingle` (struct) | ExactIn | SingleHop | `uniswap.v3.{feeTier, sqrtPriceLimitX96, entryPoint}` |
| Uniswap V3 | `exactInput` (encoded path) | ExactIn | MultiHop(decoded) | `uniswap.v3.{encodedPath, feeTiers[]}` |
| Uniswap V3 | `exactOutputSingle` | ExactOut | SingleHop | 동상 |
| Uniswap V4 | `swap` via `PoolManager.unlock` | ExactIn/Out | SingleHop | `uniswap.v4.{poolKey, hooks, hookData, deltas[]}` |
| Universal Router | `execute(commands, inputs)` (`0x3593564c`) | — | router_plan | `uniswap.universalRouter.{commands, mask:0x7f, deadline}` (자식 swap별로 개별 Extension) |
| PancakeSwap V2 | (Uniswap V2 동일 selector) | … | … | `pancakeswap.{component:"v2", path}` |
| PancakeSwap V3 | (Uniswap V3 fork) | … | … | `pancakeswap.{component:"v3", feeTier, ...}` (callback `pancakeV3SwapCallback`) |
| PancakeSwap SmartRouter | `multicall` 안 V2/V3/Stable 혼합 | — | Split | `pancakeswap.{component:"smartRouter", branches[]}` |
| PancakeSwap UR | `execute` (mask `& 0x3f`) | — | router_plan | `pancakeswap.{component:"universalRouter", commands, mask:0x3f}` |
| PancakeSwap Infinity | INFI_SWAP via UR (opcode `0x10`) | ExactIn/Out | SingleHop | `pancakeswap.{component:"infinity", poolKey, parameters}` |
| Aerodrome V1 | `swapExactTokensForTokens` (Solidly path) | ExactIn | MultiHop | `aerodrome.v1.{stable[], factory}` |
| Aerodrome Slipstream | (V3 fork, tickSpacing 기반) | ExactIn/Out | SingleHop/MultiHop | `aerodrome.slipstream.{tickSpacing, sqrtPriceLimitX96}` |

---

## 6. Lending 함수 매트릭스

| Protocol | Function | ActionType | 공통 매핑 | Extension |
|---|---|---|---|---|
| Aave V3 Pool | `supply(asset, amount, onBehalfOf, referralCode)` | Supply | asset, amount, on_behalf_of | `aave.v3.{referralCode}` |
| Aave V3 Pool | `withdraw(asset, amount, to)` | Withdraw | asset, amount, recipients | `aave.v3.{}` |
| Aave V3 Pool | `borrow(asset, amount, interestRateMode, referralCode, onBehalfOf)` | Borrow | asset, amount, on_behalf_of, interest_rate_mode | `aave.v3.{referralCode}` |
| Aave V3 Pool | `repay(asset, amount, rateMode, onBehalfOf)` | Repay | asset, amount, on_behalf_of, interest_rate_mode | `aave.v3.{}` |
| Morpho Blue | `supply(marketParams, assets, shares, onBehalf, data)` | Supply | asset(=loanToken), amount, on_behalf_of | `morpho.blue.{marketParams, shares, data}` |
| Morpho Blue | `withdraw(marketParams, assets, shares, onBehalf, receiver)` | Withdraw | asset, amount, recipients, on_behalf_of | `morpho.blue.{marketParams, shares}` |
| Morpho Blue | `borrow(marketParams, assets, shares, onBehalf, receiver)` | Borrow | asset, amount, recipients, on_behalf_of | `morpho.blue.{marketParams, shares}` |
| Morpho Blue | `repay(marketParams, assets, shares, onBehalf, data)` | Repay | asset, amount, on_behalf_of | `morpho.blue.{marketParams, shares, data}` |

`marketParams` 필드 5종: `loanToken`, `collateralToken`, `oracle`, `irm`, `lltv`.

---

## 7. LST 함수 매트릭스 (Lido)

| Protocol Component | Function | ActionType | 공통 매핑 | Extension |
|---|---|---|---|---|
| `lido` (stETH) | `submit(referral)` payable | Stake | asset_in=ETH, asset_out=stETH, amount=msg.value | `lido.{component:"stETH", referral}` |
| `lido` (withdrawalQueue) | `requestWithdrawals(amounts[], owner)` | RequestWithdrawal | asset_in=stETH, asset_out=None, amount=sum, recipients.recipient=owner | `lido.{component:"withdrawalQueue", amounts[]}` |
| `lido` (withdrawalQueue) | `claimWithdrawal(requestId)` | ClaimWithdrawal | asset_out=ETH | `lido.{component:"withdrawalQueue", requestId}` |
| `lido` (wstETH) | `wrap(stETHAmount)` | Wrap (Swap 카테고리) | SwapFields 사용 | `lido.{component:"wstETH"}` |
| `lido` (wstETH) | `unwrap(wstETHAmount)` | Unwrap (Swap 카테고리) | SwapFields 사용 | `lido.{component:"wstETH"}` |

---

## 8. 서명 매트릭스

| Protocol | primaryType | ActionType | 공통 매핑 | Extension |
|---|---|---|---|---|
| Permit2 | `PermitSingle` | SignPermit2Approve | spender, tokens=[1], deadlines.deadline=sigDeadline, nonce | `permit2.{component:"single"}` |
| Permit2 | `PermitBatch` | SignPermit2Approve | spender, tokens=[N], deadlines.deadline, nonce | `permit2.{component:"batch"}` |
| Permit2 | `PermitTransferFrom` | SignPermit2TransferFrom | spender, transfers, deadlines.deadline, nonce | `permit2.{component:"transferFrom"}` |
| Permit2 | `PermitBatchTransferFrom` | SignPermit2TransferFrom | … | `permit2.{component:"batchTransferFrom"}` |
| Permit2 | `PermitWitnessTransferFrom` | SignPermit2TransferFrom | + witness | `permit2.{component:"witness", witnessTypeString}` |
| Permit2 | `PermitBatchWitnessTransferFrom` | SignPermit2TransferFrom | + witness | `permit2.{component:"batchWitness", witnessTypeString}` |
| ERC20 | `Permit` (EIP-2612) | SignEip2612Permit | token, owner, spender, value(`AmountSpec`), deadlines.deadline, nonce | `eip2612.{}` |
| 기타 | (인식되지 않은 primaryType) | SignEip712Other | — | `eip712.{typesJson, messageJson}` |

---

## 부록: opcode 마스킹 규칙

| Family | Mask | 비트 7 (high) 의미 |
|---|---|---|
| Uniswap UR | `& 0x7f` | `0x80` = `FLAG_ALLOW_REVERT` |
| PancakeSwap UR | `& 0x3f` | (다른 opcode 공간) |
| Uniswap V4 Actions | (마스킹 없음) | — |

family 판별이 디코딩의 첫 단계 — `tx.to`로 결정.
