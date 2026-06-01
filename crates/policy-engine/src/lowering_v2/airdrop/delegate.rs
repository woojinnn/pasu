//! `Airdrop::Delegate` lowering → `Airdrop::DelegateContext`.

use serde_json::{Map, Value};

use policy_transition::action::airdrop::DelegateGovernanceAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};

/// Lower an `Airdrop::Delegate` action into the `Airdrop::DelegateContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &DelegateGovernanceAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("token".into(), lower_token_ref(&action.token));
    m.insert("delegatee".into(), Value::String(addr(&action.delegatee)));

    // ----- Live inputs (LiveField<T> inlined to T) -----
    // `current_delegate` is LiveField<Option<Address>>; omit when None.
    if let Some(current_delegate) = &action.live_inputs.current_delegate.value {
        m.insert(
            "currentDelegate".into(),
            Value::String(addr(current_delegate)),
        );
    }
    m.insert(
        "votingPower".into(),
        Value::String(u256_hex(action.live_inputs.voting_power.value)),
    );
    // `custom` is OMITTED — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Airdrop::Action::"Delegate""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, ChainId, U256};
    use policy_state::LiveField;
    use policy_transition::action::airdrop::{
        AirdropAction, DelegateGovernanceAction, DelegateLiveInputs,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, now, onchain_meta, onchain_source, sample_token_ref,
    };

    /// A delegate with a current delegate set + voting power, on-chain meta.
    fn sample_delegate() -> (ActionBody, policy_transition::action::ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let delegate = AirdropAction::Delegate(DelegateGovernanceAction {
            token: sample_token_ref(&chain),
            delegatee: Address::from_str("0x000000000000000000000000000000000000b0b0").unwrap(),
            live_inputs: DelegateLiveInputs {
                current_delegate: LiveField::new(
                    Some(Address::from_str("0x000000000000000000000000000000000000c0c0").unwrap()),
                    onchain_source(),
                    now(),
                ),
                voting_power: LiveField::new(
                    U256::from(1_000_000_000_000_000_000u64),
                    onchain_source(),
                    now(),
                ),
            },
        });

        (ActionBody::Airdrop(delegate), onchain_meta())
    }

    #[test]
    fn delegate_lowering_conforms_to_schema() {
        let (body, meta) = sample_delegate();
        assert_conforms("delegate", &body, &meta);
    }

    /// A delegate with no current delegate (None) — exercises the omitted
    /// `currentDelegate` branch.
    #[test]
    fn delegate_no_current_delegate_conforms_to_schema() {
        let chain = ChainId::arbitrum();
        let delegate = AirdropAction::Delegate(DelegateGovernanceAction {
            token: sample_token_ref(&chain),
            delegatee: Address::from_str("0x000000000000000000000000000000000000b0b0").unwrap(),
            live_inputs: DelegateLiveInputs {
                current_delegate: LiveField::new(None, onchain_source(), now()),
                voting_power: LiveField::new(U256::ZERO, onchain_source(), now()),
            },
        });

        let body = ActionBody::Airdrop(delegate);
        assert_conforms("delegate", &body, &onchain_meta());
    }
}
