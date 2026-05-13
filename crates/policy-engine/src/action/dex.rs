//! DEX action schema types.

use serde::{Deserialize, Serialize};

use super::common::{Address, AmountConstraint, AssetRef, DecimalString, UsdValuation, Validity};

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
/// Optional host-derived valuation and allowance facts for a swap.
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
    /// Whether current allowance covers the input amount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowance_covers_input: Option<bool>,
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
            && self.allowance_covers_input.is_none()
            && self.input_fraction_of_portfolio_bps.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Single-hop token swap action.
pub struct SwapAction {
    /// Swap amount mode.
    pub mode: SwapMode,
    /// Asset sent by the user.
    pub token_in: AssetRef,
    /// Asset received by the user.
    pub token_out: AssetRef,
    /// Input amount constraint.
    pub amount_in: AmountConstraint,
    /// Output amount constraint.
    pub amount_out: AmountConstraint,
    /// Recipient of the output asset.
    pub recipient: Address,
    /// Slippage tolerance in basis points, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slippage_bps: Option<u32>,
    /// Validity window, when present in calldata or wrapper context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
    /// Pool fee in basis points, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_bps: Option<u32>,
    /// Host-derived enrichment facts.
    #[serde(default, skip_serializing_if = "SwapEnrichment::is_empty")]
    pub enrichment: SwapEnrichment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Add liquidity to a fungible LP pool.
pub struct AddLiquidityAction {
    /// Target pool, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<PoolRef>,
    /// Assets deposited into the pool.
    pub tokens: Vec<AssetRef>,
    /// Deposit amount constraints matching `tokens`.
    pub amounts: Vec<AmountConstraint>,
    /// Minimum per-token input amounts, when represented by the protocol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_amounts_in: Option<Vec<AmountConstraint>>,
    /// LP token minted by the pool, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lp_token: Option<AssetRef>,
    /// LP amount constraint, when represented by the protocol.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lp_amount: Option<AmountConstraint>,
    /// Recipient of the LP token.
    pub recipient: Address,
    /// Validity window, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Remove liquidity from a fungible LP pool.
pub struct RemoveLiquidityAction {
    /// Withdrawal mode.
    pub exit_mode: RemoveLiquidityExitMode,
    /// Source pool, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<PoolRef>,
    /// LP token being burned, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lp_token: Option<AssetRef>,
    /// LP burn amount constraint, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lp_burn_amount: Option<AmountConstraint>,
    /// Underlying pool assets.
    pub tokens: Vec<AssetRef>,
    /// Minimum output constraints matching `tokens`.
    pub min_amounts_out: Vec<AmountConstraint>,
    /// Recipient of the withdrawn assets.
    pub recipient: Address,
    /// Validity window, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Mint a concentrated-liquidity position NFT.
pub struct MintLiquidityNftAction {
    /// Target pool, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<PoolRef>,
    /// Pool fee in basis points.
    pub fee_bps: u32,
    /// Minted position tick range.
    pub tick_range: TickRange,
    /// Position token pair.
    pub tokens: [AssetRef; 2],
    /// Desired deposit amounts matching `tokens`.
    pub amounts: [AmountConstraint; 2],
    /// Minimum deposit amounts matching `tokens`, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_amounts_in: Option<[AmountConstraint; 2]>,
    /// NFT collection for the minted position.
    pub nft: AssetRef,
    /// Recipient of the minted NFT.
    pub recipient: Address,
    /// Validity window, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Burn a concentrated-liquidity position NFT.
pub struct BurnLiquidityNftAction {
    /// NFT collection for the position.
    pub nft: AssetRef,
    /// Position token id.
    pub token_id: DecimalString,
    /// Burn semantics.
    pub burn_kind: BurnKind,
    /// Output assets for auto-decrease burns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<[AssetRef; 2]>,
    /// Minimum output amounts for auto-decrease burns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_amounts_out: Option<[AmountConstraint; 2]>,
    /// Recipient for auto-decrease burn outputs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient: Option<Address>,
    /// Validity window, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Increase liquidity in an existing position NFT.
pub struct IncreaseLiquidityAction {
    /// NFT collection for the position.
    pub nft: AssetRef,
    /// Position token id.
    pub token_id: DecimalString,
    /// Position token pair.
    pub tokens: [AssetRef; 2],
    /// Desired input amounts matching `tokens`.
    pub amounts: [AmountConstraint; 2],
    /// Minimum input amounts matching `tokens`, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_amounts_in: Option<[AmountConstraint; 2]>,
    /// Validity window, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Decrease liquidity in an existing position NFT.
pub struct DecreaseLiquidityAction {
    /// NFT collection for the position.
    pub nft: AssetRef,
    /// Position token id.
    pub token_id: DecimalString,
    /// Internal liquidity amount to remove.
    pub liquidity_delta: AmountConstraint,
    /// Output assets.
    pub outputs: [AssetRef; 2],
    /// Minimum output amounts matching `outputs`.
    pub min_amounts_out: [AmountConstraint; 2],
    /// Validity window, when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validity: Option<Validity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::common::{
        Address, AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString, UsdValuation,
        Validity, ValiditySource,
    };
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Value};
    use std::{fmt::Debug, str::FromStr as _};

