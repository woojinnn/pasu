//! Shared helpers for ERC-20 / ERC-721 approval & transfer mappers.

use abi_resolver::{DecodedCall, DecodedValue};
use policy_engine::action::common::Address;

use crate::mapper::MapperError;

/// Tolerant arg-name match. Non-standard ERC-20 deployments (notably USDT)
/// ship their ABI with underscore-prefixed names (`_spender`, `_value` …);
/// the canonical naming is `spender`, `amount`/`value`. We treat either form
/// as a match and also accept a small set of well-known synonyms.
pub(super) fn name_matches(actual: &str, expected: &str) -> bool {
    let strip_underscore = |s: &str| s.strip_prefix('_').unwrap_or(s).to_ascii_lowercase();
    let a = strip_underscore(actual);
    let e = strip_underscore(expected);
    if a == e {
        return true;
    }
    // ERC-20 historic synonyms: `value` ↔ `amount`.
    matches!((e.as_str(), a.as_str()), ("amount", "value") | ("value", "amount"))
}

pub(super) fn find_address(decoded: &DecodedCall, name: &str) -> Result<Address, MapperError> {
    decoded
        .args
        .iter()
        .find(|a| name_matches(&a.name, name))
        .and_then(|a| match &a.value {
            DecodedValue::Address(addr) => Some(addr.clone()),
            _ => None,
        })
        .ok_or_else(|| MapperError::MissingArgument(name.into()))
}

pub(super) fn find_bool(decoded: &DecodedCall, name: &str) -> Result<bool, MapperError> {
    decoded
        .args
        .iter()
        .find(|a| name_matches(&a.name, name))
        .and_then(|a| match &a.value {
            DecodedValue::Bool(value) => Some(*value),
            _ => None,
        })
        .ok_or_else(|| MapperError::MissingArgument(name.into()))
}

pub(super) fn find_uint(
    decoded: &DecodedCall,
    name: &str,
) -> Result<alloy_primitives::U256, MapperError> {
    decoded
        .args
        .iter()
        .find(|a| name_matches(&a.name, name))
        .and_then(|a| match &a.value {
            DecodedValue::Uint(u) => Some(*u),
            _ => None,
        })
        .ok_or_else(|| MapperError::MissingArgument(name.into()))
}
