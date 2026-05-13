//! Swap enrichment stage: fill [`SwapEnrichment`] from [`HostCapabilities`].
//!
//! The new envelope pipeline emits `Action::Swap(SwapAction { enrichment: default, .. })`
//! from Mappers — Mappers know nothing about prices, balances, or allowances.
//! This stage runs after routing and before lowering, consulting the host
//! capability bag to fill in the optional enrichment fields. It is purely
//! additive and best-effort: any host trait that is missing or that returns an
//! error simply leaves the corresponding field as `None`.
//!
//! Fields populated:
//! - `value_in_usd` from `Oracle::price(token_in)` × `amount_in`.
//! - `expected_value_out_usd` from `Oracle::price(token_out)` × `amount_out`
//!   (unconditional — `amount_out.value` is treated as the expected output).
//! - `min_value_out_usd` from `Oracle::price(token_out)` × `amount_out`, only
//!   when `amount_out.kind == AmountKind::Min`.
//! - `allowance_covers_input` from `Approvals::allowance(from, token_in, target)`
//!   vs `amount_in`. Skipped for native input assets.
//! - `input_fraction_of_portfolio_bps` is left as `None` for now: the
//!   `Portfolio` trait does not expose enumeration, so a true portfolio total
//!   (sum across all tokens) cannot be computed from the current host surface.

use alloy_primitives::U256;

use crate::action::common::{
    AmountConstraint, AmountKind, AssetKind, AssetRef, UsdValuation as ActionUsdValuation,
};
use crate::action::dex::SwapAction;
use crate::action::{Action, ActionEnvelope, Address as ActionAddress};
use crate::core::{Address as CoreAddress, Token};
use crate::host::HostCapabilities;

/// Decimal fractional precision used for USD values emitted by enrichment.
///
/// Matches `lowering::decimal::DECIMAL_SCALE` so downstream Cedar lowering
/// can consume the strings without re-scaling.
const USD_SCALE: u32 = 4;

/// Populate host-derived facts on a swap envelope.
///
/// Returns a new envelope with [`crate::action::dex::SwapEnrichment`] filled
/// in for `Action::Swap`. For non-swap actions the envelope is returned
/// unchanged.
///
/// Best-effort: any field whose source data is missing from `host` stays
/// `None`. Errors from host traits are swallowed silently — enrichment is
/// non-critical.
#[must_use]
pub fn enrich_swap_envelope(
    envelope: ActionEnvelope,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) -> ActionEnvelope {
    let ActionEnvelope { category, action } = envelope;
    let Action::Swap(mut swap) = action else {
        return ActionEnvelope {
            category,
            action,
        };
    };

    enrich_swap_action(&mut swap, from, target, host);

    ActionEnvelope {
        category,
        action: Action::Swap(swap),
    }
}

fn enrich_swap_action(
    swap: &mut SwapAction,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) {
    if swap.enrichment.value_in_usd.is_none() {
        swap.enrichment.value_in_usd = usd_value_for_amount(&swap.token_in, &swap.amount_in, host);
    }

    // Expected output: always use amount_out.value regardless of kind.
    if swap.enrichment.expected_value_out_usd.is_none() {
        swap.enrichment.expected_value_out_usd =
            usd_value_for_amount(&swap.token_out, &swap.amount_out, host);
    }

    // Minimum output: only meaningful when amount_out.kind == Min.
    if swap.enrichment.min_value_out_usd.is_none()
        && matches!(swap.amount_out.kind, AmountKind::Min)
    {
        swap.enrichment.min_value_out_usd =
            usd_value_for_amount(&swap.token_out, &swap.amount_out, host);
    }

    if swap.enrichment.allowance_covers_input.is_none() {
        swap.enrichment.allowance_covers_input =
            allowance_covers_input(&swap.token_in, &swap.amount_in, from, target, host);
    }

    // `input_fraction_of_portfolio_bps` requires a true portfolio total, which
    // the current `Portfolio` trait cannot supply (no enumeration). Leave it
    // as `None` until the host surface gains a `portfolio_total_usd` capability.
}

fn usd_value_for_amount(
    asset: &AssetRef,
    amount: &AmountConstraint,
    host: &HostCapabilities<'_>,
) -> Option<ActionUsdValuation> {
    let raw = amount.value.as_ref()?.to_string();
    let token = token_from_asset(asset)?;
    let unit_price = host.oracle().price(&token).ok()?;
    let value = scale_amount_to_usd(&raw, u32::from(token.decimals_u8()?), &unit_price.value)?;
    Some(ActionUsdValuation {
        value,
        as_of_ts: Some(unit_price.as_of_ts),
        sources: if unit_price.sources.is_empty() {
            None
        } else {
            Some(unit_price.sources.clone())
        },
        stale_sec: Some(unit_price.stale_sec),
    })
}

