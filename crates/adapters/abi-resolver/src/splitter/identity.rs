//! Identity splitter — wraps the input calldata as a single sub-call.
//!
//! Used as the fallback when no protocol-specific splitter is registered for
//! `(chain_id, to, selector)`. Most top-level contract calls (ERC20
//! `approve`, V2/V3 direct swaps, lending interactions, …) take this path:
//! the outer call IS the only sub-call.

use policy_engine::action::DecimalString;
use std::str::FromStr as _;

use super::{SplitContext, SplitError, SubCall};

/// Returns a single [`SubCall`] mirroring the outer call. Lifetimeless,
/// zero-state — the splitter registry can hand out a shared instance.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdentitySplitter;

impl IdentitySplitter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Synthesise the single SubCall directly. Doesn't go through the
    /// `Splitter` trait because the identity splitter intentionally does
    /// not register match keys — the registry falls back to it instead.
    pub fn split(ctx: &SplitContext<'_>, calldata: &[u8]) -> Result<Vec<SubCall>, SplitError> {
        if calldata.len() < 4 {
            return Err(SplitError::CalldataTooShort(calldata.len()));
        }
        Ok(vec![SubCall {
            to: ctx.to.clone(),
            value_wei: ctx.value_wei.clone(),
            calldata: calldata.to_vec(),
            decoded: None,
        }])
    }
}

/// Convenience constant for value-only / no-data sub-calls (rare, but the
/// API allows it). Used in tests.
#[allow(dead_code)]
pub(super) fn zero_value() -> DecimalString {
    DecimalString::from_str("0").expect("0 is a valid decimal string")
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_engine::action::Address;

    fn addr(s: &str) -> Address {
        s.parse().unwrap()
    }

    #[test]
    fn identity_returns_single_subcall_with_full_calldata() {
        let from = addr("0x1111111111111111111111111111111111111111");
        let to = addr("0x2222222222222222222222222222222222222222");
        let value = zero_value();
        let ctx = SplitContext {
            chain_id: 1,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: None,
        };
        let calldata = vec![0x09, 0x5e, 0xa7, 0xb3, 0xde, 0xad, 0xbe, 0xef];
        let sub_calls = IdentitySplitter::split(&ctx, &calldata).unwrap();
        assert_eq!(sub_calls.len(), 1);
        assert_eq!(sub_calls[0].to, to);
        assert_eq!(sub_calls[0].calldata, calldata);
    }

    #[test]
    fn identity_rejects_short_calldata() {
        let from = addr("0x1111111111111111111111111111111111111111");
        let to = addr("0x2222222222222222222222222222222222222222");
        let value = zero_value();
        let ctx = SplitContext {
            chain_id: 1,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: None,
        };
        let err = IdentitySplitter::split(&ctx, &[0x09, 0x5e]).unwrap_err();
        assert!(matches!(err, SplitError::CalldataTooShort(2)));
    }
}
