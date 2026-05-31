//! `token_holdings` CRUD.

use alloy_primitives::Address;
use rusqlite::{params, Transaction};

use simulation_state::token::{TokenHolding, TokenKey};

use crate::codec::live_field::{
    datasource_from_json, datasource_to_json, decode_optional_price_live_field,
    encode_optional_price_live_field,
};
use crate::codec::{
    decode_balance, decode_token_key, encode_balance, encode_token_key, token_hash, BalanceColumns,
    TokenColumns,
};
use crate::error::{DbError, DbResult};

use simulation_state::token::TokenKind;

/// (`wallet_id`, `token_key`) 의 holding 을 INSERT or REPLACE.
///
/// 호출 전에 `tokens.upsert` 로 token catalog 가 보장되어야 함 (FK).
pub fn upsert(tx: &Transaction<'_>, wallet_id: i64, holding: &TokenHolding) -> DbResult<()> {
    let token_hash = token_hash(&holding.key);
    let balance = encode_balance(&holding.balance);
    let committed = encode_balance(&holding.committed);
    let (pv, ps, pt, pc, psrc) = encode_optional_price_live_field(holding.price_usd.as_ref())?;
    let primitives_src = datasource_to_json(&holding.primitives_source)?;
    let approved_to = holding.approved_to.as_ref().map(|a| format!("{a:#x}"));
    let last_synced_at = i64::try_from(holding.last_synced_at.as_unix())
        .map_err(|_| DbError::Invariant("last_synced_at overflow".into()))?;

    tx.execute(
        "INSERT INTO token_holdings ( \
            wallet_id, token_hash, \
            balance_form, balance_amount, \
            committed_form, committed_amount, \
            approved_to, \
            price_value, price_synced_at, price_ttl_sec, price_confidence_bp, price_source_json, \
            last_synced_at, primitives_source_json \
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14) \
         ON CONFLICT(wallet_id, token_hash) DO UPDATE SET \
            balance_form = excluded.balance_form, \
            balance_amount = excluded.balance_amount, \
            committed_form = excluded.committed_form, \
            committed_amount = excluded.committed_amount, \
            approved_to = excluded.approved_to, \
            price_value = excluded.price_value, \
            price_synced_at = excluded.price_synced_at, \
            price_ttl_sec = excluded.price_ttl_sec, \
            price_confidence_bp = excluded.price_confidence_bp, \
            price_source_json = excluded.price_source_json, \
            last_synced_at = excluded.last_synced_at, \
            primitives_source_json = excluded.primitives_source_json",
        params![
            wallet_id,
            token_hash.to_vec(),
            balance.form,
            balance.amount,
            committed.form,
            committed.amount,
            approved_to,
            pv,
            ps,
            pt,
            pc,
            psrc,
            last_synced_at,
            primitives_src,
        ],
    )?;
    Ok(())
}

