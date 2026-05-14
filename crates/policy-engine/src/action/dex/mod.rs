//! DEX action schema types.

use serde::{Deserialize, Serialize};

use super::common::{Address, UsdValuation};

mod add_liquidity;
mod burn_liquidity_nft;
mod decrease_liquidity;
mod increase_liquidity;
mod mint_liquidity_nft;
mod remove_liquidity;
mod swap;

pub use add_liquidity::AddLiquidityAction;
pub use burn_liquidity_nft::BurnLiquidityNftAction;
pub use decrease_liquidity::DecreaseLiquidityAction;
pub use increase_liquidity::IncreaseLiquidityAction;
pub use mint_liquidity_nft::MintLiquidityNftAction;
pub use remove_liquidity::RemoveLiquidityAction;
pub use swap::SwapAction;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Swap amount mode.
pub enum SwapMode {
    /// Input amount is exact and output amount is minimum acceptable.
    ExactIn,
    /// Output amount is exact and input amount is maximum acceptable.
    ExactOut,
    /// Market swap without slippage protection.
    Market,
    /// Unknown or unsupported swap mode.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Liquidity withdrawal mode for fungible LP positions.
pub enum RemoveLiquidityExitMode {
    /// Withdraw all pool assets proportionally.
    Proportional,
    /// Withdraw into one underlying asset.
    SingleAsset,
    /// Burn enough LP to receive exact underlying amounts.
    ExactOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Burn semantics for concentrated-liquidity position NFTs.
pub enum BurnKind {
    /// Burn an already-empty NFT position.
    EmptyOnly,
    /// Decrease all liquidity and burn the NFT atomically.
    AutoDecrease,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Protocol-agnostic pool reference.
pub struct PoolRef {
    /// Pool contract address, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    /// Protocol-specific pool identifier, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Human-readable pool label, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Concentrated-liquidity tick bounds.
pub struct TickRange {
    /// Lower tick.
    pub lower: i32,
    /// Upper tick.
    pub upper: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Optional host-derived valuation facts for a swap.
pub struct SwapEnrichment {
    /// USD value of the input amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_in_usd: Option<UsdValuation>,
    /// Minimum USD value of the output amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value_out_usd: Option<UsdValuation>,
    /// Expected USD value of the output amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_value_out_usd: Option<UsdValuation>,
    /// Input amount as basis points of the portfolio value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_fraction_of_portfolio_bps: Option<u32>,
}

impl SwapEnrichment {
    /// Returns true when no enrichment fields are populated.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.value_in_usd.is_none()
            && self.min_value_out_usd.is_none()
            && self.expected_value_out_usd.is_none()
            && self.input_fraction_of_portfolio_bps.is_none()
    }
}

#[cfg(test)]
pub(super) mod test_support {
    use std::str::FromStr as _;

    use crate::action::common::{
        Address, AmountConstraint, AmountKind, AssetKind, AssetRef, AssetRefWithAmountConstraint,
        DecimalString, UsdValuation, Validity, ValiditySource,
    };

    use super::PoolRef;

    pub(crate) fn address(value: &str) -> Address {
        Address::from_str(value).unwrap()
    }

    pub(crate) fn decimal(value: &str) -> DecimalString {
        DecimalString::from_str(value).unwrap()
    }

    pub(crate) fn erc20(address_value: &str, symbol: &str, decimals: u8) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            address: Some(address(address_value)),
            token_id: None,
            symbol: Some(symbol.to_owned()),
            decimals: Some(decimals),
        }
    }

    pub(crate) fn erc721(address_value: &str, symbol: &str) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc721,
            address: Some(address(address_value)),
            token_id: None,
            symbol: Some(symbol.to_owned()),
            decimals: None,
        }
    }

    pub(crate) fn nft_position() -> AssetRef {
        AssetRef {
            token_id: Some(decimal("42")),
            ..erc721("0x4444444444444444444444444444444444444444", "UNI-V3-POS")
        }
    }

    pub(crate) fn amount(kind: AmountKind, value: &str) -> AmountConstraint {
        AmountConstraint {
            kind,
            value: Some(decimal(value)),
        }
    }

    pub(crate) fn validity() -> Validity {
        Validity {
            expires_at: decimal("1700000000"),
            source: ValiditySource::TxDeadline,
        }
    }

    pub(crate) fn pool() -> PoolRef {
        PoolRef {
            address: Some(address("0x1111111111111111111111111111111111111111")),
            id: Some(
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            ),
            label: Some("ETH/USDC 0.05%".to_owned()),
        }
    }

    pub(crate) fn token_pair() -> [AssetRef; 2] {
        [
            erc20("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "WETH", 18),
            erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
        ]
    }

    pub(crate) fn asset_amount_pair(
        first: AmountKind,
        second: AmountKind,
    ) -> Vec<AssetRefWithAmountConstraint> {
        let [asset_a, asset_b] = token_pair();
        vec![
            AssetRefWithAmountConstraint {
                asset: asset_a,
                amount: amount(first, "1000000000000000000"),
            },
            AssetRefWithAmountConstraint {
                asset: asset_b,
                amount: amount(second, "2000000"),
            },
        ]
    }

    pub(crate) fn usd(value: &str) -> UsdValuation {
        UsdValuation {
            value: value.to_owned(),
            as_of_ts: Some(1_700_000_000),
            sources: Some(vec!["oracle".to_owned()]),
            stale_sec: Some(30),
        }
    }

    pub(crate) fn assert_roundtrip<T>(value: &T)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let roundtrip = serde_json::from_str::<T>(&json).unwrap();
        assert_eq!(&roundtrip, value);
    }
}