    fn address(value: &str) -> Address {
        Address::from_str(value).unwrap()
    }

    fn decimal(value: &str) -> DecimalString {
        DecimalString::from_str(value).unwrap()
    }

    fn erc20(address_value: &str, symbol: &str, decimals: u8) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            chain_id: 1,
            address: Some(address(address_value)),
            symbol: Some(symbol.to_owned()),
            decimals: Some(decimals),
        }
    }

    fn erc721(address_value: &str, symbol: &str) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc721,
            chain_id: 1,
            address: Some(address(address_value)),
            symbol: Some(symbol.to_owned()),
            decimals: None,
        }
    }

    fn amount(kind: AmountKind, value: &str) -> AmountConstraint {
        AmountConstraint {
            kind,
            value: Some(decimal(value)),
        }
    }

    fn validity() -> Validity {
        Validity {
            expires_at: decimal("1700000000"),
            source: ValiditySource::TxDeadline,
        }
    }

    fn pool() -> PoolRef {
        PoolRef {
            address: Some(address("0x1111111111111111111111111111111111111111")),
            id: Some(
                "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            ),
            label: Some("ETH/USDC 0.05%".to_owned()),
        }
    }

    fn token_pair() -> [AssetRef; 2] {
        [
            erc20("0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "WETH", 18),
            erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
        ]
    }

    fn amount_pair(first: AmountKind, second: AmountKind) -> [AmountConstraint; 2] {
        [
            amount(first, "1000000000000000000"),
            amount(second, "2000000"),
        ]
    }

    fn usd(value: &str) -> UsdValuation {
        UsdValuation {
            value: value.to_owned(),
            as_of_ts: Some(1_700_000_000),
            sources: Some(vec!["oracle".to_owned()]),
            stale_sec: Some(30),
        }
    }

    fn assert_roundtrip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + PartialEq + Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let roundtrip = serde_json::from_str::<T>(&json).unwrap();
        assert_eq!(&roundtrip, value);
    }

    #[test]
    fn test_swap_action_serde_roundtrip_minimal() {
        let action = SwapAction {
            mode: SwapMode::ExactIn,
            token_in: token_pair()[0].clone(),
            token_out: token_pair()[1].clone(),
            amount_in: amount(AmountKind::Exact, "1000000000000000000"),
            amount_out: amount(AmountKind::Min, "1900000"),
            recipient: address("0x2222222222222222222222222222222222222222"),
            slippage_bps: None,
            validity: None,
            fee_bps: None,
            enrichment: SwapEnrichment::default(),
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_swap_action_serde_roundtrip_full() {
        let action = SwapAction {
            mode: SwapMode::ExactOut,
            token_in: token_pair()[0].clone(),
            token_out: token_pair()[1].clone(),
            amount_in: amount(AmountKind::Max, "1100000000000000000"),
            amount_out: amount(AmountKind::Exact, "2000000"),
            recipient: address("0x2222222222222222222222222222222222222222"),
            slippage_bps: Some(50),
            validity: Some(validity()),
            fee_bps: Some(5),
            enrichment: SwapEnrichment {
                value_in_usd: Some(usd("2000.00")),
                min_value_out_usd: Some(usd("1990.00")),
                expected_value_out_usd: Some(usd("2001.00")),
                allowance_covers_input: Some(true),
                input_fraction_of_portfolio_bps: Some(125),
            },
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_add_liquidity_action_serde_roundtrip_minimal() {
        let action = AddLiquidityAction {
            pool: None,
            tokens: token_pair().to_vec(),
            amounts: amount_pair(AmountKind::Exact, AmountKind::Exact).to_vec(),
            min_amounts_in: None,
            lp_token: None,
            lp_amount: None,
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_add_liquidity_action_serde_roundtrip_full() {
        let action = AddLiquidityAction {
            pool: Some(pool()),
            tokens: token_pair().to_vec(),
            amounts: amount_pair(AmountKind::Estimated, AmountKind::Estimated).to_vec(),
            min_amounts_in: Some(amount_pair(AmountKind::Min, AmountKind::Min).to_vec()),
            lp_token: Some(erc20(
                "0x3333333333333333333333333333333333333333",
                "UNI-V2",
                18,
            )),
            lp_amount: Some(amount(AmountKind::Min, "100000000000000000")),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_remove_liquidity_action_serde_roundtrip_minimal() {
        let action = RemoveLiquidityAction {
            exit_mode: RemoveLiquidityExitMode::Proportional,
            pool: None,
            lp_token: None,
            lp_burn_amount: None,
            tokens: token_pair().to_vec(),
            min_amounts_out: amount_pair(AmountKind::Min, AmountKind::Min).to_vec(),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_remove_liquidity_action_serde_roundtrip_full() {
        let action = RemoveLiquidityAction {
            exit_mode: RemoveLiquidityExitMode::SingleAsset,
            pool: Some(pool()),
            lp_token: Some(erc20(
                "0x3333333333333333333333333333333333333333",
                "UNI-V2",
                18,
            )),
            lp_burn_amount: Some(amount(AmountKind::Exact, "100000000000000000")),
            tokens: token_pair().to_vec(),
            min_amounts_out: amount_pair(AmountKind::Min, AmountKind::Min).to_vec(),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_mint_liquidity_nft_action_serde_roundtrip_minimal() {
        let action = MintLiquidityNftAction {
            pool: None,
            fee_bps: 5,
            tick_range: TickRange {
                lower: -60,
                upper: 60,
            },
            tokens: token_pair(),
            amounts: amount_pair(AmountKind::Estimated, AmountKind::Estimated),
            min_amounts_in: None,
            nft: erc721("0x4444444444444444444444444444444444444444", "UNI-V3-POS"),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_mint_liquidity_nft_action_serde_roundtrip_full() {
        let action = MintLiquidityNftAction {
            pool: Some(pool()),
            fee_bps: 30,
            tick_range: TickRange {
                lower: -120,
                upper: 120,
            },
            tokens: token_pair(),
            amounts: amount_pair(AmountKind::Estimated, AmountKind::Estimated),
            min_amounts_in: Some(amount_pair(AmountKind::Min, AmountKind::Min)),
            nft: erc721("0x4444444444444444444444444444444444444444", "UNI-V3-POS"),
            recipient: address("0x2222222222222222222222222222222222222222"),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_burn_liquidity_nft_action_serde_roundtrip_minimal() {
        let action = BurnLiquidityNftAction {
            nft: erc721("0x4444444444444444444444444444444444444444", "UNI-V3-POS"),
            token_id: decimal("42"),
            burn_kind: BurnKind::EmptyOnly,
            outputs: None,
            min_amounts_out: None,
            recipient: None,
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_burn_liquidity_nft_action_serde_roundtrip_full() {
        let action = BurnLiquidityNftAction {
            nft: erc721("0x4444444444444444444444444444444444444444", "UNI-V3-POS"),
            token_id: decimal("42"),
            burn_kind: BurnKind::AutoDecrease,
            outputs: Some(token_pair()),
            min_amounts_out: Some(amount_pair(AmountKind::Min, AmountKind::Min)),
            recipient: Some(address("0x2222222222222222222222222222222222222222")),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_increase_liquidity_action_serde_roundtrip_minimal() {
        let action = IncreaseLiquidityAction {
            nft: erc721("0x4444444444444444444444444444444444444444", "UNI-V3-POS"),
            token_id: decimal("42"),
            tokens: token_pair(),
            amounts: amount_pair(AmountKind::Estimated, AmountKind::Estimated),
            min_amounts_in: None,
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_increase_liquidity_action_serde_roundtrip_full() {
        let action = IncreaseLiquidityAction {
            nft: erc721("0x4444444444444444444444444444444444444444", "UNI-V3-POS"),
            token_id: decimal("42"),
            tokens: token_pair(),
            amounts: amount_pair(AmountKind::Estimated, AmountKind::Estimated),
            min_amounts_in: Some(amount_pair(AmountKind::Min, AmountKind::Min)),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_decrease_liquidity_action_serde_roundtrip_minimal() {
        let action = DecreaseLiquidityAction {
            nft: erc721("0x4444444444444444444444444444444444444444", "UNI-V3-POS"),
            token_id: decimal("42"),
            liquidity_delta: amount(AmountKind::Exact, "100000000000000000"),
            outputs: token_pair(),
            min_amounts_out: amount_pair(AmountKind::Min, AmountKind::Min),
            validity: None,
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_decrease_liquidity_action_serde_roundtrip_full() {
        let action = DecreaseLiquidityAction {
            nft: erc721("0x4444444444444444444444444444444444444444", "UNI-V3-POS"),
            token_id: decimal("42"),
            liquidity_delta: amount(AmountKind::Exact, "100000000000000000"),
            outputs: token_pair(),
            min_amounts_out: amount_pair(AmountKind::Min, AmountKind::Min),
            validity: Some(validity()),
        };

        assert_roundtrip(&action);
    }

    #[test]
    fn test_swap_enrichment_omitted_when_empty() {
        let action = SwapAction {
            mode: SwapMode::ExactIn,
            token_in: token_pair()[0].clone(),
            token_out: token_pair()[1].clone(),
            amount_in: amount(AmountKind::Exact, "1000000000000000000"),
            amount_out: amount(AmountKind::Min, "1900000"),
            recipient: address("0x2222222222222222222222222222222222222222"),
            slippage_bps: None,
            validity: None,
            fee_bps: None,
            enrichment: SwapEnrichment::default(),
        };

        let value = serde_json::to_value(action).unwrap();

        assert!(value.get("enrichment").is_none());
    }

    #[test]
    fn test_swap_action_matches_schema_fixture() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../schema/schema/actions/dex/swap.json"
        ))
        .unwrap();
        let fixture = schema
            .get("examples")
            .and_then(Value::as_array)
            .and_then(|examples| examples.first())
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "mode": "exact_in",
                    "tokenIn": {
                        "kind": "erc20",
                        "chainId": 1,
                        "address": "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                        "symbol": "WETH",
                        "decimals": 18
                    },
                    "tokenOut": {
                        "kind": "erc20",
                        "chainId": 1,
                        "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
                        "symbol": "USDC",
                        "decimals": 6
                    },
                    "amountIn": {
                        "kind": "exact",
                        "value": "1000000000000000000"
                    },
                    "amountOut": {
                        "kind": "min",
                        "value": "1900000"
                    },
                    "recipient": "0x2222222222222222222222222222222222222222"
                })
            });

        let action = serde_json::from_value::<SwapAction>(fixture).unwrap();

        assert_eq!(action.mode, SwapMode::ExactIn);
        assert!(action.enrichment.is_empty());
    }
}
