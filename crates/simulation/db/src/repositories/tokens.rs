//! `tokens` 글로벌 카탈로그 CRUD.
//!
//! 같은 USDC 를 여러 wallet 이 들고 있어도 tokens row 는 1개. `token_hash` 가
//! PK 이므로 [`upsert`] 가 멱등.

use rusqlite::{params, Transaction};

use simulation_state::token::TokenKey;

use crate::codec::{encode_token_key, token_hash};
use crate::error::DbResult;

/// (입력) 새 token 을 등록하거나 cache 만 갱신.
pub fn upsert(
    tx: &Transaction<'_>,
    key: &TokenKey,
    symbol: Option<&str>,
    decimals: Option<u8>,
    first_seen_at: i64,
) -> DbResult<[u8; 16]> {
    let cols = encode_token_key(key);
    tx.execute(
        "INSERT INTO tokens (token_hash, standard, chain, address, contract, token_id, \
         symbol_cache, decimals_cache, first_seen_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
         ON CONFLICT(token_hash) DO UPDATE SET \
           symbol_cache = COALESCE(excluded.symbol_cache, symbol_cache), \
           decimals_cache = COALESCE(excluded.decimals_cache, decimals_cache)",
        params![
            cols.token_hash.to_vec(),
            cols.standard,
            cols.chain,
            cols.address,
            cols.contract,
            cols.token_id,
            symbol,
            decimals.map(i64::from),
            first_seen_at,
        ],
    )?;
    Ok(cols.token_hash)
}

/// Metadata fields for a token — logo/website/description from CoinGecko
/// (or any future registry). All fields optional so partial updates
/// don't clobber existing values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenMetadataUpdate {
    pub logo_url: Option<String>,
    pub website_url: Option<String>,
    pub description: Option<String>,
    pub coingecko_id: Option<String>,
}

impl TokenMetadataUpdate {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.logo_url.is_none()
            && self.website_url.is_none()
            && self.description.is_none()
            && self.coingecko_id.is_none()
    }
}

/// Update metadata columns for a token. Only non-None fields are
/// written; existing values for None fields stay put.
pub fn update_metadata(
    tx: &Transaction<'_>,
    token_hash: [u8; 16],
    md: &TokenMetadataUpdate,
    synced_at: i64,
) -> DbResult<bool> {
    if md.is_empty() {
        return Ok(false);
    }
    let n = tx.execute(
        "UPDATE tokens SET \
           logo_url           = COALESCE(?2, logo_url), \
           website_url        = COALESCE(?3, website_url), \
           description        = COALESCE(?4, description), \
           coingecko_id       = COALESCE(?5, coingecko_id), \
           metadata_synced_at = ?6 \
         WHERE token_hash = ?1",
        params![
            token_hash.to_vec(),
            md.logo_url,
            md.website_url,
            md.description,
            md.coingecko_id,
            synced_at,
        ],
    )?;
    Ok(n > 0)
}

/// hash 로 한 token 의 metadata 가져옴.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenRow {
    pub token_hash: [u8; 16],
    pub key: TokenKey,
    pub symbol: Option<String>,
    pub decimals: Option<u8>,
    pub first_seen_at: i64,
    pub logo_url: Option<String>,
    pub website_url: Option<String>,
    pub description: Option<String>,
    pub coingecko_id: Option<String>,
    pub metadata_synced_at: Option<i64>,
}

pub fn get(tx: &Transaction<'_>, token_hash: [u8; 16]) -> DbResult<Option<TokenRow>> {
    let row = tx
        .prepare(
            "SELECT standard, chain, address, contract, token_id, symbol_cache, \
             decimals_cache, first_seen_at, logo_url, website_url, description, \
             coingecko_id, metadata_synced_at \
             FROM tokens WHERE token_hash = ?1",
        )?
        .query_row(params![token_hash.to_vec()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, Option<String>>(11)?,
                r.get::<_, Option<i64>>(12)?,
            ))
        });
    let (
        standard,
        chain,
        address,
        contract,
        token_id,
        symbol,
        decimals,
        first_seen_at,
        logo_url,
        website_url,
        description,
        coingecko_id,
        metadata_synced_at,
    ) = match row {
        Ok(t) => t,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let cols = crate::codec::TokenColumns {
        token_hash,
        standard: leak_static(&standard)?,
        chain,
        address,
        contract,
        token_id,
    };
    let key = crate::codec::decode_token_key(&cols)?;
    Ok(Some(TokenRow {
        token_hash,
        key,
        symbol,
        decimals: decimals.map(|d| u8::try_from(d).unwrap_or(0)),
        first_seen_at,
        logo_url,
        website_url,
        description,
        coingecko_id,
        metadata_synced_at,
    }))
}

