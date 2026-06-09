//! Normalize an HL human-decimal amount to the EVM token amount projection.
//!
//! An HL fund-movement action (`usd_send` / `spot_send` / `vault_transfer` /
//! `sub_account_transfer`) carries its amount as a human decimal string
//! (e.g. `"250"`, `"500.25"`). The `Token::Erc20Transfer` context it now lowers
//! to expects the EVM 3-layer projection: `amount` (raw smallest-unit `U256`
//! hex) + the host-populated `amountNano` (`Long`). `amountUsd` (Cedar
//! `decimal`) stays host-populated and is NOT emitted here — every token leaf
//! omits it (see `token/erc20_transfer.rs`).

use policy_state::primitives::{Decimal, U256};

use super::super::common::cedar::u256_hex;

/// The amount layers a `Token::Erc20Transfer` context carries from a lowering.
pub(super) struct HlAmount {
    /// `context.amount` — raw smallest-unit `U256`, lower-hex.
    pub raw_hex: String,
    /// `context.amountNano` — raw × 10^(9 − decimals), or `None` if it overflows i64.
    pub nano: Option<i64>,
}

/// Normalize an HL human-decimal `amount` (e.g. `"250"`, `"500.25"`) with
/// `decimals` (USDC = 6) to raw smallest-unit hex + nano. Parses the decimal as
/// integer-part + fractional-part to avoid float error; pads/truncates the
/// fraction to `decimals` digits.
pub(super) fn hl_amount_projection(amount: &Decimal, decimals: u32) -> HlAmount {
    let s = amount.0.as_str();
    let (int_part, frac_part) = s.split_once('.').unwrap_or((s, ""));
    let mut digits = String::new();
    digits.push_str(int_part.trim_start_matches('+'));
    let mut frac: String = frac_part.chars().take(decimals as usize).collect();
    while (frac.len() as u32) < decimals {
        frac.push('0');
    }
    digits.push_str(&frac);
    let trimmed = digits.trim_start_matches('0');
    let raw = if trimmed.is_empty() {
        U256::ZERO
    } else {
        U256::from_str_radix(trimmed, 10).unwrap_or(U256::ZERO)
    };
    let nano_scale = 9i32 - decimals as i32;
    let nano = compute_nano(raw, nano_scale);
    HlAmount {
        raw_hex: u256_hex(raw),
        nano,
    }
}

/// raw × 10^scale (scale = 9 − decimals), as i64; `None` on overflow.
fn compute_nano(raw: U256, scale: i32) -> Option<i64> {
    let scaled = if scale >= 0 {
        raw.checked_mul(U256::from(10u64).pow(U256::from(scale as u64)))?
    } else {
        raw / U256::from(10u64).pow(U256::from((-scale) as u64))
    };
    i64::try_from(scaled).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::primitives::Decimal;

    #[test]
    fn usdc_250_normalizes() {
        // 250 USDC, 6 dp → raw 250_000000 → hex 0xee6b280; nano = raw*10^(9-6)=2.5e11.
        let p = hl_amount_projection(&Decimal::new("250"), 6);
        assert_eq!(p.raw_hex, "0xee6b280");
        assert_eq!(p.nano, Some(250_000_000_000));
    }

    #[test]
    fn fractional_round_trips() {
        let p = hl_amount_projection(&Decimal::new("500.25"), 6);
        assert_eq!(p.raw_hex, "0x1dd13590"); // 500_250000
        assert_eq!(p.nano, Some(500_250_000_000));
    }

    #[test]
    fn zero_amount_is_zero_hex() {
        let p = hl_amount_projection(&Decimal::new("0"), 6);
        assert_eq!(p.raw_hex, "0x0");
        assert_eq!(p.nano, Some(0));
    }
}
