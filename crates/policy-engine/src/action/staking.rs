//! Staking action schema types.

use serde::{Deserialize, Serialize};

use super::common::{Address, AmountConstraint, AssetRef, DecimalString, Hex};

/// Claim ticket for a delayed unstake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketRef {
    /// Ticket NFT collection, when the claim right is represented as an NFT.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nft: Option<AssetRef>,
    /// Ticket token id or sequence id, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<DecimalString>,
    /// Bytes identifier, when the ticket is hash-based.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Hex>,
}

/// Stake a base asset and receive a staking receipt token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StakeAction {
    /// Asset being staked.
    pub token_in: AssetRef,
    /// Receipt token received from staking.
    pub receipt_token: AssetRef,
    /// Staked amount.
    pub amount_in: AmountConstraint,
    /// Expected or minimum receipt amount, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<AmountConstraint>,
    /// Recipient of the staking receipt.
    pub recipient: Address,
}

/// Request delayed unstaking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestUnstakeAction {
    /// Receipt token being locked or burned.
    pub receipt_token: AssetRef,
    /// Asset expected after cooldown, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_out: Option<AssetRef>,
    /// Receipt token amount locked or burned.
    pub amount_in: AmountConstraint,
    /// Expected output amount, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_out: Option<AmountConstraint>,
    /// Claim ticket produced by the request, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket: Option<TicketRef>,
    /// Recipient of the claim right.
    pub recipient: Address,
}

/// Claim a completed unstake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimUnstakeAction {
    /// Asset received from the unstake claim.
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

    fn ticket() -> Value {
        json!({
            "nft": erc721("WITHDRAWAL"),
            "tokenId": "42",
            "id": hex32(0x20)
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
        test_stake_action_serde_roundtrip_minimal,
        StakeAction,
        json!({
            "tokenIn": native("ETH"),
            "receiptToken": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_stake_action_serde_roundtrip_full,
        StakeAction,
        json!({
            "tokenIn": native("ETH"),
            "receiptToken": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "amountOut": amount("estimated", "999"),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_request_unstake_action_serde_roundtrip_minimal,
        RequestUnstakeAction,
        json!({
            "receiptToken": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_request_unstake_action_serde_roundtrip_full,
        RequestUnstakeAction,
        json!({
            "receiptToken": erc20("stETH"),
            "tokenOut": native("ETH"),
            "amountIn": amount("exact", "1000"),
            "amountOut": amount("estimated", "999"),
            "ticket": ticket(),
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_claim_unstake_action_serde_roundtrip_minimal,
        ClaimUnstakeAction,
        json!({
            "tokenOut": native("ETH"),
            "ticket": {},
            "recipient": address(0x30)
        })
    );

    roundtrip_test!(
        test_claim_unstake_action_serde_roundtrip_full,
        ClaimUnstakeAction,
        json!({
            "tokenOut": native("ETH"),
            "amountOut": amount("exact", "999"),
            "ticket": ticket(),
            "recipient": address(0x30)
        })
    );
}
