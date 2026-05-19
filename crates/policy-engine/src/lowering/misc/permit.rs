use crate::action::misc::{PermitAction, PermitKind};
use crate::context_keys::RECIPIENT;
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::asset_ref_json;
use crate::lowering::common::validity::validity_json;
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::LoweringError;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "permit";

const PERMIT_KIND: &str = "permitKind";
const TOKEN: &str = "token";
const OWNER: &str = "owner";
const SPENDER: &str = "spender";
const AMOUNT: &str = "amount";
const REQUESTED_AMOUNT: &str = "requestedAmount";
const OPERATOR: &str = "operator";
const APPROVED: &str = "approved";
const VALIDITY: &str = "validity";
const SIGNATURE_VALIDITY: &str = "signatureValidity";

impl Lower for PermitAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

const fn permit_kind_str(k: &PermitKind) -> &'static str {
    match k {
        PermitKind::Eip2612 => "eip2612",
        PermitKind::Erc721Permit => "erc721_permit",
        PermitKind::Erc721PermitForAll => "erc721_permit_for_all",
        PermitKind::Permit2Single => "permit2_single",
        PermitKind::Permit2Transfer => "permit2_transfer",
        PermitKind::Permit2Batch => "permit2_batch",
    }
}

fn context(p: &PermitAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    context.insert(
        PERMIT_KIND.into(),
        Value::from(permit_kind_str(&p.permit_kind)),
    );
    context.insert(TOKEN.into(), asset_ref_json(&p.token)?);
    context.insert(OWNER.into(), Value::from(p.owner.to_string()));

    if let Some(spender) = &p.spender {
        context.insert(SPENDER.into(), Value::from(spender.to_string()));
    }
    if let Some(recipient) = &p.recipient {
        context.insert(RECIPIENT.into(), Value::from(recipient.to_string()));
    }
    if let Some(amount) = &p.amount {
        context.insert(AMOUNT.into(), amount_constraint_json(amount));
    }
    if let Some(req_amount) = &p.requested_amount {
        context.insert(REQUESTED_AMOUNT.into(), amount_constraint_json(req_amount));
    }
    if let Some(operator) = &p.operator {
        context.insert(OPERATOR.into(), Value::from(operator.to_string()));
    }
    if let Some(approved) = p.approved {
        context.insert(APPROVED.into(), Value::from(approved));
    }
    context.insert(VALIDITY.into(), validity_json(&p.validity));
    if let Some(sv) = &p.signature_validity {
        context.insert(SIGNATURE_VALIDITY.into(), validity_json(sv));
    }
    Ok(Value::Object(context))
}
