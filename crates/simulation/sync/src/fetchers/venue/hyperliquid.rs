//! Hyperliquid REST fetcher — mark price, funding rate, open orders.
//!
//! API: <https://api.hyperliquid.xyz/info>  (POST JSON body)
//!
//! `DataSource::VenueApi { endpoint, parser_id, .. }` 의 `parser_id` 로 메서드 식별:
//! - `hl_all_mids`         → 전체 mark price (key=coin symbol, val=price string)
//! - `hl_funding`          → 각 perp 의 funding rate
//! - `hl_open_orders`      → 한 유저의 미체결 주문 lifecycle 추적
//!
//! body 는 endpoint URL 옆에 별도로 들고와야 하지만, `parser_id` 가 같으면 body 구조도
//! 같다는 가정 하에 간단한 매핑 테이블로 처리.

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal as RustDecimal;
use serde_json::{json, Value};

use simulation_reducer::action::perp::PerpAccountState;
use simulation_state::position::{
    HlAccount, HlAgentApproval, HlLeverageSetting, HlOpenOrder, HlPosition,
};
use simulation_state::{Address, DataSource, Decimal, MarketRef, VenueRef, U256};

use crate::config::HyperliquidConfig;
use crate::error::SyncError;
use crate::walker::{ActionSlot, FieldLocation};

/// Hyperliquid API 기본 endpoint. `scopeball-sync.toml` 의
/// `[venues.hyperliquid]` 가 비어있을 때 fallback 으로 사용.
pub const HL_API_BASE: &str = "https://api.hyperliquid.xyz";

pub struct HyperliquidFetcher {
    client: reqwest::Client,
    /// venue API 의 base URL. `DataSource::VenueApi.endpoint` 가 절대 URL 이면
    /// 그쪽이 우선; relative path 만 들어올 경우를 대비한 base.
    base_url: String,
}

impl Default for HyperliquidFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HyperliquidFetcher {
    /// 기본 endpoint (`HL_API_BASE`) 로 초기화.
    #[must_use]
    pub fn new() -> Self {
        Self::with_base_url(HL_API_BASE.to_string())
    }

