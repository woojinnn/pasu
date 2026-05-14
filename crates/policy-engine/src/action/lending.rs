//! Lending action schema types.

use serde::{Deserialize, Serialize};

use super::common::{Address, AmountConstraint, AssetRef, DecimalString, Hex, Validity};

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

/// Supply assets to a lending market.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplyAction {
    /// Target lending market, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Asset being supplied.
    pub asset: AssetRef,
    /// Supply amount.
    pub amount: AmountConstraint,
    /// Amount dimension, when explicitly known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_mode: Option<AmountMode>,
    /// Account that receives the supply position.
    pub recipient: Address,
    /// Account that provides the asset, when different or explicit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    /// Validity window, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

/// Withdraw supplied assets from a lending market.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawAction {
    /// Source lending market, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Asset being withdrawn.
    pub asset: AssetRef,
    /// Withdrawal amount.
    pub amount: AmountConstraint,
    /// Amount dimension, when explicitly known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_mode: Option<AmountMode>,
    /// Account receiving withdrawn assets.
    pub recipient: Address,
    /// Supply position owner, when different or explicit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_behalf: Option<Address>,
}

/// Borrow assets from a lending market.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BorrowAction {
    /// Source lending market, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Borrowed asset.
    pub asset: AssetRef,
    /// Borrow amount.
    pub amount: AmountConstraint,
    /// Amount dimension, when explicitly known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_mode: Option<AmountMode>,
    /// Account receiving borrowed assets.
    pub recipient: Address,
    /// Debt position owner.
    pub on_behalf: Address,
    /// Validity window, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

/// Repay debt in a lending market.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepayAction {
    /// Target lending market, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Repayment asset.
    pub asset: AssetRef,
    /// Repayment amount.
    pub amount: AmountConstraint,
    /// Amount dimension, when explicitly known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_mode: Option<AmountMode>,
    /// Debt position owner.
    pub on_behalf: Address,
    /// Repayment funding source.
    pub repay_kind: RepayKind,
    /// Validity window, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

/// Liquidate an unhealthy lending position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiquidateAction {
    /// Lending market where liquidation occurs, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Borrower being liquidated.
    pub borrower: Address,
    /// Collateral asset being seized, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collateral_asset: Option<AssetRef>,
    /// Debt asset being repaid or absorbed.
    pub debt_asset: AssetRef,
    /// Debt amount to cover, when debt-side input is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debt_to_cover: Option<AmountConstraint>,
    /// Collateral amount to seize, when collateral-side input is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seized_collateral_amount: Option<AmountConstraint>,
    /// Liquidation mechanism.
    pub liquidation_kind: LiquidationKind,
    /// Liquidation input mode, when explicit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquidate_mode: Option<LiquidateMode>,
    /// Recipient of seized assets, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
    /// Whether Aave collateral is received as aToken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receive_a_token: Option<bool>,
}

/// Borrow assets and repay them in the same transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashLoanAction {
    /// Pool or market issuing the flash loan, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<MarketRef>,
    /// Borrowed assets.
    pub assets: Vec<AssetRef>,
    /// Borrowed amounts matching `assets`.
    pub amounts: Vec<AmountConstraint>,
    /// Callback receiver contract.
    pub receiver: Address,
    /// Account that receives debt if the loan is converted to debt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_behalf: Option<Address>,
    /// Flash loan variant.
    pub flash_loan_kind: FlashLoanKind,
    /// Flash loan fee, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<AmountConstraint>,
}

/// Grant or revoke lending authorization on-chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAuthorizationAction {
    /// Market or protocol where authorization applies, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<MarketRef>,
    /// Account granting or revoking authority.
    pub authorizer: Address,
    /// Account receiving or losing authority.
    pub authorized: Address,
    /// Whether authority is granted.
    pub is_authorized: bool,
    /// Authorization scope.
    pub authorization_scope: AuthorizationScope,
    /// Delegation amount cap, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<AmountConstraint>,
}

/// Sign a lending authorization payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignAuthorizationAction {
    /// Market or verifying contract, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<ContractRef>,
    /// Account signing the authorization.
    pub authorizer: Address,
    /// Account receiving or losing authority.
    pub authorized: Address,
    /// Whether authority is granted.
    pub is_authorized: bool,
    /// Signed authorization scope.
    pub authorization_scope: SignAuthorizationScope,
    /// Delegation amount cap, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<AmountConstraint>,
    /// Signature nonce, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<DecimalString>,
    /// Signature validity window.
    pub validity: Validity,
}

