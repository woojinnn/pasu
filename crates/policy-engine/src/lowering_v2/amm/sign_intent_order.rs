//! `Amm::SignIntentOrder` lowering → `Amm::SignIntentOrderContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::amm::{IntentOrderKind, SignIntentOrderAction};

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_intent_venue;

/// Lower an `Amm::SignIntentOrder` action into the
/// `Amm::SignIntentOrderContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &SignIntentOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_intent_venue(&action.venue));
    m.insert("sell".into(), lower_token_ref(&action.sell));
    m.insert("buy".into(), lower_token_ref(&action.buy));
    m.insert("sellAmount".into(), Value::String(u256_hex(action.sell_amount)));
    // `sellAmountNano` / `sellAmountUsd` are host-populated 3-layer siblings —
    // always omitted here.
    m.insert("buyMin".into(), Value::String(u256_hex(action.buy_min)));
    // `buyMinNano` / `buyMinUsd` are host-populated — always omitted.
    m.insert(
        "orderKind".into(),
        Value::String(intent_order_kind(&action.order_kind).into()),
    );
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    m.insert("validUntil".into(), Value::from(action.valid_until.as_unix()));
    // `expected_fill_price` is LiveField<Price>; Price is a decimal-string.
    m.insert(
        "expectedFillPrice".into(),
        Value::String(action.live_inputs.expected_fill_price.value.to_string()),
    );
    m.insert(
        "competingOrders".into(),
        Value::from(i64::from(action.live_inputs.competing_orders.value)),
    );
    // `custom` is OMITTED — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Amm::Action::"SignIntentOrder""#, Value::Object(m)))
}

/// Map an [`IntentOrderKind`] to its `snake_case` cedarschema `orderKind`
/// spelling.
const fn intent_order_kind(kind: &IntentOrderKind) -> &'static str {
    match kind {
        IntentOrderKind::Dutch => "dutch",
        IntentOrderKind::Limit => "limit",
        IntentOrderKind::Rfq => "rfq",
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use std::str::FromStr;

    use simulation_reducer::action::amm::{
        AmmAction, IntentOrderKind, IntentVenue, SignIntentOrderAction, SignIntentOrderLiveInputs,
    };
    use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};
    use simulation_state::primitives::{Address, ChainId, Decimal, Time, U256};
    use simulation_state::LiveField;

    use super::super::test_support::{now, onchain_source, sample_token_ref, submitter, user};

    /// A UniswapX Dutch sign-intent order, offchain-sig meta.
    fn sample_sign_intent() -> (ActionBody, ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let venue = IntentVenue::UniswapX {
            chain: chain.clone(),
            reactor: Address::from_str("0x6000da47483062a0d734ba3dc7576ce6a0b645c4").unwrap(),
        };

        let sign = AmmAction::SignIntentOrder(SignIntentOrderAction {
            venue,
            sell: sample_token_ref(&chain),
            buy: sample_token_ref(&chain),
            sell_amount: U256::from(1_000_000_000u64),
            buy_min: U256::from(300_000_000_000_000_000u64),
            order_kind: IntentOrderKind::Dutch,
            recipient: user(),
            valid_until: Time::from_unix(1_738_003_600),
            live_inputs: SignIntentOrderLiveInputs {
                expected_fill_price: LiveField::new(
                    Decimal::new("3050.25"),
                    onchain_source(),
                    now(),
                ),
                competing_orders: LiveField::new(3u32, onchain_source(), now()),
            },
        });

        let meta = ActionMeta {
            submitted_at: now(),
            submitter: submitter(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "UniswapX".into(),
                    version: Some("1".into()),
                    chain_id: Some(1),
                    verifying_contract: None,
                    salt: None,
                },
                deadline: Time::from_unix(1_738_003_600),
                nonce_key: None,
            },
        };

        (ActionBody::Amm(sign), meta)
    }

    #[test]
    fn sign_intent_order_lowering_conforms_to_schema() {
        let (body, meta) = sample_sign_intent();
        super::super::test_support::assert_conforms("sign_intent_order", &body, &meta);
    }
}
