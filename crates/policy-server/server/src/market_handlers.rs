//! Marketplace HTTP handlers.
//!
//! All routes are mounted behind `require_auth`, so every handler receives an
//! [`AuthUser`] via `Extension`. The user's `user_id` becomes `publisher_id`
//! for writes and `installer` for install events.
//!
//! Stats (install count, average rating) are computed on read inside the
//! store layer's `LATERAL` join, not denormalized on `market_listings`.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use policy_db::market::{
    create_listing as db_create_listing, create_version as db_create_version,
    get_latest_version as db_get_latest_version, get_listing_by_id as db_get_listing_by_id,
    get_listing_by_slug as db_get_listing_by_slug, get_version as db_get_version,
    list_listings as db_list_listings, list_reviews as db_list_reviews,
    list_watches as db_list_watches, record_install as db_record_install, unwatch as db_unwatch,
    upsert_review as db_upsert_review, validate_semver, vote_helpful as db_vote_helpful,
    watch as db_watch, ListingFilter, ListingRow, ListingSort as DbListingSort, NewListing,
    ReviewRow, VersionBody, VersionRow, LIST_LIMIT_DEFAULT,
};

use crate::app::AppState;
use crate::auth::AuthUser;
use crate::market_dto::{
    CreateInstallReq, CreateListingReq, CreateReviewReq, CreateVersionReq, I18nText,
    ListListingsQuery, ListingDetail, ListingKind, ListingSort, ListingStatus, ListingSummary,
    ListingVersion, PublisherTier, Review, SetMember, Severity,
};

// ---------------------------------------------------------------------------
// Read endpoints
// ---------------------------------------------------------------------------

/// `GET /market/listings` — browse + filter + sort.
pub async fn list_listings(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Query(q): Query<ListListingsQuery>,
) -> Response {
    let pool = state.global_db.pool();
    let filter = ListingFilter {
        kind: q.kind.map(|k| serde_kind(k).to_owned()),
        domain: q.domain,
        publisher_id: q.publisher_id,
        publisher_tier: q.publisher_tier.map(|t| serde_tier(t).to_owned()),
        q: q.q,
    };
    let sort = match q.sort {
        ListingSort::Popular => DbListingSort::Popular,
        ListingSort::New => DbListingSort::New,
        ListingSort::Rating => DbListingSort::Rating,
    };
    let limit = q.limit.unwrap_or(LIST_LIMIT_DEFAULT);
    let offset = q.offset.unwrap_or(0);

    match db_list_listings(pool, &filter, sort, limit, offset, Some(&user.user_id)).await {
        Ok(rows) => {
            let summaries: Vec<ListingSummary> = rows.iter().map(listing_row_to_summary).collect();
            Json(summaries).into_response()
        }
        Err(e) => server_error(&e.to_string()),
    }
}

/// `GET /market/listings/:slug` — listing detail + latest version + recent reviews.
pub async fn get_listing(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(slug): Path<String>,
) -> Response {
    let pool = state.global_db.pool();
    let listing = match db_get_listing_by_slug(pool, &slug, Some(&user.user_id)).await {
        Ok(Some(row)) => row,
        Ok(None) => return (StatusCode::NOT_FOUND, "listing not found").into_response(),
        Err(e) => return server_error(&e.to_string()),
    };

    let latest = match db_get_latest_version(pool, listing.id).await {
        Ok(v) => v,
        Err(e) => return server_error(&e.to_string()),
    };

    let reviews = match db_list_reviews(pool, listing.id, 10).await {
        Ok(r) => r,
        Err(e) => return server_error(&e.to_string()),
    };

    let detail = ListingDetail {
        summary: listing_row_to_summary(&listing),
        latest_version: latest.map(version_row_to_dto),
        recent_reviews: reviews.iter().map(review_row_to_dto).collect(),
    };
    Json(detail).into_response()
}

/// `GET /market/listings/:id/versions/:ver` — a specific version body.
/// Used by install: the client fetches this to copy into its editor.
pub async fn get_version(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    Path((listing_id, version)): Path<(Uuid, String)>,
) -> Response {
    let pool = state.global_db.pool();
    match db_get_version(pool, listing_id, &version).await {
        Ok(Some(v)) => Json(version_row_to_dto(v)).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "version not found").into_response(),
        Err(e) => server_error(&e.to_string()),
    }
}

