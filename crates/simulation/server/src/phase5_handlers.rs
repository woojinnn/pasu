//! Phase 5 — auxiliary endpoints used by the Simulation page in the
//! dashboard. Three independent operations:
//!
//!   * `POST /tx/decode` — selector → human-readable function name +
//!     action-envelope hint. Tiny built-in catalog of common ERC-20 /
//!     ERC-721 / Uniswap V3 / Aave selectors. No external lookup
//!     (Sourcify / openchain) — that costs latency and we want this
//!     route synchronous and offline-safe.
//!   * `POST /approvals/revoke-plan` — given `(token, spender, chain)`
//!     pairs, return the `approve(spender, 0)` calldata that revokes
//!     each. Wallet-side composes it into a tx; we never submit.
//!   * `POST /simulate/sequence` — batch-evaluate a sequence of Cedar
//!     requests against the user's installed policies and roll the
//!     verdicts up. Schema-less (same path as `/policies/:id/test`).
//!
//! These three keep their own DTOs rather than reusing the editor /
//! verdict shapes — the Simulation page has different ergonomics
//! (multi-step rollup, calldata builder) and the overlap is thin.

use std::str::FromStr;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use cedar_policy::{Authorizer, Context, Entities, EntityUid, PolicySet, Request};
use serde::{Deserialize, Serialize};

use simulation_db::repositories::user_policies;

use crate::app::AppState;
use crate::auth::AuthUser;

