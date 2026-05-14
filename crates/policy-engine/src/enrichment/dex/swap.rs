//! Swap enrichment from host oracle capabilities.

use crate::action::common::AmountKind;
use crate::action::dex::SwapAction;
use crate::action::Address as ActionAddress;
use crate::enrichment::dispatch::Enrich;
use crate::enrichment::usd::usd_value_for_amount;
use crate::host::HostCapabilities;

impl Enrich for SwapAction {
    fn enrich(
        &mut self,
        _from: &ActionAddress,
        _target: &ActionAddress,
        host: &HostCapabilities<'_>,
    ) {
        if self.enrichment.value_in_usd.is_none() {
            self.enrichment.value_in_usd =
                usd_value_for_amount(&self.token_in, &self.amount_in, host);
        }

        if self.enrichment.expected_value_out_usd.is_none() {
            self.enrichment.expected_value_out_usd =
                usd_value_for_amount(&self.token_out, &self.amount_out, host);
        }

        if self.enrichment.min_value_out_usd.is_none()
            && matches!(self.amount_out.kind, AmountKind::Min)
        {
            self.enrichment.min_value_out_usd =
                usd_value_for_amount(&self.token_out, &self.amount_out, host);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr as _;

    use crate::action::common::{AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString};
    use crate::action::dex::{SwapEnrichment, SwapMode};
    use crate::action::{Action, ActionEnvelope, Category};
    use crate::core::{Address as CoreAddress, Token, UsdValuation as CoreUsdValuation};
    use crate::enrichment::enrich_envelope;
    use crate::host::{HostCapabilities, MockOracle};

    fn action_addr(value: &str) -> ActionAddress {
        ActionAddress::from_str(value).unwrap()
    }

    fn decimal(value: &str) -> DecimalString {
        DecimalString::from_str(value).unwrap()
    }

    fn erc20(address_value: &str, symbol: &str, decimals: u8) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            address: Some(action_addr(address_value)),
            token_id: None,
            symbol: Some(symbol.to_owned()),
            decimals: Some(decimals),
        }
    }

    fn amount(kind: AmountKind, value: &str) -> AmountConstraint {
        AmountConstraint {
            kind,
            value: Some(decimal(value)),
        }
    }

    fn token_for(asset: &AssetRef) -> Token {
        Token {
            chain_id: 0,
            address: CoreAddress::new(asset.address.as_ref().unwrap().to_string().as_str())
                .unwrap(),
            symbol: asset.symbol.clone().unwrap_or_default(),
            decimals: u32::from(asset.decimals.unwrap_or(0)),
            is_native: matches!(asset.kind, AssetKind::Native),
        }
    }

    fn swap_action(
        token_in: AssetRef,
        token_out: AssetRef,
        amount_out_kind: AmountKind,
    ) -> SwapAction {
        SwapAction {
            swap_mode: SwapMode::ExactIn,
            token_in,
            token_out,
            amount_in: amount(AmountKind::Exact, "1000000000000000000"),
            amount_out: amount(amount_out_kind, "2000000000"),
            recipient: action_addr("0x2222222222222222222222222222222222222222"),
            validity: None,
            fee_bps: None,
            enrichment: SwapEnrichment::default(),
        }
    }

    fn swap_envelope(swap: SwapAction) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(swap),
        }
    }

    fn from_addr() -> ActionAddress {
        action_addr("0x1111111111111111111111111111111111111111")
    }

    fn target_addr() -> ActionAddress {
        action_addr("0x3333333333333333333333333333333333333333")
    }

    fn weth_asset() -> AssetRef {
        erc20("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "WETH", 18)
    }

    fn usdc_asset() -> AssetRef {
        erc20("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6)
    }

    fn weth_priced_oracle() -> MockOracle {
        MockOracle::new().with_price(
            &token_for(&weth_asset()),
            CoreUsdValuation {
                value: "2000.0000".to_owned(),
                as_of_ts: 1_700_000_000,
                sources: vec!["mock-oracle".to_owned()],
                stale_sec: 30,
            },
        )
    }

    fn usdc_priced_oracle() -> MockOracle {
        MockOracle::new()
            .with_price(
                &token_for(&weth_asset()),
                CoreUsdValuation {
                    value: "2000.0000".to_owned(),
                    as_of_ts: 1_700_000_000,
                    sources: vec!["mock-oracle".to_owned()],
                    stale_sec: 30,
                },
            )
            .with_price(
                &token_for(&usdc_asset()),
                CoreUsdValuation {
                    value: "1.0000".to_owned(),
                    as_of_ts: 1_700_000_000,
                    sources: vec!["mock-oracle".to_owned()],
                    stale_sec: 30,
                },
            )
    }

    #[test]
    fn enrich_swap_with_oracle_fills_value_in_usd() {
        let oracle = weth_priced_oracle();
        let host = HostCapabilities::new(&oracle);
        let envelope = swap_envelope(swap_action(weth_asset(), usdc_asset(), AmountKind::Min));

        let enriched = enrich_envelope(envelope, &from_addr(), &target_addr(), &host);

        let Action::Swap(swap) = enriched.action else {
            panic!("expected swap action");
        };
        let value_in = swap
            .enrichment
            .value_in_usd
            .expect("value_in_usd should be filled when oracle has a price for token_in");
        assert_eq!(value_in.value, "2000.0000");
        assert_eq!(value_in.as_of_ts, Some(1_700_000_000));
        assert_eq!(
            value_in.sources.as_deref(),
            Some(&["mock-oracle".to_owned()][..])
        );
        assert_eq!(value_in.stale_sec, Some(30));
    }

    #[test]
    fn enrich_min_amount_out_sets_min_value_out_usd() {
        let oracle = usdc_priced_oracle();
        let host = HostCapabilities::new(&oracle);
        let envelope = swap_envelope(swap_action(weth_asset(), usdc_asset(), AmountKind::Min));

        let enriched = enrich_envelope(envelope, &from_addr(), &target_addr(), &host);

        let Action::Swap(swap) = enriched.action else {
            panic!("expected swap action");
        };
        let min_out = swap
            .enrichment
            .min_value_out_usd
            .expect("min_value_out_usd should be filled when amount_out is Min");
        assert_eq!(min_out.value, "2000.0000");
    }

    #[test]
    fn enrich_exact_amount_out_skips_min_value_out_usd() {
        let oracle = usdc_priced_oracle();
        let host = HostCapabilities::new(&oracle);
        let envelope = swap_envelope(swap_action(weth_asset(), usdc_asset(), AmountKind::Exact));

        let enriched = enrich_envelope(envelope, &from_addr(), &target_addr(), &host);

        let Action::Swap(swap) = enriched.action else {
            panic!("expected swap action");
        };
        assert_eq!(
            swap.enrichment.min_value_out_usd, None,
            "Exact amount_out should not produce a min_value_out_usd"
        );
        assert!(swap.enrichment.expected_value_out_usd.is_some());
    }
}
