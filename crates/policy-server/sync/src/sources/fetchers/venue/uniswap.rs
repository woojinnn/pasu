//! Uniswap venue/API fetcher for AMM swap quotes and `UniswapX` order context.

use std::str::FromStr;
use std::time::Duration;

use rust_decimal::Decimal as RustDecimal;
use serde_json::{json, Value};

use policy_state::{Address, DataSource, TokenKey, TokenRef};
use policy_transition::action::amm::{
    AmmAction, IntentVenue, SignIntentOrderAction, SwapAction, SwapDirection,
};
use policy_transition::action::ActionBody;

use crate::error::SyncError;
use crate::walker::ActionSlot;

/// Fetcher for Uniswap hosted APIs (`api.uniswap.org`).
pub struct UniswapFetcher {
    client: reqwest::Client,
}

impl Default for UniswapFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl UniswapFetcher {
    /// Creates a fetcher with a conservative HTTP timeout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("reqwest client init"),
        }
    }

    /// Fetch and parse an action-level Uniswap venue live input.
    pub async fn fetch_action_value(
        &self,
        source: &DataSource,
        slot: &ActionSlot,
        body: &ActionBody,
        user: &Address,
    ) -> Result<Value, SyncError> {
        let (endpoint, parser_id) = venue_source_parts(source)?;
        let payload = self.fetch_payload(endpoint, parser_id, body, user).await?;
        parse_live_input_value(parser_id, slot, body, &payload)
    }

    async fn fetch_payload(
        &self,
        endpoint: &str,
        parser_id: &str,
        body: &ActionBody,
        user: &Address,
    ) -> Result<Value, SyncError> {
        let req_body = request_body_for_action(parser_id, body, user);
        let response = if let Some(req_body) = req_body {
            self.client.post(endpoint).json(&req_body).send().await
        } else {
            self.client.get(endpoint).send().await
        }
        .map_err(|e| sync_error(format!("http: {e}")))?;

        if !response.status().is_success() {
            return Err(sync_error(format!("status {}", response.status())));
        }
        response
            .json::<Value>()
            .await
            .map_err(|e| sync_error(format!("json: {e}")))
    }
}

fn request_body_for_action(parser_id: &str, body: &ActionBody, user: &Address) -> Option<Value> {
    match (parser_id, body) {
        ("uniswapx_quote", ActionBody::Amm(AmmAction::Swap(s))) => {
            Some(swap_quote_request_body(s, user))
        }
        (
            "uniswapx_quote" | "uniswapx_open_orders",
            ActionBody::Amm(AmmAction::SignIntentOrder(s)),
        ) => Some(intent_order_request_body(s, user)),
        _ => None,
    }
}

fn swap_quote_request_body(action: &SwapAction, user: &Address) -> Value {
    let amount = match &action.params.direction {
        SwapDirection::ExactInput { amount_in, .. } => amount_in,
        SwapDirection::ExactOutput { amount_out, .. } => amount_out,
    };
    let quote_type = match action.params.direction {
        SwapDirection::ExactInput { .. } => "EXACT_INPUT",
        SwapDirection::ExactOutput { .. } => "EXACT_OUTPUT",
    };
    json!({
        "type": quote_type,
        "swapper": format!("{user:#x}"),
        "tokenIn": token_address_or_native(&action.params.token_in),
        "tokenOut": action
            .params
            .token_out
            .as_ref()
            .map_or(Value::Null, token_address_or_native),
        "tokenInChainId": eip155_chain_id(action.params.token_in.key.chain().as_str()),
        "tokenOutChainId": action
            .params
            .token_out
            .as_ref()
            .and_then(|t| eip155_chain_id(t.key.chain().as_str())),
        "amount": amount.to_string(),
        "slippageTolerance": action.params.slippage_bp,
    })
}

fn intent_order_request_body(action: &SignIntentOrderAction, user: &Address) -> Value {
    json!({
        "swapper": format!("{user:#x}"),
        "chainId": intent_chain_id(&action.venue),
        "sellToken": token_address_or_native(&action.sell),
        "buyToken": token_address_or_native(&action.buy),
        "sellAmount": action.sell_amount.to_string(),
        "buyMin": action.buy_min.to_string(),
        "recipient": format!("{:#x}", action.recipient),
        "validUntil": action.valid_until.as_unix(),
    })
}

