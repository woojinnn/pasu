//! `ActionFields` — `ActionCategory`별 타입화된 페이로드.
//!
//! v0.1 보강에서 5 variant → **13 variant** (ActionCategory와 1:1):
//!
//! | ActionCategory  | Variant         | 사용 ActionType                                  |
//! |-----------------|-----------------|--------------------------------------------------|
//! | Swap            | `Swap`          | swap, batch_swap, hooked_operation, wrap, unwrap |
//! | Liquidity       | `Liquidity`     | add/remove_liquidity, join/exit_pool, NPM 5종   |
//! | Lending         | `Lending`       | supply, borrow, repay, ... (총 11종)             |
//! | LiquidStaking   | `LiquidStaking` | stake, unstake_request, claim_unstake, ...       |
//! | Restaking       | `Restaking`     | restake, delegate_operator, ... (총 8종)         |
//! | Rwa             | `Rwa`           | subscribe, request_redemption, ... (총 8종)      |
//! | Governance      | `Governance`    | governance_propose/vote/execute/delegate         |
//! | Nft             | `Nft`           | nft_mint/transfer/buy/sell                       |
//! | Vault           | `Vault`         | vault_deposit/withdraw                           |
//! | Utility         | `Utility`       | approval, permit, transfer, claim_rewards, ...   |
//! | Aggregation     | `Aggregation`   | router_plan                                      |
//! | Sign            | `Sign`          | SignPermit2*, SignEip2612Permit, SignEip712Other,|
//! |                 |                 | SignSafeTx, SignSessionKey                       |
//! | Unknown         | `Unknown`       | unknown (catch-all)                              |
//!
//! 모든 variant가 embed:
//!  - `actor: Address` — 보편 표면 (`tx.from` / `signer` 미러)
//!  - `protocol_ids: Vec<String>` — 그 action이 거치는 namespace ID들
//!  - 해당하는 곳에 `RecipientFields` / `DeadlineFields` fragment

use serde::{Deserialize, Serialize};

use crate::confidence::Confidence;
use crate::types::{Address, AmountSpec, ChainId, DeadlineFields, RecipientFields, Token};

/// 타입화된 action 페이로드의 discriminated union (13 variant).
///
/// `kind` 태그는 `ActionCategory`의 직렬화 명칭과 일치 (예: `liquid_staking`).
/// 단 `LiquidStaking` variant는 v0.1 fixture 호환을 위해 `staking` alias도 받음.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionFields {
    Swap(SwapFields),
    Liquidity(LiquidityFields),
    Lending(LendingFields),
    /// `staking`은 v0.1 fixture 호환용 alias.
    #[serde(alias = "staking")]
    LiquidStaking(LiquidStakingFields),
    Restaking(RestakingFields),
    Rwa(RwaFields),
    Governance(GovernanceFields),
    Nft(NftFields),
    Vault(VaultFields),
    Utility(UtilityFields),
    Aggregation(AggregationFields),
    Sign(SignFields),
    /// 분류 실패 catch-all — 원본 args를 그대로 보존.
    Unknown(UnknownFields),
}

// ===========================================================================
// SwapFields  (ActionCategory::Swap)
// ===========================================================================