/// Voluntarily revoke previously granted lending authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeAction {
    /// Contract or token whose authority is being revoked, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<ContractRef>,
    /// Account calling the revoke flow.
    pub caller: Address,
    /// Account whose grant is being renounced or revoked.
    pub subject: Address,
    /// Revocation variant.
    pub revoke_kind: RevokeKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Value};
    use std::fmt::Debug;

    #[allow(clippy::needless_pass_by_value)]
    fn assert_json_roundtrip<T>(fixture: Value)
    where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        let action = serde_json::from_value::<T>(fixture.clone()).unwrap();
        let serialized = serde_json::to_value(action).unwrap();

        assert_eq!(serialized, fixture);
    }

    fn address(value: u8) -> String {
        format!("0x{value:040x}")
    }

    fn hex32(value: u8) -> String {
        format!("0x{}", format!("{value:02x}").repeat(32))
    }

    fn asset(symbol: &str) -> Value {
        json!({
            "kind": "erc20",
            "address": address(0x10),
            "symbol": symbol,
            "decimals": 18
        })
    }

    fn amount(kind: &str, value: &str) -> Value {
        json!({ "kind": kind, "value": value })
    }

    fn market() -> Value {
        json!({
            "address": address(0x20),
            "id": hex32(0x21),
            "label": "Example Market"
        })
    }

    fn contract_ref() -> Value {
        json!({
            "address": address(0x22),
            "label": "Example Contract"
        })
    }

    fn validity() -> Value {
        json!({
            "expiresAt": "1700000000",
            "source": "signature-deadline"
        })
    }

    macro_rules! roundtrip_test {
        ($name:ident, $ty:ty, $fixture:expr) => {
            #[test]
            fn $name() {
                assert_json_roundtrip::<$ty>($fixture);
            }
        };
    }

    roundtrip_test!(
        test_supply_action_serde_roundtrip_minimal,
        SupplyAction,
        json!({
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_supply_action_serde_roundtrip_full,
        SupplyAction,
        json!({
            "market": market(),
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "amountMode": "shares",
            "recipient": address(0x30),
            "from": address(0x31),
            "validity": validity()
        })
    );

    roundtrip_test!(
        test_withdraw_action_serde_roundtrip_minimal,
        WithdrawAction,
        json!({
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_withdraw_action_serde_roundtrip_full,
        WithdrawAction,
        json!({
            "market": market(),
            "asset": asset("USDC"),
            "amount": amount("unlimited", "0"),
            "amountMode": "shares",
            "recipient": address(0x30),
            "onBehalf": address(0x31)
        })
    );

    roundtrip_test!(
        test_borrow_action_serde_roundtrip_minimal,
        BorrowAction,
        json!({
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30),
            "onBehalf": address(0x31)
        })
    );

    roundtrip_test!(
        test_borrow_action_serde_roundtrip_full,
        BorrowAction,
        json!({
            "market": market(),
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "amountMode": "shares",
            "recipient": address(0x30),
            "onBehalf": address(0x31),
            "validity": validity()
        })
    );

    roundtrip_test!(
        test_repay_action_serde_roundtrip_minimal,
        RepayAction,
        json!({
            "asset": asset("USDC"),
            "amount": amount("exact", "1000"),
            "onBehalf": address(0x31),
            "repayKind": "debt_asset"
        })
    );

    roundtrip_test!(
        test_repay_action_serde_roundtrip_full,
        RepayAction,
        json!({
            "market": market(),
            "asset": asset("USDC"),
            "amount": amount("unlimited", "0"),
            "amountMode": "shares",
            "onBehalf": address(0x31),
            "repayKind": "atoken_direct",
            "validity": validity()
        })
    );

    roundtrip_test!(
        test_liquidate_action_serde_roundtrip_minimal,
        LiquidateAction,
        json!({
            "borrower": address(0x40),
            "debtAsset": asset("USDC"),
            "liquidationKind": "pool_share"
        })
    );

    roundtrip_test!(
        test_liquidate_action_serde_roundtrip_full,
        LiquidateAction,
        json!({
            "market": market(),
            "borrower": address(0x40),
            "collateralAsset": asset("WETH"),
            "debtAsset": asset("USDC"),
            "debtToCover": amount("exact", "1000"),
            "seizedCollateralAmount": amount("estimated", "1"),
            "liquidationKind": "socializable",
            "liquidateMode": "seize",
            "recipient": address(0x30),
            "receiveAToken": true
        })
    );

    roundtrip_test!(
        test_flash_loan_action_serde_roundtrip_minimal,
        FlashLoanAction,
        json!({
            "assets": [asset("USDC")],
            "amounts": [amount("exact", "1000")],
            "receiver": address(0x50),
            "flashLoanKind": "simple"
        })
    );

    roundtrip_test!(
        test_flash_loan_action_serde_roundtrip_full,
        FlashLoanAction,
        json!({
            "pool": market(),
            "assets": [asset("USDC"), asset("WETH")],
            "amounts": [amount("exact", "1000"), amount("exact", "2")],
            "receiver": address(0x50),
            "onBehalf": address(0x31),
            "flashLoanKind": "multi",
            "fee": amount("exact", "5")
        })
    );

    roundtrip_test!(
        test_set_authorization_action_serde_roundtrip_minimal,
        SetAuthorizationAction,
        json!({
            "authorizer": address(0x60),
            "authorized": address(0x61),
            "isAuthorized": true,
            "authorizationScope": "all"
        })
    );

    roundtrip_test!(
        test_set_authorization_action_serde_roundtrip_full,
        SetAuthorizationAction,
        json!({
            "market": market(),
            "authorizer": address(0x60),
            "authorized": address(0x61),
            "isAuthorized": false,
            "authorizationScope": "debt_only",
            "amount": amount("unlimited", "0")
        })
    );

    roundtrip_test!(
        test_sign_authorization_action_serde_roundtrip_minimal,
        SignAuthorizationAction,
        json!({
            "authorizer": address(0x60),
            "authorized": address(0x61),
            "isAuthorized": true,
            "authorizationScope": "all",
            "validity": validity()
        })
    );

    roundtrip_test!(
        test_sign_authorization_action_serde_roundtrip_full,
        SignAuthorizationAction,
        json!({
            "market": contract_ref(),
            "authorizer": address(0x60),
            "authorized": address(0x61),
            "isAuthorized": false,
            "authorizationScope": "debt_only",
            "amount": amount("exact", "1000"),
            "nonce": "7",
            "validity": validity()
        })
    );

    roundtrip_test!(
        test_revoke_action_serde_roundtrip_minimal,
        RevokeAction,
        json!({
            "caller": address(0x70),
            "subject": address(0x71),
            "revokeKind": "erc20_allowance"
        })
    );

    roundtrip_test!(
        test_revoke_action_serde_roundtrip_full,
        RevokeAction,
        json!({
            "target": contract_ref(),
            "caller": address(0x70),
            "subject": address(0x71),
            "revokeKind": "position_manager_role"
        })
    );
}
