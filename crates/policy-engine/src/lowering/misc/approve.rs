//! Lowering for [`ApproveAction`] → Cedar `approve` policy request.
//!
//! Phase 7B. Mirrors [`super::permit`] — context keys are local `const`s, so
//! `context_keys.rs` needs no change. The emitted context attributes are
//! exactly those declared by `schema/policy-schema/actions/misc/approve.cedarschema`
//! (`approvalKind`, `token`, `spender`, `amount`, `validity?`); the
//! `ApproveAction` struct additionally carries `spender_label` /
//! `current_allowance`, but the Cedar `ApproveContext` does not model those,
//! so lowering deliberately omits them.

use crate::action::misc::{ApprovalKind, ApproveAction};
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "approve";

const APPROVAL_KIND: &str = "approvalKind";
const TOKEN: &str = "token";
const SPENDER: &str = "spender";
const AMOUNT: &str = "amount";
const VALIDITY: &str = "validity";

impl Lower for ApproveAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

const fn approval_kind_str(k: &ApprovalKind) -> &'static str {
    match k {
        ApprovalKind::Erc20 => "erc20",
        ApprovalKind::Erc20Increase => "erc20_increase",
        ApprovalKind::Erc20Decrease => "erc20_decrease",
        ApprovalKind::Permit2 => "permit2",
        ApprovalKind::Erc721 => "erc721",
    }
}

fn context(a: &ApproveAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(
        APPROVAL_KIND.into(),
        Value::from(approval_kind_str(&a.approval_kind)),
    );
    context.insert(TOKEN.into(), asset_ref_json(&a.token)?);
    context.insert(SPENDER.into(), Value::from(a.spender.to_string()));
    context.insert(AMOUNT.into(), amount_constraint_json(&a.amount));
    if let Some(validity) = &a.validity {
        context.insert(VALIDITY.into(), validity_json(validity));
    }
    Ok(Value::Object(context))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{
        Address, AmountConstraint, AmountKind, AssetKind, AssetRef, DecimalString, Validity,
        ValiditySource,
    };
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

    fn sample_approve(validity: Option<Validity>) -> ApproveAction {
        ApproveAction {
            token: AssetRef {
                kind: AssetKind::Erc20,
                address: Some(ctx_addr(0x10)),
                token_id: None,
                symbol: None,
                decimals: None,
            },
            spender: ctx_addr(0x40),
            spender_label: None,
            amount: AmountConstraint {
                kind: AmountKind::Exact,
                value: Some(DecimalString::from_str("1000").unwrap()),
            },
            approval_kind: ApprovalKind::Permit2,
            current_allowance: None,
            validity,
        }
    }

    /// The lowered context surfaces exactly the `ApproveContext` schema keys:
    /// `approvalKind`, `token`, `spender`, `amount` (and `validity` when set).
    #[test]
    fn approve_lowers_to_policy_request() {
        let from = ctx_addr(0xAA);
        let to = ctx_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = lowering_ctx(&from, &to, &value);

        let action = sample_approve(Some(Validity {
            expires_at: DecimalString::from_str("1700000900").unwrap(),
            source: ValiditySource::GrantExpiration,
        }));
        let request = action.build(&ctx).expect("approve lowers");

        assert!(request.action.contains("approve"));
        let obj = request.context.as_object().expect("context is an object");
        assert_eq!(
            obj.get(APPROVAL_KIND).and_then(Value::as_str),
            Some("permit2")
        );
        assert!(obj.contains_key(TOKEN), "token key present");
        assert!(obj.contains_key(SPENDER), "spender key present");
        assert!(obj.contains_key(AMOUNT), "amount key present");
        assert!(obj.contains_key(VALIDITY), "validity key present when set");
    }

    /// `validity` is omitted from the context when the action carries none.
    #[test]
    fn approve_lowers_without_validity() {
        let from = ctx_addr(0xAA);
        let to = ctx_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = lowering_ctx(&from, &to, &value);

        let request = sample_approve(None).build(&ctx).expect("approve lowers");
        let obj = request.context.as_object().expect("context is an object");
        assert!(!obj.contains_key(VALIDITY), "validity omitted when None");
        // Required keys still present.
        assert!(obj.contains_key(TOKEN));
        assert!(obj.contains_key(SPENDER));
        assert!(obj.contains_key(AMOUNT));
        assert!(obj.contains_key(APPROVAL_KIND));
    }
}