// ───────────────────────────────────────────────────────────────────────
//  POST /tx/decode
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DecodeReq {
    /// CAIP-2 chain id (`eip155:1`). Currently unused for the lookup but
    /// echoed back so multi-call sequences keep their chain context.
    #[serde(default)]
    pub chain: Option<String>,
    /// `0x` lowercase target contract.
    pub to: String,
    /// `0x` calldata. May be empty (`0x` or `""`) for ETH transfers.
    #[serde(default)]
    pub data: String,
    /// Optional value (hex) — shown verbatim, never decoded.
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DecodeResp {
    pub chain: Option<String>,
    pub to: String,
    pub selector: String,
    /// `null` for native ETH transfers (no calldata).
    pub function_signature: Option<&'static str>,
    pub function_name: Option<&'static str>,
    /// Hint of which adapter envelope this calldata maps to. The
    /// extension's adapter layer fills in concrete arg values; here we
    /// only label `domain` and `kind` so the FE can render an icon /
    /// short summary.
    pub action_envelope: Option<ActionHint>,
    /// Pretty single-line label for the verdict row in the UI.
    pub display_label: String,
}

#[derive(Debug, Serialize)]
pub struct ActionHint {
    pub domain: &'static str,
    pub kind: &'static str,
}

/// `POST /tx/decode` — selector → known function + action hint.
pub async fn decode_tx(Json(req): Json<DecodeReq>) -> Response {
    let data = req.data.trim();
    let data_clean = data.strip_prefix("0x").unwrap_or(data);

    // No calldata → native ETH transfer / contract creation.
    if data_clean.is_empty() {
        return Json(DecodeResp {
            chain: req.chain.clone(),
            to: req.to.clone(),
            selector: String::new(),
            function_signature: None,
            function_name: None,
            action_envelope: Some(ActionHint {
                domain: "native",
                kind: "transfer",
            }),
            display_label: format!("native transfer → {}", short_addr(&req.to)),
        })
        .into_response();
    }

    if data_clean.len() < 8 {
        return bad_request("calldata shorter than 4 bytes — no selector");
    }
    let selector = format!("0x{}", &data_clean[..8].to_lowercase());

    let entry = lookup_selector(&selector);
    let label = entry.map_or_else(
        || format!("unknown selector {selector}"),
        |e| format!("{} → {}", e.function_name, short_addr(&req.to)),
    );

    Json(DecodeResp {
        chain: req.chain.clone(),
        to: req.to.clone(),
        selector,
        function_signature: entry.map(|e| e.signature),
        function_name: entry.map(|e| e.function_name),
        action_envelope: entry.map(|e| ActionHint {
            domain: e.domain,
            kind: e.kind,
        }),
        display_label: label,
    })
    .into_response()
}

struct SelectorEntry {
    function_name: &'static str,
    signature: &'static str,
    domain: &'static str,
    kind: &'static str,
}

/// Curated list of selectors the FE care about most. Order doesn't
/// matter — `lookup_selector` does a linear scan (<20 entries).
const SELECTORS: &[(&str, SelectorEntry)] = &[
    // ── ERC-20 ──
    (
        "0xa9059cbb",
        SelectorEntry {
            function_name: "transfer",
            signature: "transfer(address,uint256)",
            domain: "token",
            kind: "erc20.transfer",
        },
    ),
    (
        "0x095ea7b3",
        SelectorEntry {
            function_name: "approve",
            signature: "approve(address,uint256)",
            domain: "token",
            kind: "erc20.approve",
        },
    ),
    (
        "0x23b872dd",
        SelectorEntry {
            function_name: "transferFrom",
            signature: "transferFrom(address,address,uint256)",
            domain: "token",
            kind: "erc20.transferFrom",
        },
    ),
    // ── ERC-721 / 1155 ──
    (
        "0x42842e0e",
        SelectorEntry {
            function_name: "safeTransferFrom",
            signature: "safeTransferFrom(address,address,uint256)",
            domain: "nft",
            kind: "erc721.transfer",
        },
    ),
    (
        "0xa22cb465",
        SelectorEntry {
            function_name: "setApprovalForAll",
            signature: "setApprovalForAll(address,bool)",
            domain: "nft",
            kind: "erc721.setApprovalForAll",
        },
    ),
    // ── Uniswap V3 Router ──
    (
        "0x414bf389",
        SelectorEntry {
            function_name: "exactInputSingle",
            signature: "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))",
            domain: "amm",
            kind: "swap",
        },
    ),
    (
        "0x04e45aaf",
        SelectorEntry {
            function_name: "exactInputSingle",
            signature: "exactInputSingle((address,address,uint24,address,uint256,uint256,uint160))",
            domain: "amm",
            kind: "swap",
        },
    ),
    (
        "0xc04b8d59",
        SelectorEntry {
            function_name: "exactInput",
            signature: "exactInput((bytes,address,uint256,uint256,uint256))",
            domain: "amm",
            kind: "swap",
        },
    ),
    // ── Aave V3 Pool ──
    (
        "0x617ba037",
        SelectorEntry {
            function_name: "supply",
            signature: "supply(address,uint256,address,uint16)",
            domain: "lending",
            kind: "supply",
        },
    ),
    (
        "0x573ade81",
        SelectorEntry {
            function_name: "repay",
            signature: "repay(address,uint256,uint256,address)",
            domain: "lending",
            kind: "repay",
        },
    ),
    (
        "0xa415bcad",
        SelectorEntry {
            function_name: "borrow",
            signature: "borrow(address,uint256,uint256,uint16,address)",
            domain: "lending",
            kind: "borrow",
        },
    ),
    (
        "0x69328dec",
        SelectorEntry {
            function_name: "withdraw",
            signature: "withdraw(address,uint256,address)",
            domain: "lending",
            kind: "withdraw",
        },
    ),
    // ── Permit2 ──
    (
        "0x36c78516",
        SelectorEntry {
            function_name: "permitTransferFrom",
            signature: "permitTransferFrom((address,uint256,uint256,uint256),(address,uint256),address,bytes)",
            domain: "permit2",
            kind: "transferFrom",
        },
    ),
    // ── Wrapped ETH ──
    (
        "0xd0e30db0",
        SelectorEntry {
            function_name: "deposit",
            signature: "deposit()",
            domain: "wrap",
            kind: "deposit",
        },
    ),
    (
        "0x2e1a7d4d",
        SelectorEntry {
            function_name: "withdraw",
            signature: "withdraw(uint256)",
            domain: "wrap",
            kind: "withdraw",
        },
    ),
];

fn lookup_selector(selector: &str) -> Option<&'static SelectorEntry> {
    let needle = selector.to_lowercase();
    SELECTORS.iter().find(|(sel, _)| *sel == needle).map(|(_, e)| e)
}

// ───────────────────────────────────────────────────────────────────────
//  POST /approvals/revoke-plan
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RevokePlanReq {
    pub items: Vec<RevokeItem>,
}

