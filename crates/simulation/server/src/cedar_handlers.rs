//! Cedar editor support — `POST /policies/validate` and
//! `POST /policies/:id/test`.
//!
//! These endpoints back the policy editor's live syntax-check and the
//! "test against TX" panel. Live evaluation (in the extension) is
//! untouched; this is purely an authoring convenience.
//!
//! Implementation notes:
//! - Validation uses `cedar_policy::PolicySet::from_str` directly so a
//!   bad policy text returns the Cedar parser's own error message
//!   (line/column included).
//! - The test endpoint accepts an already-constructed `PolicyRequest`
//!   (principal/action/resource/entities/context) — the FE builds it
//!   from the example fixture's `action` + `context`. Mapping action
//!   envelopes → Cedar entity graph happens in the extension's
//!   adapter layer; replicating that here would just diverge.

use std::str::FromStr;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use cedar_policy::{
    Authorizer, Context, Decision, Entities, EntityUid, PolicyId, PolicySet, Request,
};
use serde::{Deserialize, Serialize};

use simulation_db::repositories::user_policies;

use crate::app::AppState;
use crate::auth::AuthUser;

// ---------- POST /policies/validate ----------

#[derive(Debug, Deserialize)]
pub struct ValidateReq {
    pub cedar_text: String,
}

#[derive(Debug, Serialize)]
pub struct ValidateResp {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `POST /policies/validate` — does the Cedar text parse? Returns
/// `{ ok: true }` on success, `{ ok: false, error: "<parse msg>" }`
/// otherwise. Schema validation is intentionally NOT run here — that's
/// left to the per-policy bundle build path used at install time.
pub async fn validate_policy(Json(req): Json<ValidateReq>) -> Response {
    if req.cedar_text.trim().is_empty() {
        return Json(ValidateResp {
            ok: false,
            error: Some("cedar_text must not be empty".into()),
        })
        .into_response();
    }
    match PolicySet::from_str(&req.cedar_text) {
        Ok(_) => Json(ValidateResp {
            ok: true,
            error: None,
        })
        .into_response(),
        Err(e) => Json(ValidateResp {
            ok: false,
            error: Some(e.to_string()),
        })
        .into_response(),
    }
}

// ---------- POST /policies/:id/test ----------

#[derive(Debug, Deserialize)]
pub struct TestReq {
    /// Fully-formed Cedar request. Mirror of
    /// `policy_engine::policy::request::PolicyRequest`.
    pub request: PolicyRequestDto,
}

#[derive(Debug, Deserialize)]
pub struct PolicyRequestDto {
    pub principal: String,
    pub action: String,
    pub resource: String,
    #[serde(default)]
    pub entities: serde_json::Value,
    #[serde(default)]
    pub context: serde_json::Value,
}

/// Test endpoint response. Mirrors the Verdict enum:
/// `pass` → empty matched list. `warn`/`fail` → matched policies (with
/// id, severity, optional reason annotation).
#[derive(Debug, Serialize)]
pub struct TestResp {
    /// `"pass"` | `"warn"` | `"fail"`.
    pub verdict: String,
    pub matched: Vec<MatchedPolicyDto>,
}

#[derive(Debug, Serialize)]
pub struct MatchedPolicyDto {
    pub policy_id: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `POST /policies/:id/test` — run a sample Cedar request against the
/// saved policy text. The engine is built schema-less so any well-formed
/// `cedar_text` will run; users can iterate without a schema yet.
pub async fn test_policy(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(req): Json<TestReq>,
) -> Response {
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };
    let pool = store.pool().clone();
    let policy_row =
        match tokio::task::spawn_blocking(move || pool.with_tx(|tx| user_policies::get(tx, id)))
            .await
        {
            Ok(Ok(Some(row))) => row,
            Ok(Ok(None)) => return not_found("policy not found"),
            Ok(Err(e)) => return internal(&format!("load policy: {e}")),
            Err(e) => return internal(&format!("join: {e}")),
        };

    // Schema-less evaluation: use cedar-policy directly so authors can
    // experiment with any principal/action/resource without first wiring
    // a schema. Production evaluation (in the extension) goes through
    // `PolicyEngine` with the bundled schema; that's strictly stricter.
    let pset = match PolicySet::from_str(&policy_row.cedar_text) {
        Ok(p) => p,
        Err(e) => return bad_request(&format!("policy parse: {e}")),
    };

    let principal: EntityUid = match req.request.principal.parse() {
        Ok(p) => p,
        Err(e) => return bad_request(&format!("principal: {e}")),
    };
    let action: EntityUid = match req.request.action.parse() {
        Ok(a) => a,
        Err(e) => return bad_request(&format!("action: {e}")),
    };
    let resource: EntityUid = match req.request.resource.parse() {
        Ok(r) => r,
        Err(e) => return bad_request(&format!("resource: {e}")),
    };
    let entities = match Entities::from_json_value(req.request.entities, None) {
        Ok(e) => e,
        Err(e) => return bad_request(&format!("entities: {e}")),
    };
    let context = match Context::from_json_value(req.request.context, None) {
        Ok(c) => c,
        Err(e) => return bad_request(&format!("context: {e}")),
    };
    let cedar_req = match Request::new(principal, action, resource, context, None) {
        Ok(r) => r,
        Err(e) => return bad_request(&format!("request: {e}")),
    };

    let response = Authorizer::new().is_authorized(&cedar_req, &pset, &entities);
    Json(authorize_to_dto(&response)).into_response()
}

fn authorize_to_dto(response: &cedar_policy::Response) -> TestResp {
    let determining: Vec<PolicyId> = response.diagnostics().reason().cloned().collect();
    let errors: Vec<String> = response
        .diagnostics()
        .errors()
        .map(std::string::ToString::to_string)
        .collect();
    let (verdict, matched) = match response.decision() {
        Decision::Allow => ("pass", Vec::new()),
        Decision::Deny if errors.is_empty() => (
            "fail",
            determining
                .iter()
                .map(|pid| MatchedPolicyDto {
                    policy_id: pid.to_string(),
                    severity: "deny".into(),
                    reason: None,
                })
                .collect(),
        ),
        Decision::Deny => (
            "fail",
            errors
                .iter()
                .map(|e| MatchedPolicyDto {
                    policy_id: "__eval_error__".into(),
                    severity: "deny".into(),
                    reason: Some(e.clone()),
                })
                .collect(),
        ),
    };
    TestResp {
        verdict: verdict.into(),
        matched,
    }
}

fn bad_request(reason: &str) -> Response {
    (StatusCode::BAD_REQUEST, reason.to_owned()).into_response()
}

fn not_found(reason: &str) -> Response {
    (StatusCode::NOT_FOUND, reason.to_owned()).into_response()
}

fn internal(reason: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, reason.to_owned()).into_response()
}