fn allowance_covers_input(
    token_in: &AssetRef,
    amount_in: &AmountConstraint,
    from: &ActionAddress,
    target: &ActionAddress,
    host: &HostCapabilities<'_>,
) -> Option<bool> {
    if matches!(token_in.kind, AssetKind::Native) {
        return None;
    }
    let approvals = host.approvals()?;
    let token = token_from_asset(token_in)?;
    let owner = core_address(from)?;
    let spender = core_address(target)?;
    let allowance = approvals.allowance(&owner, &token, &spender).ok()?;
    let raw = amount_in.value.as_ref()?.to_string();
    let amount_u256 = U256::from_str_radix(&raw, 10).ok()?;
    let allowance_u256 = U256::from_str_radix(&allowance.raw, 10).ok()?;
    Some(allowance_u256 >= amount_u256)
}

fn token_from_asset(asset: &AssetRef) -> Option<Token> {
    let address = asset.address.as_ref()?;
    let core_address = CoreAddress::new(&address.to_string()).ok()?;
    Some(Token {
        chain_id: asset.chain_id,
        address: core_address,
        symbol: asset.symbol.clone().unwrap_or_default(),
        decimals: u32::from(asset.decimals.unwrap_or(0)),
        is_native: matches!(asset.kind, AssetKind::Native),
    })
}

fn core_address(addr: &ActionAddress) -> Option<CoreAddress> {
    CoreAddress::new(&addr.to_string()).ok()
}

trait TokenDecimals {
    fn decimals_u8(&self) -> Option<u8>;
}

impl TokenDecimals for Token {
    fn decimals_u8(&self) -> Option<u8> {
        u8::try_from(self.decimals).ok()
    }
}

/// Compute `(raw / 10^decimals) * price` as a decimal string with fixed
/// fractional precision [`USD_SCALE`].
///
/// `raw` is the integer token amount in base units (wei-scale). `price` is the
/// USD price for one whole token as a decimal string. Returns `None` if either
/// input is malformed.
fn scale_amount_to_usd(raw: &str, decimals: u32, price: &str) -> Option<String> {
    let raw_u = U256::from_str_radix(raw, 10).ok()?;
    let price_fixed = decimal_to_fixed_u256(price, USD_SCALE)?;
    let product = raw_u.checked_mul(price_fixed)?;
    let divisor = U256::from(10u8).checked_pow(U256::from(decimals))?;
    if divisor.is_zero() {
        return None;
    }
    let scaled = product / divisor;
    Some(fixed_to_decimal_u256(scaled, USD_SCALE))
}

fn decimal_to_fixed_u256(value: &str, scale: u32) -> Option<U256> {
    let (whole, frac) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let scale_usize = scale as usize;
    let mut frac_padded = String::from(frac);
    if frac_padded.len() < scale_usize {
        frac_padded.extend(std::iter::repeat_n('0', scale_usize - frac_padded.len()));
    } else if frac_padded.len() > scale_usize {
        frac_padded.truncate(scale_usize);
    }
    let combined = format!("{}{}", if whole.is_empty() { "0" } else { whole }, frac_padded);
    U256::from_str_radix(&combined, 10).ok()
}