/// `TokenColumns.standard` 가 `&'static str` 라서 DB 에서 읽은 `String` 을 다시
/// 정적 슬라이스 reference 로 변환 필요. 미리 정의된 4개 (`native`/`erc20`/
/// `erc721`/`erc1155`) 중 매칭.
fn leak_static(s: &str) -> DbResult<&'static str> {
    match s {
        "native" => Ok("native"),
        "erc20" => Ok("erc20"),
        "erc721" => Ok("erc721"),
        "erc1155" => Ok("erc1155"),
        other => Err(crate::error::DbError::Invariant(format!(
            "unknown token standard from DB: {other}"
        ))),
    }
}

/// hash 만 결정적이므로 `hash()` 헬퍼 노출 — 테스트 / 외부 호출자가 `token_hash`
/// 미리 계산할 때 사용.
#[must_use]
pub fn hash(key: &TokenKey) -> [u8; 16] {
    token_hash(key)
}

/// Every token row in the user's catalog. Drives `GET /tokens`.
pub fn list_all(tx: &Transaction<'_>) -> DbResult<Vec<TokenRow>> {
    let mut stmt = tx.prepare(
        "SELECT token_hash, standard, chain, address, contract, token_id, symbol_cache, \
         decimals_cache, first_seen_at, logo_url, website_url, description, coingecko_id, \
         metadata_synced_at \
         FROM tokens ORDER BY chain, symbol_cache",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, i64>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, Option<String>>(11)?,
                r.get::<_, Option<String>>(12)?,
                r.get::<_, Option<i64>>(13)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut out = Vec::with_capacity(rows.len());
    for (
        token_hash_vec,
        standard,
        chain,
        address,
        contract,
        token_id,
        symbol,
        decimals,
        first_seen_at,
        logo_url,
        website_url,
        description,
        coingecko_id,
        metadata_synced_at,
    ) in rows
    {
        let mut th = [0u8; 16];
        let n = token_hash_vec.len().min(16);
        th[..n].copy_from_slice(&token_hash_vec[..n]);

        let cols = crate::codec::TokenColumns {
            token_hash: th,
            standard: leak_static(&standard)?,
            chain,
            address,
            contract,
            token_id,
        };
        let key = crate::codec::decode_token_key(&cols)?;
        out.push(TokenRow {
            token_hash: th,
            key,
            symbol,
            decimals: decimals.map(|d| u8::try_from(d).unwrap_or(0)),
            first_seen_at,
            logo_url,
            website_url,
            description,
            coingecko_id,
            metadata_synced_at,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::Pool;
    use alloy_primitives::Address;
    use simulation_state::primitives::ChainId;
    use std::str::FromStr;

    fn fresh_pool() -> Pool {
        let pool = Pool::open_in_memory();
        crate::run_migrations(&pool).unwrap();
        pool
    }

    fn usdc() -> TokenKey {
        TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        }
    }

    #[test]
    fn upsert_then_get() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let h = upsert(tx, &usdc(), Some("USDC"), Some(6), 1_700_000_000).unwrap();
            let r = get(tx, h).unwrap().unwrap();
            assert_eq!(r.key, usdc());
            assert_eq!(r.symbol.as_deref(), Some("USDC"));
            assert_eq!(r.decimals, Some(6));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn upsert_preserves_existing_metadata_when_passed_none() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            upsert(tx, &usdc(), Some("USDC"), Some(6), 1_700_000_000).unwrap();
            // 두 번째 upsert: symbol/decimals 미지정 — 기존 값 유지돼야.
            upsert(tx, &usdc(), None, None, 1_700_000_500).unwrap();
            let r = get(tx, hash(&usdc())).unwrap().unwrap();
            assert_eq!(r.symbol.as_deref(), Some("USDC"));
            assert_eq!(r.decimals, Some(6));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn get_unknown_hash_returns_none() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            let r = get(tx, [0u8; 16]).unwrap();
            assert!(r.is_none());
            Ok(())
        })
        .unwrap();
    }
}