#[derive(Debug, Deserialize)]
pub struct RevokeItem {
    /// CAIP-2 chain id (`eip155:1`).
    pub chain: String,
    /// `0x` token contract.
    pub token: String,
    /// `0x` spender (the address losing approval).
    pub spender: String,
    /// Optional label for the FE (echoed back).
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RevokePlanResp {
    pub calls: Vec<RevokeCall>,
}

#[derive(Debug, Serialize)]
pub struct RevokeCall {
    pub chain: String,
    pub to: String,
    /// `0x` ERC-20 approve(spender, 0) calldata. 4 + 32 + 32 = 68 bytes.
    pub data: String,
    pub value: &'static str, // always "0x0"
    /// Selector echoed back for FE display ("0x095ea7b3").
    pub selector: &'static str,
    pub label: Option<String>,
}

/// `POST /approvals/revoke-plan` — build approve(spender, 0) calldata
/// for each requested (token, spender) pair. We never submit a tx;
/// the wallet UI composes and signs.
pub async fn revoke_plan(Json(req): Json<RevokePlanReq>) -> Response {
    let mut calls = Vec::with_capacity(req.items.len());
    for item in req.items {
        let spender = match normalize_address(&item.spender) {
            Some(s) => s,
            None => return bad_request(&format!("invalid spender: {}", item.spender)),
        };
        let token = match normalize_address(&item.token) {
            Some(t) => t,
            None => return bad_request(&format!("invalid token: {}", item.token)),
        };
        // 0x095ea7b3 = selector(approve(address,uint256))
        // spender padded to 32 bytes, amount = 32-byte zero
        let mut data = String::with_capacity(2 + 8 + 64 + 64);
        data.push_str("0x095ea7b3");
        data.push_str(&"0".repeat(24));
        data.push_str(&spender[2..]);
        data.push_str(&"0".repeat(64));
        calls.push(RevokeCall {
            chain: item.chain,
            to: token,
            data,
            value: "0x0",
            selector: "0x095ea7b3",
            label: item.label,
        });
    }
    Json(RevokePlanResp { calls }).into_response()
}

/// Returns the lowercase 0x-prefixed address if it parses, else None.
fn normalize_address(s: &str) -> Option<String> {
    let trimmed = s.trim();
    let body = trimmed.strip_prefix("0x").unwrap_or(trimmed);
    if body.len() != 40 || !body.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("0x{}", body.to_lowercase()))
}

// ───────────────────────────────────────────────────────────────────────
//  POST /simulate/sequence
// ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SequenceReq {
    /// Steps to evaluate in order. Each gets its own Cedar request.
    pub steps: Vec<SequenceStep>,
    /// Optional policy filter — if omitted, all enabled policies run.
    #[serde(default)]
    pub policy_ids: Option<Vec<i64>>,
}