    #[must_use]
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client init"),
            base_url,
        }
    }

    /// `scopeball-sync.toml` 의 `[venues.hyperliquid]` 섹션에서 endpoint 주입.
    #[must_use]
    pub fn from_sync_config(cfg: &HyperliquidConfig) -> Self {
        Self::with_base_url(cfg.endpoint.clone())
    }

    /// 현재 설정된 base URL — 호출자가 endpoint 결정 시 참고용.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Effective Hyperliquid `/info` endpoint for this fetcher.
    #[must_use]
    pub fn info_endpoint(&self) -> String {
        self.info_url("")
    }

    pub async fn fetch(&self, source: &DataSource) -> Result<Value, SyncError> {
        let (endpoint, parser_id) = match source {
            DataSource::VenueApi {
                endpoint,
                parser_id,
                ..
            } => (endpoint.clone(), parser_id.clone()),
            _ => {
                return Err(SyncError::FetchFailed {
                    source_id: "hyperliquid".into(),
                    reason: "not a VenueApi source".into(),
                });
            }
        };

        self.fetch_payload_for_parser(&endpoint, &parser_id, None, None)
            .await
    }

    pub async fn fetch_meta(&self, endpoint: &str) -> Result<Value, SyncError> {
        self.fetch_info(endpoint, json!({ "type": "meta" })).await
    }

    pub async fn fetch_clearinghouse_state(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            json!({ "type": "clearinghouseState", "user": hl_user(user) }),
        )
        .await
    }

    pub async fn fetch_open_orders(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            json!({ "type": "frontendOpenOrders", "user": hl_user(user) }),
        )
        .await
    }

    pub async fn fetch_agents(&self, endpoint: &str, user: &Address) -> Result<Value, SyncError> {
        self.fetch_info(
            endpoint,
            json!({ "type": "extraAgents", "user": hl_user(user) }),
        )
        .await
    }

    pub async fn fetch_account_snapshot(
        &self,
        endpoint: &str,
        user: &Address,
    ) -> Result<HlAccount, SyncError> {
        let clearinghouse = self.fetch_clearinghouse_state(endpoint, user).await?;
        let open_orders = self.fetch_open_orders(endpoint, user).await?;
        let agents = self.fetch_agents(endpoint, user).await?;
        let meta = self.fetch_meta(endpoint).await?;
        parse_account_snapshot(&clearinghouse, &open_orders, &agents, &meta)
    }

    pub async fn fetch_action_value(
        &self,
        source: &DataSource,
        slot: &ActionSlot,
        market_symbol: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        let (endpoint, parser_id) = venue_source_parts(source)?;
        let payload = self
            .fetch_payload_for_parser(endpoint, parser_id, Some(user), Some(market_symbol))
            .await?;
        parse_live_input_value(parser_id, slot, market_symbol, &payload)
    }

    pub async fn fetch_state_value(
        &self,
        source: &DataSource,
        location: &FieldLocation,
        market_symbol: &str,
        user: &Address,
    ) -> Result<Value, SyncError> {
        let (endpoint, parser_id) = venue_source_parts(source)?;
        let payload = self
            .fetch_payload_for_parser(endpoint, parser_id, Some(user), Some(market_symbol))
            .await?;
        parse_state_value(parser_id, location, market_symbol, &payload)
    }

    async fn fetch_payload_for_parser(
        &self,
        endpoint: &str,
        parser_id: &str,
        user: Option<&Address>,
        market_symbol: Option<&str>,
    ) -> Result<Value, SyncError> {
        let zero_user = Address::ZERO;
        let user = user.unwrap_or(&zero_user);
        let body = match parser_id {
            "hl_mids" | "hl_all_mids" => json!({ "type": "allMids" }),
            "hl_oracle" | "hl_funding" | "hl_oi" | "hl_market_meta" => {
                json!({ "type": "metaAndAssetCtxs" })
            }
            "hl_open_orders" => {
                json!({ "type": "frontendOpenOrders", "user": hl_user(user) })
            }
            "hl_account" | "hl_clearinghouse" => {
                json!({ "type": "clearinghouseState", "user": hl_user(user) })
            }
            "hl_fees" => json!({ "type": "userFees", "user": hl_user(user) }),
            "hl_l2_book" => {
                let Some(symbol) = market_symbol else {
                    return Err(sync_error("hl_l2_book requires a market symbol"));
                };
                json!({ "type": "l2Book", "coin": hl_coin(symbol) })
            }
            "hl_meta" => json!({ "type": "meta" }),
            "hl_agents" => json!({ "type": "extraAgents", "user": hl_user(user) }),
            other => {
                return Err(SyncError::FetchFailed {
                    source_id: "hyperliquid".into(),
                    reason: format!("unknown parser_id: {other}"),
                });
            }
        };
        self.fetch_info(endpoint, body).await
    }

    async fn fetch_info(&self, endpoint: &str, body: Value) -> Result<Value, SyncError> {
        let url = self.info_url(endpoint);

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: "hyperliquid".into(),
                reason: format!("http: {e}"),
            })?;

        if !resp.status().is_success() {
            return Err(SyncError::FetchFailed {
                source_id: "hyperliquid".into(),
                reason: format!("status {}", resp.status()),
            });
        }

        let value: Value = resp.json().await.map_err(|e| SyncError::FetchFailed {
            source_id: "hyperliquid".into(),
            reason: format!("json: {e}"),
        })?;
        Ok(value)
    }

    fn info_url(&self, endpoint: &str) -> String {
        let raw = if endpoint.is_empty() {
            if self.base_url.is_empty() {
                HL_API_BASE
            } else {
                self.base_url.as_str()
            }
        } else {
            endpoint
        };
        if raw.contains("/info") {
            raw.to_owned()
        } else {
            format!("{}/info", raw.trim_end_matches('/'))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HlAssetMetric {
    MarkPrice,
    OraclePrice,
    Funding,
    OpenInterest,
    MaxLeverage,
    InitialMarginBp,
    MaintenanceMarginBp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FeeSide {
    Maker,
    Taker,
}

pub(crate) fn parse_account_snapshot(
    clearinghouse: &Value,
    open_orders: &Value,
    agents: &Value,
    meta: &Value,
) -> Result<HlAccount, SyncError> {
    let symbols = symbol_index(meta);
    let positions = parse_hl_positions(clearinghouse, &symbols)?;
    let open_orders = parse_hl_open_orders(open_orders, &symbols)?;
    let leverage_settings = parse_hl_leverage_settings(clearinghouse, &symbols)?;
    let agents = parse_hl_agents(agents)?;
    let perp_usdc = value_at(clearinghouse, &["withdrawable"])
        .map(state_decimal_from_value)
        .transpose()?;

    Ok(HlAccount {
        perp_usdc,
        pending_outflow: Decimal::new("0"),
        positions,
        open_orders,
        leverage_settings,
        agents,
    })
}

pub(crate) fn parse_all_mids_value(
    payload: &Value,
    market_symbol: &str,
) -> Result<Value, SyncError> {
    let obj = payload
        .as_object()
        .ok_or_else(|| sync_error("allMids response is not an object"))?;
    for candidate in symbol_candidates(market_symbol) {
        if let Some(v) = obj.get(&candidate) {
            return decimal_string_value(v);
        }
    }
    Err(sync_error(format!(
        "allMids missing market {market_symbol}"
    )))
}

pub(crate) fn parse_asset_ctx_value(
    payload: &Value,
    market_symbol: &str,
    metric: HlAssetMetric,
) -> Result<Value, SyncError> {
    let arr = payload
        .as_array()
        .ok_or_else(|| sync_error("metaAndAssetCtxs response is not an array"))?;
    let meta = arr
        .first()
        .ok_or_else(|| sync_error("metaAndAssetCtxs missing meta"))?;
    let ctxs = arr
        .get(1)
        .and_then(Value::as_array)
        .ok_or_else(|| sync_error("metaAndAssetCtxs missing asset contexts"))?;
    let index = symbol_index(meta)
        .into_iter()
        .find_map(|(sym, ix)| {
            symbol_candidates(market_symbol)
                .into_iter()
                .any(|candidate| candidate == sym)
                .then_some(ix)
        })
        .ok_or_else(|| sync_error(format!("meta missing market {market_symbol}")))?;
    let ctx = ctxs
        .get(usize::try_from(index).map_err(|e| sync_error(format!("asset index: {e}")))?)
        .ok_or_else(|| sync_error(format!("asset context missing index {index}")))?;

    match metric {
        HlAssetMetric::MarkPrice => decimal_string_from_key(ctx, "markPx"),
        HlAssetMetric::OraclePrice => decimal_string_from_key(ctx, "oraclePx"),
        HlAssetMetric::Funding => decimal_string_from_key(ctx, "funding"),
        HlAssetMetric::OpenInterest => decimal_integer_string_from_key(ctx, "openInterest"),
        HlAssetMetric::MaxLeverage => {
            let market = universe_item(meta, market_symbol)?;
            decimal_string_from_key(market, "maxLeverage")
        }
        HlAssetMetric::InitialMarginBp => {
            let market = universe_item(meta, market_symbol)?;
            let max_lev = decimal_from_value(
                value_at(market, &["maxLeverage"])
                    .ok_or_else(|| sync_error("missing maxLeverage"))?,
            )?;
            let bp = (RustDecimal::from(10_000_u32) / max_lev)
                .ceil()
                .to_u64()
                .ok_or_else(|| sync_error("initial margin bp out of range"))?;
            Ok(Value::from(bp))
        }
        HlAssetMetric::MaintenanceMarginBp => {
            let market = universe_item(meta, market_symbol)?;
            let max_lev = decimal_from_value(
                value_at(market, &["maxLeverage"])
                    .ok_or_else(|| sync_error("missing maxLeverage"))?,
            )?;
            let initial = (RustDecimal::from(10_000_u32) / max_lev).ceil();
            let bp = (initial / RustDecimal::from(2_u32))
                .ceil()
                .to_u64()
                .ok_or_else(|| sync_error("maintenance margin bp out of range"))?;
            Ok(Value::from(bp))
        }
    }
}

pub(crate) fn parse_user_fee_bp(payload: &Value, side: FeeSide) -> Result<Value, SyncError> {
    let key = match side {
        FeeSide::Maker => "userAddRate",
        FeeSide::Taker => "userCrossRate",
    };
    let rate = decimal_from_value(
        value_at(payload, &[key]).ok_or_else(|| sync_error(format!("userFees missing {key}")))?,
    )?;
    let bp = (rate * RustDecimal::from(10_000_u32))
        .round()
        .to_u64()
        .ok_or_else(|| sync_error("fee bp out of range"))?;
    Ok(Value::from(bp))
}

pub(crate) fn parse_live_input_value(
    parser_id: &str,
    slot: &ActionSlot,
    market_symbol: &str,
    payload: &Value,
) -> Result<Value, SyncError> {
    match (parser_id, slot) {
        (
            "hl_mids" | "hl_all_mids",
            ActionSlot::PerpOpenMarkPrice
            | ActionSlot::PerpCloseMarkPrice
            | ActionSlot::PerpIncreaseMarkPrice
            | ActionSlot::PerpDecreaseMarkPrice
            | ActionSlot::PerpPlaceLimitMarkPrice
            | ActionSlot::PerpPlaceStopMarkPrice,
        ) => parse_all_mids_value(payload, market_symbol),

        ("hl_oracle", ActionSlot::PerpOpenOraclePrice | ActionSlot::PerpIncreaseOraclePrice) => {
            parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::OraclePrice)
        }

        ("hl_funding", ActionSlot::PerpOpenFundingRate | ActionSlot::PerpIncreaseFundingRate) => {
            parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::Funding)
        }

        ("hl_oi", ActionSlot::PerpOpenAvailableOi | ActionSlot::PerpIncreaseAvailableOi) => {
            parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::OpenInterest)
        }

        (
            "hl_market_meta",
            ActionSlot::PerpOpenMaxLeverage
            | ActionSlot::PerpIncreaseMaxLeverage
            | ActionSlot::PerpChangeLeverageMaxLeverage,
        ) => parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::MaxLeverage),

        (
            "hl_market_meta",
            ActionSlot::PerpOpenInitialMarginBp | ActionSlot::PerpIncreaseInitialMarginBp,
        ) => parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::InitialMarginBp),

        (
            "hl_market_meta",
            ActionSlot::PerpOpenMaintenanceBp | ActionSlot::PerpIncreaseMaintenanceBp,
        ) => parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::MaintenanceMarginBp),

        ("hl_fees", ActionSlot::PerpOpenFeeMakerBp | ActionSlot::PerpIncreaseFeeMakerBp) => {
            parse_user_fee_bp(payload, FeeSide::Maker)
        }

        (
            "hl_fees",
            ActionSlot::PerpOpenFeeTakerBp
            | ActionSlot::PerpIncreaseFeeTakerBp
            | ActionSlot::PerpCloseFeeBp
            | ActionSlot::PerpDecreaseFeeBp,
        ) => parse_user_fee_bp(payload, FeeSide::Taker),

        (
            "hl_account",
            ActionSlot::PerpOpenUserAccountState
            | ActionSlot::PerpIncreaseUserAccountState
            | ActionSlot::PerpPlaceLimitUserAccountState
            | ActionSlot::PerpPlaceStopUserAccountState,
        ) => serde_json::to_value(parse_perp_account_state(payload)?)
            .map_err(|e| sync_error(format!("serialize account state: {e}"))),

        ("hl_open_orders", ActionSlot::PerpPlaceLimitOpenOrdersCount) => payload
            .as_array()
            .map(|orders| Value::from(orders.len() as u64))
            .ok_or_else(|| sync_error("openOrders response is not an array")),

        ("hl_l2_book", ActionSlot::PerpPlaceLimitBestBidAsk) => parse_l2_best_bid_ask(payload),

        _ => Err(sync_error(format!(
            "unsupported Hyperliquid parser/slot: {parser_id}/{slot:?}"
        ))),
    }
}

