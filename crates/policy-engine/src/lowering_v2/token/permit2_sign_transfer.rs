//! `Token::Permit2SignTransfer` lowering → `Token::Permit2SignTransferContext`.

use serde_json::{Map, Value};

use policy_transition::action::token::Permit2SignTransferAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower a `Token::Permit2SignTransfer` action into the
/// `Token::Permit2SignTransferContext` shape.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &Permit2SignTransferAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("owner".into(), Value::String(addr(&action.owner)));
    m.insert("spender".into(), Value::String(addr(&action.spender)));
    m.insert("amount".into(), Value::String(u256_hex(action.amount)));
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

    Ok(ctx.lowered(r#"Token::Action::"Permit2SignTransfer""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use policy_state::primitives::{Time, U256};
    use policy_transition::action::token::{Permit2SignTransferAction, TokenAction};
    use policy_transition::action::ActionBody;

    use super::super::test_support::{live_nonce_pair, offchain_meta, sample_erc20_token, user};

    #[test]
    fn permit2_sign_transfer_lowering_conforms_to_schema() {
        let body = ActionBody::Token(TokenAction::Permit2SignTransfer(
            Permit2SignTransferAction {
                token: sample_erc20_token(),
                owner: user(),
                spender: super::super::test_support::spender(),
                amount: U256::from(750_000_000u64),
                sig_deadline: Time::from_unix(1_738_001_800),
                nonce: live_nonce_pair(U256::from(3u64), 5),
                witness_type: Some("PermitTransferFrom".into()),
            },
        ));
        let meta = offchain_meta();
        super::super::test_support::assert_conforms("permit2_sign_transfer", &body, &meta);
    }
}
