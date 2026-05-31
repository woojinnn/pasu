//! `LiveField<Price>` ↔ 평탄 5 컬럼 + source JSON.
//!
//! 하이브리드 전략:
//! * `value` (Price = Decimal string) — TEXT 컬럼
//! * `synced_at` — INTEGER unix sec
//! * `ttl_sec` — INTEGER (`LiveField.ttl.as_secs()`)
//! * `confidence_bp` — INTEGER (`Confidence.deviation_bp`; `is_stale` 은 sync 가 매번
//!   recompute 하므로 영속화 X)
//! * `source` — `DataSource` 전체 JSON
//!
//! → "stale price 검색" 쿼리는 평탄 컬럼 WHERE 절로 가능, 새 `DataSource` variant
//! 추가는 JSON 파싱에 흡수 (migration 0).

use serde_json::Value;

use simulation_state::live_field::{Confidence, DataSource, LiveField};
use simulation_state::primitives::{Duration, Price, Time};

use crate::error::{DbError, DbResult};

/// `LiveField<Price>` 의 SQL 표현 (5 컬럼 + JSON).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveFieldColumns {
    pub value: String,  // Price 의 decimal string
    pub synced_at: i64, // unix sec
    pub ttl_sec: Option<i64>,
    pub confidence_bp: Option<i64>, // Confidence.deviation_bp
    pub source_json: String,        // DataSource JSON
}

pub fn encode_price_live_field(lf: &LiveField<Price>) -> DbResult<LiveFieldColumns> {
    let source_json = serde_json::to_string(&lf.source)?;
    Ok(LiveFieldColumns {
        value: lf.value.as_str().to_string(),
        synced_at: i64::try_from(lf.synced_at.as_unix())
            .map_err(|_| DbError::Invariant("synced_at overflow i64".into()))?,
        ttl_sec: lf
            .ttl
            .as_ref()
            .map(|d| i64::try_from(d.as_secs()))
            .transpose()
            .map_err(|_| DbError::Invariant("ttl_sec overflow i64".into()))?,
        confidence_bp: lf.confidence.as_ref().map(|c| i64::from(c.deviation_bp)),
        source_json,
    })
}

pub fn decode_price_live_field(c: &LiveFieldColumns) -> DbResult<LiveField<Price>> {
    let source: DataSource = serde_json::from_str(&c.source_json)?;
    let synced_at =
        u64::try_from(c.synced_at).map_err(|_| DbError::Invariant("synced_at negative".into()))?;
    let ttl = c
        .ttl_sec
        .map(|s| u64::try_from(s).map(Duration::from_secs))
        .transpose()
        .map_err(|_| DbError::Invariant("ttl_sec negative".into()))?;
    let confidence = c
        .confidence_bp
        .map(|bp| {
            let bp32 = u32::try_from(bp)
                .map_err(|_| DbError::Invariant("confidence_bp out of u32".into()))?;
            Ok::<_, DbError>(Confidence {
                deviation_bp: bp32,
                // is_stale 은 영속화 안 함 — sync 가 ttl + synced_at 기준 매번 재계산.
                is_stale: false,
            })
        })
        .transpose()?;
    Ok(LiveField {
        value: Price::new(c.value.clone()),
        source,
        synced_at: Time::from_unix(synced_at),
        ttl,
        confidence,
    })
}

/// `LiveField` 전체가 NULL 인 경우 (`price_usd` 컬럼이 None) 를 위한 보조.
///
/// SQL 의 5 컬럼이 모두 NULL → `Option<LiveField<Price>>` = None.
/// 하나라도 채워져있으면 → 모두 채워져있어야 함 (invariant).
pub fn decode_optional_price_live_field(
    value: Option<String>,
    synced_at: Option<i64>,
    ttl_sec: Option<i64>,
    confidence_bp: Option<i64>,
    source_json: Option<String>,
) -> DbResult<Option<LiveField<Price>>> {
    match (value, synced_at, source_json) {
        (None, None, None) => Ok(None),
        (Some(v), Some(s), Some(src)) => {
            let cols = LiveFieldColumns {
                value: v,
                synced_at: s,
                ttl_sec,
                confidence_bp,
                source_json: src,
            };
            decode_price_live_field(&cols).map(Some)
        }
        _ => Err(DbError::Invariant(
            "partial LiveField: value/synced_at/source must all be present together".into(),
        )),
    }
}

