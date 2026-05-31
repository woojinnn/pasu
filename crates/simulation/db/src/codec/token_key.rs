//! `TokenKey` enum ↔ SQL `tokens` 테이블의 평탄 컬럼 + 16-byte hash.
//!
//! hash 는 `BLAKE3(canonical_json(TokenKey))[..16]`. 같은 enum value 는 항상
//! 같은 hash 를 갖도록 serde 가 결정적으로 직렬화 한다는 보장이 필요한데, 우리
//! `TokenKey` 의 모든 필드 (`ChainId`/`Address`/`U256`) 는 String 기반 / 순서가
//! 결정적이라 안전.

use std::str::FromStr;

use alloy_primitives::{Address, U256};

use simulation_state::primitives::ChainId;
use simulation_state::token::TokenKey;

use crate::error::{DbError, DbResult};

/// `tokens` 테이블의 한 row 모양 — codec 의 산출.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenColumns {
    pub token_hash: [u8; 16],
    pub standard: &'static str,
    pub chain: String,
    pub address: Option<String>,
    pub contract: Option<String>,
    pub token_id: Option<String>,
}

/// `TokenKey` 의 결정적 16-byte 해시.
#[must_use]
pub fn token_hash(key: &TokenKey) -> [u8; 16] {
    // serde 가 BTreeMap-ish 결정적 — 우리 TokenKey 는 enum + 단순 필드 라 OK.
    let canonical = serde_json::to_string(key).expect("TokenKey serialize");
    let full = blake3::hash(canonical.as_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&full.as_bytes()[..16]);
    out
}

/// `TokenKey` → 평탄 컬럼들.
#[must_use]
pub fn encode_token_key(key: &TokenKey) -> TokenColumns {
    let token_hash = token_hash(key);
    match key {
        TokenKey::Native { chain } => TokenColumns {
            token_hash,
            standard: "native",
            chain: chain.to_string(),
            address: None,
            contract: None,
            token_id: None,
        },
        TokenKey::Erc20 { chain, address } => TokenColumns {
            token_hash,
            standard: "erc20",
            chain: chain.to_string(),
            address: Some(addr_to_string(address)),
            contract: None,
            token_id: None,
        },
        TokenKey::Erc721 {
            chain,
            contract,
            token_id,
        } => TokenColumns {
            token_hash,
            standard: "erc721",
            chain: chain.to_string(),
            address: None,
            contract: Some(addr_to_string(contract)),
            token_id: Some(token_id.to_string()),
        },
        TokenKey::Erc1155 {
            chain,
            contract,
            token_id,
        } => TokenColumns {
            token_hash,
            standard: "erc1155",
            chain: chain.to_string(),
            address: None,
            contract: Some(addr_to_string(contract)),
            token_id: Some(token_id.to_string()),
        },
    }
}

/// 평탄 컬럼들 → `TokenKey`. 잘못된 조합은 [`DbError::Invariant`] 반환.
pub fn decode_token_key(c: &TokenColumns) -> DbResult<TokenKey> {
    let chain = ChainId::from(c.chain.as_str());
    match c.standard {
        "native" => Ok(TokenKey::Native { chain }),
        "erc20" => {
            let addr = parse_addr(c.address.as_deref(), "address (erc20)")?;
            Ok(TokenKey::Erc20 {
                chain,
                address: addr,
            })
        }
        "erc721" => {
            let contract = parse_addr(c.contract.as_deref(), "contract (erc721)")?;
            let token_id = parse_u256(c.token_id.as_deref(), "token_id (erc721)")?;
            Ok(TokenKey::Erc721 {
                chain,
                contract,
                token_id,
            })
        }
        "erc1155" => {
            let contract = parse_addr(c.contract.as_deref(), "contract (erc1155)")?;
            let token_id = parse_u256(c.token_id.as_deref(), "token_id (erc1155)")?;
            Ok(TokenKey::Erc1155 {
                chain,
                contract,
                token_id,
            })
        }
        other => Err(DbError::Invariant(format!(
            "unknown token standard: {other}"
        ))),
    }
}

fn addr_to_string(a: &Address) -> String {
    // 소문자 정규화 — checksum 형태는 UI 가 알아서 표시.
    format!("{a:#x}")
}

fn parse_addr(s: Option<&str>, field: &str) -> DbResult<Address> {
    let raw = s.ok_or_else(|| DbError::Invariant(format!("missing {field}")))?;
    Address::from_str(raw).map_err(|e| DbError::Invariant(format!("bad {field}: {e}")))
}

fn parse_u256(s: Option<&str>, field: &str) -> DbResult<U256> {
    let raw = s.ok_or_else(|| DbError::Invariant(format!("missing {field}")))?;
    U256::from_str_radix(raw, 10).map_err(|e| DbError::Invariant(format!("bad {field}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn usdc_mainnet() -> TokenKey {
        TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        }
    }

    fn weth_mainnet() -> TokenKey {
        TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
        }
    }

    #[test]
    fn hash_is_deterministic() {
        let k = usdc_mainnet();
        assert_eq!(token_hash(&k), token_hash(&k));
    }

    #[test]
    fn hash_differs_for_different_tokens() {
        assert_ne!(token_hash(&usdc_mainnet()), token_hash(&weth_mainnet()));
    }

    #[test]
    fn round_trip_erc20() {
        let original = usdc_mainnet();
        let cols = encode_token_key(&original);
        assert_eq!(cols.standard, "erc20");
        assert!(cols.address.is_some());
        let decoded = decode_token_key(&cols).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn round_trip_native() {
        let original = TokenKey::Native {
            chain: ChainId::ethereum_mainnet(),
        };
        let cols = encode_token_key(&original);
        assert_eq!(cols.standard, "native");
        assert!(cols.address.is_none());
        let decoded = decode_token_key(&cols).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn round_trip_erc721() {
        let original = TokenKey::Erc721 {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d").unwrap(), // BAYC
            token_id: U256::from(1234u64),
        };
        let cols = encode_token_key(&original);
        assert_eq!(cols.standard, "erc721");
        assert_eq!(cols.token_id.as_deref(), Some("1234"));
        let decoded = decode_token_key(&cols).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn unknown_standard_errors() {
        let bad = TokenColumns {
            token_hash: [0u8; 16],
            standard: "wat",
            chain: "eip155:1".into(),
            address: None,
            contract: None,
            token_id: None,
        };
        let err = decode_token_key(&bad).unwrap_err();
        assert!(format!("{err}").contains("unknown token standard"));
    }
}
