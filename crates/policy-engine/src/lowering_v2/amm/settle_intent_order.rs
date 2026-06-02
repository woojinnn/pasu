//! `Amm::SettleIntentOrder` lowering → `Amm::SettleIntentOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::amm::{IntentOrderKind, SettleIntentOrderAction};

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_intent_venue;

/// Lower an `Amm::SettleIntentOrder` action into the
/// `Amm::SettleIntentOrderContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// lowering contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &SettleIntentOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_intent_venue(&action.venue));
    m.insert("swapper".into(), Value::String(addr(&action.swapper)));
    m.insert("sell".into(), lower_token_ref(&action.sell));
    m.insert("buy".into(), lower_token_ref(&action.buy));
    m.insert(
        "sellAmount".into(),
        Value::String(u256_hex(action.sell_amount)),
    );
    m.insert("buyMin".into(), Value::String(u256_hex(action.buy_min)));
    m.insert(
        "orderKind".into(),
        Value::String(intent_order_kind(&action.order_kind).into()),
    );
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    m.insert(
        "validUntil".into(),
        Value::from(action.valid_until.as_unix()),
    );
    m.insert(
        "orderNonce".into(),
        Value::String(u256_hex(action.order_nonce)),
    );
    if let Some(signature) = &action.signature {
        m.insert("signature".into(), Value::String(signature.clone()));
    }

    Ok(ctx.lowered(r#"Amm::Action::"SettleIntentOrder""#, Value::Object(m)))
}

const fn intent_order_kind(kind: &IntentOrderKind) -> &'static str {
    match kind {
        IntentOrderKind::Dutch => "dutch",
        IntentOrderKind::Limit => "limit",
        IntentOrderKind::Rfq => "rfq",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, ChainId, U256};
    use policy_transition::action::amm::{
        AmmAction, IntentOrderKind, IntentVenue, SettleIntentOrderAction,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{now, onchain_meta, sample_token_ref, user};

    #[test]
    fn settle_intent_order_lowering_conforms_to_schema() {
        let chain = ChainId::ethereum_mainnet();
        let reactor = Address::from_str("0x00000011f84b9aa48e5f8aa8b9897600006289be").unwrap();
        let action = AmmAction::SettleIntentOrder(SettleIntentOrderAction {
            venue: IntentVenue::UniswapX {
                chain: chain.clone(),
                reactor,
            },
            swapper: user(),
            sell: sample_token_ref(&chain),
            buy: sample_token_ref(&chain),
            sell_amount: U256::from(1_000_000u64),
            buy_min: U256::from(900_000u64),
            order_kind: IntentOrderKind::Dutch,
            recipient: user(),
            valid_until: now(),
            order_nonce: U256::from(42u64),
            signature: Some("0x1234".into()),
        });
        let meta = onchain_meta();

        let body = ActionBody::Amm(action);
        super::super::test_support::assert_conforms("settle_intent_order", &body, &meta);
    }
}