/// 5-튜플 — value / synced_at / ttl_sec / confidence_bp / source_json.
pub type OptionalLiveFieldCols = (
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

/// `Option<LiveField<Price>>` → 컬럼 값 5쌍 (모두 None 이거나, 모두 채워짐).
pub fn encode_optional_price_live_field(
    lf: Option<&LiveField<Price>>,
) -> DbResult<OptionalLiveFieldCols> {
    match lf {
        None => Ok((None, None, None, None, None)),
        Some(lf) => {
            let c = encode_price_live_field(lf)?;
            Ok((
                Some(c.value),
                Some(c.synced_at),
                c.ttl_sec,
                c.confidence_bp,
                Some(c.source_json),
            ))
        }
    }
}

// JSON serialization helper for DataSource alone (used outside LiveField — e.g.
// primitives_source 컬럼).
pub fn datasource_to_json(s: &DataSource) -> DbResult<String> {
    serde_json::to_string(s).map_err(Into::into)
}

pub fn datasource_from_json(s: &str) -> DbResult<DataSource> {
    serde_json::from_str(s).map_err(Into::into)
}

// (serde_json::Value → DataSource 도 같은 모양 — JSON 컬럼을 직접 받을 때.)
pub fn datasource_from_value(v: Value) -> DbResult<DataSource> {
    serde_json::from_value(v).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::Duration;

    fn sample_lf() -> LiveField<Price> {
        LiveField::new(
            Price::new("0.99955"),
            DataSource::OracleFeed {
                provider: OracleProvider::Chainlink,
                feed_id: "USDC/USD".into(),
            },
            Time::from_unix(1_738_000_000),
        )
        .with_ttl(Duration::from_secs(12))
    }

    #[test]
    fn round_trip_basic() {
        let lf = sample_lf();
        let cols = encode_price_live_field(&lf).unwrap();
        assert_eq!(cols.value, "0.99955");
        assert_eq!(cols.synced_at, 1_738_000_000);
        assert_eq!(cols.ttl_sec, Some(12));
        let back = decode_price_live_field(&cols).unwrap();
        assert_eq!(back, lf);
    }

    #[test]
    fn round_trip_optional_none() {
        let (v, s, t, c, src) = encode_optional_price_live_field(None).unwrap();
        assert!(v.is_none() && s.is_none() && t.is_none() && c.is_none() && src.is_none());
        let back = decode_optional_price_live_field(v, s, t, c, src).unwrap();
        assert!(back.is_none());
    }

    #[test]
    fn round_trip_optional_some() {
        let lf = sample_lf();
        let (v, s, t, c, src) = encode_optional_price_live_field(Some(&lf)).unwrap();
        let back = decode_optional_price_live_field(v, s, t, c, src).unwrap();
        assert_eq!(back, Some(lf));
    }

    #[test]
    fn partial_columns_error() {
        // value 만 있고 synced_at 없음 → invariant 위반.
        let err = decode_optional_price_live_field(
            Some("1.0".into()),
            None,
            None,
            None,
            Some(r#"{"kind":"user_supplied"}"#.into()),
        )
        .unwrap_err();
        assert!(format!("{err}").contains("partial LiveField"));
    }

    #[test]
    fn datasource_json_round_trip() {
        let ds = DataSource::OracleFeed {
            provider: OracleProvider::Chainlink,
            feed_id: "ETH/USD".into(),
        };
        let json = datasource_to_json(&ds).unwrap();
        let back = datasource_from_json(&json).unwrap();
        assert_eq!(back, ds);
    }
}