/// `GET /market/listings/:id/reviews` — full review list (helpful-first).
pub async fn list_reviews(
    State(state): State<AppState>,
    Extension(_user): Extension<AuthUser>,
    Path(listing_id): Path<Uuid>,
) -> Response {
    let pool = state.global_db.pool();
    match db_list_reviews(pool, listing_id, 200).await {
        Ok(rows) => {
            let dtos: Vec<Review> = rows.iter().map(review_row_to_dto).collect();
            Json(dtos).into_response()
        }
        Err(e) => server_error(&e.to_string()),
    }
}

/// `GET /market/watches` — caller's watched listings (with stats).
pub async fn list_watches(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Response {
    let pool = state.global_db.pool();
    match db_list_watches(pool, &user.user_id).await {
        Ok(rows) => {
            let dtos: Vec<ListingSummary> = rows.iter().map(listing_row_to_summary).collect();
            Json(dtos).into_response()
        }
        Err(e) => server_error(&e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Write endpoints
// ---------------------------------------------------------------------------

/// `POST /market/listings` — publish a new listing + initial version atomically.
pub async fn create_listing(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(req): Json<CreateListingReq>,
) -> Response {
    // ---- request validation ------------------------------------------------
    if let Err(msg) = validate_create_req(&req) {
        return (StatusCode::BAD_REQUEST, msg).into_response();
    }

    let display_name = i18n_to_json(&req.display_name);
    let description = req.description.as_ref().map(i18n_to_json);
    let intents = req
        .intents
        .as_ref()
        .map(|v| Value::Array(v.iter().map(|s| Value::String(s.clone())).collect()));

    let kind_str = serde_kind(req.kind);

    let body = match req.kind {
        ListingKind::Policy => VersionBody {
            cedar_text: req.cedar_text.clone(),
            manifest: req.manifest.clone(),
            policy_tree: req.policy_tree.clone(),
            members: None,
            changelog: req.changelog.as_ref().map(i18n_to_json),
        },
        ListingKind::Set => VersionBody {
            cedar_text: None,
            manifest: None,
            policy_tree: None,
            members: req.members.as_deref().map(members_to_json),
            changelog: req.changelog.as_ref().map(i18n_to_json),
        },
    };

    let new_listing = NewListing {
        slug: req.slug.clone(),
        kind: kind_str.to_owned(),
        publisher_id: user.user_id.clone(),
        publisher_tier: "community".to_owned(), // tier promotion is out of band
        display_name,
        description,
        domain: req.domain.clone(),
        intents,
        severity: req.severity.map(|s| serde_severity(s).to_owned()),
        forked_from: req.forked_from,
        initial_version: req.version.clone(),
        initial_body: body,
    };

    match db_create_listing(state.global_db.pool(), new_listing, now_secs()).await {
        Ok(row) => Json(listing_row_to_summary(&row)).into_response(),
        Err(e) => {
            let msg = e.to_string();
            // Surface duplicate-slug / SemVer / CHECK violations as 400, the
            // rest as 500. The store wraps both as DbError::Invariant, so we
            // pattern-match on substring — coarser than ideal but stable.
            if msg.contains("duplicate key") || msg.contains("UNIQUE") || msg.contains("CHECK") {
                (StatusCode::BAD_REQUEST, msg).into_response()
            } else {
                server_error(&msg)
            }
        }
    }
}

/// `POST /market/listings/:id/versions` — publish a new `SemVer` version. Only
/// the original publisher may do this.
pub async fn create_version(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(listing_id): Path<Uuid>,
    Json(req): Json<CreateVersionReq>,
) -> Response {
    let pool = state.global_db.pool();
    let listing = match db_get_listing_by_id(pool, listing_id, Some(&user.user_id)).await {
        Ok(Some(l)) => l,
        Ok(None) => return (StatusCode::NOT_FOUND, "listing not found").into_response(),
        Err(e) => return server_error(&e.to_string()),
    };
    if listing.publisher_id != user.user_id {
        return (
            StatusCode::FORBIDDEN,
            "only the publisher can release new versions",
        )
            .into_response();
    }
    if let Err(e) = validate_semver(&req.version) {
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    if !version_is_strictly_greater(&listing, &req.version) {
        return (
            StatusCode::BAD_REQUEST,
            "new version must be strictly greater than current_version",
        )
            .into_response();
    }

    // Match body kind to listing kind
    let body = match listing.kind.as_str() {
        "policy" => {
            if req.cedar_text.is_none() {
                return (StatusCode::BAD_REQUEST, "policy version needs cedar_text")
                    .into_response();
            }
            VersionBody {
                cedar_text: req.cedar_text,
                manifest: req.manifest,
                policy_tree: req.policy_tree,
                members: None,
                changelog: req.changelog.as_ref().map(i18n_to_json),
            }
        }
        "set" => {
            if req.members.as_ref().is_none_or(std::vec::Vec::is_empty) {
                return (StatusCode::BAD_REQUEST, "set version needs members[]").into_response();
            }
            VersionBody {
                cedar_text: None,
                manifest: None,
                policy_tree: None,
                members: req.members.as_deref().map(members_to_json),
                changelog: req.changelog.as_ref().map(i18n_to_json),
            }
        }
        other => return server_error(&format!("unknown listing kind: {other}")),
    };

    match db_create_version(pool, listing_id, &req.version, body, now_secs()).await {
        Ok(v) => Json(version_row_to_dto(v)).into_response(),
        Err(e) => server_error(&e.to_string()),
    }
}

/// `POST /market/listings/:id/install` — record one install event.
/// Server returns the version body so the client can write it locally in
/// the same round-trip.
pub async fn create_install(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(listing_id): Path<Uuid>,
    Json(req): Json<CreateInstallReq>,
) -> Response {
    let pool = state.global_db.pool();
    let version = match db_get_version(pool, listing_id, &req.version).await {
        Ok(Some(v)) => v,
        Ok(None) => return (StatusCode::NOT_FOUND, "version not found").into_response(),
        Err(e) => return server_error(&e.to_string()),
    };
    if let Err(e) =
        db_record_install(pool, listing_id, &req.version, &user.user_id, now_secs()).await
    {
        return server_error(&e.to_string());
    }
    Json(version_row_to_dto(version)).into_response()
}

/// `POST /market/listings/:id/reviews` — write or replace caller's review.
pub async fn create_review(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(listing_id): Path<Uuid>,
    Json(req): Json<CreateReviewReq>,
) -> Response {
    if !(1..=5).contains(&req.rating) {
        return (StatusCode::BAD_REQUEST, "rating must be 1..=5").into_response();
    }
    if req.body.en.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "review body.en is required").into_response();
    }
    let body_json = i18n_to_json(&req.body);
    match db_upsert_review(
        state.global_db.pool(),
        listing_id,
        &user.user_id,
        &req.version,
        req.rating,
        &body_json,
        now_secs(),
    )
    .await
    {
        Ok(r) => Json(review_row_to_dto(&r)).into_response(),
        Err(e) => server_error(&e.to_string()),
    }
}

/// `POST /market/reviews/:id/helpful` — vote helpful (idempotent per user).
pub async fn vote_helpful(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(review_id): Path<Uuid>,
) -> Response {
    match db_vote_helpful(state.global_db.pool(), review_id, &user.user_id, now_secs()).await {
        Ok(inserted) => Json(serde_json::json!({ "newly_voted": inserted })).into_response(),
        Err(e) => server_error(&e.to_string()),
    }
}

/// `POST /market/listings/:id/watch` — subscribe to new-version notifications.
pub async fn watch(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(listing_id): Path<Uuid>,
) -> Response {
    match db_watch(
        state.global_db.pool(),
        &user.user_id,
        listing_id,
        now_secs(),
    )
    .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => server_error(&e.to_string()),
    }
}

/// `DELETE /market/listings/:id/watch` — cancel subscription.
pub async fn unwatch(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(listing_id): Path<Uuid>,
) -> Response {
    match db_unwatch(state.global_db.pool(), &user.user_id, listing_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => server_error(&e.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn validate_create_req(req: &CreateListingReq) -> Result<(), String> {
    if req.slug.trim().is_empty() {
        return Err("slug is required".into());
    }
    if req.display_name.en.trim().is_empty() {
        return Err("display_name.en is required".into());
    }
    if let Err(e) = validate_semver(&req.version) {
        return Err(e.to_string());
    }
    match req.kind {
        ListingKind::Policy => {
            if req.cedar_text.as_deref().is_none_or(str::is_empty) {
                return Err("policy listing needs cedar_text".into());
            }
            if req.domain.as_deref().is_none_or(str::is_empty) {
                return Err("policy listing needs domain".into());
            }
            if req.severity.is_none() {
                return Err("policy listing needs severity".into());
            }
            if req.members.is_some() {
                return Err("policy listing must not carry members[]".into());
            }
        }
        ListingKind::Set => {
            if req.members.as_ref().is_none_or(std::vec::Vec::is_empty) {
                return Err("set listing needs at least one member".into());
            }
            if req.cedar_text.is_some() {
                return Err("set listing must not carry cedar_text".into());
            }
        }
    }
    Ok(())
}

fn version_is_strictly_greater(listing: &ListingRow, new_version: &str) -> bool {
    let Ok((nmaj, nmin, npat)) = validate_semver(new_version) else {
        return false;
    };
    let Some(cur) = listing.current_version.as_deref() else {
        return true;
    };
    let Ok((cmaj, cmin, cpat)) = validate_semver(cur) else {
        return true;
    };
    (nmaj, nmin, npat) > (cmaj, cmin, cpat)
}

fn listing_row_to_summary(r: &ListingRow) -> ListingSummary {
    ListingSummary {
        id: r.id,
        slug: r.slug.clone(),
        kind: parse_kind(&r.kind),
        publisher_id: r.publisher_id.clone(),
        publisher_tier: parse_tier(&r.publisher_tier),
        display_name: json_to_i18n(&r.display_name),
        description: r.description.as_ref().map(json_to_i18n),
        domain: r.domain.clone(),
        intents: r.intents.as_ref().and_then(json_to_string_array),
        severity: r.severity.as_deref().and_then(parse_severity),
        status: parse_status(&r.status),
        current_version: r.current_version.clone(),
        created_at: r.created_at,
        updated_at: r.updated_at,
        install_count: r.install_count,
        rating_avg: r.rating_avg,
        rating_count: r.rating_count,
        is_installed: r.is_installed,
        publisher_email: r.publisher_email.clone(),
    }
}

fn version_row_to_dto(v: VersionRow) -> ListingVersion {
    ListingVersion {
        listing_id: v.listing_id,
        version: v.version,
        major: v.major,
        minor: v.minor,
        patch: v.patch,
        cedar_text: v.cedar_text,
        manifest: v.manifest,
        policy_tree: v.policy_tree,
        members: v.members.and_then(json_to_members),
        changelog: v.changelog.as_ref().map(json_to_i18n),
        published_at: v.published_at,
    }
}

fn review_row_to_dto(r: &ReviewRow) -> Review {
    Review {
        id: r.id,
        listing_id: r.listing_id,
        user_id: r.user_id.clone(),
        version: r.version.clone(),
        rating: r.rating,
        body: json_to_i18n(&r.body),
        helpful_count: r.helpful_count,
        created_at: r.created_at,
    }
}

fn i18n_to_json(t: &I18nText) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("en".into(), Value::String(t.en.clone()));
    if let Some(ko) = &t.ko {
        m.insert("ko".into(), Value::String(ko.clone()));
    }
    Value::Object(m)
}

fn json_to_i18n(v: &Value) -> I18nText {
    let en = v
        .get("en")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_owned();
    let ko = v.get("ko").and_then(|x| x.as_str()).map(str::to_owned);
    I18nText { en, ko }
}

fn members_to_json(members: &[SetMember]) -> Value {
    serde_json::to_value(members).unwrap_or(Value::Array(Vec::new()))
}

fn json_to_members(v: Value) -> Option<Vec<SetMember>> {
    serde_json::from_value(v).ok()
}

fn json_to_string_array(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|arr| {
        arr.iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect()
    })
}

const fn serde_kind(k: ListingKind) -> &'static str {
    match k {
        ListingKind::Policy => "policy",
        ListingKind::Set => "set",
    }
}

fn parse_kind(s: &str) -> ListingKind {
    match s {
        "set" => ListingKind::Set,
        _ => ListingKind::Policy,
    }
}

const fn serde_tier(t: PublisherTier) -> &'static str {
    match t {
        PublisherTier::Official => "official",
        PublisherTier::Verified => "verified",
        PublisherTier::Community => "community",
    }
}

fn parse_tier(s: &str) -> PublisherTier {
    match s {
        "official" => PublisherTier::Official,
        "verified" => PublisherTier::Verified,
        _ => PublisherTier::Community,
    }
}

const fn serde_severity(s: Severity) -> &'static str {
    match s {
        Severity::Deny => "deny",
        Severity::Warn => "warn",
    }
}

fn parse_severity(s: &str) -> Option<Severity> {
    match s {
        "deny" => Some(Severity::Deny),
        "warn" => Some(Severity::Warn),
        _ => None,
    }
}

fn parse_status(s: &str) -> ListingStatus {
    match s {
        "pending" => ListingStatus::Pending,
        "archived" => ListingStatus::Archived,
        "rejected" => ListingStatus::Rejected,
        _ => ListingStatus::Published,
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn server_error(msg: &str) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, msg.to_owned()).into_response()
}

// `Deserialize` flag used by axum's Query extractor — needed because the
// `Default` impl for `ListListingsQuery` is generated above via #[derive].
#[allow(dead_code)]
#[derive(Deserialize)]
struct _QueryProbe;