fn intent_chain_id(venue: &IntentVenue) -> Option<u64> {
    match venue {
        IntentVenue::UniswapX { chain, .. } | IntentVenue::CowSwap { chain, .. } => {
            eip155_chain_id(chain.as_str())
        }
        IntentVenue::OneInchFusion { chain } | IntentVenue::Bebop { chain } => {
            eip155_chain_id(chain.as_str())
        }
        // 1inch LOP v4 is not served by the Uniswap quote builder; only its
        // chain id is meaningful here.
        IntentVenue::OneInchLimitOrder { chain, .. } => eip155_chain_id(chain.as_str()),
    }
}

fn eip155_chain_id(chain: &str) -> Option<u64> {
    chain.strip_prefix("eip155:")?.parse().ok()
}

fn token_address_or_native(token: &TokenRef) -> Value {
    match &token.key {
        TokenKey::Native { .. } => Value::String("native".into()),
        TokenKey::Erc20 { address, .. } => Value::String(format!("{address:#x}")),
        TokenKey::Erc721 { contract, .. } | TokenKey::Erc1155 { contract, .. } => {
            Value::String(format!("{contract:#x}"))
        }
    }
}

pub(crate) fn parse_live_input_value(
    parser_id: &str,
    slot: &ActionSlot,
    body: &ActionBody,
    payload: &Value,
) -> Result<Value, SyncError> {
    match (parser_id, slot) {
        ("uniswapx_quote", ActionSlot::AmmSwapExpectedAmountOut) => {
            parse_expected_amount_out(payload)
        }
        ("uniswapx_quote", ActionSlot::AmmSignIntentExpectedFillPrice) => {
            parse_expected_fill_price(body, payload)
        }
        ("uniswapx_open_orders", ActionSlot::AmmSignIntentCompetingOrders) => {
            parse_open_orders_count(payload)
        }
        _ => Err(sync_error(format!(
            "unsupported Uniswap parser/slot: {parser_id}/{slot:?}"
        ))),
    }
}

fn parse_expected_amount_out(payload: &Value) -> Result<Value, SyncError> {
    let value = first_path(
        payload,
        &[
            &["quote", "amountOut"],
            &["quote", "output", "amount"],
            &["quote", "outputAmount"],
            &["amountOut"],
            &["output", "amount"],
            &["outputAmount"],
            &["buyAmount"],
        ],
    )
    .ok_or_else(|| sync_error("quote response missing output amount"))?;
    integer_string_value(value)
}

fn parse_expected_fill_price(body: &ActionBody, payload: &Value) -> Result<Value, SyncError> {
    if let Some(value) = first_path(
        payload,
        &[
            &["expectedFillPrice"],
            &["price"],
            &["executionPrice"],
            &["quote", "expectedFillPrice"],
            &["quote", "price"],
            &["quote", "executionPrice"],
        ],
    ) {
        return decimal_string_value(value);
    }

    let buy = first_path(
        payload,
        &[
            &["buyAmount"],
            &["amountOut"],
            &["outputAmount"],
            &["quote", "buyAmount"],
            &["quote", "amountOut"],
        ],
    )
    .map(decimal_from_value)
    .transpose()?;
    let sell = first_path(
        payload,
        &[
            &["sellAmount"],
            &["amountIn"],
            &["inputAmount"],
            &["quote", "sellAmount"],
            &["quote", "amountIn"],
        ],
    )
    .map(decimal_from_value)
    .transpose()?;
    let (Some(buy), Some(sell)) = (buy, sell) else {
        return expected_fill_price_from_action(body);
    };
    if sell.is_zero() {
        return Err(sync_error("quote sell amount is zero"));
    }
    Ok(Value::String((buy / sell).normalize().to_string()))
}

fn expected_fill_price_from_action(body: &ActionBody) -> Result<Value, SyncError> {
    let ActionBody::Amm(AmmAction::SignIntentOrder(action)) = body else {
        return Err(sync_error("expected SignIntentOrder action body"));
    };
    if action.sell_amount.is_zero() {
        return Err(sync_error("action sell amount is zero"));
    }
    let buy = RustDecimal::from_str(&action.buy_min.to_string())
        .map_err(|e| sync_error(format!("buy_min decimal: {e}")))?;
    let sell = RustDecimal::from_str(&action.sell_amount.to_string())
        .map_err(|e| sync_error(format!("sell_amount decimal: {e}")))?;
    Ok(Value::String((buy / sell).normalize().to_string()))
}

