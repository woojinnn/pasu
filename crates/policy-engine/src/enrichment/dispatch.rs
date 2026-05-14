//! Dispatch normalized actions to their per-action enrichment handlers.

use crate::action::{Action, ActionEnvelope, Address as ActionAddress};
use crate::host::HostCapabilities;

use super::dex;

/// Populate host-derived facts on an action envelope.
///
/// Swap actions currently receive USD and allowance enrichment. Other DEX
/// actions are routed through no-op stubs so their enrichers can grow in place.
/// Non-DEX actions pass through unchanged.
#[must_use]
pub fn enrich_envelope(
    envelope: ActionEnvelope,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) -> ActionEnvelope {
    let ActionEnvelope { category, action } = envelope;
    let action = match action {
        Action::Swap(mut swap) => {
            dex::enrich_swap(&mut swap, from, target, host);
            Action::Swap(swap)
        }
        Action::AddLiquidity(mut action) => {
            dex::enrich_add_liquidity(&mut action, from, target, host);
            Action::AddLiquidity(action)
        }
        Action::RemoveLiquidity(mut action) => {
            dex::enrich_remove_liquidity(&mut action, from, target, host);
            Action::RemoveLiquidity(action)
        }
        Action::MintLiquidityNft(mut action) => {
            dex::enrich_mint_liquidity_nft(&mut action, from, target, host);
            Action::MintLiquidityNft(action)
        }
        Action::BurnLiquidityNft(mut action) => {
            dex::enrich_burn_liquidity_nft(&mut action, from, target, host);
            Action::BurnLiquidityNft(action)
        }
        Action::IncreaseLiquidity(mut action) => {
            dex::enrich_increase_liquidity(&mut action, from, target, host);
            Action::IncreaseLiquidity(action)
        }
        Action::DecreaseLiquidity(mut action) => {
            dex::enrich_decrease_liquidity(&mut action, from, target, host);
            Action::DecreaseLiquidity(action)
        }
        other => other,
    };

    ActionEnvelope { category, action }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr as _;

    use crate::action::common::{AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString};
    use crate::action::misc::{ApprovalKind, ApproveAction};
    use crate::action::Category;
    use crate::host::{HostCapabilities, MockOracle};

    fn action_addr(value: &str) -> ActionAddress {
        ActionAddress::from_str(value).unwrap()
    }

    fn decimal(value: &str) -> DecimalString {
        DecimalString::from_str(value).unwrap()
    }

    fn usdc_asset() -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            address: Some(action_addr("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48")),
            token_id: None,
            symbol: Some("USDC".to_owned()),
            decimals: Some(6),
        }
    }

    fn amount(kind: AmountKind, value: &str) -> AmountConstraint {
        AmountConstraint {
            kind,
            value: Some(decimal(value)),
        }
    }

    fn from_addr() -> ActionAddress {
        action_addr("0x1111111111111111111111111111111111111111")
    }

    fn target_addr() -> ActionAddress {
        action_addr("0x3333333333333333333333333333333333333333")
    }

    #[test]
    fn enrich_non_dex_returns_unchanged() {
        let oracle = MockOracle::new();
        let host = HostCapabilities::new(&oracle);
        let envelope = ActionEnvelope {
            category: Category::Misc,
            action: Action::Approve(ApproveAction {
                token: usdc_asset(),
                spender: target_addr(),
                spender_label: None,
                amount: amount(AmountKind::Exact, "1000"),
                approval_kind: ApprovalKind::Erc20,
                current_allowance: None,
                validity: None,
            }),
        };
        let original = envelope.clone();

        let enriched = enrich_envelope(envelope, &from_addr(), &target_addr(), &host);

        assert_eq!(enriched, original);
    }
}
