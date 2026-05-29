//! `Amm::CancelIntentOrder` lowering → `Amm::CancelIntentOrderContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::amm::CancelIntentOrderAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_intent_venue;

/// Lower an `Amm::CancelIntentOrder` action into the
/// `Amm::CancelIntentOrderContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &CancelIntentOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_intent_venue(&action.venue));
    m.insert(
        "orderHash".into(),
        Value::String(action.order_hash.clone()),
    );
    // `signature` (EIP-712 cancel sig, 0x-hex bytes) → String; omitted when
    // absent.
    if let Some(signature) = &action.signature {
        m.insert("signature".into(), Value::String(signature.clone()));
    }
    // `custom` is OMITTED — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Amm::Action::"CancelIntentOrder""#, Value::Object(m)))
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

    use simulation_reducer::action::amm::{AmmAction, CancelIntentOrderAction, IntentVenue};
    use simulation_reducer::action::{ActionBody, ActionMeta, ActionNature, Eip712Domain};
    use simulation_state::primitives::{Address, ChainId, Time};

    use super::super::test_support::{now, submitter};

    /// A CowSwap off-chain cancel (signature present), offchain-sig meta.
    fn sample_cancel_with_sig() -> (ActionBody, ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let venue = IntentVenue::CowSwap {
            chain,
            settlement: Address::from_str("0x9008d19f58aabd9ed0d60971565aa8510560ab41").unwrap(),
        };

        let cancel = AmmAction::CancelIntentOrder(CancelIntentOrderAction {
            venue,
            order_hash: "0xabc0000000000000000000000000000000000000000000000000000000000000"
                .into(),
            signature: Some("0xdeadbeef".into()),
        });

        let meta = ActionMeta {
            submitted_at: now(),
            submitter: submitter(),
            nature: ActionNature::OffchainSig {
                domain: Eip712Domain {
                    name: "GPv2Settlement".into(),
                    version: Some("v2".into()),
                    chain_id: Some(1),
                    verifying_contract: None,
                    salt: None,
                },
                deadline: Time::from_unix(1_738_003_600),
                nonce_key: None,
            },
        };

        (ActionBody::Amm(cancel), meta)
    }

    /// A Bebop on-chain cancel with no signature — exercises the omitted
    /// `signature` branch + the `{chain}`-only IntentVenue arm.
    fn sample_cancel_no_sig() -> (ActionBody, ActionMeta) {
        let chain = ChainId::arbitrum();
        let venue = IntentVenue::Bebop { chain };

        let cancel = AmmAction::CancelIntentOrder(CancelIntentOrderAction {
            venue,
            order_hash: "0xdef0000000000000000000000000000000000000000000000000000000000000"
                .into(),
            signature: None,
        });

        (
            ActionBody::Amm(cancel),
            super::super::test_support::onchain_meta(),
        )
    }

    #[test]
    fn cancel_intent_order_with_sig_conforms_to_schema() {
        let (body, meta) = sample_cancel_with_sig();
        super::super::test_support::assert_conforms("cancel_intent_order", &body, &meta);
    }

    #[test]
    fn cancel_intent_order_no_sig_conforms_to_schema() {
        let (body, meta) = sample_cancel_no_sig();
        super::super::test_support::assert_conforms("cancel_intent_order", &body, &meta);
    }
}
