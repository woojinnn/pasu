//! `signature.*` enrichment-fact namespace — host-side derivations over an
//! EIP-712 signature request (sim-server fact host, ADR-009).
//!
//! Every method in this module is a `server: sim-server` planned method drawn
//! from `schema/method-catalog.json` (namespace `signature.`). Unlike most
//! sibling namespaces these facts read **no wallet-state DB** — they compare the
//! signature's own EIP-712 domain fields against the evaluation context — but
//! they keep the same `fn(ctx, params) -> Value` shape so the dispatch surface
//! is uniform across `facts/`.
//!
//! ## Scaffold contract (FROZEN dispatch, stub bodies)
//!
//! [`dispatch`] is generated to mirror the catalog 1:1 and is **frozen**: one arm
//! per sim-server `signature.*` method plus a catch-all. Devs filling in the
//! bodies must never edit the match. Each per-method fn currently returns
//! [`FactError::NotImplemented`] so the server still boots and serves the methods
//! that ARE implemented in sibling namespaces.
//!
//! ## Param shape contract
//!
//! Like the rest of `facts/`, `params` arrive as **lowered Cedar** shapes from
//! the extension (not `simulation-state` shapes):
//!   - `chain_id`: the connected chain under evaluation (`EvalContext.chain`),
//!     forwarded as a `Long` (e.g. `1`) per the catalog.
//!   - `domain_chain_id`: the `Long` `chainId` carried by the signature's EIP-712
//!     domain (`$.action.meta.nature.domain.chainId`).

use serde_json::{json, Value};

use super::params::param_long;
use super::FactCtx;
use super::FactError;

/// Dispatch a `signature.*` enrichment fact against `ctx`.
///
/// FROZEN at scaffold time: one arm per sim-server `signature.*` method from the
/// catalog, plus a catch-all. Do not edit this match when filling in bodies.
///
/// # Errors
///
/// Returns [`FactError::UnknownMethod`] when `method` is not a `signature.*`
/// method in this registry, [`FactError::NotImplemented`] when the matched fact
/// body is still a scaffold stub, or [`FactError::BadParams`] from an
/// implemented body whose `params` are missing/ill-shaped.
pub(super) fn dispatch(method: &str, params: &Value, ctx: &FactCtx) -> Result<Value, FactError> {
    match method {
        "signature.chain_mismatch" => chain_mismatch(params, ctx),
        _ => Err(FactError::UnknownMethod(method.into())),
    }
}

/// `signature.chain_mismatch` — GEN-17: does the EIP-712 signature's domain
/// `chainId` differ from the chain the wallet is connected to? The mismatch is
/// `domain_chain_id != EvalContext.chain`. The action carries `domain.chainId`
/// (a `Long`) but the connected chain is the eval context, not an action field,
/// so the comparison is made host-side.
///
/// readKind: `derived`
///
/// Params (catalog):
///   - `chain_id`: Long (required) — `$.root.chain_id` (the connected /
///     under-evaluation chain, `EvalContext.chain`)
///   - `domain_chain_id`: Long (required) — `$.action.meta.nature.domain.chainId`
///     (chainId from the signature's EIP-712 domain)
///
/// Outputs (catalog): `mismatch`: Bool — from `$.result.mismatch`
///
/// State accessors to call (Ground list): NONE. stateDependency is "none" — this
/// fact compares the two `Long` params (`domain_chain_id` vs `chain_id`) and does
/// no wallet-state DB read. `ctx` is accepted only to keep the uniform dispatch
/// signature.
fn chain_mismatch(params: &Value, _ctx: &FactCtx) -> Result<Value, FactError> {
    let chain_id = param_long(params, "chain_id")?;
    let domain_chain_id = param_long(params, "domain_chain_id")?;

    Ok(json!({
        "mismatch": domain_chain_id != chain_id,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use serde_json::json;

    use policy_state::{WalletId, WalletState};

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            "0x000000000000000000000000000000000000a01c"
                .parse()
                .unwrap(),
            [policy_state::primitives::ChainId::ethereum_mainnet()],
        ))
    }

    fn params(chain_id: i64, domain_chain_id: i64) -> Value {
        json!({
            "chain_id": chain_id,
            "domain_chain_id": domain_chain_id,
        })
    }

    #[test]
    fn same_chain_is_no_mismatch() {
        let state = empty_state();
        let out = dispatch(
            "signature.chain_mismatch",
            &params(1, 1),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["mismatch"], json!(false));
    }

    #[test]
    fn different_chain_is_mismatch() {
        let state = empty_state();
        // Connected to mainnet (1) but the signature's domain claims Polygon (137).
        let out = dispatch(
            "signature.chain_mismatch",
            &params(1, 137),
            &FactCtx { state: &state },
        )
        .unwrap();
        assert_eq!(out["mismatch"], json!(true));
    }

    #[test]
    fn missing_param_is_bad_params() {
        let state = empty_state();
        let err = dispatch(
            "signature.chain_mismatch",
            &json!({ "chain_id": 1 }),
            &FactCtx { state: &state },
        )
        .unwrap_err();
        assert!(matches!(err, FactError::BadParams(_)), "{err:?}");
    }
}