/// 모든 swap-class action(Swap, BatchSwap, HookedOperation, Wrap, Unwrap)의 공통 표면.
///
/// **필드 출처** (전체 매핑은 `docs/protocol-comparison.md` §1):
/// - `actor`: action-derived (`tx.from`)
/// - `protocol_ids`: action-derived
/// - `input_tokens` / `output_tokens`: action-derived (`path` / struct / `PoolKey`에서)
/// - `mode`: action-derived (function name 또는 `amountSpecified` 부호)
/// - `amount_in` / `amount_out`: action-derived
/// - `route`: action-derived (path 길이 / opcode 목록)
/// - `slippage`: action-derived
/// - `settlement`: adapter:metadata (프로토콜별 정산 모델)
/// - `recipients`: action-derived (`to` / `recipient` / TAKE 대상)
/// - `deadlines`: action-derived
/// - `max_fee_bps`: adapter:metadata (V2 fixed 30, V3 `feeTier`, V4 `PoolKey.fee`, ...)
/// - `has_zero_min_output`: derived (모든 hop의 `amount_out.raw == 0` 검사)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwapFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    #[serde(rename = "inputTokens")]
    pub input_tokens: Vec<Token>,
    #[serde(rename = "outputTokens")]
    pub output_tokens: Vec<Token>,
    pub mode: SwapMode,
    #[serde(rename = "amountIn")]
    pub amount_in: AmountSpec,
    #[serde(rename = "amountOut")]
    pub amount_out: AmountSpec,
    pub route: SwapRoute,
    pub slippage: SlippageInfo,
    pub settlement: SettlementKind,
    pub recipients: RecipientFields,
    pub deadlines: DeadlineFields,
    #[serde(rename = "maxFeeBps", skip_serializing_if = "Option::is_none")]
    pub max_fee_bps: Option<u32>,
    #[serde(rename = "hasZeroMinOutput")]
    pub has_zero_min_output: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwapMode {
    ExactIn,
    ExactOut,
    /// V4 등의 시장가 swap (slippage 없음 또는 다른 모델).
    Market,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SwapRoute {
    SingleHop { hop: HopRef },
    MultiHop { hops: Vec<HopRef> },
    Split { branches: Vec<HopRef> },
    Batch { steps: Vec<HopRef> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HopRef {
    pub id: String,
    pub protocol: String,
    #[serde(rename = "tokenIn")]
    pub token_in: Token,
    #[serde(rename = "tokenOut")]
    pub token_out: Token,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<String>,
    #[serde(rename = "feeBps", skip_serializing_if = "Option::is_none")]
    pub fee_bps: Option<u32>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlippageInfo {
    pub source: SlippageSource,
    #[serde(rename = "amountOutMin", skip_serializing_if = "Option::is_none")]
    pub amount_out_min: Option<AmountSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlippageSource {
    Calldata,
    Derived,
    Unspecified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementKind {
    Router,
    Direct,
    Callback,
    Unknown,
}

// ===========================================================================
// LiquidityFields  (ActionCategory::Liquidity) — 신규 ⭐
// ===========================================================================

/// 모든 liquidity-class action 공통 표면 (add/remove_liquidity, join/exit_pool,
/// V3/V4 NPM 작업).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiquidityFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    /// 풀에 들어가거나 나오는 토큰들 (V2 양쪽 토큰, V3 0/1 currency, Balancer 풀 자산 N개).
    pub tokens: Vec<Token>,
    /// 토큰별 amount (입금 또는 인출). `tokens`와 길이 일치.
    pub amounts: Vec<AmountSpec>,
    /// V3/V4 NPM 작업 한정 — 포지션 NFT tokenId.
    #[serde(rename = "positionTokenId", skip_serializing_if = "Option::is_none")]
    pub position_token_id: Option<String>,
    /// V3/V4 한정 — fee tier (basis points × 100, 예: 500 = 0.05%).
    #[serde(rename = "feeTier", skip_serializing_if = "Option::is_none")]
    pub fee_tier: Option<u32>,
    /// V3/V4 한정 — 가격 범위 lower tick.
    #[serde(rename = "tickLower", skip_serializing_if = "Option::is_none")]
    pub tick_lower: Option<i32>,
    /// V3/V4 한정 — 가격 범위 upper tick.
    #[serde(rename = "tickUpper", skip_serializing_if = "Option::is_none")]
    pub tick_upper: Option<i32>,
    /// V3/V4 NPM `collect`의 누적 fee 인출 한도 (`amount0Max` / `amount1Max`).
    #[serde(rename = "collectMax", skip_serializing_if = "Option::is_none")]
    pub collect_max: Option<Vec<AmountSpec>>,
    pub recipients: RecipientFields,
    pub deadlines: DeadlineFields,
}

// ===========================================================================
// LendingFields  (ActionCategory::Lending)
// ===========================================================================

/// 모든 lending-class action 공통 표면 (supply/borrow/repay/withdraw_collateral
/// /set_collateral/liquidation_repay/flash_loan/repay_with_atokens
/// /swap_borrow_rate_mode/set_e_mode/mint_unbacked, 총 11종).
///
/// 일부 ActionType은 추가 필드를 사용 — `BorrowFields`/`LiquidationRepayFields`
/// 등 별도 변형 대신 `Option<T>` 필드들로 통합.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LendingFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    /// 입금/인출/대출/상환되는 자산. set_collateral, set_e_mode, swap_borrow_rate_mode는 단일 자산.
    pub asset: Token,
    /// `kind`는 보통 `Exact`; repay-max 패턴은 `Unlimited`.
    /// set_collateral / set_e_mode / swap_borrow_rate_mode는 amount 의미 없음 → `Unspecified`.
    pub amount: AmountSpec,
    /// `onBehalfOf` (Aave) / `onBehalf` (Morpho).
    #[serde(rename = "onBehalfOf")]
    pub on_behalf_of: Address,
    /// Borrow / Repay / SwapBorrowRateMode 한정.
    #[serde(rename = "interestRateMode", skip_serializing_if = "Option::is_none")]
    pub interest_rate_mode: Option<InterestRateMode>,
    /// SetCollateral 한정 — `useAsCollateral` 인자.
    #[serde(rename = "useAsCollateral", skip_serializing_if = "Option::is_none")]
    pub use_as_collateral: Option<bool>,
    /// SetEMode 한정 — Aave V3 `categoryId` (0 = exit eMode).
    #[serde(rename = "eModeCategoryId", skip_serializing_if = "Option::is_none")]
    pub e_mode_category_id: Option<u8>,
    /// LiquidationRepay 한정 — 청산 대상 사용자.
    #[serde(rename = "liquidationTarget", skip_serializing_if = "Option::is_none")]
    pub liquidation_target: Option<Address>,
    /// LiquidationRepay 한정 — 담보로 받을 자산.
    #[serde(rename = "collateralAsset", skip_serializing_if = "Option::is_none")]
    pub collateral_asset: Option<Token>,
    /// FlashLoan 한정 — 빌리는 자산들 (다중).
    #[serde(rename = "flashAssets", skip_serializing_if = "Option::is_none")]
    pub flash_assets: Option<Vec<Token>>,
    /// FlashLoan 한정 — 자산별 amount (`flash_assets`와 동일 길이).
    #[serde(rename = "flashAmounts", skip_serializing_if = "Option::is_none")]
    pub flash_amounts: Option<Vec<AmountSpec>>,
    /// FlashLoan 한정 — Aave 한정 mode 0=none / 2=variable.
    #[serde(rename = "flashModes", skip_serializing_if = "Option::is_none")]
    pub flash_modes: Option<Vec<u8>>,
    /// `to` / `receiver` for Withdraw / Borrow 등.
    pub recipients: RecipientFields,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterestRateMode {
    Variable,
    Stable,
    None,
}

// ===========================================================================
// LiquidStakingFields  (ActionCategory::LiquidStaking)
// ===========================================================================

/// Lido / Rocket Pool 등 LST 작업 공통 (stake/unstake_request/claim_unstake
/// /wrap_receipt/unwrap_receipt).
///
/// (구 v0.1 `StakingFields`를 이름만 바꿈 — `LiquidStakingFields`로.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiquidStakingFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    #[serde(rename = "assetIn")]
    pub asset_in: Token,
    #[serde(rename = "assetOut", skip_serializing_if = "Option::is_none")]
    pub asset_out: Option<Token>,
    pub amount: AmountSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referral: Option<Address>,
    #[serde(rename = "withdrawalRequestId", skip_serializing_if = "Option::is_none")]
    pub withdrawal_request_id: Option<String>,
    pub recipients: RecipientFields,
}

// ===========================================================================
// RestakingFields  (ActionCategory::Restaking) — 신규 ⭐
// ===========================================================================

/// EigenLayer / Renzo / etherfi / Kelp 등 재스테이킹 공통 (8 ActionType 통합).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestakingFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    /// 재스테이킹 또는 LRT mint 시 입력 자산 (LST 또는 native ETH).
    /// delegate/undelegate에는 unset.
    #[serde(rename = "assetIn", skip_serializing_if = "Option::is_none")]
    pub asset_in: Option<Token>,
    /// LRT mint의 경우 receipt 토큰.
    #[serde(rename = "assetOut", skip_serializing_if = "Option::is_none")]
    pub asset_out: Option<Token>,
    /// 입력 amount. delegate/undelegate에는 unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<AmountSpec>,
    /// EigenLayer `delegateTo`의 대상 operator.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<Address>,
    /// EigenLayer Strategy 컨트랙트 주소.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<Address>,
    /// queue/claim withdrawal 시 withdrawal root (또는 ID).
    #[serde(rename = "withdrawalRoot", skip_serializing_if = "Option::is_none")]
    pub withdrawal_root: Option<String>,
    pub recipients: RecipientFields,
}

// ===========================================================================
// RwaFields  (ActionCategory::Rwa) — 신규 ⭐
// ===========================================================================

/// Centrifuge / Ondo / Securitize / BlackRock BUIDL 등 RWA 공통 (8 ActionType).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RwaFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    /// RWA receipt 토큰 (예: USDY, BUIDL, ERC-7540 share).
    pub asset: Token,
    /// 입금 자산 (subscribe) 또는 출금 자산 (claim_redemption).
    #[serde(rename = "settlementAsset", skip_serializing_if = "Option::is_none")]
    pub settlement_asset: Option<Token>,
    pub amount: AmountSpec,
    /// 비동기 vault — 진행 중 request의 `requestId` (ERC-7540).
    #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// KYC controller 또는 RestrictionManager 주소.
    #[serde(rename = "controllerAddress", skip_serializing_if = "Option::is_none")]
    pub controller_address: Option<Address>,
    pub recipients: RecipientFields,
}

// ===========================================================================
// GovernanceFields  (ActionCategory::Governance) — 신규 ⭐
// ===========================================================================

/// Compound/Aave Governor, Uniswap UNI 거버넌스 등.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernanceFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    /// Governor 컨트랙트.
    pub governor: Address,
    /// vote/execute 시 proposal id.
    #[serde(rename = "proposalId", skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
    /// `castVote`의 support: 0=against, 1=for, 2=abstain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support: Option<u8>,
    /// `castVoteWithReason` 한정.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// `delegate(delegatee)` 한정.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegatee: Option<Address>,
    /// propose 시 description 첫 N자 (실제 description은 보통 매우 김).
    #[serde(rename = "descriptionPreview", skip_serializing_if = "Option::is_none")]
    pub description_preview: Option<String>,
}

// ===========================================================================
// NftFields  (ActionCategory::Nft) — 신규 ⭐
// ===========================================================================

/// NFT 작업 공통 (mint/transfer/buy/sell).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NftFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    /// NFT 컬렉션 주소.
    pub collection: Address,
    /// ERC-721 / ERC-1155 식별자. `None`이면 batch mint.
    #[serde(rename = "tokenId", skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    /// ERC-1155 한정 — quantity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<AmountSpec>,
    /// buy/sell 시 거래 가격 (현금 자산 기준 — 보통 ETH/WETH/USDC).
    #[serde(rename = "priceAsset", skip_serializing_if = "Option::is_none")]
    pub price_asset: Option<Token>,
    #[serde(rename = "priceAmount", skip_serializing_if = "Option::is_none")]
    pub price_amount: Option<AmountSpec>,
    /// 마켓플레이스 (Seaport, Blur, X2Y2 등).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace: Option<String>,
    pub recipients: RecipientFields,
}

