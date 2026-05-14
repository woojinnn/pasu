//! Per-action lowering for DEX actions.
//!
//! Each submodule provides an `impl Lower for <Action>` so the dispatcher in
//! [`crate::lowering::dispatch`] can call `action.build(&ctx)` uniformly.

pub(crate) mod add_liquidity;
pub(crate) mod burn_liquidity_nft;
pub(crate) mod decrease_liquidity;
pub(crate) mod increase_liquidity;
pub(crate) mod mint_liquidity_nft;
pub(crate) mod remove_liquidity;
pub(crate) mod swap;

#[cfg(test)]
pub(crate) mod test_support {
    use std::str::FromStr as _;

    use crate::action::dex::{PoolRef, TickRange};
    use crate::action::{
        Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
        AssetRefWithAmountConstraint, Category, DecimalString, UsdValuation, Validity,
        ValiditySource,
    };
    use crate::policy::PolicyRequest;

    pub(crate) const BLOCK_TIMESTAMP: u64 = 1_700_000_000;

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

    pub(crate) fn nft(token_id: &str) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc721,
            address: Some(address("0x4444444444444444444444444444444444444444")),
            token_id: Some(decimal(token_id)),
            symbol: Some("UNI-V3-POS".to_owned()),
            decimals: None,
        }
    }

    pub(crate) fn amount(kind: AmountKind, value: &str) -> AmountConstraint {
        AmountConstraint {
            kind,
            value: Some(decimal(value)),
        }
    }

    pub(crate) fn amount_without_value(kind: AmountKind) -> AmountConstraint {
        AmountConstraint { kind, value: None }
    }

    pub(crate) fn usd(value: &str) -> UsdValuation {
        UsdValuation {
            value: value.to_owned(),
            as_of_ts: Some(BLOCK_TIMESTAMP),
            sources: Some(vec!["oracle".to_owned()]),
            stale_sec: Some(30),
        }
    }

    pub(crate) fn validity(expires_at: u64) -> Validity {
        Validity {
            expires_at: decimal(&expires_at.to_string()),
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

    pub(crate) fn tick_range() -> TickRange {
        TickRange {
            lower: -887_220,
            upper: 887_220,
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

    pub(crate) fn envelope(action: Action) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Dex,
            action,
        }
    }

    pub(crate) fn policy_request(envelope: &ActionEnvelope, from: &Address) -> PolicyRequest {
        crate::lowering::policy_request_from_envelope(
            envelope,
            from,
            &address("0x2222222222222222222222222222222222222222"),
            &decimal("0"),
            1,
            BLOCK_TIMESTAMP,
        )
        .expect("DEX envelope lowers to policy request")
    }
}
