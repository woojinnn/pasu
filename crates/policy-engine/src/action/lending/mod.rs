//! Lending action schema types.

use serde::{Deserialize, Serialize};

use crate::action::common::{Address, Hex};

mod borrow;
mod flash_loan;
mod liquidate;
mod repay;
mod revoke;
mod set_authorization;
mod sign_authorization;
mod supply;
mod withdraw;

pub use borrow::BorrowAction;
pub use flash_loan::FlashLoanAction;
pub use liquidate::LiquidateAction;
pub use repay::RepayAction;
pub use revoke::RevokeAction;
pub use set_authorization::SetAuthorizationAction;
pub use sign_authorization::SignAuthorizationAction;
pub use supply::SupplyAction;
pub use withdraw::WithdrawAction;

/// Protocol-agnostic lending market reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRef {
    /// Market contract address, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    /// Protocol-specific market identifier, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Hex>,
    /// Human-readable market label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Contract reference for lending authorization or revoke targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractRef {
    /// Contract address, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    /// Human-readable contract label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Lending amount dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmountMode {
    /// Amount is denominated in underlying assets.
    Assets,
    /// Amount is denominated in protocol shares.
    Shares,
}

/// Repayment funding source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepayKind {
    /// Repay with the debt asset.
    DebtAsset,
    /// Repay by burning an Aave aToken balance directly.
    AtokenDirect,
}

/// Liquidation mechanism.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiquidationKind {
    /// Liquidator repays debt and receives a share of collateral.
    PoolShare,
    /// Protocol absorbs the collateral and debt.
    ProtocolAbsorb,
    /// Bad debt can be socialized across suppliers.
    Socializable,
    /// Liquidation is scoped to a single asset.
    SingleAsset,
}

/// Liquidation input dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiquidateMode {
    /// Debt input and collateral output are handled in one step.
    SingleStep,
    /// Collateral amount is the primary input.
    Seize,
    /// Debt repayment amount is the primary input.
    Repay,
}

/// Flash loan protocol variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlashLoanKind {
    /// Aave multi-asset flash loan.
    Multi,
    /// Aave simple single-asset flash loan.
    Simple,
    /// Morpho single-asset flash loan.
    Morpho,
}

/// On-chain authorization scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationScope {
    /// All supported lending operations.
    All,
    /// Borrowing or debt delegation only.
    DebtOnly,
    /// Broad manager role.
    ManagerRole,
    /// Aave position manager role.
    PositionManagerRole,
}

/// Signed authorization scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignAuthorizationScope {
    /// All supported lending operations.
    All,
    /// Borrowing or debt delegation only.
    DebtOnly,
    /// Broad manager role.
    ManagerRole,
}

/// Voluntary authority revocation kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevokeKind {
    /// ERC-20 allowance renounce flow.
    Erc20Allowance,
    /// Credit delegation renounce flow.
    CreditDelegation,
    /// Position manager role renounce flow.
    PositionManagerRole,
    /// Manager role revocation flow.
    ManagerRole,
}

#[cfg(test)]
pub(super) mod test_support {
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Value};
    use std::fmt::Debug;

    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn assert_json_roundtrip<T>(fixture: Value)
    where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        let action = serde_json::from_value::<T>(fixture.clone()).unwrap();
        let serialized = serde_json::to_value(action).unwrap();
        assert_eq!(serialized, fixture);
    }

    pub(crate) fn address(value: u8) -> String {
        format!("0x{value:040x}")
    }

    pub(crate) fn hex32(value: u8) -> String {
        format!("0x{}", format!("{value:02x}").repeat(32))
    }

    pub(crate) fn asset(symbol: &str) -> Value {
        json!({
            "kind": "erc20",
            "address": address(0x10),
            "symbol": symbol,
            "decimals": 18
        })
    }

    pub(crate) fn amount(kind: &str, value: &str) -> Value {
        json!({ "kind": kind, "value": value })
    }

    pub(crate) fn market() -> Value {
        json!({
            "address": address(0x20),
            "id": hex32(0x21),
            "label": "Example Market"
        })
    }

    pub(crate) fn contract_ref() -> Value {
        json!({
            "address": address(0x22),
            "label": "Example Contract"
        })
    }

    pub(crate) fn validity() -> Value {
        json!({
            "expiresAt": "1700000000",
            "source": "signature-deadline"
        })
    }
}
