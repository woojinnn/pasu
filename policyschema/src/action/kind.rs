//! `ActionType` — 구체적인 atomic 종류. 1:1로 타입화된 `ActionFields`에 매핑.
//!
//! v0.1 보강에서 16종 → **72종**으로 확장:
//!  - schema_v260508의 43종을 모두 포함 (v1 보존 14 + Lending 7 + LST 5 +
//!    Restaking 8 + RWA 8 + Utility cross-cutting 1).
//!  - 추가 25종: Liquidity 5 (NPM 작업) + Lending 4 (Aave 고급) +
//!    Utility 4 (multicall·sign_message·airdrop·merkle) + Sign 6 +
//!    Governance 4 + NFT 4 + Vault 2.
//!
//! Bridge·Perp는 의도적 제외 (v0.2 후보).

use serde::{Deserialize, Serialize};

use crate::action::category::ActionCategory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    // ==========================================================================
    // v1 보존 14종 — schema_v260508과 동일 명칭·동일 의미
    // ==========================================================================
    /// Universal Router `execute(...)`의 의미적 컨테이너 (`promote = false`).
    RouterPlan,
    /// 일반 swap (V2/V3 single 또는 multi-hop, single-protocol).
    Swap,
    /// 1:1 토큰 변환 (예: ETH → WETH, stETH → wstETH). `SwapFields` 재사용.
    Wrap,
    /// 1:1 역변환 (WETH → ETH, wstETH → stETH).
    Unwrap,
    /// ERC20 `approve(spender, amount)` 호출.
    Approval,
    /// EIP-2612 `permit(...)` 인라인 호출 (calldata에 포함된 서명, 즉
    /// trans 안의 self-permit. typed-data 서명 자체는 `SignEip2612Permit`).
    Permit,
    /// ERC20 / ERC721 / ERC1155 transfer / safeTransferFrom.
    Transfer,
    /// V2 / Aerodrome V1 add_liquidity.
    AddLiquidity,
    /// V2 / Aerodrome V1 remove_liquidity.
    RemoveLiquidity,
    /// Balancer V2 `joinPool`.
    JoinPool,
    /// Balancer V2 `exitPool`.
    ExitPool,
    /// Balancer V2 `batchSwap` 또는 멀티-protocol split (PancakeSwap SmartRouter).
    BatchSwap,
    /// V4 hook을 거치는 swap (PoolKey.hooks ≠ 0). hook 의미가 외부 의존.
    HookedOperation,
    /// 분류 실패 catch-all.
    Unknown,

    // ==========================================================================
    // Lending 7+4 = 11종
    // ==========================================================================
    /// Aave/Morpho `supply(asset, amount, onBehalfOf, ...)`.
    Supply,
    /// Aave/Morpho `borrow(asset, amount, ..., onBehalfOf)`.
    Borrow,
    /// Aave/Morpho `repay(asset, amount, ...)`.
    Repay,
    /// 담보 인출 (Aave V3 `withdraw`는 supply 반대 — 이름이 헷갈림).
    WithdrawCollateral,
    /// Aave V3 `setUserUseReserveAsCollateral(asset, useAsCollateral)`.
    SetCollateral,
    /// Aave V3 `liquidationCall(...)`.
    LiquidationRepay,
    /// Aave/Balancer/Morpho `flashLoan(...)`.
    FlashLoan,
    /// Aave V3 `repayWithATokens(asset, amount, rateMode)` — aToken으로 직접 상환.
    RepayWithATokens,
    /// Aave V3 `swapBorrowRateMode(asset, rateMode)` — variable ↔ stable.
    SwapBorrowRateMode,
    /// Aave V3 `setUserEMode(categoryId)` — eMode 진입/종료.
    SetEMode,
    /// Aave V3 `mintUnbacked(asset, amount, onBehalfOf, referralCode)` — bridge mode.
    MintUnbacked,

    // ==========================================================================
    // Liquid Staking 5종
    // ==========================================================================
    /// Lido `submit(referral)` payable 등.
    Stake,
    /// Lido `requestWithdrawals(amounts[], owner)`.
    /// `request_withdrawal` alias는 v0.1 fixture 호환을 위함.
    #[serde(alias = "request_withdrawal")]
    UnstakeRequest,
    /// Lido `claimWithdrawal(requestId)`.
    ClaimUnstake,
    /// Lido wstETH `wrap(stETHAmount)` — `Wrap`과 별도 분류 (LST receipt 한정).
    WrapReceipt,
    /// Lido wstETH `unwrap(wstETHAmount)`.
    UnwrapReceipt,

    // ==========================================================================
    // Restaking 8종
    // ==========================================================================
    /// EigenLayer `depositIntoStrategy(...)` 등.
    Restake,
    /// EigenLayer `delegateTo(operator, ...)`.
    DelegateOperator,
    /// EigenLayer `undelegate(staker)`.
    Undelegate,
    /// EigenLayer `queueWithdrawals(...)`.
    QueueWithdrawal,
    /// EigenLayer `completeQueuedWithdrawals(...)`.
    ClaimWithdrawal,
    /// LRT mint (Renzo `deposit`, etherfi `deposit` 등).
    MintLrt,
    /// LRT redemption 요청.
    RequestLrtRedemption,
    /// LRT redemption 청구.
    ClaimLrtRedemption,

    // ==========================================================================
    // RWA 8종
    // ==========================================================================
    /// Centrifuge `requestDeposit(...)` 등.
    Subscribe,
    /// Centrifuge `requestRedeem(...)` 등.
    RequestRedemption,
    /// 비동기 vault subscription claim.
    ClaimSubscription,
    /// 비동기 vault redemption claim.
    ClaimRedemption,
    /// 진행 중 request 취소.
    CancelRequest,
    /// 취소 후 잔여 자산 청구.
    ClaimCancel,
    /// KYC-restricted token transfer (Securitize 등).
    TransferRestricted,
    /// RWA yield 청구 (예: Ondo USDY rebase distribution).
    ClaimYield,

    // ==========================================================================
    // Liquidity 5종 신규 (V3/V4 NPM 작업)
    // ==========================================================================
    /// Uniswap V3/V4 NPM `mint(params)` — 새 NFT 포지션 생성.
    MintPosition,
    /// Uniswap V3/V4 NPM `burn(tokenId)` — 빈 포지션 NFT 삭제.
    BurnPosition,
    /// Uniswap V3/V4 NPM `increaseLiquidity(params)`.
    IncreaseLiquidity,
    /// Uniswap V3/V4 NPM `decreaseLiquidity(params)`.
    DecreaseLiquidity,
    /// Uniswap V3/V4 NPM `collect(params)` — 누적 수수료 인출.
    CollectFees,

    // ==========================================================================
    // Utility 6+4 = 10종
    // ==========================================================================
    /// 어떤 카테고리든 보상 청구 (lending/restaking/liquid_staking/rwa의 횡단).
    /// `source: "lending" | "restaking" | "liquid_staking" | "rwa" | "other"` 분기.
    ClaimRewards,
    /// 단순 multicall 컨테이너 — `RouterPlan`과 달리 의미적 분해 안 함.
    /// (예: 일반 컨트랙트의 `multicall(bytes[])`이 모두 view-only인 경우.)
    Multicall,
    /// EIP-191 `personal_sign` 또는 단순 메시지 서명.
    SignMessage,
    /// Merkle proof 기반 airdrop 청구 (Uniswap UNI airdrop, OP airdrop 등).
    AirdropClaim,
    /// 일반 Merkle distributor (airdrop 외 — 예: lockdrop 보상 청구).
    MerkleClaim,

    // ==========================================================================
    // Governance 4종 신규
    // ==========================================================================
    /// Governor `propose(targets, values, calldatas, description)`.
    GovernancePropose,
    /// Governor `castVote(proposalId, support)` 또는 `castVoteWithReason`.
    GovernanceVote,
    /// Governor `execute(proposalId, ...)` — timelock 후.
    GovernanceExecute,
    /// ERC20Votes `delegate(delegatee)` — voting power 위임.
    GovernanceDelegate,

    // ==========================================================================
    // NFT 4종 신규
    // ==========================================================================
    /// NFT mint (collection 직접 호출 또는 minter contract).
    NftMint,
    /// `transferFrom(from, to, tokenId)` 또는 `safeTransferFrom`.
    NftTransfer,
    /// 마켓플레이스를 통한 NFT 매수 (Seaport `fulfillOrder`, Blur `execute` 등).
    NftBuy,
    /// 마켓플레이스를 통한 NFT 매도 (offer accept).
    NftSell,

    // ==========================================================================
    // Vault 2종 신규 (ERC-4626 등 일반 vault)
    // ==========================================================================
    /// ERC-4626 `deposit(assets, receiver)` 또는 `mint(shares, receiver)`.
    VaultDeposit,
    /// ERC-4626 `withdraw(assets, receiver, owner)` 또는 `redeem(shares, ...)`.
    VaultWithdraw,

    // ==========================================================================
    // Sign 6종 (policyschema 신규 — typed-data 서명 흐름)
    // ==========================================================================
    /// Permit2 `PermitSingle` / `PermitBatch` 서명 — 토큰 권한 부여.
    SignPermit2Approve,
    /// Permit2 `PermitTransferFrom` / `*WitnessTransferFrom`.
    SignPermit2TransferFrom,
    /// EIP-2612 `Permit` typed-data 서명 (token contract 자체).
    SignEip2612Permit,
    /// 인식되지 않는 EIP-712 — 도메인·타입·메시지 원본 보존.
    SignEip712Other,
    /// Gnosis Safe 트랜잭션 서명 (`SafeTx` typed-data).
    SignSafeTx,
    /// Account Abstraction 세션 키 서명 / approval.
    SignSessionKey,
}