pub(crate) fn parse_state_value(
    parser_id: &str,
    location: &FieldLocation,
    market_symbol: &str,
    payload: &Value,
) -> Result<Value, SyncError> {
    match (parser_id, location) {
        ("hl_mids" | "hl_all_mids", FieldLocation::PerpMarkPrice { .. }) => {
            parse_all_mids_value(payload, market_symbol)
        }
        (
            "hl_oracle" | "hl_funding" | "hl_oi" | "hl_market_meta",
            FieldLocation::PerpMarkPrice { .. },
        ) => parse_asset_ctx_value(payload, market_symbol, HlAssetMetric::MarkPrice),
        ("hl_account" | "hl_clearinghouse", FieldLocation::PerpLiqPrice { .. }) => {
            parse_clearinghouse_position_value(
                payload,
                market_symbol,
                HlPositionMetric::LiquidationPrice,
            )
        }
        ("hl_account" | "hl_clearinghouse", FieldLocation::PerpUnrealizedPnl { .. }) => {
            parse_clearinghouse_position_value(
                payload,
                market_symbol,
                HlPositionMetric::UnrealizedPnl,
            )
        }
        ("hl_account" | "hl_clearinghouse", FieldLocation::PerpFundingOwed { .. }) => {
            parse_clearinghouse_position_value(
                payload,
                market_symbol,
                HlPositionMetric::FundingOwed,
            )
        }
        ("hl_account" | "hl_clearinghouse", FieldLocation::PerpLeverage { .. }) => {
            parse_clearinghouse_position_value(payload, market_symbol, HlPositionMetric::Leverage)
        }
        _ => Err(sync_error(format!(
            "unsupported Hyperliquid parser/state location: {parser_id}/{location:?}"
        ))),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HlPositionMetric {
    LiquidationPrice,
    UnrealizedPnl,
    FundingOwed,
    Leverage,
}

fn parse_clearinghouse_position_value(
    clearinghouse: &Value,
    market_symbol: &str,
    metric: HlPositionMetric,
) -> Result<Value, SyncError> {
    let position = clearinghouse_position(clearinghouse, market_symbol)?;
    match metric {
        HlPositionMetric::LiquidationPrice => value_at(position, &["liquidationPx"])
            .filter(|v| !v.is_null())
            .map(decimal_string_value)
            .transpose()
            .map(|v| v.unwrap_or(Value::Null)),
        HlPositionMetric::UnrealizedPnl => {
            decimal_integer_string_from_key(position, "unrealizedPnl")
        }
        HlPositionMetric::FundingOwed => decimal_integer_string_from_key(
            value_at(position, &["cumFunding"])
                .ok_or_else(|| sync_error("position missing cumFunding"))?,
            "sinceOpen",
        ),
        HlPositionMetric::Leverage => decimal_string_from_key(
            value_at(position, &["leverage"])
                .ok_or_else(|| sync_error("position missing leverage"))?,
            "value",
        ),
    }
}

fn parse_perp_account_state(clearinghouse: &Value) -> Result<PerpAccountState, SyncError> {
    let total_collateral_usd = decimal_u256_at(clearinghouse, &["marginSummary", "accountValue"])?;
    let used_margin_usd = decimal_u256_at(clearinghouse, &["marginSummary", "totalMarginUsed"])?;
    let free_margin_usd = decimal_u256_at(clearinghouse, &["withdrawable"])?;
    let mut open_positions = Vec::new();

    if let Some(items) = value_at(clearinghouse, &["assetPositions"]).and_then(Value::as_array) {
        for item in items {
            let position = value_at(item, &["position"])
                .ok_or_else(|| sync_error("assetPosition missing position"))?;
            let symbol = string_at(position, &["coin"])?;
            let szi = decimal_from_value(
                value_at(position, &["szi"]).ok_or_else(|| sync_error("position missing szi"))?,
            )?;
            if szi.is_zero() {
                continue;
            }
            let exposure = value_at(position, &["positionValue"])
                .map(decimal_from_value)
                .transpose()?
                .unwrap_or_else(|| szi.abs());
            open_positions.push((
                MarketRef {
                    symbol,
                    venue: VenueRef::new("hyperliquid"),
                },
                decimal_to_u256(exposure)?,
            ));
        }
    }

    Ok(PerpAccountState {
        total_collateral_usd,
        used_margin_usd,
        free_margin_usd,
        open_positions,
    })
}

fn parse_l2_best_bid_ask(payload: &Value) -> Result<Value, SyncError> {
    let levels = value_at(payload, &["levels"])
        .and_then(Value::as_array)
        .ok_or_else(|| sync_error("l2Book response missing levels"))?;
    let bid = levels
        .first()
        .and_then(Value::as_array)
        .and_then(|side| side.first())
        .and_then(|level| value_at(level, &["px"]))
        .ok_or_else(|| sync_error("l2Book missing best bid"))?;
    let ask = levels
        .get(1)
        .and_then(Value::as_array)
        .and_then(|side| side.first())
        .and_then(|level| value_at(level, &["px"]))
        .ok_or_else(|| sync_error("l2Book missing best ask"))?;

    Ok(json!([
        decimal_string_value(bid)?,
        decimal_string_value(ask)?
    ]))
}

fn parse_hl_positions(
    clearinghouse: &Value,
    symbols: &HashMap<String, u32>,
) -> Result<Vec<HlPosition>, SyncError> {
    let mut out = Vec::new();
    let Some(items) = value_at(clearinghouse, &["assetPositions"]).and_then(Value::as_array) else {
        return Ok(out);
    };
    for item in items {
        let position = value_at(item, &["position"])
            .ok_or_else(|| sync_error("assetPosition missing position"))?;
        let symbol = string_at(position, &["coin"])?;
        let szi = decimal_from_value(
            value_at(position, &["szi"]).ok_or_else(|| sync_error("position missing szi"))?,
        )?;
        if szi.is_zero() {
            continue;
        }
        let entry_price = value_at(position, &["entryPx"])
            .filter(|v| !v.is_null())
            .map(state_decimal_from_value)
            .transpose()?
            .unwrap_or_else(|| Decimal::new("0"));
        let asset_index = symbol_to_index(symbols, &symbol)?;
        out.push(HlPosition {
            asset_index,
            symbol: Some(symbol),
            is_long: szi.is_sign_positive(),
            size: Decimal::new(szi.abs().normalize().to_string()),
            entry_price,
        });
    }
    Ok(out)
}

fn parse_hl_open_orders(
    open_orders: &Value,
    symbols: &HashMap<String, u32>,
) -> Result<Vec<HlOpenOrder>, SyncError> {
    let mut out = Vec::new();
    let Some(items) = open_orders.as_array() else {
        return Ok(out);
    };
    for item in items {
        let symbol = string_at(item, &["coin"])?;
        out.push(HlOpenOrder {
            asset_index: symbol_to_index(symbols, &symbol)?,
            symbol: Some(symbol),
            is_buy: parse_side_is_buy(string_at(item, &["side"])?)?,
            price: state_decimal_from_value(
                value_at(item, &["limitPx"]).ok_or_else(|| sync_error("order missing limitPx"))?,
            )?,
            size: state_decimal_from_value(
                value_at(item, &["sz"]).ok_or_else(|| sync_error("order missing sz"))?,
            )?,
            reduce_only: value_at(item, &["reduceOnly"])
                .and_then(Value::as_bool)
                .unwrap_or(false),
            tif: normalize_tif(value_at(item, &["tif"]).and_then(Value::as_str)),
            oid: value_at(item, &["oid"]).and_then(Value::as_u64),
        });
    }
    Ok(out)
}

fn parse_hl_leverage_settings(
    clearinghouse: &Value,
    symbols: &HashMap<String, u32>,
) -> Result<Vec<HlLeverageSetting>, SyncError> {
    let mut out = Vec::new();
    let Some(items) = value_at(clearinghouse, &["assetPositions"]).and_then(Value::as_array) else {
        return Ok(out);
    };
    for item in items {
        let position = value_at(item, &["position"])
            .ok_or_else(|| sync_error("assetPosition missing position"))?;
        let symbol = string_at(position, &["coin"])?;
        let leverage = value_at(position, &["leverage"])
            .ok_or_else(|| sync_error("position missing leverage"))?;
        let value = value_at(leverage, &["value"])
            .and_then(Value::as_u64)
            .ok_or_else(|| sync_error("position leverage value missing"))?;
        out.push(HlLeverageSetting {
            asset_index: symbol_to_index(symbols, &symbol)?,
            is_cross: matches!(
                value_at(leverage, &["type"]).and_then(Value::as_str),
                Some("cross" | "Cross")
            ),
            leverage: u32::try_from(value).map_err(|e| sync_error(format!("leverage: {e}")))?,
        });
    }
    Ok(out)
}

fn parse_hl_agents(agents: &Value) -> Result<Vec<HlAgentApproval>, SyncError> {
    let mut out = Vec::new();
    let Some(items) = agents.as_array() else {
        return Ok(out);
    };
    for item in items {
        let address = string_at(item, &["address"])?;
        out.push(HlAgentApproval {
            agent_address: Address::from_str(&address)
                .map_err(|e| sync_error(format!("agent address {address}: {e}")))?,
            agent_name: value_at(item, &["name"])
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        });
    }
    Ok(out)
}

fn clearinghouse_position<'a>(
    clearinghouse: &'a Value,
    market_symbol: &str,
) -> Result<&'a Value, SyncError> {
    let items = value_at(clearinghouse, &["assetPositions"])
        .and_then(Value::as_array)
        .ok_or_else(|| sync_error("clearinghouseState missing assetPositions"))?;
    let candidates = symbol_candidates(market_symbol);
    items
        .iter()
        .filter_map(|item| value_at(item, &["position"]))
        .find(|position| {
            value_at(position, &["coin"])
                .and_then(Value::as_str)
                .is_some_and(|coin| candidates.iter().any(|candidate| candidate == coin))
        })
        .ok_or_else(|| sync_error(format!("clearinghouseState missing market {market_symbol}")))
}

