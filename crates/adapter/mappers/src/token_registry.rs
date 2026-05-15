//! Token metadata lookup. Used by Mappers to fill host:registry fields.

pub trait TokenRegistry: Send + Sync {
    fn lookup(
        &self,
        chain_id: u64,
        address: &policy_engine::action::Address,
    ) -> Option<TokenMetadata>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenMetadata {
    pub symbol: String,
    pub decimals: u8,
}

pub struct EmptyTokenRegistry;

impl TokenRegistry for EmptyTokenRegistry {
    fn lookup(
        &self,
        _chain_id: u64,
        _address: &policy_engine::action::Address,
    ) -> Option<TokenMetadata> {
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
        let address =
            policy_engine::action::Address::from_str("0x1111111111111111111111111111111111111111")
                .unwrap();

        assert_eq!(registry.lookup(1, &address), None);
    }
}