#[derive(Debug, Deserialize)]
pub struct SequenceStep {
    /// Human label echoed in the response so the FE can label rows.
    #[serde(default)]
    pub label: Option<String>,
    pub principal: String,
    pub action: String,
    pub resource: String,
    #[serde(default)]
    pub entities: serde_json::Value,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct SequenceResp {
    /// `pass` if every step passed, `warn` if any warned (no fails), else `fail`.
    pub overall: &'static str,
    pub steps: Vec<SequenceStepResult>,
}

#[derive(Debug, Serialize)]
pub struct SequenceStepResult {
    pub label: Option<String>,
    /// `pass` / `warn` / `fail`.
    pub verdict: &'static str,
    /// Per-policy outcomes for this step.
    pub policy_results: Vec<PolicyOutcome>,
}

#[derive(Debug, Serialize)]
pub struct PolicyOutcome {
    pub policy_id: i64,
    pub policy_name: String,
    pub severity: String,
    pub decision: &'static str, // "allow" | "deny"
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched: Vec<String>,
}

/// `POST /simulate/sequence` — for each step, evaluate every (or the
/// selected) installed policy against the supplied Cedar request and
/// roll up an overall verdict. Like `/policies/:id/test` ×N but
/// fan-out across all policies, returned in one round-trip.
pub async fn simulate_sequence(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<SequenceReq>,
) -> Response {
    let store = match state.multi_user.for_user(&user.user_id) {
        Ok(s) => s,
        Err(e) => return internal(&format!("open user store: {e}")),
    };
    let pool = store.pool().clone();

    // Load enabled (or filtered) installed policies up-front.
    let want_ids = req.policy_ids.clone();
    let policies: Vec<simulation_db::repositories::user_policies::UserPolicyRow> =
        match tokio::task::spawn_blocking(move || {
            pool.with_tx(|tx| {
                let mut rows = user_policies::list_enabled(tx)?;
                if let Some(ids) = &want_ids {
                    let keep: std::collections::HashSet<i64> = ids.iter().copied().collect();
                    rows.retain(|r| keep.contains(&r.id));
                }
                Ok(rows)
            })
        })
        .await
        {
            Ok(Ok(rows)) => rows,
            Ok(Err(e)) => return internal(&format!("load policies: {e}")),
            Err(e) => return internal(&format!("join: {e}")),
        };

    // Pre-parse each policy's Cedar text once.
    let parsed: Vec<(i64, String, String, PolicySet)> = {
        let mut out = Vec::with_capacity(policies.len());
        for p in &policies {
            match PolicySet::from_str(&p.cedar_text) {
                Ok(ps) => out.push((p.id, p.name.clone(), p.severity.clone(), ps)),
                Err(_) => continue, // skip unparseable; surface elsewhere via /policies/validate
            }
        }
        out
    };

    let auth = Authorizer::new();
    let mut step_results = Vec::with_capacity(req.steps.len());
    let mut any_fail = false;
    let mut any_warn = false;

    for step in req.steps {
        let principal: EntityUid = match step.principal.parse() {
            Ok(p) => p,
            Err(e) => return bad_request(&format!("principal: {e}")),
        };
        let action: EntityUid = match step.action.parse() {
            Ok(a) => a,
            Err(e) => return bad_request(&format!("action: {e}")),
        };
        let resource: EntityUid = match step.resource.parse() {
            Ok(r) => r,
            Err(e) => return bad_request(&format!("resource: {e}")),
        };
        let entities = match Entities::from_json_value(step.entities, None) {
            Ok(e) => e,
            Err(e) => return bad_request(&format!("entities: {e}")),
        };
        let context = match Context::from_json_value(step.context, None) {
            Ok(c) => c,
            Err(e) => return bad_request(&format!("context: {e}")),
        };
        let cedar_req = match Request::new(principal, action, resource, context, None) {
            Ok(r) => r,
            Err(e) => return bad_request(&format!("request: {e}")),
        };

        let mut outcomes = Vec::with_capacity(parsed.len());
        let mut step_has_fail = false;
        let mut step_has_warn = false;
        for (pid, pname, severity, pset) in &parsed {
            let resp = auth.is_authorized(&cedar_req, pset, &entities);
            let decision = match resp.decision() {
                cedar_policy::Decision::Allow => "allow",
                cedar_policy::Decision::Deny => "deny",
            };
            let matched: Vec<String> = resp
                .diagnostics()
                .reason()
                .map(std::string::ToString::to_string)
                .collect();
            if decision == "deny" {
                match severity.as_str() {
                    "warn" => step_has_warn = true,
                    _ => step_has_fail = true,
                }
            }
            outcomes.push(PolicyOutcome {
                policy_id: *pid,
                policy_name: pname.clone(),
                severity: severity.clone(),
                decision,
                matched,
            });
        }

        let step_verdict = if step_has_fail {
            "fail"
        } else if step_has_warn {
            "warn"
        } else {
            "pass"
        };
        if step_verdict == "fail" {
            any_fail = true;
        } else if step_verdict == "warn" {
            any_warn = true;
        }
        step_results.push(SequenceStepResult {
            label: step.label,
            verdict: step_verdict,
            policy_results: outcomes,
        });
    }

    let overall = if any_fail {
        "fail"
    } else if any_warn {
        "warn"
    } else {
        "pass"
    };

    Json(SequenceResp {
        overall,
        steps: step_results,
    })
    .into_response()
}

// ── helpers ───────────────────────────────────────────────────────────

fn short_addr(addr: &str) -> String {
    let trimmed = addr.trim();
    if trimmed.len() < 12 {
        return trimmed.to_string();
    }
    format!("{}…{}", &trimmed[..6], &trimmed[trimmed.len() - 4..])
}

fn bad_request(reason: &str) -> Response {
    (StatusCode::BAD_REQUEST, reason.to_owned()).into_response()
}

fn internal(reason: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, reason.to_owned()).into_response()
}