fn venue_source_parts(source: &DataSource) -> Result<(&str, &str), SyncError> {
    match source {
        DataSource::VenueApi {
            endpoint,
            parser_id,
            ..
        } => Ok((endpoint.as_str(), parser_id.as_str())),
        _ => Err(SyncError::FetchFailed {
            source_id: "hyperliquid".into(),
            reason: "not a VenueApi source".into(),
        }),
    }
}

fn hl_user(user: &Address) -> String {
    format!("{user:#x}")
}

fn hl_coin(market_symbol: &str) -> String {
    symbol_candidates(market_symbol)
        .into_iter()
        .min_by_key(String::len)
        .unwrap_or_else(|| market_symbol.to_owned())
}

fn symbol_index(meta: &Value) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    if let Some(universe) = value_at(meta, &["universe"]).and_then(Value::as_array) {
        for (i, item) in universe.iter().enumerate() {
            if let Some(name) = value_at(item, &["name"]).and_then(Value::as_str) {
                if let Ok(ix) = u32::try_from(i) {
                    out.insert(name.to_owned(), ix);
                }
            }
        }
    }
    out
}

fn universe_item<'a>(meta: &'a Value, market_symbol: &str) -> Result<&'a Value, SyncError> {
    let universe = value_at(meta, &["universe"])
        .and_then(Value::as_array)
        .ok_or_else(|| sync_error("meta missing universe"))?;
    let candidates = symbol_candidates(market_symbol);
    universe
        .iter()
        .find(|item| {
            value_at(item, &["name"])
                .and_then(Value::as_str)
                .is_some_and(|name| candidates.iter().any(|candidate| candidate == name))
        })
        .ok_or_else(|| sync_error(format!("meta missing market {market_symbol}")))
}

