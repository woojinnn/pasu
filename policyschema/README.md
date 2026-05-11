# `policyschema`

정책 엔진(Cedar 등) 직전 단계의 **공통 Action 데이터 모델**을 정의하는 Rust 크레이트.

지갑이 트랜잭션·서명을 *서명 직전* 사용자에게 설명할 때, 여러 DEX·렌딩·LST·Restaking·RWA·NFT·Governance·Vault 프로토콜이 각기 다른 calldata 형태를 갖더라도 **사용자가 보는 의미 단위 행위(swap, supply, stake, vote, sign)는 공통**입니다. 이 공통 의미를 Rust 타입으로 정의해 정책 엔진이 프로토콜을 가리지 않고 일관되게 적용할 수 있도록 합니다.

## 계층 다이어그램

```
┌─────────────────────────────────────────────────────────────────┐
│                       정책 평가 (Cedar 등)                       │  ← 외부
└─────────────────────────────────────────────────────────────────┘
                                ▲
                                │ NormalizedRequestV2
                                │
┌─────────────────────────────────────────────────────────────────┐
│  스키마 정의 — ActionType / Category / Fields / Extension /     │  ← policyschema (이 크레이트)
│                dispatch table 정의                               │
└─────────────────────────────────────────────────────────────────┘
                                ▲
                                │ ActionFields
                                │
┌─────────────────────────────────────────────────────────────────┐
│  세미-어댑터 — args JSON + Context → 정규화된 ActionFields       │  ← policyschema (이 크레이트)
│                (build_*_fields, classify_call)                   │
└─────────────────────────────────────────────────────────────────┘
                                ▲
                                │ args (JSON)
                                │
┌─────────────────────────────────────────────────────────────────┐
│  풀 어댑터 — raw calldata bytes → ABI 디코드 (alloy-sol-types)  │  ← 외부 (정책 엔진 어댑터 팀)
└─────────────────────────────────────────────────────────────────┘
                                ▲
                                │ eth_sendTransaction / eth_signTypedData
                                │
                          (지갑·dApp)
```

policyschema는 **스키마 정의 + 세미-어댑터 두 계층**을 책임집니다. 풀 어댑터(raw calldata 디코드)와 정책 평가는 외부 컴포넌트.

## Repo 구조

```
policyschema/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs
│   ├── core.rs                 # NormalizedRequestV2 최상위
│   ├── request.rs              # Transaction | TypedData
│   ├── target.rs               # ContractTarget + role taxonomy
│   ├── call.rs                 # DecodedCall
│   ├── types.rs                # 공통 fragment (Token, AmountSpec, PoolKey, RecipientFields, DeadlineFields)
│   ├── action/
│   │   ├── category.rs         # ActionCategory (13종)
│   │   ├── kind.rs             # ActionType (72종)
│   │   └── fields.rs           # 13 variant ActionFields (Swap/Liquidity/Lending/LiquidStaking/Restaking/Rwa/Governance/Nft/Vault/Utility/Aggregation/Sign/Unknown)
│   ├── extension.rs            # Extension + ExtensionNamespace (41종)
│   ├── confidence.rs
│   ├── raw.rs
│   ├── dispatch.rs             # 스키마 영역 — DispatchKey/Entry/SemiAdapterId enum + sub-table const 배열
│   └── semi_adapter/           # 세미-어댑터 영역
│       ├── classify.rs         # classify_call/slipstream/v4_swap (실행 함수)
│       ├── common.rs           # helper (v3_path_to_hops, swap_hook_flags, ...)
│       ├── error.rs            # SemiAdapterError
│       ├── registry.rs         # token + UR family 카탈로그
│       ├── uniswap_v2.rs       # 9 swap 변형
│       ├── uniswap_v3.rs       # exactInput[Single]·exactOutput[Single]
│       ├── uniswap_v4.rs       # PoolKey + hook 14bit 권한
│       ├── uniswap_ur.rs       # execute opcode dispatch
│       ├── pancakeswap.rs      # V2/V3 fork + UR mask 0x3f
│       ├── aerodrome_v1.rs     # Solidly 3-tuple path
│       ├── aerodrome_slipstream.rs  # tickSpacing 기반
│       ├── aave_v3.rs          # supply/withdraw/borrow/repay
│       ├── morpho_blue.rs      # marketParams 5튜플
│       ├── lido.rs             # submit/request + wstETH wrap/unwrap
│       └── sign.rs             # Permit2/EIP-2612/EIP-712/SafeTx
├── docs/
│   ├── baseline.md             # 데이터 모델 헌법
│   ├── protocol-comparison.md  # 프로토콜 × 필드 매핑 매트릭스
│   └── extensions/             # namespace별 명세
├── examples/
│   └── fixtures/
│       └── NN.{input,expected}.json   # 21+ 페어
└── tests/
    ├── fixtures.rs                          # deserialize 회귀
    └── semi_adapter_round_trip.rs           # input → classify → expected 검증
```

