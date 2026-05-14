//! Restaking action schema types.

use serde::{Deserialize, Serialize};

use super::{
    common::{Address, AmountConstraint, AssetRef, Hex},
    staking::TicketRef,
};

/// Restaking strategy or vault reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyRef {
    /// Strategy or vault contract address, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    /// Strategy or vault identifier, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Hex>,
    /// Human-readable strategy label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Restake an asset into a strategy or vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestakeAction {
    /// Asset being restaked.
    pub token_in: AssetRef,
    /// Receipt token received from restaking, when one is minted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_token: Option<AssetRef>,
    /// Restaked amount.
    pub amount_in: AmountConstraint,
    /// Expected or minimum receipt amount, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<AmountConstraint>,
    /// Strategy or vault receiving the asset, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<StrategyRef>,
    /// Recipient of shares or receipt tokens.
    pub recipient: Address,
}

/// Request a delayed restaking withdrawal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestRestakeWithdrawalAction {
    /// Asset expected after escrow, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_out: Option<AssetRef>,
    /// Receipt token being burned, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_token: Option<AssetRef>,
    /// Amount locked or burned.
    pub amount_in: AmountConstraint,
    /// Expected output amount, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<AmountConstraint>,
    /// Strategy or vault being unwound, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<StrategyRef>,
    /// Claim ticket produced by the request, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketRef>,
    /// Recipient of the claim right.
    pub recipient: Address,
}

/// Claim a completed restaking withdrawal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRestakeWithdrawalAction {
    /// Asset received from the withdrawal claim.
    pub token_out: AssetRef,
    /// Claimed amount, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<AmountConstraint>,
    /// Claim ticket being consumed.
    pub ticket: TicketRef,
    /// Recipient of claimed assets.
    pub recipient: Address,
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

    fn native(symbol: &str) -> Value {
        json!({
            "kind": "native",
            "symbol": symbol,
            "decimals": 18
        })
    }

    fn erc20(symbol: &str) -> Value {
        json!({
            "kind": "erc20",
            "address": address(0x10),
            "symbol": symbol,
            "decimals": 18
        })
    }

    fn erc721(symbol: &str) -> Value {
        json!({
            "kind": "erc721",
            "address": address(0x11),
            "symbol": symbol
        })
    }

    fn amount(kind: &str, value: &str) -> Value {
        json!({ "kind": kind, "value": value })
    }

    fn strategy() -> Value {
        json!({
            "address": address(0x20),
            "id": hex32(0x21),
            "label": "Example Strategy"
        })
    }

    fn ticket() -> Value {
        json!({
            "nft": erc721("WITHDRAWAL"),
            "tokenId": "42",
            "id": hex32(0x22)
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
        test_restake_action_serde_roundtrip_minimal,
        RestakeAction,
        json!({
            "tokenIn": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_restake_action_serde_roundtrip_full,
        RestakeAction,
        json!({
            "tokenIn": erc20("stETH"),
            "receiptToken": erc20("ezETH"),
            "amountIn": amount("exact", "1000"),
            "amountOut": amount("estimated", "999"),
            "strategy": strategy(),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_request_restake_withdrawal_action_serde_roundtrip_minimal,
        RequestRestakeWithdrawalAction,
        json!({
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_request_restake_withdrawal_action_serde_roundtrip_full,
        RequestRestakeWithdrawalAction,
        json!({
            "tokenOut": native("ETH"),
            "receiptToken": erc20("ezETH"),
            "amountIn": amount("exact", "1000"),
            "amountOut": amount("estimated", "999"),
            "strategy": strategy(),
            "ticket": ticket(),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_claim_restake_withdrawal_action_serde_roundtrip_minimal,
        ClaimRestakeWithdrawalAction,
        json!({
            "tokenOut": native("ETH"),
            "ticket": {},
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_claim_restake_withdrawal_action_serde_roundtrip_full,
        ClaimRestakeWithdrawalAction,
        json!({
            "tokenOut": native("ETH"),
            "amountOut": amount("exact", "999"),
            "ticket": ticket(),
            "recipient": address(0x30)
        })
    );
}
