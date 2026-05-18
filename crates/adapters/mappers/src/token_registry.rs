//! Token metadata lookup. Used by Mappers to fill host:registry fields and
//! to resolve canonical wrapped-native addresses on each chain.

use policy_engine::action::Address;

pub trait TokenRegistry: Send + Sync {
    fn lookup(&self, chain_id: u64, address: &Address) -> Option<TokenMetadata>;

    /// Canonical wrapped-native address for `chain_id` (WETH on Ethereum/L2s,
    /// WMATIC on Polygon, …).
    ///
    /// Used by the Universal Router mappers to construct the WRAP_ETH /
    /// UNWRAP_WETH envelope's `wrapped_asset.address`. The compactor's ledger
    /// keys ERC-20 buckets by address, so a missing address here causes
    /// WRAP+SWAP envelopes to land in different buckets and skip the
    /// collapse pass.
    ///
    /// Default impl falls back to a small static table covering the chains
    /// our curated Sourcify bundle ships for (Ethereum mainnet + canonical
    /// L2s). Returns `None` on chains we don't have a mapping for — those
    /// requests will surface uncollapsed (WRAP + SWAP) envelopes; the
    /// underlying simulation is still correct, just verbose. Override this
    /// in a custom registry to add chain coverage without a code change.
    fn wrapped_native(&self, chain_id: u64) -> Option<Address> {
        default_wrapped_native(chain_id)
    }
}

/// Static `(chain_id → wrapped-native address)` table. Pulled out so callers
/// (e.g. the `EmptyTokenRegistry` used in tests) can reuse the exact same
/// lookup the default trait method does.
#[must_use]
pub fn default_wrapped_native(chain_id: u64) -> Option<Address> {
    let hex = match chain_id {
        // Ethereum mainnet — WETH9
        1 => "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        // Optimism — WETH
        10 => "0x4200000000000000000000000000000000000006",
        // Base — WETH
        8453 => "0x4200000000000000000000000000000000000006",
        // Arbitrum One — WETH
        42161 => "0x82af49447d8a07e3bd95bd0d56f35241523fbab1",
        // Polygon PoS — WMATIC (the chain's wrapped-native, even though
        // technically it's not "WETH" — Universal Router treats them the
        // same role).
        137 => "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619",
        _ => return None,
    };
    hex.parse().ok()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenMetadata {
    pub symbol: String,
    pub decimals: u8,
}

pub struct EmptyTokenRegistry;

impl TokenRegistry for EmptyTokenRegistry {
    fn lookup(&self, _chain_id: u64, _address: &Address) -> Option<TokenMetadata> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_empty_token_registry_returns_none() {
        let registry = EmptyTokenRegistry;
        let address = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();

        assert_eq!(registry.lookup(1, &address), None);
    }

    #[test]
    fn test_default_wrapped_native_known_chains() {
        let weth_mainnet = Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap();
        assert_eq!(default_wrapped_native(1), Some(weth_mainnet));
        let weth_l2 = Address::from_str("0x4200000000000000000000000000000000000006").unwrap();
        assert_eq!(default_wrapped_native(10), Some(weth_l2.clone()));
        assert_eq!(default_wrapped_native(8453), Some(weth_l2));
    }

    #[test]
    fn test_default_wrapped_native_unknown_chain_returns_none() {
        // Avalanche (43114) isn't in our table.
        assert_eq!(default_wrapped_native(43114), None);
    }

    #[test]
    fn test_empty_registry_uses_default_wrapped_native() {
        let registry = EmptyTokenRegistry;
        assert!(registry.wrapped_native(1).is_some());
        assert!(registry.wrapped_native(43114).is_none());
    }
}
