//! `Bridge::Send` lowering.

use serde_json::{Map, Value};

use policy_state::primitives::Address;
use policy_transition::action::bridge::{BridgeRecipient, BridgeSendAction, BridgeVenue};

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

fn lower_bridge_venue(venue: &BridgeVenue) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(venue.name().into()));
    Value::Object(m)
}

fn lower_bridge_recipient(r: &BridgeRecipient) -> Value {
    let mut m = Map::new();
    match r {
        BridgeRecipient::Evm { address } => {
            m.insert("kind".into(), Value::String("evm".into()));
            m.insert("address".into(), Value::String(addr(address)));
        }
        BridgeRecipient::Raw { bytes32 } => {
            m.insert("kind".into(), Value::String("raw".into()));
            m.insert("bytes32".into(), Value::String(bytes32.clone()));
        }
    }
    Value::Object(m)
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &BridgeSendAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_bridge_venue(&action.venue));
    m.insert("srcToken".into(), lower_token_ref(&action.src_token));
    m.insert(
        "inputAmount".into(),
        Value::String(u256_hex(action.input_amount)),
    );
    m.insert(
        "dstChainId".into(),
        Value::String(action.dst_chain_id.to_string()),
    );
    m.insert(
        "dstRecipient".into(),
        lower_bridge_recipient(&action.dst_recipient),
    );
    if let Some(t) = &action.dst_token {
        m.insert("dstToken".into(), lower_token_ref(t));
    }
    if let Some(out) = action.output_amount {
        m.insert("outputAmount".into(), Value::String(u256_hex(out)));
    }
    if let Some(r) = &action.exclusive_relayer {
        if *r != Address::ZERO {
            m.insert("exclusiveRelayer".into(), Value::String(addr(r)));
        }
    }
    m.insert("hasMessage".into(), Value::Bool(action.has_message));

    Ok(ctx.lowered(r#"Bridge::Action::"Send""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::{ChainId, U256};
    use policy_transition::action::bridge::{BridgeAction, BridgeSendAction, BridgeVenue};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, dst_token, evm_recipient, onchain_meta, src_token,
    };

    #[test]
    fn send_lowering_conforms() {
        let body = ActionBody::Bridge(BridgeAction::Send(BridgeSendAction {
            venue: BridgeVenue::AcrossSpokePool,
            src_token: src_token(),
            input_amount: U256::from(1_000_000u64),
            dst_chain_id: ChainId("eip155:10".into()),
            dst_recipient: evm_recipient(),
            dst_token: Some(dst_token()),
            output_amount: Some(U256::from(999_000u64)),
            exclusive_relayer: None,
            has_message: false,
        }));
        assert_conforms("send", &body, &onchain_meta());
    }
}