fn symbol_to_index(symbols: &HashMap<String, u32>, symbol: &str) -> Result<u32, SyncError> {
    for candidate in symbol_candidates(symbol) {
        if let Some(ix) = symbols.get(&candidate) {
            return Ok(*ix);
        }
    }
    Err(sync_error(format!("unknown Hyperliquid symbol {symbol}")))
}

fn symbol_candidates(symbol: &str) -> Vec<String> {
    let mut out = vec![symbol.to_owned()];
    for suffix in ["-USD", "-USDC", "-PERP", "/USD", "/USDC"] {
        if let Some(stripped) = symbol.strip_suffix(suffix) {
            out.push(stripped.to_owned());
        }
    }
    out.sort();
    out.dedup();
    out
}

fn parse_side_is_buy(side: String) -> Result<bool, SyncError> {
    match side.as_str() {
        "B" | "b" | "bid" | "buy" | "long" | "Long" => Ok(true),
        "A" | "a" | "ask" | "sell" | "short" | "Short" => Ok(false),
        other => Err(sync_error(format!("unknown Hyperliquid side {other}"))),
    }
}

fn normalize_tif(tif: Option<&str>) -> String {
    match tif.unwrap_or("Gtc") {
        "Gtc" | "gtc" => "gtc",
        "Ioc" | "ioc" => "ioc",
        "Alo" | "alo" | "PostOnly" | "post_only" => "post_only",
        "Fok" | "fok" => "fok",
        other => other,
    }
    .to_owned()
}

