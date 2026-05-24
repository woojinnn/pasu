//! Lowering for [`SetApprovalForAllAction`] → Cedar `set_approval_for_all`
//! policy request.
//!
//! Phase 7B. Mirrors [`super::permit`] — context keys are local `const`s, so
//! `context_keys.rs` needs no change. The emitted context attributes are
//! exactly those declared by
//! `schema/policy-schema/actions/misc/set_approval_for_all.cedarschema`
//! (`collection`, `operator`, `approved`); the `SetApprovalForAllAction`
//! struct additionally carries `operator_label` / `previously_approved`,
//! which the Cedar `SetApprovalForAllContext` does not model, so lowering
//! deliberately omits them.

use crate::action::misc::SetApprovalForAllAction;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "set_approval_for_all";

const COLLECTION: &str = "collection";
const OPERATOR: &str = "operator";
const APPROVED: &str = "approved";

impl Lower for SetApprovalForAllAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(a: &SetApprovalForAllAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(COLLECTION.into(), asset_ref_json(&a.collection)?);
    context.insert(OPERATOR.into(), Value::from(a.operator.to_string()));
    context.insert(APPROVED.into(), Value::from(a.approved));
    Ok(Value::Object(context))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Address, AssetKind, AssetRef, DecimalString};
    use serde_json::Value;
    use std::str::FromStr as _;

    fn ctx_addr(label: u8) -> Address {
        Address::from_str(&format!("0x{}{label:02x}", "0".repeat(38))).unwrap()
    }

    fn lowering_ctx<'a>(
        from: &'a Address,
        to: &'a Address,
        value: &'a DecimalString,
    ) -> LoweringCtx<'a> {
        LoweringCtx {
            from,
            to,
            value_wei: value,
            chain_id: 1,
            block_timestamp: 1_700_000_000,
        }
    }

    fn sample_set_approval(approved: bool) -> SetApprovalForAllAction {
        SetApprovalForAllAction {
            collection: AssetRef {
                kind: AssetKind::Erc721,
                address: Some(ctx_addr(0x11)),
                token_id: None,
                symbol: None,
                decimals: None,
            },
            operator: ctx_addr(0x41),
            operator_label: None,
            approved,
            previously_approved: None,
        }
    }

    /// The lowered context surfaces exactly the `SetApprovalForAllContext`
    /// schema keys: `collection`, `operator`, `approved`.
    #[test]
    fn set_approval_for_all_lowers_to_policy_request() {
        let from = ctx_addr(0xAA);
        let to = ctx_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = lowering_ctx(&from, &to, &value);

        let request = sample_set_approval(true)
            .build(&ctx)
            .expect("set_approval_for_all lowers");

        assert!(request.action.contains("set_approval_for_all"));
        let obj = request.context.as_object().expect("context is an object");
        assert!(obj.contains_key(COLLECTION), "collection key present");
        assert!(obj.contains_key(OPERATOR), "operator key present");
        assert_eq!(obj.get(APPROVED).and_then(Value::as_bool), Some(true));
    }

    /// `approved: false` (revocation) round-trips into the context.
    #[test]
    fn set_approval_for_all_revocation_lowers() {
        let from = ctx_addr(0xAA);
        let to = ctx_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = lowering_ctx(&from, &to, &value);

        let request = sample_set_approval(false)
            .build(&ctx)
            .expect("set_approval_for_all lowers");
        let obj = request.context.as_object().expect("context is an object");
        assert_eq!(obj.get(APPROVED).and_then(Value::as_bool), Some(false));
    }
}