fn fixed_to_decimal_u256(value: U256, scale: u32) -> String {
    let raw = value.to_string();
    let scale_usize = scale as usize;
    let padded = if raw.len() <= scale_usize {
        let mut s = String::with_capacity(scale_usize + 1);
        s.push('0');
        for _ in 0..(scale_usize - raw.len()) {
            s.push('0');
        }
        s.push_str(&raw);
        s
    } else {
        raw
    };
    let split = padded.len() - scale_usize;
    let (whole, frac) = padded.split_at(split);
    if scale_usize == 0 {
        whole.to_owned()
    } else {
        format!("{whole}.{frac}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr as _;

    use crate::action::common::{
        Address as ActionAddress, AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString,
    };
    use crate::action::dex::{SwapAction, SwapEnrichment, SwapMode};
    use crate::action::misc::{ApprovalKind, ApproveAction};
    use crate::action::{Action, ActionEnvelope, Category};
    use crate::core::{Address as CoreAddress, Token, UsdValuation as CoreUsdValuation};
    use crate::host::{HostCapabilities, MockApprovals, MockOracle};

    fn action_addr(value: &str) -> ActionAddress {
        ActionAddress::from_str(value).unwrap()
    }

    fn decimal(value: &str) -> DecimalString {
        DecimalString::from_str(value).unwrap()
    }

    fn erc20(address_value: &str, symbol: &str, decimals: u8) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            chain_id: 1,
            address: Some(action_addr(address_value)),
            symbol: Some(symbol.to_owned()),
            decimals: Some(decimals),
        }
    }

    fn native_asset() -> AssetRef {
        AssetRef {
            kind: AssetKind::Native,
            chain_id: 1,
            address: None,
            symbol: Some("ETH".to_owned()),
            decimals: Some(18),
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
            chain_id: asset.chain_id,
            address: CoreAddress::new(asset.address.as_ref().unwrap().to_string().as_str())
                .unwrap(),
            symbol: asset.symbol.clone().unwrap_or_default(),
            decimals: u32::from(asset.decimals.unwrap_or(0)),
            is_native: matches!(asset.kind, AssetKind::Native),
        }
    }

    fn swap_action(token_in: AssetRef, token_out: AssetRef, amount_out_kind: AmountKind) -> SwapAction {
        SwapAction {
            mode: SwapMode::ExactIn,
            token_in,
            token_out,
            amount_in: amount(AmountKind::Exact, "1000000000000000000"), // 1 token (18 dp)
            amount_out: amount(amount_out_kind, "2000000000"), // 2000 USDC (6 dp)
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
        MockOracle::new().with_price(
            &token_for(&weth_asset()),
            CoreUsdValuation {
                value: "2000.0000".to_owned(),
                as_of_ts: 1_700_000_000,
                sources: vec!["mock-oracle".to_owned()],
                stale_sec: 30,
            },
        ).with_price(
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

        let enriched = enrich_swap_envelope(envelope, &from_addr(), &target_addr(), &host);

        let Action::Swap(swap) = enriched.action else {
            panic!("expected swap action");
        };
        let value_in = swap
            .enrichment
            .value_in_usd
            .expect("value_in_usd should be filled when oracle has a price for token_in");
        // 1 WETH (1e18 base units) * 2000.00 USD = 2000.0000 USD
        assert_eq!(value_in.value, "2000.0000");
        assert_eq!(value_in.as_of_ts, Some(1_700_000_000));
        assert_eq!(value_in.sources.as_deref(), Some(&["mock-oracle".to_owned()][..]));
        assert_eq!(value_in.stale_sec, Some(30));
    }

    #[test]
    fn enrich_native_token_skips_allowance() {
        let oracle = MockOracle::new();
        let approvals = MockApprovals::new();
        let host = HostCapabilities::new(&oracle).with_approvals(&approvals);

        let envelope = swap_envelope(swap_action(native_asset(), usdc_asset(), AmountKind::Min));

        let enriched = enrich_swap_envelope(envelope, &from_addr(), &target_addr(), &host);

        let Action::Swap(swap) = enriched.action else {
            panic!("expected swap action");
        };
        assert_eq!(
            swap.enrichment.allowance_covers_input, None,
            "native token input should leave allowance_covers_input as None"
        );
    }

    #[test]
    fn enrich_min_amount_out_sets_min_value_out_usd() {
        let oracle = usdc_priced_oracle();
        let host = HostCapabilities::new(&oracle);
        let envelope = swap_envelope(swap_action(weth_asset(), usdc_asset(), AmountKind::Min));

        let enriched = enrich_swap_envelope(envelope, &from_addr(), &target_addr(), &host);

        let Action::Swap(swap) = enriched.action else {
            panic!("expected swap action");
        };
        let min_out = swap
            .enrichment
            .min_value_out_usd
            .expect("min_value_out_usd should be filled when amount_out is Min");
        // 2_000_000_000 / 1e6 * 1.0 = 2000.0000
        assert_eq!(min_out.value, "2000.0000");
    }

    #[test]
    fn enrich_exact_amount_out_skips_min_value_out_usd() {
        let oracle = usdc_priced_oracle();
        let host = HostCapabilities::new(&oracle);
        let envelope = swap_envelope(swap_action(weth_asset(), usdc_asset(), AmountKind::Exact));

        let enriched = enrich_swap_envelope(envelope, &from_addr(), &target_addr(), &host);

        let Action::Swap(swap) = enriched.action else {
            panic!("expected swap action");
        };
        assert_eq!(
            swap.enrichment.min_value_out_usd, None,
            "Exact amount_out should not produce a min_value_out_usd"
        );
        // expected_value_out_usd is still filled (it's unconditional)
        assert!(swap.enrichment.expected_value_out_usd.is_some());
    }

    #[test]
    fn enrich_non_swap_returns_unchanged() {
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

        let enriched = enrich_swap_envelope(envelope, &from_addr(), &target_addr(), &host);

        assert_eq!(enriched, original);
    }

    #[test]
    fn scale_amount_to_usd_basic_cases() {
        // 1 WETH (1e18 wei) at $2000 → 2000.0000
        assert_eq!(
            scale_amount_to_usd("1000000000000000000", 18, "2000.00"),
            Some("2000.0000".to_owned())
        );
        // 1 USDC (1e6) at $1 → 1.0000
        assert_eq!(
            scale_amount_to_usd("1000000", 6, "1.00"),
            Some("1.0000".to_owned())
        );
        // Half a token at $10 → 5.0000
        assert_eq!(
            scale_amount_to_usd("500000000000000000", 18, "10.00"),
            Some("5.0000".to_owned())
        );
    }

    #[test]
    fn scale_amount_to_usd_rejects_malformed_inputs() {
        assert_eq!(scale_amount_to_usd("not-a-number", 18, "1.00"), None);
        assert_eq!(scale_amount_to_usd("1", 18, "not-a-price"), None);
    }
}