fn parse_open_orders_count(payload: &Value) -> Result<Value, SyncError> {
    let orders = payload
        .as_array()
        .or_else(|| value_at(payload, &["orders"]).and_then(Value::as_array))
        .or_else(|| value_at(payload, &["data", "orders"]).and_then(Value::as_array))
        .ok_or_else(|| sync_error("open orders response is not an array"))?;
    Ok(Value::from(orders.len() as u64))
}

fn first_path<'a>(payload: &'a Value, paths: &[&[&str]]) -> Option<&'a Value> {
    paths.iter().find_map(|path| value_at(payload, path))
}

fn value_at<'a>(mut value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for key in path {
        value = value.get(*key)?;
    }
    Some(value)
}

fn integer_string_value(value: &Value) -> Result<Value, SyncError> {
    let value = decimal_from_value(value)?;
    if value.is_sign_negative() {
        return Err(sync_error(format!("negative integer amount {value}")));
    }
    Ok(Value::String(value.trunc().normalize().to_string()))
}

fn decimal_string_value(value: &Value) -> Result<Value, SyncError> {
    Ok(Value::String(
        decimal_from_value(value)?.normalize().to_string(),
    ))
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

fn venue_source_parts(source: &DataSource) -> Result<(&str, &str), SyncError> {
    match source {
        DataSource::VenueApi {
            endpoint,
            parser_id,
            ..
        } => Ok((endpoint.as_str(), parser_id.as_str())),
        _ => Err(sync_error("not a VenueApi source")),
    }
}

fn sync_error(reason: impl Into<String>) -> SyncError {
    SyncError::FetchFailed {
        source_id: "uniswap".into(),
        reason: reason.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::{ChainId, DataSource, Decimal, LiveField, Time, U256};
    use policy_transition::action::amm::{IntentOrderKind, SignIntentOrderLiveInputs};

    fn token(address: &str) -> TokenRef {
        TokenRef::new(TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str(address).unwrap(),
        })
    }

    fn intent_body() -> ActionBody {
        ActionBody::Amm(AmmAction::SignIntentOrder(SignIntentOrderAction {
            venue: IntentVenue::UniswapX {
                chain: ChainId::ethereum_mainnet(),
                reactor: Address::from_str("0x6000da47483062a0d734ba3dc7576ce6a0b645c4").unwrap(),
            },
            sell: token("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
            buy: token("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
            sell_amount: U256::from(1_000_000u64),
            buy_min: U256::from(500_000_000_000_000_000u64),
            order_kind: IntentOrderKind::Dutch,
            recipient: Address::ZERO,
            valid_until: Time::from_unix(1_800_000_000),
            live_inputs: SignIntentOrderLiveInputs {
                expected_fill_price: LiveField::new(
                    Decimal::new("0"),
                    DataSource::UserSupplied,
                    Time::from_unix(0),
                ),
                competing_orders: LiveField::new(0, DataSource::UserSupplied, Time::from_unix(0)),
            },
        }))
    }

    #[test]
    fn parses_swap_quote_amount_out() {
        let value = parse_live_input_value(
            "uniswapx_quote",
            &ActionSlot::AmmSwapExpectedAmountOut,
            &intent_body(),
            &json!({ "quote": { "output": { "amount": "12345" } } }),
        )
        .unwrap();
        assert_eq!(value, Value::String("12345".into()));
    }

    #[test]
    fn parses_intent_quote_fill_price_from_payload() {
        let value = parse_live_input_value(
            "uniswapx_quote",
            &ActionSlot::AmmSignIntentExpectedFillPrice,
            &intent_body(),
            &json!({ "quote": { "price": "2500.1250" } }),
        )
        .unwrap();
        assert_eq!(value, Value::String("2500.125".into()));
    }

    #[test]
    fn parses_open_order_count() {
        let value = parse_live_input_value(
            "uniswapx_open_orders",
            &ActionSlot::AmmSignIntentCompetingOrders,
            &intent_body(),
            &json!({ "orders": [{ "hash": "0x1" }, { "hash": "0x2" }] }),
        )
        .unwrap();
        assert_eq!(value, Value::from(2_u64));
    }
}
