//! Marketplace DTOs — JSON wire shapes for `/market/...` endpoints.
//!
//! The schema lives in `db/migrations/0002_market.sql`. Listings come in two
//! kinds (`policy` / `set`); a set version stores its member policies inline
//! as snapshots so receivers get a self-contained payload that can be copied
//! into the local editor in one shot.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// `policy` or `set`. Drives which version body fields are populated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListingKind {
    Policy,
    Set,
}

/// Publisher trust tier. Affects ranking and badge rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PublisherTier {
    Official,
    Verified,
    Community,
}

/// Moderation status. Non-`Published` listings are hidden from browse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListingStatus {
    Pending,
    Published,
    Archived,
    Rejected,
}

/// Cedar policy severity. Policy listings carry this; sets do not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Deny,
    Warn,
}

/// Two-locale display string. `en` is the canonical fallback when the
/// requested locale is missing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct I18nText {
    pub en: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ko: Option<String>,
}

/// Sort modes accepted by `GET /market/listings`. `popular` is the default
/// landing sort (the homepage is folded into the browse page).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ListingSort {
    #[default]
    Popular,
    New,
    Rating,
}

/// Query parameters for `GET /market/listings`. All filters are optional.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ListListingsQuery {
    pub kind: Option<ListingKind>,
    pub domain: Option<String>,
    /// Action-based taxonomy filter (approvals, swap, perps, …). See migration
    /// 0003. Independent of `domain`.
    pub category: Option<String>,
    pub publisher_id: Option<String>,
    pub publisher_tier: Option<PublisherTier>,
    /// Substring match against `display_name.en` / `display_name.ko`.
    pub q: Option<String>,
    #[serde(default)]
    pub sort: ListingSort,
    /// Cap at 100 server-side regardless of caller value.
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Listing card payload — everything the browse grid needs without the
/// version body. Stats are computed on read; do not store them.
#[derive(Clone, Debug, Serialize)]
pub struct ListingSummary {
    pub id: Uuid,
    pub slug: String,
    pub kind: ListingKind,
    pub publisher_id: String,
    pub publisher_tier: PublisherTier,
    pub display_name: I18nText,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<I18nText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<Severity>,
    pub status: ListingStatus,
    pub current_version: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub install_count: i64,
    pub rating_avg: Option<f64>,
    pub rating_count: i64,
    /// True when the currently-authenticated user has at least one row in
    /// `market_installs` for this listing.
    pub is_installed: bool,
    /// Publisher's email, joined from `users` on read. The frontend renders
    /// the local part (before `@`) as a display name for non-official
    /// publishers; official listings keep their tier badge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher_email: Option<String>,
}

/// One member policy snapshot inside a set version. The publish-time copy
/// of the member's editor body, so receivers get a self-contained payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetMember {
    pub slug: String,
    pub display_name: String,
    pub cedar_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<Value>,
}

/// Immutable per-version body. For `policy` listings `cedar_text` is set;
/// for `set` listings `members` is set. Server CHECK enforces exactly one.
#[derive(Clone, Debug, Serialize)]
pub struct ListingVersion {
    pub listing_id: Uuid,
    pub version: String,
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cedar_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_tree: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<SetMember>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<I18nText>,
    pub published_at: i64,
}

/// Listing detail — summary + latest version body + recent reviews.
/// Returned by `GET /market/listings/:slug`. The detail page renders
/// from this one response.
#[derive(Clone, Debug, Serialize)]
pub struct ListingDetail {
    #[serde(flatten)]
    pub summary: ListingSummary,
    pub latest_version: Option<ListingVersion>,
    pub recent_reviews: Vec<Review>,
}

/// `POST /market/listings` body — publish a new listing along with its
/// initial v1.0.0 version in one call. Tail fields are gated by `kind`.
#[derive(Clone, Debug, Deserialize)]
pub struct CreateListingReq {
    pub slug: String,
    pub kind: ListingKind,
    pub display_name: I18nText,
    #[serde(default)]
    pub description: Option<I18nText>,

    // Policy-only — required when kind = Policy
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub intents: Option<Vec<String>>,
    #[serde(default)]
    pub severity: Option<Severity>,

    /// Initial `SemVer`. Defaults to "1.0.0" when omitted.
    #[serde(default = "default_initial_version")]
    pub version: String,

    // Body — exactly one shape depending on kind
    #[serde(default)]
    pub cedar_text: Option<String>,
    #[serde(default)]
    pub manifest: Option<Value>,
    #[serde(default)]
    pub policy_tree: Option<String>,
    #[serde(default)]
    pub members: Option<Vec<SetMember>>,

    #[serde(default)]
    pub changelog: Option<I18nText>,
    #[serde(default)]
    pub forked_from: Option<Uuid>,
}

fn default_initial_version() -> String {
    "1.0.0".to_string()
}

/// `POST /market/listings/:id/versions` body — publish a new `SemVer`
/// version of an existing listing. The kind is locked at the listing
/// level; the body fields here must match.
#[derive(Clone, Debug, Deserialize)]
pub struct CreateVersionReq {
    pub version: String,
    #[serde(default)]
    pub cedar_text: Option<String>,
    #[serde(default)]
    pub manifest: Option<Value>,
    #[serde(default)]
    pub policy_tree: Option<String>,
    #[serde(default)]
    pub members: Option<Vec<SetMember>>,
    #[serde(default)]
    pub changelog: Option<I18nText>,
}

/// `POST /market/listings/:id/install` body. The version the client just
/// downloaded — recorded so install counts attribute to the right release.
#[derive(Clone, Debug, Deserialize)]
pub struct CreateInstallReq {
    pub version: String,
}

/// A review row. `helpful_count` is the cached denormalization; the
/// authoritative votes live in `market_review_helpful`.
#[derive(Clone, Debug, Serialize)]
pub struct Review {
    pub id: Uuid,
    pub listing_id: Uuid,
    pub user_id: String,
    pub version: String,
    pub rating: i16,
    pub body: I18nText,
    pub helpful_count: i32,
    pub created_at: i64,
}

/// `POST /market/listings/:id/reviews` body.
#[derive(Clone, Debug, Deserialize)]
pub struct CreateReviewReq {
    pub version: String,
    pub rating: i16,
    pub body: I18nText,
}

/// User-facing reason categories for marketplace reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportReason {
    UnsafePolicy,
    Misleading,
    Spam,
    Abuse,
    Other,
}

/// Moderation status for a marketplace report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Open,
    Resolved,
}

/// A marketplace report submitted by an authenticated user. Exactly one of
/// `listing_id` or `review_id` is set by the creation endpoint.
#[derive(Clone, Debug, Serialize)]
pub struct MarketReport {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listing_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_id: Option<Uuid>,
    pub reporter_id: String,
    pub reason: ReportReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    pub status: ReportStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<i64>,
    pub created_at: i64,
}

/// `POST /market/listings/:id/report` and
/// `POST /market/reviews/:id/report` body.
#[derive(Clone, Debug, Deserialize)]
pub struct CreateReportReq {
    pub reason: ReportReason,
    #[serde(default)]
    pub details: Option<String>,
}

/// `PATCH /market/reports/:id` body.
#[derive(Clone, Debug, Deserialize)]
pub struct UpdateReportStatusReq {
    pub status: ReportStatus,
}

/// Mirror of `market_listings` row used by the publisher's
/// "My Publishes" view. Same shape as `ListingSummary` for now; kept
/// distinct so future fields (draft state, analytics) can land here.
pub type MyListing = ListingSummary;