fn decimal_string_from_key(obj: &Value, key: &str) -> Result<Value, SyncError> {
    decimal_string_value(value_at(obj, &[key]).ok_or_else(|| sync_error(format!("missing {key}")))?)
}

fn decimal_integer_string_from_key(obj: &Value, key: &str) -> Result<Value, SyncError> {
    let d = decimal_from_value(
        value_at(obj, &[key]).ok_or_else(|| sync_error(format!("missing {key}")))?,
    )?;
    Ok(Value::String(d.trunc().normalize().to_string()))
}

fn decimal_string_value(value: &Value) -> Result<Value, SyncError> {
    Ok(Value::String(state_decimal_from_value(value)?.0))
}

fn state_decimal_from_value(value: &Value) -> Result<Decimal, SyncError> {
    let d = decimal_from_value(value)?;
    Ok(Decimal::new(d.normalize().to_string()))
}

fn decimal_u256_at(obj: &Value, path: &[&str]) -> Result<U256, SyncError> {
    let value = value_at(obj, path)
        .ok_or_else(|| sync_error(format!("missing decimal {}", path.join("."))))?;
    decimal_to_u256(decimal_from_value(value)?)
}

fn decimal_to_u256(value: RustDecimal) -> Result<U256, SyncError> {
    if value.is_sign_negative() {
        return Err(sync_error(format!("negative unsigned decimal {value}")));
    }
    let s = value.trunc().normalize().to_string();
    U256::from_str_radix(&s, 10).map_err(|e| sync_error(format!("u256 {s}: {e}")))
}