// ===========================================================================
// VaultFields  (ActionCategory::Vault) — 신규 ⭐
// ===========================================================================

/// ERC-4626 등 일반 vault deposit/withdraw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VaultFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    /// vault 컨트랙트 (ERC-4626 share token이기도 함).
    pub vault: Address,
    /// underlying 자산.
    pub asset: Token,
    /// `assets` 또는 `shares` 단위 amount. Deposit/Withdraw 모두 의미는 `assets` 기준.
    pub amount: AmountSpec,
    /// `mint(shares)` / `redeem(shares)` 변형이면 true.
    #[serde(rename = "isShareDenominated", skip_serializing_if = "Option::is_none")]
    pub is_share_denominated: Option<bool>,
    pub recipients: RecipientFields,
}

// ===========================================================================
// UtilityFields  (ActionCategory::Utility) — 신규 ⭐
// ===========================================================================

/// 가로지르는 유틸리티 (approval, permit(inline), transfer, claim_rewards,
/// multicall, sign_message, airdrop_claim, merkle_claim, wrap, unwrap).
///
/// 매우 다양한 ActionType이 들어가므로 `kind` discriminator + 필드 모두 optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UtilityFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    /// approval / permit / transfer 한정 — 대상 토큰.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<Token>,
    /// approval / permit 한정 — spender.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spender: Option<Address>,
    /// approval / permit / transfer / claim_rewards 한정 — amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<AmountSpec>,
    /// claim_rewards 한정 — 출처 분기.
    /// `lending` | `restaking` | `liquid_staking` | `rwa` | `other`.
    #[serde(rename = "rewardsSource", skip_serializing_if = "Option::is_none")]
    pub rewards_source: Option<String>,
    /// claim_rewards / airdrop_claim 한정 — 청구되는 토큰들.
    #[serde(rename = "rewardTokens", skip_serializing_if = "Option::is_none")]
    pub reward_tokens: Option<Vec<Token>>,
    /// merkle_claim / airdrop_claim 한정 — Merkle root 또는 distributor 주소.
    #[serde(rename = "merkleRoot", skip_serializing_if = "Option::is_none")]
    pub merkle_root: Option<String>,
    /// merkle_claim / airdrop_claim 한정 — proof 길이 (실제 proof는 너무 김).
    #[serde(rename = "proofLength", skip_serializing_if = "Option::is_none")]
    pub proof_length: Option<usize>,
    /// sign_message 한정 — 서명할 메시지의 hex.
    #[serde(rename = "messageHex", skip_serializing_if = "Option::is_none")]
    pub message_hex: Option<String>,
    /// multicall 한정 — 자식 호출 수.
    #[serde(rename = "subCallCount", skip_serializing_if = "Option::is_none")]
    pub sub_call_count: Option<usize>,
    /// transfer 한정 — recipient.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipients: Option<RecipientFields>,
    pub deadlines: DeadlineFields,
}

