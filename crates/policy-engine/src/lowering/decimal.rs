//! Decimal helpers keep `multiply_decimal_strings` and `add_decimal_strings` at a
//! fixed 4-decimal precision because Cedar extension decimals in this project are
//! serialized to 4 fractional places.

use alloy_primitives::U256;

const DECIMAL_SCALE: u32 = 4;
/// Largest Cedar decimal integer part accepted by the policy schema.
pub(crate) const HUMAN_INT_CEILING: u128 = 922_337_203_685_477;
const CEDAR_DECIMAL_CEILING_FRACTION: u128 = 5_807;

#[cfg(test)]
pub(crate) fn multiply_decimal_strings(raw: &str, decimals: u32, price: &str) -> String {
    try_multiply_decimal_strings(raw, decimals, price)
        .unwrap_or_else(|| zero_decimal(DECIMAL_SCALE))
}

pub(crate) fn try_multiply_decimal_strings(
    raw: &str,
    decimals: u32,
    price: &str,
) -> Option<String> {
    let raw_u = U256::from_str_radix(raw, 10).ok()?;
    let price_int = decimal_to_fixed(price, DECIMAL_SCALE)?;

    let product = raw_u.saturating_mul(U256::from(price_int));
    let scale = U256::from(10u64).pow(U256::from(decimals));
    let scaled = if scale.is_zero() {
        product
    } else {
        product / scale
    };
    if exceeds_cedar_decimal_ceiling(scaled, DECIMAL_SCALE) {
        return None;
    }

    Some(fixed_to_decimal(scaled, DECIMAL_SCALE))
}

pub(crate) fn add_decimal_strings(left: &str, right: &str) -> String {
    match (
        decimal_to_fixed(left, DECIMAL_SCALE),
        decimal_to_fixed(right, DECIMAL_SCALE),
    ) {
        (Some(left_fixed), Some(right_fixed)) => fixed_to_decimal(
            U256::from(left_fixed.saturating_add(right_fixed)),
            DECIMAL_SCALE,
        ),
        (Some(left_fixed), None) => fixed_to_decimal(U256::from(left_fixed), DECIMAL_SCALE),
        (None, Some(right_fixed)) => fixed_to_decimal(U256::from(right_fixed), DECIMAL_SCALE),
        (None, None) => zero_decimal(DECIMAL_SCALE),
    }
}

pub(crate) fn try_add_decimal_strings(left: &str, right: &str) -> Option<String> {
    let left_fixed = decimal_to_fixed(left, DECIMAL_SCALE)?;
    let right_fixed = decimal_to_fixed(right, DECIMAL_SCALE)?;
    let total = U256::from(left_fixed.saturating_add(right_fixed));
    if exceeds_cedar_decimal_ceiling(total, DECIMAL_SCALE) {
        return None;
    }
    Some(fixed_to_decimal(total, DECIMAL_SCALE))
}

fn exceeds_cedar_decimal_ceiling(value: U256, scale: u32) -> bool {
    value > cedar_decimal_ceiling_fixed(scale)
}

fn cedar_decimal_ceiling_fixed(scale: u32) -> U256 {
    let scale_factor = U256::from(10u64).pow(U256::from(scale));
    U256::from(HUMAN_INT_CEILING)
        .saturating_mul(scale_factor)
        .saturating_add(U256::from(CEDAR_DECIMAL_CEILING_FRACTION))
}

pub(super) fn decimal_to_fixed(s: &str, scale: u32) -> Option<u128> {
    let (whole, frac) = match s.split_once('.') {
        Some((w, f)) => (w, f),
        None => (s, ""),
    };
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if !whole.chars().all(|ch| ch.is_ascii_digit()) || !frac.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let mut frac_padded = String::from(frac);
    while frac_padded.len() < scale as usize {
        frac_padded.push('0');
    }
    if frac_padded.len() > scale as usize {
        frac_padded.truncate(scale as usize);
    }
    let combined = format!("{whole}{frac_padded}");
    combined.parse::<u128>().ok()
}

pub(super) fn fixed_to_decimal(value: U256, scale: u32) -> String {
    let value_str = value.to_string();
    let scale = scale as usize;
    let padded = if value_str.len() <= scale {
        format!("{}{}", "0".repeat(scale + 1 - value_str.len()), value_str)
    } else {
        value_str
    };
    let split = padded.len() - scale;
    let (whole, frac) = padded.split_at(split);
    if scale == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{frac}")
    }
}

fn zero_decimal(scale: u32) -> String {
    fixed_to_decimal(U256::from(0u64), scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiply_decimal_strings_basic() {
        assert_eq!(multiply_decimal_strings("200000000", 6, "1.00"), "200.0000");
    }

    #[test]
    fn multiply_decimal_strings_weth_at_3000() {
        assert_eq!(
            multiply_decimal_strings("1000000000000000000", 18, "3000.0000"),
            "3000.0000"
        );
    }

    #[test]
    fn multiply_decimal_strings_fractional_token() {
        assert_eq!(
            multiply_decimal_strings("500000000000000000", 18, "3000.00"),
            "1500.0000"
        );
    }

    #[test]
    fn decimal_to_fixed_pads_short_fraction() {
        assert_eq!(super::decimal_to_fixed("1.5", 4), Some(15000));
        assert_eq!(super::decimal_to_fixed("1", 4), Some(10000));
        assert_eq!(super::decimal_to_fixed("0", 4), Some(0));
    }

    #[test]
    fn decimal_to_fixed_truncates_long_fraction() {
        assert_eq!(super::decimal_to_fixed("1.123456", 4), Some(11234));
    }

    #[test]
    fn decimal_to_fixed_rejects_malformed_decimal() {
        assert_eq!(super::decimal_to_fixed("not-a-decimal", 4), None);
        assert_eq!(super::decimal_to_fixed("1.12x", 4), None);
    }

    #[test]
    fn multiply_decimal_strings_returns_zero_for_malformed_input() {
        assert_eq!(multiply_decimal_strings("not-a-u256", 6, "1.00"), "0.0000");
        assert_eq!(multiply_decimal_strings("1000000", 6, "bad"), "0.0000");
    }

    #[test]
    fn try_multiply_decimal_strings_returns_none_above_cedar_decimal_ceiling() {
        assert_eq!(
            try_multiply_decimal_strings(
                "115792089237316195423570985008687907853269984665640564039457584007913129639935",
                18,
                "1.0000"
            ),
            None
        );
    }

    #[test]
    fn try_add_decimal_strings_returns_none_above_cedar_decimal_ceiling() {
        assert_eq!(
            try_add_decimal_strings("922337203685477.5807", "0.0001"),
            None
        );
    }

    #[test]
    fn add_decimal_strings_skips_malformed_operand() {
        assert_eq!(add_decimal_strings("1.00", "bad"), "1.0000");
        assert_eq!(add_decimal_strings("bad", "2.00"), "2.0000");
        assert_eq!(add_decimal_strings("bad", "also-bad"), "0.0000");
    }
}
