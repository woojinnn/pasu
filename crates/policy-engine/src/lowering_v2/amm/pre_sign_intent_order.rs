//! `Amm::PreSignIntentOrder` lowering → `Amm::PreSignIntentOrderContext`.

use serde_json::{Map, Value};

use policy_transition::action::amm::PreSignIntentOrderAction;

use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_intent_venue;

/// Lower an `Amm::PreSignIntentOrder` action into the
/// `Amm::PreSignIntentOrderContext` shape.
///
/// CoW Protocol `setPreSignature(bytes orderUid, bool signed)`: the SC-wallet
/// order-placement path. The calldata carries only the opaque `orderUid` and
/// the `signed` direction (terms live in the off-chain order), so the lowered
/// context exposes `venue` + `orderHash` + `signed` and leaves enrichment to
/// fill the rest.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &PreSignIntentOrderAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_intent_venue(&action.venue));
    m.insert("orderHash".into(), Value::String(action.order_hash.clone()));
    m.insert("signed".into(), Value::Bool(action.signed));
    // `custom` is OMITTED — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Amm::Action::"PreSignIntentOrder""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use policy_state::primitives::{Address, ChainId};
    use policy_transition::action::amm::{AmmAction, IntentVenue, PreSignIntentOrderAction};
    use policy_transition::action::ActionBody;

    /// CoW Swap on-chain pre-sign (`signed = true`): conforms + emits the
    /// `signed` bool and the `cow_swap` venue settlement address.
    fn sample_presign(signed: bool) -> ActionBody {
        let venue = IntentVenue::CowSwap {
            chain: ChainId::ethereum_mainnet(),
            settlement: Address::from_str("0x9008d19f58aabd9ed0d60971565aa8510560ab41").unwrap(),
        };
        ActionBody::Amm(AmmAction::PreSignIntentOrder(PreSignIntentOrderAction {
            venue,
            order_hash: "0x8047cdedf01403bf8b5869daa8ca232e5a4b85af666a667943bebf4b17b46a292c5e078be0137049f6fdf92ab01792563e698e6b6a0e024d".into(),
            signed,
        }))
    }

    #[test]
    fn pre_sign_signed_true_conforms_and_pins_fields() {
        let body = sample_presign(true);
        let meta = super::super::test_support::onchain_meta();
        super::super::test_support::assert_conforms("pre_sign_intent_order", &body, &meta);

        let lowered = crate::lowering_v2::lower_action(
            &body,
            &meta,
            &crate::lowering_v2::TxMeta {
                from: "0x1111111111111111111111111111111111111111",
                to: "0x9008d19f58aabd9ed0d60971565aa8510560ab41",
            },
        )
        .unwrap();
        assert_eq!(lowered.context["signed"], serde_json::json!(true));
        assert_eq!(
            lowered.context["venue"]["name"],
            serde_json::json!("cow_swap")
        );
        assert_eq!(
            lowered.context["venue"]["settlement"],
            serde_json::json!("0x9008d19f58aabd9ed0d60971565aa8510560ab41")
        );
    }

    /// `signed = false` (revoke) lowers identically with `signed: false`.
    #[test]
    fn pre_sign_signed_false_conforms() {
        let body = sample_presign(false);
        let meta = super::super::test_support::onchain_meta();
        super::super::test_support::assert_conforms("pre_sign_intent_order", &body, &meta);

        let lowered = crate::lowering_v2::lower_action(
            &body,
            &meta,
            &crate::lowering_v2::TxMeta {
                from: "0x1111111111111111111111111111111111111111",
                to: "0x9008d19f58aabd9ed0d60971565aa8510560ab41",
            },
        )
        .unwrap();
        assert_eq!(lowered.context["signed"], serde_json::json!(false));
    }
}
