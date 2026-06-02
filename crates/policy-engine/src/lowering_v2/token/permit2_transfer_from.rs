//! `Token::Permit2TransferFrom` lowering → `Token::Permit2TransferFromContext`.

use serde_json::{Map, Value};

use policy_transition::action::token::Permit2TransferFromAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::Permit2TransferFrom` action into the
/// `Token::Permit2TransferFromContext` shape.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &Permit2TransferFromAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("owner".into(), Value::String(addr(&action.owner)));
    m.insert("spender".into(), Value::String(addr(&action.spender)));
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
    m.insert(
        "permittedAmount".into(),
        Value::String(u256_hex(action.permitted_amount)),
    );
    m.insert(
        "sigDeadline".into(),
        Value::from(action.sig_deadline.as_unix()),
    );
    let (word, bit) = action.nonce.value;
    let mut nonce = Map::new();
    nonce.insert("word".into(), Value::String(u256_hex(word)));
    nonce.insert("bit".into(), Value::from(i64::from(bit)));
    m.insert("nonce".into(), Value::Object(nonce));
    if let Some(witness_type) = &action.witness_type {
        m.insert("witnessType".into(), Value::String(witness_type.clone()));
    }

    Ok(ctx.lowered(r#"Token::Action::"Permit2TransferFrom""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::{Address, Time, U256};
    use policy_transition::action::token::{Permit2TransferFromAction, TokenAction};
    use policy_transition::action::ActionBody;
    use std::str::FromStr;

    use super::super::test_support::{
        live_nonce_pair, onchain_meta, recipient, sample_erc20_token, spender, user,
    };

    #[test]
    fn permit2_transfer_from_lowering_conforms_to_schema() {
        let body = ActionBody::Token(TokenAction::Permit2TransferFrom(
            Permit2TransferFromAction {
                token: sample_erc20_token(),
                owner: user(),
                spender: spender(),
                recipient: recipient(),
                amount: U256::from(500_000_000u64),
                permitted_amount: U256::from(750_000_000u64),
                sig_deadline: Time::from_unix(1_738_001_800),
                nonce: live_nonce_pair(U256::from(3u64), 5),
                witness_type: None,
            },
        ));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("permit2_transfer_from", &body, &meta);
    }

    #[test]
    fn permit2_transfer_from_lowering_includes_witness_type() {
        let body = ActionBody::Token(TokenAction::Permit2TransferFrom(
            Permit2TransferFromAction {
                token: sample_erc20_token(),
                owner: user(),
                spender: Address::from_str("0x000000000000000000000000000000000000bEEF").unwrap(),
                recipient: recipient(),
                amount: U256::from(500_000_000u64),
                permitted_amount: U256::from(750_000_000u64),
                sig_deadline: Time::from_unix(1_738_001_800),
                nonce: live_nonce_pair(U256::from(3u64), 5),
                witness_type: Some("ExampleWitness".into()),
            },
        ));
        let meta = onchain_meta();
        super::super::test_support::assert_conforms("permit2_transfer_from", &body, &meta);
    }
}