// ===========================================================================
// AggregationFields  (ActionCategory::Aggregation)
// ===========================================================================

/// Universal Router `execute(...)`의 Aggregation 컨테이너. 자식(각 opcode)은
/// `parent_action_id`로 연결된 별도 `Action`이 됨.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregationFields {
    pub actor: Address,
    #[serde(rename = "protocolIds")]
    pub protocol_ids: Vec<String>,
    pub family: String,
    #[serde(rename = "commandsHex")]
    pub commands_hex: String,
    pub mask: u8,
    #[serde(rename = "childCount")]
    pub child_count: usize,
    pub deadlines: DeadlineFields,
}

// ===========================================================================
// SignFields  (ActionCategory::Sign)
// ===========================================================================

/// 모든 EIP-712 typed-data 서명 흐름의 공통 표면.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignFields {
    pub signer: Address,
    #[serde(rename = "chainId")]
    pub chain_id: ChainId,
    pub domain: crate::request::Eip712Domain,
    #[serde(rename = "primaryType")]
    pub primary_type: String,
    pub semantic: SignSemantic,
    pub deadlines: DeadlineFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignSemantic {
    Permit2Approve {
        spender: Address,
        tokens: Vec<TokenAmountWithExpiry>,
        nonce: String,
    },
    Permit2TransferFrom {
        spender: Address,
        transfers: Vec<TokenAmount>,
        nonce: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        witness: Option<String>,
        #[serde(rename = "witnessTypeString", skip_serializing_if = "Option::is_none")]
        witness_type_string: Option<String>,
    },
    Eip2612Permit {
        token: Address,
        owner: Address,
        spender: Address,
        value: AmountSpec,
        nonce: String,
    },
    /// Gnosis Safe `SafeTx` — to/value/data/operation/safeTxGas 등.
    SafeTx {
        safe: Address,
        to: Address,
        value: String,
        data: String,
        operation: u8,
        nonce: String,
    },
    /// Account Abstraction 세션 키 (4337 / 7702).
    SessionKey {
        account: Address,
        #[serde(rename = "sessionKey")]
        session_key: Address,
        #[serde(rename = "validUntil")]
        valid_until: u64,
        #[serde(rename = "permissionsHash", skip_serializing_if = "Option::is_none")]
        permissions_hash: Option<String>,
    },
    /// catch-all — 원본 typed-data JSON을 그대로 보관.
    Other {
        #[serde(rename = "typesJson")]
        types_json: serde_json::Value,
        #[serde(rename = "messageJson")]
        message_json: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenAmount {
    pub token: Address,
    pub amount: AmountSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenAmountWithExpiry {
    pub token: Address,
    pub amount: AmountSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration: Option<u64>,
}

// ===========================================================================
// UnknownFields  (ActionCategory::Unknown) — 신규 ⭐
// ===========================================================================

/// 분류 실패 catch-all. 원본 args를 그대로 보존하여 디버깅 가능.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnknownFields {
    pub actor: Address,
    /// 원본 selector / opcode / primary_type 등 분류 시도된 키.
    #[serde(rename = "attemptedKey", skip_serializing_if = "Option::is_none")]
    pub attempted_key: Option<String>,
    /// 디코드 시도가 실패한 사유.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 원본 calldata args (디버깅용).
    #[serde(rename = "rawArgs", default)]
    pub raw_args: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AmountKind;

    #[test]
    fn swap_mode_snake_case() {
        let json = serde_json::to_string(&SwapMode::ExactIn).unwrap();
        assert_eq!(json, "\"exact_in\"");
    }

    #[test]
    fn sign_semantic_tagged_round_trip() {
        let s = SignSemantic::Eip2612Permit {
            token: "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48".parse().unwrap(),
            owner: "0x1111111111111111111111111111111111111111".parse().unwrap(),
            spender: "0x2222222222222222222222222222222222222222".parse().unwrap(),
            value: AmountSpec {
                raw: "1000000000".into(),
                kind: AmountKind::Exact,
            },
            nonce: "0".into(),
        };
        let j = serde_json::to_string(&s).unwrap();
        let back: SignSemantic = serde_json::from_str(&j).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn liquid_staking_kind_alias_staking_works() {
        // v0.1 fixture 호환 — "kind": "staking"이 LiquidStaking으로 deserialize되어야.
        let json = serde_json::json!({
            "kind": "staking",
            "actor": "0x1111111111111111111111111111111111111111",
            "protocolIds": ["lido"],
            "assetIn": { "chainId": 1, "address": "0x0000000000000000000000000000000000000000", "symbol": "ETH", "decimals": 18, "isNative": true },
            "amount": { "raw": "1000000000000000000", "kind": "Exact" },
            "recipients": { "recipientEqualsActor": true, "hasExternalRecipient": false }
        });
        let af: ActionFields = serde_json::from_value(json).unwrap();
        assert!(matches!(af, ActionFields::LiquidStaking(_)));
    }

    #[test]
    fn new_variants_round_trip() {
        // 새 13 variant 중 신규 6개의 round-trip
        let liquidity = ActionFields::Liquidity(LiquidityFields {
            actor: "0x1111111111111111111111111111111111111111".parse().unwrap(),
            protocol_ids: vec!["uniswap.v3".into()],
            tokens: vec![],
            amounts: vec![],
            position_token_id: Some("12345".into()),
            fee_tier: Some(500),
            tick_lower: Some(-887220),
            tick_upper: Some(887220),
            collect_max: None,
            recipients: RecipientFields {
                recipient: None,
                recipient_equals_actor: true,
                has_external_recipient: false,
            },
            deadlines: DeadlineFields {
                deadline: None,
                deadline_horizon_seconds: None,
            },
        });
        let j = serde_json::to_string(&liquidity).unwrap();
        let back: ActionFields = serde_json::from_str(&j).unwrap();
        assert_eq!(liquidity, back);
    }
}