/// 한 wallet 의 모든 holding 들. `TokenKind` 는 DB 에 저장되지 않아 호출자가
/// 별도 카탈로그에서 보충하거나 placeholder 로 채워야 함. Phase 1 은 단순화 —
/// `TokenKind::default()` (없으면 fallback) 대신 caller 가 hydrate.
///
/// 반환은 (`token_hash`, `symbol_cache`, `decimals_cache`, `balance_cols`, `committed_cols`,
/// `approved_to`, `price_cols`, `last_synced_at`, `primitives_source`, metadata 컬럼들).
#[allow(clippy::type_complexity)]
pub fn raw_list_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<Vec<HoldingRowRaw>> {
    let mut stmt = tx.prepare(
        "SELECT h.token_hash, t.standard, t.chain, t.address, t.contract, t.token_id, \
                t.symbol_cache, t.decimals_cache, \
                h.balance_form, h.balance_amount, \
                h.committed_form, h.committed_amount, \
                h.approved_to, \
                h.price_value, h.price_synced_at, h.price_ttl_sec, h.price_confidence_bp, h.price_source_json, \
                h.last_synced_at, h.primitives_source_json, \
                t.logo_url, t.website_url, t.description, t.coingecko_id \
         FROM token_holdings h \
         JOIN tokens t ON h.token_hash = t.token_hash \
         WHERE h.wallet_id = ?1 \
         ORDER BY t.standard, t.chain, t.address",
    )?;
    let rows = stmt
        .query_map(params![wallet_id], |r| {
            Ok(HoldingRowRaw {
                token_hash: vec_to_hash(&r.get::<_, Vec<u8>>(0)?),
                standard: r.get::<_, String>(1)?,
                chain: r.get::<_, String>(2)?,
                address: r.get::<_, Option<String>>(3)?,
                contract: r.get::<_, Option<String>>(4)?,
                token_id: r.get::<_, Option<String>>(5)?,
                symbol_cache: r.get::<_, Option<String>>(6)?,
                decimals_cache: r.get::<_, Option<i64>>(7)?,
                balance_form: r.get::<_, String>(8)?,
                balance_amount: r.get::<_, Option<String>>(9)?,
                committed_form: r.get::<_, String>(10)?,
                committed_amount: r.get::<_, Option<String>>(11)?,
                approved_to: r.get::<_, Option<String>>(12)?,
                price_value: r.get::<_, Option<String>>(13)?,
                price_synced_at: r.get::<_, Option<i64>>(14)?,
                price_ttl_sec: r.get::<_, Option<i64>>(15)?,
                price_confidence_bp: r.get::<_, Option<i64>>(16)?,
                price_source_json: r.get::<_, Option<String>>(17)?,
                last_synced_at: r.get::<_, i64>(18)?,
                primitives_source_json: r.get::<_, String>(19)?,
                logo_url: r.get::<_, Option<String>>(20)?,
                website_url: r.get::<_, Option<String>>(21)?,
                description: r.get::<_, Option<String>>(22)?,
                coingecko_id: r.get::<_, Option<String>>(23)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn vec_to_hash(v: &[u8]) -> [u8; 16] {
    let mut h = [0u8; 16];
    let n = v.len().min(16);
    h[..n].copy_from_slice(&v[..n]);
    h
}

/// raw row 를 `TokenHolding` 으로 재조립. `TokenKind` 는 placeholder
/// (`Unknown` family 같은 게 있다면 그걸로, 없으면 호출자가 별도 갱신).
#[derive(Clone, Debug)]
pub struct HoldingRowRaw {
    pub token_hash: [u8; 16],
    pub standard: String,
    pub chain: String,
    pub address: Option<String>,
    pub contract: Option<String>,
    pub token_id: Option<String>,
    pub symbol_cache: Option<String>,
    pub decimals_cache: Option<i64>,
    pub balance_form: String,
    pub balance_amount: Option<String>,
    pub committed_form: String,
    pub committed_amount: Option<String>,
    pub approved_to: Option<String>,
    pub price_value: Option<String>,
    pub price_synced_at: Option<i64>,
    pub price_ttl_sec: Option<i64>,
    pub price_confidence_bp: Option<i64>,
    pub price_source_json: Option<String>,
    pub last_synced_at: i64,
    pub primitives_source_json: String,
    pub logo_url: Option<String>,
    pub website_url: Option<String>,
    pub description: Option<String>,
    pub coingecko_id: Option<String>,
}

impl HoldingRowRaw {
    /// `TokenHolding` 으로 변환. `kind` 는 호출자가 결정해서 주입 (DB 에 보관 X
    /// — 정책 / sync 가 별도 카탈로그에서).
    pub fn into_holding(self, kind: TokenKind) -> DbResult<TokenHolding> {
        let token_cols = TokenColumns {
            token_hash: self.token_hash,
            standard: match self.standard.as_str() {
                "native" => "native",
                "erc20" => "erc20",
                "erc721" => "erc721",
                "erc1155" => "erc1155",
                other => {
                    return Err(DbError::Invariant(format!(
                        "unknown standard from DB: {other}"
                    )));
                }
            },
            chain: self.chain,
            address: self.address,
            contract: self.contract,
            token_id: self.token_id,
        };
        let key = decode_token_key(&token_cols)?;

        let balance = decode_balance(&BalanceColumns {
            form: match self.balance_form.as_str() {
                "fungible" => "fungible",
                "owned" => "owned",
                other => return Err(DbError::Invariant(format!("balance form: {other}"))),
            },
            amount: self.balance_amount,
        })?;
        let committed = decode_balance(&BalanceColumns {
            form: match self.committed_form.as_str() {
                "fungible" => "fungible",
                "owned" => "owned",
                other => return Err(DbError::Invariant(format!("committed form: {other}"))),
            },
            amount: self.committed_amount,
        })?;

        let price_usd = decode_optional_price_live_field(
            self.price_value,
            self.price_synced_at,
            self.price_ttl_sec,
            self.price_confidence_bp,
            self.price_source_json,
        )?;

        let primitives_source = datasource_from_json(&self.primitives_source_json)?;
        let approved_to = self
            .approved_to
            .map(|s| {
                Address::parse_checksummed(&s, None)
                    .or_else(|_| s.parse::<Address>())
                    .map_err(|e| DbError::Invariant(format!("approved_to: {e}")))
            })
            .transpose()?;
        let last_synced_at = u64::try_from(self.last_synced_at)
            .map_err(|_| DbError::Invariant("last_synced_at negative".into()))?;

        let metadata = {
            let md = simulation_state::token::TokenMetadata {
                logo_url: self.logo_url,
                website_url: self.website_url,
                description: self.description,
                coingecko_id: self.coingecko_id,
            };
            if md.is_empty() {
                None
            } else {
                Some(md)
            }
        };

        Ok(TokenHolding {
            key,
            kind,
            symbol: self.symbol_cache.unwrap_or_default(),
            decimals: self
                .decimals_cache
                .map_or(0, |d| u8::try_from(d).unwrap_or(0)),
            balance,
            committed,
            approved_to,
            price_usd,
            metadata,
            value_usd: None,
            last_synced_at: simulation_state::primitives::Time::from_unix(last_synced_at),
            primitives_source,
        })
    }
}

/// 한 (`wallet_id`, `token_hash`) 삭제.
pub fn delete(tx: &Transaction<'_>, wallet_id: i64, token_key: &TokenKey) -> DbResult<bool> {
    let th = token_hash(token_key);
    let _ = encode_token_key(token_key); // silence unused
    let n = tx.execute(
        "DELETE FROM token_holdings WHERE wallet_id = ?1 AND token_hash = ?2",
        params![wallet_id, th.to_vec()],
    )?;
    Ok(n > 0)
}

/// Remove all holdings for a wallet before writing a fresh snapshot.
pub fn delete_for_wallet(tx: &Transaction<'_>, wallet_id: i64) -> DbResult<usize> {
    let n = tx.execute(
        "DELETE FROM token_holdings WHERE wallet_id = ?1",
        params![wallet_id],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::Pool;
    use crate::repositories::tokens;
    use crate::repositories::wallets::{insert as insert_wallet, WalletInsert};
    use alloy_primitives::U256;
    use simulation_state::live_field::{DataSource, LiveField, OracleProvider};
    use simulation_state::primitives::{ChainId, Duration, Price, Time};
    use simulation_state::token::{Balance, BaseCategory, FiatCurrency, PegTarget, TokenKind};
    use std::str::FromStr;

    fn fresh_pool() -> Pool {
        let pool = Pool::open_in_memory();
        crate::run_migrations(&pool).unwrap();
        pool
    }

    fn usdc_key() -> TokenKey {
        TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        }
    }

    fn sample_holding(key: TokenKey) -> TokenHolding {
        TokenHolding {
            key,
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::Fungible {
                amount: U256::from(2_500_000_000u64),
            },
            committed: Balance::Fungible { amount: U256::ZERO },
            approved_to: None,
            price_usd: Some(
                LiveField::new(
                    Price::new("0.99955"),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Chainlink,
                        feed_id: "USDC/USD".into(),
                    },
                    Time::from_unix(1_738_000_000),
                )
                .with_ttl(Duration::from_secs(12)),
            ),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(1_738_000_000),
            primitives_source: DataSource::UserSupplied,
        }
    }

    #[test]
    fn upsert_and_load_round_trip() {
        let pool = fresh_pool();
        pool.with_tx(|tx| {
            // 사전 셋업
            let wallet_id = insert_wallet(
                tx,
                &WalletInsert {
                    address: "0xowner".into(),
                    label: None,
                    is_owned: true,
                    created_at: 1_700_000_000,
                    chains: vec![ChainId::ethereum_mainnet()],
                },
            )?;
            tokens::upsert(tx, &usdc_key(), Some("USDC"), Some(6), 1_700_000_000)?;

            let original = sample_holding(usdc_key());
            upsert(tx, wallet_id, &original)?;

            let rows = raw_list_for_wallet(tx, wallet_id)?;
            assert_eq!(rows.len(), 1);
            let row = rows.into_iter().next().unwrap();
            let restored = row.into_holding(original.kind.clone())?;
            assert_eq!(restored, original);
            Ok(())
        })
        .unwrap();
    }
}
