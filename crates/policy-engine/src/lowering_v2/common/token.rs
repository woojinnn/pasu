//! [`TokenRef`] / [`TokenKey`] lowering (`Core::TokenRef` / `Core::TokenKey`).

use serde_json::{Map, Value};

use simulation_state::token::{TokenKey, TokenRef};

use super::cedar::{addr, u256_hex};

/// Lower a [`TokenRef`] → `{ "key": <TokenKey> }` (`Core::TokenRef`).
pub(crate) fn lower_token_ref(token: &TokenRef) -> Value {
    let mut m = Map::new();
    m.insert("key".into(), lower_token_key(&token.key));
    Value::Object(m)
}

/// Lower a [`TokenKey`] → discriminated `{ standard, chain, address?, contract?,
/// tokenId? }` (`Core::TokenKey`).
pub(crate) fn lower_token_key(key: &TokenKey) -> Value {
    let mut m = Map::new();
    match key {
        TokenKey::Native { chain } => {
            m.insert("standard".into(), Value::String("native".into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
        }
        TokenKey::Erc20 { chain, address } => {
            m.insert("standard".into(), Value::String("erc20".into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("address".into(), Value::String(addr(address)));
        }
        // Erc721 and Erc1155 share the `{ contract, tokenId }` shape and differ
        // only by the `standard` discriminator.
        TokenKey::Erc721 {
            chain,
            contract,
            token_id,
        }
        | TokenKey::Erc1155 {
            chain,
            contract,
            token_id,
        } => {
            let standard = if key.is_nft() { "erc721" } else { "erc1155" };
            m.insert("standard".into(), Value::String(standard.into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("contract".into(), Value::String(addr(contract)));
            m.insert("tokenId".into(), Value::String(u256_hex(*token_id)));
        }
    }
    Value::Object(m)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use simulation_state::primitives::{Address, ChainId, U256};
    use simulation_state::token::TokenKey;

    /// The `TokenKey::Native` / `Erc721` / `Erc1155` arms (the latter two share
    /// one match arm, gated by `is_nft()`) map to the right discriminator and
    /// omit the fields they don't carry.
    #[test]
    fn token_key_native_nft_sft_map_correctly() {
        let chain = ChainId::ethereum_mainnet();
        let contract = Address::from_str("0x2260fac5e5542a773aa44fbcfedf7c193bc2c599").unwrap();

        // Native → standard "native", no address/contract/tokenId.
        let native = lower_token_key(&TokenKey::Native {
            chain: chain.clone(),
        });
        assert_eq!(native["standard"], serde_json::json!("native"));
        assert!(native.get("address").is_none());
        assert!(native.get("tokenId").is_none());

        // Erc721 → standard "erc721" via the is_nft() branch, with tokenId hex.
        let nft = lower_token_key(&TokenKey::Erc721 {
            chain: chain.clone(),
            contract,
            token_id: U256::from(255u64),
        });
        assert_eq!(nft["standard"], serde_json::json!("erc721"));
        assert_eq!(nft["tokenId"], serde_json::json!("0xff"));
        assert!(nft.get("address").is_none());

        // Erc1155 → standard "erc1155" (the other half of the merged arm).
        let sft = lower_token_key(&TokenKey::Erc1155 {
            chain,
            contract,
            token_id: U256::from(1u64),
        });
        assert_eq!(sft["standard"], serde_json::json!("erc1155"));
        assert_eq!(sft["tokenId"], serde_json::json!("0x1"));
    }
}
