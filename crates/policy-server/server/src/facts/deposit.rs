//! `deposit.*` enrichment-fact namespace — host-side derivations over a
//! supply/deposit action (sim-server fact host, ADR-009).
//!
//! Every method in this module is a `server: sim-server` planned method drawn
//! from `schema/method-catalog.json` (namespace `deposit.`). Like the
//! `signature.*` siblings these facts read **no wallet-state DB** — they compare
//! the deposit action's own fields against the evaluation context / signer — but
//! they keep the same `fn(params, ctx) -> Value` shape so the dispatch surface is
//! uniform across `facts/`.
//!
//! ## Scaffold contract (FROZEN dispatch, real bodies)
//!
//! [`dispatch`] mirrors the catalog 1:1 and is **frozen**: one arm per sim-server
//! `deposit.*` method plus a catch-all. Devs filling in the bodies must never
//! edit the match.
//!
//! ## Param shape contract
//!
//! Like the rest of `facts/`, `params` arrive as **lowered Cedar** shapes from
//! the extension (not `simulation-state` shapes):
//!   - `chain_id`: the connected chain under evaluation, forwarded as a `Long`.
//!   - `owner`: the signer (`$.root.from`), a plain hex address string.
//!   - `action`: the lowered supply/deposit Action body. The supply lowering
//!     (`Lending::Supply`) emits the receiver under the `onBehalfOf` key
//!     (an address string) and OMITS it entirely when the depositor is self.

use serde_json::{json, Value};

use super::params::{param_action, param_str};
use super::FactCtx;
use super::FactError;

/// Dispatch a `deposit.*` enrichment fact by method name.
///
/// FROZEN at scaffold time: one arm per sim-server `deposit.*` method from the
/// catalog, plus a catch-all. Do not edit this match when filling in bodies.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] when `method` is not a registered
/// `deposit.*` fact, [`FactError::BadParams`] from an implemented body whose
/// `params` are missing/ill-shaped, or [`FactError::NotImplemented`] for an
/// un-filled stub.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "deposit.receiver_self" => receiver_self(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// `deposit.receiver_self` — readKind: derived.
///
/// Is the supply/deposit **receiver / onBehalfOf** NOT the signer? Catches a
/// deposit that credits the position to a *different* address than the wallet
/// signing (funds leave the signer but the resulting aToken / vault share lands
/// elsewhere).
///
/// stateDependency: `none` — compares the deposit receiver field carried by the
/// lowered action to the signer (`owner`); reads no wallet-state DB. `ctx` is
/// accepted only to keep the uniform dispatch signature.
///
/// Params (catalog):
///   - `chain_id`: Long (required) — `$.root.chain_id`
///   - `owner`: String (required) — `$.root.from` (the signer)
///   - `action`: Action (required) — `$.action` (the lowered supply/deposit body
///     carrying the receiver / `onBehalfOf`)
///
/// Outputs (catalog): `receiverNotSelf`: Bool — from `$.result.receiverNotSelf`
///
/// The `Lending::Supply` lowering emits the receiver as the `onBehalfOf` key and
/// OMITS it when the depositor supplies for themselves (`on_behalf_of == None`).
/// So an absent receiver field ≡ self-deposit ≡ `receiverNotSelf = false`. When
/// present, the comparison is case-insensitive (the lowering hex-encodes the
/// address, which may differ in checksum casing from the raw `from`). A `receiver`
/// key is accepted as a fallback alias since the catalog names the field
/// "receiver / onBehalfOf".
fn receiver_self(params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    let owner = param_str(params, "owner")?;
    let action = param_action(params, "action")?;

    let receiver = action
        .get("onBehalfOf")
        .or_else(|| action.get("receiver"))
        .and_then(Value::as_str);

    // Absent receiver ≡ self-deposit (the supply lowering omits `onBehalfOf` when
    // `on_behalf_of == None`). Present ≡ compare to signer, case-insensitively.
    let receiver_not_self = receiver.is_some_and(|r| !r.eq_ignore_ascii_case(&owner));

    Ok(json!({ "receiverNotSelf": receiver_not_self }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use policy_state::{WalletId, WalletState};

    const OWNER: &str = "0x000000000000000000000000000000000000a01c";
    const OTHER: &str = "0x0000000000000000000000000000000000000bad";

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            OWNER.parse().unwrap(),
            [policy_state::primitives::ChainId::ethereum_mainnet()],
        ))
    }

    fn params(owner: &str, action: &Value) -> Value {
        json!({
            "chain_id": 1,
            "owner": owner,
            "action": action,
        })
    }

    #[test]
    fn omitted_on_behalf_of_is_self_deposit() {
        // Supply lowering omits `onBehalfOf` when supplying for self.
        let state = empty_state();
        let out = dispatch(
            "deposit.receiver_self",
            &params(OWNER, &json!({ "amount": "0x1" })),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["receiverNotSelf"], json!(false));
    }

    #[test]
    fn on_behalf_of_self_is_not_other() {
        let state = empty_state();
        let out = dispatch(
            "deposit.receiver_self",
            &params(OWNER, &json!({ "onBehalfOf": OWNER })),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["receiverNotSelf"], json!(false));
    }

    #[test]
    fn on_behalf_of_other_is_not_self() {
        let state = empty_state();
        let out = dispatch(
            "deposit.receiver_self",
            &params(OWNER, &json!({ "onBehalfOf": OTHER })),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["receiverNotSelf"], json!(true));
    }

    #[test]
    fn checksum_casing_does_not_count_as_other() {
        // Lowering hex-encodes the address; casing must not register a mismatch.
        let state = empty_state();
        let mixed = "0x000000000000000000000000000000000000A01C";
        let out = dispatch(
            "deposit.receiver_self",
            &params(OWNER, &json!({ "onBehalfOf": mixed })),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["receiverNotSelf"], json!(false));
    }

    #[test]
    fn receiver_alias_is_accepted() {
        let state = empty_state();
        let out = dispatch(
            "deposit.receiver_self",
            &params(OWNER, &json!({ "receiver": OTHER })),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["receiverNotSelf"], json!(true));
    }

    #[test]
    fn missing_action_is_bad_params() {
        let state = empty_state();
        let err = dispatch(
            "deposit.receiver_self",
            &json!({ "chain_id": 1, "owner": OWNER }),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }

    #[test]
    fn unknown_method_errors() {
        let state = empty_state();
        let err = dispatch(
            "deposit.not_a_real_method",
            &json!({}),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::UnknownMethod(_)), "{err:?}");
    }
}
