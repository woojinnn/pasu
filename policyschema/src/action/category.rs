//! `ActionCategory` — cross-cutting 분류. 정책이 *"모든 swap에 대해 …"* 같은
//! 룰을 ActionType별로 일일이 나열하지 않고 작성할 수 있게 해 준다.
//!
//! v0.1 보강에서 6종 → **13종**으로 확장. Bridge·Perp는 의도적 제외 (v0.2 후보).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    /// `Swap`, `Wrap`, `Unwrap` 등 토큰 교환·1:1 변환 
    Swap,
    /// 풀 LP 지분 작업 — `AddLiquidity`, `RemoveLiquidity`, `JoinPool`, `ExitPool`
    /// 및 V3/V4 NPM (`MintPosition`, `BurnPosition`, `IncreaseLiquidity`,
    /// `DecreaseLiquidity`, `CollectFees`).
    Liquidity,
    /// 대출 시장 — `Supply`, `Borrow`, `Repay`, `WithdrawCollateral`,
    /// `SetCollateral`, `LiquidationRepay`, `FlashLoan`,
    /// `RepayWithATokens`, `SwapBorrowRateMode`, `SetEMode`, `MintUnbacked`.
    Lending,
    /// 액화 스테이킹 — `Stake`, `UnstakeRequest`, `ClaimUnstake`,
    /// `WrapReceipt`, `UnwrapReceipt` (Lido 등).
    LiquidStaking,
    /// 재스테이킹 — `Restake`, `DelegateOperator`, `Undelegate`,
    /// `QueueWithdrawal`, `ClaimWithdrawal`, `MintLrt`,
    /// `RequestLrtRedemption`, `ClaimLrtRedemption` (EigenLayer/Renzo/etherfi 등).
    Restaking,
    /// 실세계 자산 — `Subscribe`, `RequestRedemption`, `ClaimSubscription`,
    /// `ClaimRedemption`, `CancelRequest`, `ClaimCancel`, `TransferRestricted`,
    /// `ClaimYield` (Centrifuge/Ondo/Securitize/BlackRock 등).
    Rwa,
    /// 거버넌스 — `GovernanceVote`, `GovernancePropose`, `GovernanceExecute`,
    /// `GovernanceDelegate` (Compound/Aave Governor, Snapshot 등).
    Governance,
    /// NFT — `NftMint`, `NftTransfer`, `NftBuy`, `NftSell` (Seaport/Blur 등).
    Nft,
    /// ERC-4626 등 일반 vault — `VaultDeposit`, `VaultWithdraw` (Yearn/ERC-4626 vaults).
    Vault,
    /// 가로지르는 보조 — `Wrap`, `Unwrap`(WETH), `Approval`, `Permit`, `Transfer`,
    /// `ClaimRewards`, `Multicall`, `SignMessage`, `AirdropClaim`, `MerkleClaim`.
    Utility,
    /// 의미적 컨테이너 — Universal Router `execute(...)`의 `RouterPlan`.
    Aggregation,
    /// EIP-712 typed-data 서명 — `SignPermit2*`, `SignEip2612Permit`,
    /// `SignEip712Other`, `SignSafeTx`, `SignSessionKey`.
    Sign,
    /// 분류 실패 catch-all.
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&ActionCategory::LiquidStaking).unwrap(), "\"liquid_staking\"");
        assert_eq!(serde_json::to_string(&ActionCategory::Governance).unwrap(), "\"governance\"");
        assert_eq!(serde_json::to_string(&ActionCategory::Nft).unwrap(), "\"nft\"");
    }

    #[test]
    fn all_13_variants_round_trip() {
        for cat in [
            ActionCategory::Swap,
            ActionCategory::Liquidity,
            ActionCategory::Lending,
            ActionCategory::LiquidStaking,
            ActionCategory::Restaking,
            ActionCategory::Rwa,
            ActionCategory::Governance,
            ActionCategory::Nft,
            ActionCategory::Vault,
            ActionCategory::Utility,
            ActionCategory::Aggregation,
            ActionCategory::Sign,
            ActionCategory::Unknown,
        ] {
            let s = serde_json::to_string(&cat).unwrap();
            let back: ActionCategory = serde_json::from_str(&s).unwrap();
            assert_eq!(cat, back);
        }
    }
}