fn decimal_from_value(value: &Value) -> Result<RustDecimal, SyncError> {
    match value {
        Value::String(s) => {
            RustDecimal::from_str(s).map_err(|e| sync_error(format!("decimal {s:?}: {e}")))
        }
        Value::Number(n) => RustDecimal::from_str(&n.to_string())
            .map_err(|e| sync_error(format!("decimal {n}: {e}"))),
        other => Err(sync_error(format!("expected decimal, got {other}"))),
    }
}

fn string_at(obj: &Value, path: &[&str]) -> Result<String, SyncError> {
    value_at(obj, path)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| sync_error(format!("missing string {}", path.join("."))))
}

fn value_at<'a>(mut value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for key in path {
        value = value.get(*key)?;
    }
    Some(value)
}

fn sync_error(reason: impl Into<String>) -> SyncError {
    SyncError::FetchFailed {
        source_id: "hyperliquid".into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::DataSource;
    use simulation_state::Decimal;

    #[test]
    fn rejects_non_venue_source() {
        let f = HyperliquidFetcher::new();
        let bad = DataSource::UserSupplied;
        let res = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(f.fetch(&bad));
        assert!(res.is_err());
    }

    #[test]
    fn rejects_unknown_parser() {
        let f = HyperliquidFetcher::new();
        let bad = DataSource::VenueApi {
            endpoint: HL_API_BASE.into(),
            parser_id: "made_up".into(),
            auth: None,
        };
        let res = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(f.fetch(&bad));
        let err = format!("{}", res.unwrap_err());
        assert!(err.contains("unknown parser_id"));
    }

    #[test]
    fn parses_account_snapshot_from_hypersdk_shapes() {
        let clearinghouse = json!({
            "marginSummary": {
                "accountValue": "1000.5",
                "totalNtlPos": "6000",
                "totalRawUsd": "1000.5",
                "totalMarginUsed": "1200"
            },
            "crossMarginSummary": {
                "accountValue": "1000.5",
                "totalNtlPos": "6000",
                "totalRawUsd": "1000.5",
                "totalMarginUsed": "1200"
            },
            "crossMaintenanceMarginUsed": "50",
            "withdrawable": "600.5",
            "assetPositions": [{
                "type": "oneWay",
                "position": {
                    "coin": "BTC",
                    "szi": "0.1",
                    "leverage": { "type": "cross", "value": 5 },
                    "entryPx": "60000",
                    "positionValue": "6000",
                    "unrealizedPnl": "12.3",
                    "returnOnEquity": "0.02",
                    "liquidationPx": "50000",
                    "marginUsed": "1200",
                    "maxLeverage": 50,
                    "cumFunding": {
                        "allTime": "-1",
                        "sinceOpen": "-1",
                        "sinceChange": "0"
                    }
                }
            }],
            "time": 1710000000123u64
        });
        let open_orders = json!([{
            "timestamp": 1710000000124u64,
            "coin": "ETH",
            "side": "A",
            "limitPx": "3000",
            "sz": "0.25",
            "oid": 42,
            "origSz": "0.25",
            "cloid": null,
            "orderType": "Limit",
            "tif": "Ioc",
            "reduceOnly": true
        }]);
        let agents = json!([{
            "name": "bot",
            "address": "0x1111111111111111111111111111111111111111",
            "validUntil": 1710000000999u64
        }]);
        let meta = json!({
            "universe": [
                { "name": "BTC", "maxLeverage": 50, "szDecimals": 5 },
                { "name": "ETH", "maxLeverage": 25, "szDecimals": 4 }
            ],
            "collateralToken": 0
        });

        let acct = parse_account_snapshot(&clearinghouse, &open_orders, &agents, &meta).unwrap();

        assert_eq!(acct.perp_usdc, Some(Decimal::new("600.5")));
        assert_eq!(acct.pending_outflow, Decimal::new("0"));
        assert_eq!(acct.positions.len(), 1);
        assert_eq!(acct.positions[0].asset_index, 0);
        assert_eq!(acct.positions[0].symbol.as_deref(), Some("BTC"));
        assert!(acct.positions[0].is_long);
        assert_eq!(acct.positions[0].size, Decimal::new("0.1"));
        assert_eq!(acct.positions[0].entry_price, Decimal::new("60000"));
        assert_eq!(acct.open_orders.len(), 1);
        assert_eq!(acct.open_orders[0].asset_index, 1);
        assert_eq!(acct.open_orders[0].symbol.as_deref(), Some("ETH"));
        assert!(!acct.open_orders[0].is_buy);
        assert_eq!(acct.open_orders[0].tif, "ioc");
        assert_eq!(acct.open_orders[0].oid, Some(42));
        assert_eq!(acct.leverage_settings.len(), 1);
        assert_eq!(acct.leverage_settings[0].asset_index, 0);
        assert!(acct.leverage_settings[0].is_cross);
        assert_eq!(acct.leverage_settings[0].leverage, 5);
        assert_eq!(acct.agents.len(), 1);
        assert_eq!(acct.agents[0].agent_name.as_deref(), Some("bot"));
    }

    #[test]
    fn extracts_hyperliquid_live_values_from_info_payloads() {
        let mids = json!({ "BTC": "60001" });
        assert_eq!(parse_all_mids_value(&mids, "BTC").unwrap(), json!("60001"));

        let meta_and_ctx = json!([
            {
                "universe": [
                    { "name": "BTC", "maxLeverage": 50, "szDecimals": 5 }
                ],
                "collateralToken": 0
            },
            [{
                "funding": "0.0001",
                "openInterest": "12345.7",
                "markPx": "60000",
                "oraclePx": "59990",
                "midPx": "60001",
                "premium": "0",
                "prevDayPx": "59000",
                "dayNtlVlm": "1000000",
                "impactPxs": ["59999", "60002"]
            }]
        ]);
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::MarkPrice).unwrap(),
            json!("60000")
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::OraclePrice).unwrap(),
            json!("59990")
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::Funding).unwrap(),
            json!("0.0001")
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::OpenInterest).unwrap(),
            json!("12345")
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::MaxLeverage).unwrap(),
            json!("50")
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::InitialMarginBp).unwrap(),
            json!(200u64)
        );
        assert_eq!(
            parse_asset_ctx_value(&meta_and_ctx, "BTC", HlAssetMetric::MaintenanceMarginBp)
                .unwrap(),
            json!(100u64)
        );

        let fees = json!({
            "userAddRate": "0.0001",
            "userCrossRate": "0.0005",
            "activeReferralDiscount": "0"
        });
        assert_eq!(
            parse_user_fee_bp(&fees, FeeSide::Maker).unwrap(),
            json!(1u64)
        );
        assert_eq!(
            parse_user_fee_bp(&fees, FeeSide::Taker).unwrap(),
            json!(5u64)
        );
    }

    #[test]
    fn routes_parser_ids_and_slots_to_action_live_values() {
        use crate::walker::ActionSlot;

        let clearinghouse = json!({
            "marginSummary": {
                "accountValue": "1000",
                "totalNtlPos": "6000",
                "totalRawUsd": "1000",
                "totalMarginUsed": "200"
            },
            "crossMarginSummary": {
                "accountValue": "1000",
                "totalNtlPos": "6000",
                "totalRawUsd": "1000",
                "totalMarginUsed": "200"
            },
            "crossMaintenanceMarginUsed": "50",
            "withdrawable": "800",
            "assetPositions": [{
                "type": "oneWay",
                "position": {
                    "coin": "BTC",
                    "szi": "0.1",
                    "leverage": { "type": "cross", "value": 5 },
                    "entryPx": "60000",
                    "positionValue": "6000",
                    "unrealizedPnl": "12.3",
                    "returnOnEquity": "0.02",
                    "liquidationPx": "50000",
                    "marginUsed": "1200",
                    "maxLeverage": 50,
                    "cumFunding": {
                        "allTime": "-1",
                        "sinceOpen": "-1",
                        "sinceChange": "0"
                    }
                }
            }],
            "time": 1710000000123u64
        });
        let account_value = parse_live_input_value(
            "hl_account",
            &ActionSlot::PerpOpenUserAccountState,
            "BTC-USD",
            &clearinghouse,
        )
        .unwrap();
        assert_eq!(account_value["total_collateral_usd"], json!("0x3e8"));
        assert_eq!(account_value["used_margin_usd"], json!("0xc8"));
        assert_eq!(account_value["free_margin_usd"], json!("0x320"));
        assert_eq!(
            account_value["open_positions"][0][0]["symbol"],
            json!("BTC")
        );

        let open_orders = json!([
            { "coin": "BTC", "side": "B", "limitPx": "60000", "sz": "0.1", "oid": 7, "reduceOnly": false },
            { "coin": "ETH", "side": "A", "limitPx": "3000", "sz": "1", "oid": 8, "reduceOnly": false }
        ]);
        assert_eq!(
            parse_live_input_value(
                "hl_open_orders",
                &ActionSlot::PerpPlaceLimitOpenOrdersCount,
                "BTC-USD",
                &open_orders,
            )
            .unwrap(),
            json!(2u64)
        );

        let l2 = json!({
            "coin": "BTC",
            "time": 1710000000123u64,
            "levels": [
                [{ "px": "59999", "sz": "1" }],
                [{ "px": "60002", "sz": "1" }]
            ]
        });
        assert_eq!(
            parse_live_input_value(
                "hl_l2_book",
                &ActionSlot::PerpPlaceLimitBestBidAsk,
                "BTC-USD",
                &l2,
            )
            .unwrap(),
            json!(["59999", "60002"])
        );
    }
}