impl ActionType {
    /// 이 ActionType이 속하는 카테고리.
    ///
    /// 정책 작성 시 *카테고리 단위*로 룰을 작성할 때 유용. 매핑은 `docs/baseline.md` §3 참조.
    pub fn category(self) -> ActionCategory {
        use ActionType::*;
        match self {
            // Swap
            Swap | BatchSwap | HookedOperation | Wrap | Unwrap => ActionCategory::Swap,

            // Liquidity
            AddLiquidity | RemoveLiquidity | JoinPool | ExitPool | MintPosition | BurnPosition
            | IncreaseLiquidity | DecreaseLiquidity | CollectFees => ActionCategory::Liquidity,

            // Lending
            Supply | Borrow | Repay | WithdrawCollateral | SetCollateral | LiquidationRepay
            | FlashLoan | RepayWithATokens | SwapBorrowRateMode | SetEMode | MintUnbacked => {
                ActionCategory::Lending
            }

            // LiquidStaking
            Stake | UnstakeRequest | ClaimUnstake | WrapReceipt | UnwrapReceipt => {
                ActionCategory::LiquidStaking
            }

            // Restaking
            Restake | DelegateOperator | Undelegate | QueueWithdrawal | ClaimWithdrawal
            | MintLrt | RequestLrtRedemption | ClaimLrtRedemption => ActionCategory::Restaking,

            // RWA
            Subscribe | RequestRedemption | ClaimSubscription | ClaimRedemption | CancelRequest
            | ClaimCancel | TransferRestricted | ClaimYield => ActionCategory::Rwa,

            // Governance
            GovernancePropose | GovernanceVote | GovernanceExecute | GovernanceDelegate => {
                ActionCategory::Governance
            }

            // NFT
            NftMint | NftTransfer | NftBuy | NftSell => ActionCategory::Nft,

            // Vault
            VaultDeposit | VaultWithdraw => ActionCategory::Vault,

            // Utility
            Approval | Permit | Transfer | ClaimRewards | Multicall | SignMessage
            | AirdropClaim | MerkleClaim => ActionCategory::Utility,

            // Aggregation
            RouterPlan => ActionCategory::Aggregation,

            // Sign
            SignPermit2Approve | SignPermit2TransferFrom | SignEip2612Permit | SignEip712Other
            | SignSafeTx | SignSessionKey => ActionCategory::Sign,

            // Unknown
            Unknown => ActionCategory::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_type_snake_case() {
        assert_eq!(serde_json::to_string(&ActionType::Swap).unwrap(), "\"swap\"");
        assert_eq!(
            serde_json::to_string(&ActionType::MintPosition).unwrap(),
            "\"mint_position\""
        );
        assert_eq!(
            serde_json::to_string(&ActionType::SignPermit2Approve).unwrap(),
            "\"sign_permit2_approve\""
        );
        assert_eq!(
            serde_json::to_string(&ActionType::GovernancePropose).unwrap(),
            "\"governance_propose\""
        );
    }

    #[test]
    fn category_mapping_complete() {
        // 72종 모두 카테고리 매핑이 panic 없이 동작.
        let all = [
            ActionType::RouterPlan,
            ActionType::Swap,
            ActionType::Wrap,
            ActionType::Unwrap,
            ActionType::Approval,
            ActionType::Permit,
            ActionType::Transfer,
            ActionType::AddLiquidity,
            ActionType::RemoveLiquidity,
            ActionType::JoinPool,
            ActionType::ExitPool,
            ActionType::BatchSwap,
            ActionType::HookedOperation,
            ActionType::Unknown,
            ActionType::Supply,
            ActionType::Borrow,
            ActionType::Repay,
            ActionType::WithdrawCollateral,
            ActionType::SetCollateral,
            ActionType::LiquidationRepay,
            ActionType::FlashLoan,
            ActionType::RepayWithATokens,
            ActionType::SwapBorrowRateMode,
            ActionType::SetEMode,
            ActionType::MintUnbacked,
            ActionType::Stake,
            ActionType::UnstakeRequest,
            ActionType::ClaimUnstake,
            ActionType::WrapReceipt,
            ActionType::UnwrapReceipt,
            ActionType::Restake,
            ActionType::DelegateOperator,
            ActionType::Undelegate,
            ActionType::QueueWithdrawal,
            ActionType::ClaimWithdrawal,
            ActionType::MintLrt,
            ActionType::RequestLrtRedemption,
            ActionType::ClaimLrtRedemption,
            ActionType::Subscribe,
            ActionType::RequestRedemption,
            ActionType::ClaimSubscription,
            ActionType::ClaimRedemption,
            ActionType::CancelRequest,
            ActionType::ClaimCancel,
            ActionType::TransferRestricted,
            ActionType::ClaimYield,
            ActionType::MintPosition,
            ActionType::BurnPosition,
            ActionType::IncreaseLiquidity,
            ActionType::DecreaseLiquidity,
            ActionType::CollectFees,
            ActionType::ClaimRewards,
            ActionType::Multicall,
            ActionType::SignMessage,
            ActionType::AirdropClaim,
            ActionType::MerkleClaim,
            ActionType::GovernancePropose,
            ActionType::GovernanceVote,
            ActionType::GovernanceExecute,
            ActionType::GovernanceDelegate,
            ActionType::NftMint,
            ActionType::NftTransfer,
            ActionType::NftBuy,
            ActionType::NftSell,
            ActionType::VaultDeposit,
            ActionType::VaultWithdraw,
            ActionType::SignPermit2Approve,
            ActionType::SignPermit2TransferFrom,
            ActionType::SignEip2612Permit,
            ActionType::SignEip712Other,
            ActionType::SignSafeTx,
            ActionType::SignSessionKey,
        ];
        assert_eq!(all.len(), 72, "ActionType 총 72종이어야 함");
        for t in all {
            // panic 없이 매핑되는지만 확인
            let _ = t.category();
        }
    }

    #[test]
    fn specific_mappings() {
        assert_eq!(ActionType::Swap.category(), ActionCategory::Swap);
        assert_eq!(ActionType::Wrap.category(), ActionCategory::Swap);
        assert_eq!(ActionType::MintPosition.category(), ActionCategory::Liquidity);
        assert_eq!(ActionType::Supply.category(), ActionCategory::Lending);
        assert_eq!(ActionType::Stake.category(), ActionCategory::LiquidStaking);
        assert_eq!(ActionType::Restake.category(), ActionCategory::Restaking);
        assert_eq!(ActionType::Subscribe.category(), ActionCategory::Rwa);
        assert_eq!(ActionType::GovernanceVote.category(), ActionCategory::Governance);
        assert_eq!(ActionType::NftMint.category(), ActionCategory::Nft);
        assert_eq!(ActionType::VaultDeposit.category(), ActionCategory::Vault);
        assert_eq!(ActionType::Approval.category(), ActionCategory::Utility);
        assert_eq!(ActionType::ClaimRewards.category(), ActionCategory::Utility);
        assert_eq!(ActionType::RouterPlan.category(), ActionCategory::Aggregation);
        assert_eq!(ActionType::SignSafeTx.category(), ActionCategory::Sign);
        assert_eq!(ActionType::Unknown.category(), ActionCategory::Unknown);
    }
}
