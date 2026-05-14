//! USD valuation helpers shared by action enrichers.

use alloy_primitives::U256;

use crate::action::common::{
    AmountConstraint, AssetKind, AssetRef, UsdValuation as ActionUsdValuation,
};
use crate::core::{Address as CoreAddress, Token};
use crate::host::HostCapabilities;

/// Decimal fractional precision used for USD values emitted by enrichment.
///
/// Matches `lowering::decimal::DECIMAL_SCALE` so downstream Cedar lowering
/// can consume the strings without re-scaling.
pub(crate) const USD_SCALE: u32 = 4;

const ACTION_TOKEN_CHAIN_ID: u64 = 0;

/// Compute the USD value for an action amount using the host oracle.
pub(crate) fn usd_value_for_amount(
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

/// Build a host capability token key from an action asset reference.
pub(crate) fn token_from_asset(asset: &AssetRef) -> Option<Token> {
    let address = asset.address.as_ref()?;
    let core_address = CoreAddress::new(&address.to_string()).ok()?;
    Some(Token {
        chain_id: ACTION_TOKEN_CHAIN_ID,
        address: core_address,
        symbol: asset.symbol.clone().unwrap_or_default(),
        decimals: u32::from(asset.decimals.unwrap_or(0)),
        is_native: matches!(asset.kind, AssetKind::Native),
    })
}

/// Token decimal conversion used by USD scaling.
pub(crate) trait TokenDecimals {
    /// Return decimals when they fit the token amount scaling path.
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
/// `raw` is the integer token amount in base units. `price` is the USD price
/// for one whole token as a decimal string. Returns `None` if either input is
/// malformed.
pub(crate) fn scale_amount_to_usd(raw: &str, decimals: u32, price: &str) -> Option<String> {
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

/// Parse a decimal string into a fixed-scale unsigned integer.
pub(crate) fn decimal_to_fixed_u256(value: &str, scale: u32) -> Option<U256> {
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
    let combined = format!(
        "{}{}",
        if whole.is_empty() { "0" } else { whole },
        frac_padded
    );
    U256::from_str_radix(&combined, 10).ok()
}

/// Format a fixed-scale unsigned integer as a decimal string.
pub(crate) fn fixed_to_decimal_u256(value: U256, scale: u32) -> String {
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

    #[test]
    fn scale_amount_to_usd_basic_cases() {
        assert_eq!(
            scale_amount_to_usd("1000000000000000000", 18, "2000.00"),
            Some("2000.0000".to_owned())
        );
        assert_eq!(
            scale_amount_to_usd("1000000", 6, "1.00"),
            Some("1.0000".to_owned())
        );
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