## 빠른 시작

```sh
cargo build
cargo test            # 단위·통합 테스트 통과
cargo doc --no-deps   # API 문서
```

## ActionCategory × ActionType × 프로토콜 매핑

| Category (13) | ActionType count | 발생 프로토콜 |
|---|---|---|
| **Swap** | 5 (swap, batch_swap, hooked_operation, **wrap**, **unwrap**) | Uniswap V2/V3/V4·UR, PancakeSwap V2/V3/SmartRouter/UR/Infinity, Aerodrome V1/Slipstream, WETH, Lido wstETH wrap/unwrap |
| **Liquidity** | 9 (add/remove_liquidity, join/exit_pool, mint_position, burn_position, increase_liquidity, decrease_liquidity, collect_fees) | Uniswap V2/V3/V4 NPM, Balancer Vault, Aerodrome V1 |
| **Lending** | 11 (supply, borrow, repay, withdraw_collateral, set_collateral, liquidation_repay, flash_loan, repay_with_atokens, swap_borrow_rate_mode, set_e_mode, mint_unbacked) | Aave V3, Morpho Blue, Spark Lend, Compound V3 |
| **LiquidStaking** | 5 (stake, unstake_request, claim_unstake, wrap_receipt, unwrap_receipt) | Lido, Rocket Pool, Mantle mETH |
| **Restaking** | 8 (restake, delegate_operator, undelegate, queue_withdrawal, claim_withdrawal, mint_lrt, request_lrt_redemption, claim_lrt_redemption) | EigenLayer Core/EigenPod, etherfi, Kelp, Renzo |
| **Rwa** | 8 (subscribe, request_redemption, claim_subscription, claim_redemption, cancel_request, claim_cancel, transfer_restricted, claim_yield) | Centrifuge ERC-7540, Ondo USDY, Securitize DS, BlackRock BUIDL |
| **Governance** ⭐ | 4 (governance_propose/vote/execute/delegate) | Compound/Aave Governor, Uniswap UNI, OpenZeppelin Governor, Snapshot |
| **Nft** ⭐ | 4 (nft_mint/transfer/buy/sell) | Seaport, Blur, X2Y2 |
| **Vault** ⭐ | 2 (vault_deposit/withdraw) | ERC-4626, Yearn |
| **Utility** | 10 (approval, permit, transfer, claim_rewards, multicall, sign_message, airdrop_claim, merkle_claim) | ERC20, WETH, multicall 등 |
| **Aggregation** | 1 (router_plan) | Universal Router (Uniswap·PancakeSwap) |
| **Sign** | 6 (SignPermit2Approve, SignPermit2TransferFrom, SignEip2612Permit, SignEip712Other, SignSafeTx, SignSessionKey) | Permit2 canonical, ERC-2612, Safe, AA 4337/7702 |
| **Unknown** | 1 | catch-all |

각 프로토콜이 무엇인지의 한 줄 설명은 `docs/extensions/<namespace>.md` 참조.

## 통계

- **ActionType**: 72종
- **ActionCategory**: 13종
- **ExtensionNamespace**: 41종 (DEX 11·Lending 4·LST 3·Restaking 5·RWA 4·Governance 3·NFT 3·Vault 2·Sign 3·토큰 2·AA 1)
- **ActionFields variant**: 13 (카테고리 1:1)
- **세미-어댑터 모듈**: 12 (uniswap_v2/v3/ur/v4·pancakeswap·aerodrome_v1/slipstream·aave_v3·morpho_blue·lido·sign + classify·common·registry·error)
- **Dispatch table entry**: 25 (v0.1 스코프 — Uniswap V2 9 + V3 4 + UR 2 + Aerodrome V1 1 + Aave V3 4 + Morpho Blue 1 + Lido 4)
- **fixture**: 21 페어 (input/expected) + 신규 7 카테고리별 1개씩 추가 예정

## Out-of-scope (v0.1)

다음은 *데이터 모델 정의*는 갖추되 *세미-어댑터 빌더*는 제공하지 않음:

- ❌ Liquidity / Restaking / Rwa / Governance / Nft / Vault / Utility 추가 4 / Sign 추가 2 / Lending 추가 4 — fields struct 틀만 마련, 빌더 함수는 v0.2
- ❌ Bridge / Perp 카테고리 자체 — v0.2 (DEX 외 spot DEX 위치 확정 후)
- ❌ raw calldata bytes → args JSON 디코드 — *풀 어댑터 영역* (이 크레이트 외)
- ❌ Cedar 정책 평가 자체

자세한 설계 결정은 `docs/baseline.md` 참조.
