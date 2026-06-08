//! Marketplace store — `market_listings` + `market_listing_versions` +
//! installs / reviews / watches.
//!
//! All listings live in one table and discriminate on `kind`. Stats
//! (`install_count`, `rating_avg`) are computed on read; they're not stored
//! as denormalized columns so a missed sync can't make them lie.

// TODO: fill in `# Errors` docs + field/fn docs on the public surface.
// Suppressed for the initial market PR; documentation pass to follow.
#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::missing_docs_in_private_items,
    clippy::doc_markdown,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_arguments,
    clippy::needless_lifetimes,
    clippy::map_unwrap_or,
    clippy::option_if_let_else,
    clippy::redundant_else,
    clippy::derivable_impls,
    clippy::needless_pass_by_value
)]

use std::str::FromStr;

use serde_json::Value;
use sqlx_core::query::query;
use sqlx_core::row::Row;
use sqlx_postgres::{PgPool, PgRow};
use uuid::Uuid;

use crate::error::{DbError, DbResult};

/// Listing row pulled from `market_listings` augmented with computed stats.
/// Mirrors the wire-side `ListingSummary` field-for-field.
#[derive(Clone, Debug)]
pub struct ListingRow {
    pub id: Uuid,
    pub slug: String,
    pub kind: String,
    pub publisher_id: String,
    pub publisher_tier: String,
    pub display_name: Value,
    pub description: Option<Value>,
    pub domain: Option<String>,
    pub category: Option<String>,
    pub intents: Option<Value>,
    pub severity: Option<String>,
    pub status: String,
    pub current_version: Option<String>,
    pub forked_from: Option<Uuid>,
    pub created_at: i64,
    pub updated_at: i64,
    pub install_count: i64,
    pub rating_avg: Option<f64>,
    pub rating_count: i64,
    pub is_installed: bool,
    /// Publisher's email, joined from `users`. NULL only if the row was
    /// orphaned (FK should prevent this; LEFT JOIN keeps reads resilient).
    pub publisher_email: Option<String>,
}

/// Immutable per-version body. `cedar_text` and `members` are mutually
/// exclusive — exactly one is non-NULL per row.
#[derive(Clone, Debug)]
pub struct VersionRow {
    pub listing_id: Uuid,
    pub version: String,
    pub major: i32,
    pub minor: i32,
    pub patch: i32,
    pub cedar_text: Option<String>,
    pub manifest: Option<Value>,
    pub policy_tree: Option<String>,
    pub members: Option<Value>,
    pub changelog: Option<Value>,
    pub published_at: i64,
}

#[derive(Clone, Debug)]
pub struct ReviewRow {
    pub id: Uuid,
    pub listing_id: Uuid,
    pub user_id: String,
    pub version: String,
    pub rating: i16,
    pub body: Value,
    pub helpful_count: i32,
    pub created_at: i64,
}

/// Sort orderings exposed to the browse query. The integer values keep the
/// caller stable when serde maps query strings to the enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListingSort {
    Popular,
    New,
    Rating,
}

/// Browse filters. Every field is optional; absence means "any".
#[derive(Clone, Debug, Default)]
pub struct ListingFilter {
    pub kind: Option<String>,
    pub domain: Option<String>,
    pub category: Option<String>,
    pub publisher_id: Option<String>,
    pub publisher_tier: Option<String>,
    /// Substring match against `display_name` jsonb fields (en + ko).
    pub q: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct VersionBody {
    pub cedar_text: Option<String>,
    pub manifest: Option<Value>,
    pub policy_tree: Option<String>,
    pub members: Option<Value>,
    pub changelog: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct NewListing {
    pub slug: String,
    pub kind: String,
    pub publisher_id: String,
    pub publisher_tier: String,
    pub display_name: Value,
    pub description: Option<Value>,
    pub domain: Option<String>,
    pub category: Option<String>,
    pub intents: Option<Value>,
    pub severity: Option<String>,
    pub forked_from: Option<Uuid>,
    pub initial_version: String,
    pub initial_body: VersionBody,
}

/// Cap server-side regardless of caller value. Browse queries should never
/// return more than this in one shot.
pub const LIST_LIMIT_MAX: i64 = 100;
pub const LIST_LIMIT_DEFAULT: i64 = 30;

/// `SemVer` regex check duplicated from the SQL CHECK so failures surface
/// as a typed error instead of a Postgres constraint violation string.
pub fn validate_semver(v: &str) -> DbResult<(i32, i32, i32)> {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        return Err(DbError::Invariant(format!(
            "version must be MAJOR.MINOR.PATCH (got {v})"
        )));
    }
    let major = i32::from_str(parts[0])
        .map_err(|_| DbError::Invariant(format!("major must be a non-negative integer ({v})")))?;
    let minor = i32::from_str(parts[1])
        .map_err(|_| DbError::Invariant(format!("minor must be a non-negative integer ({v})")))?;
    let patch = i32::from_str(parts[2])
        .map_err(|_| DbError::Invariant(format!("patch must be a non-negative integer ({v})")))?;
    if major < 0 || minor < 0 || patch < 0 {
        return Err(DbError::Invariant(format!(
            "version components must be >= 0 ({v})"
        )));
    }
    Ok((major, minor, patch))
}

/// Browse: filter + sort + paginate. Joins `LATERAL` subqueries for install
/// count and rating so the per-row stats hit the DB in one round-trip.
/// `viewer_id` keys `is_installed` per-caller — pass `None` for unauthenticated
/// reads (the flag comes back `false` for every row).
pub async fn list_listings(
    pool: &PgPool,
    filter: &ListingFilter,
    sort: ListingSort,
    limit: i64,
    offset: i64,
    viewer_id: Option<&str>,
) -> DbResult<Vec<ListingRow>> {
    let limit = limit.clamp(1, LIST_LIMIT_MAX);
    let offset = offset.max(0);

    // Order-by clause is built statically (no SQL injection vector) so the
    // pg planner can pick a real plan instead of treating sort as a param.
    let order = match sort {
        ListingSort::Popular => "stats.install_count DESC, l.created_at DESC",
        ListingSort::New => "l.created_at DESC",
        ListingSort::Rating => {
            "stats.rating_avg DESC NULLS LAST, stats.rating_count DESC, l.created_at DESC"
        }
    };

    let sql = format!(
        "SELECT l.id, l.slug, l.kind, l.publisher_id, l.publisher_tier,
                l.display_name, l.description, l.domain, l.category, l.intents, l.severity,
                l.status, l.current_version, l.forked_from, l.created_at, l.updated_at,
                stats.install_count, stats.rating_avg, stats.rating_count,
                stats.is_installed,
                u.email AS publisher_email
         FROM market_listings l
         LEFT JOIN users u ON u.user_id = l.publisher_id
         CROSS JOIN LATERAL (
           SELECT
             (SELECT COUNT(*) FROM market_installs i WHERE i.listing_id = l.id) AS install_count,
             (SELECT AVG(rating)::float8 FROM market_reviews r WHERE r.listing_id = l.id) AS rating_avg,
             (SELECT COUNT(*) FROM market_reviews r WHERE r.listing_id = l.id) AS rating_count,
             ($9::text IS NOT NULL AND EXISTS (
                SELECT 1 FROM market_installs i
                WHERE i.listing_id = l.id AND i.user_id = $9
             )) AS is_installed
         ) stats
         WHERE l.status = 'published'
           AND ($1::text IS NULL OR l.kind = $1)
           AND ($2::text IS NULL OR l.domain = $2)
           AND ($3::text IS NULL OR l.category = $3)
           AND ($4::text IS NULL OR l.publisher_id = $4)
           AND ($5::text IS NULL OR l.publisher_tier = $5)
           AND ($6::text IS NULL OR
                l.display_name->>'en' ILIKE '%' || $6 || '%' OR
                l.display_name->>'ko' ILIKE '%' || $6 || '%')
         ORDER BY {order}
         LIMIT $7 OFFSET $8"
    );

    let rows = query(&sql)
        .bind(filter.kind.as_deref())
        .bind(filter.domain.as_deref())
        .bind(filter.category.as_deref())
        .bind(filter.publisher_id.as_deref())
        .bind(filter.publisher_tier.as_deref())
        .bind(filter.q.as_deref())
        .bind(limit)
        .bind(offset)
        .bind(viewer_id)
        .fetch_all(pool)
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;

    Ok(rows.iter().map(row_to_listing).collect())
}

pub async fn get_listing_by_slug(
    pool: &PgPool,
    slug: &str,
    viewer_id: Option<&str>,
) -> DbResult<Option<ListingRow>> {
    let row = query(
        "SELECT l.id, l.slug, l.kind, l.publisher_id, l.publisher_tier,
                l.display_name, l.description, l.domain, l.category, l.intents, l.severity,
                l.status, l.current_version, l.forked_from, l.created_at, l.updated_at,
                stats.install_count, stats.rating_avg, stats.rating_count,
                stats.is_installed,
                u.email AS publisher_email
         FROM market_listings l
         LEFT JOIN users u ON u.user_id = l.publisher_id
         CROSS JOIN LATERAL (
           SELECT
             (SELECT COUNT(*) FROM market_installs i WHERE i.listing_id = l.id) AS install_count,
             (SELECT AVG(rating)::float8 FROM market_reviews r WHERE r.listing_id = l.id) AS rating_avg,
             (SELECT COUNT(*) FROM market_reviews r WHERE r.listing_id = l.id) AS rating_count,
             ($2::text IS NOT NULL AND EXISTS (
                SELECT 1 FROM market_installs i
                WHERE i.listing_id = l.id AND i.user_id = $2
             )) AS is_installed
         ) stats
         WHERE l.slug = $1",
    )
    .bind(slug)
    .bind(viewer_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(row.as_ref().map(row_to_listing))
}

pub async fn get_listing_by_id(
    pool: &PgPool,
    id: Uuid,
    viewer_id: Option<&str>,
) -> DbResult<Option<ListingRow>> {
    let row = query(
        "SELECT l.id, l.slug, l.kind, l.publisher_id, l.publisher_tier,
                l.display_name, l.description, l.domain, l.category, l.intents, l.severity,
                l.status, l.current_version, l.forked_from, l.created_at, l.updated_at,
                stats.install_count, stats.rating_avg, stats.rating_count,
                stats.is_installed,
                u.email AS publisher_email
         FROM market_listings l
         LEFT JOIN users u ON u.user_id = l.publisher_id
         CROSS JOIN LATERAL (
           SELECT
             (SELECT COUNT(*) FROM market_installs i WHERE i.listing_id = l.id) AS install_count,
             (SELECT AVG(rating)::float8 FROM market_reviews r WHERE r.listing_id = l.id) AS rating_avg,
             (SELECT COUNT(*) FROM market_reviews r WHERE r.listing_id = l.id) AS rating_count,
             ($2::text IS NOT NULL AND EXISTS (
                SELECT 1 FROM market_installs i
                WHERE i.listing_id = l.id AND i.user_id = $2
             )) AS is_installed
         ) stats
         WHERE l.id = $1",
    )
    .bind(id)
    .bind(viewer_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(row.as_ref().map(row_to_listing))
}

pub async fn get_version(
    pool: &PgPool,
    listing_id: Uuid,
    version: &str,
) -> DbResult<Option<VersionRow>> {
    let row = query(
        "SELECT listing_id, version, major, minor, patch,
                cedar_text, manifest, policy_tree, members, changelog, published_at
         FROM market_listing_versions
         WHERE listing_id = $1 AND version = $2",
    )
    .bind(listing_id)
    .bind(version)
    .fetch_optional(pool)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(row.as_ref().map(row_to_version))
}

pub async fn get_latest_version(pool: &PgPool, listing_id: Uuid) -> DbResult<Option<VersionRow>> {
    let row = query(
        "SELECT listing_id, version, major, minor, patch,
                cedar_text, manifest, policy_tree, members, changelog, published_at
         FROM market_listing_versions
         WHERE listing_id = $1
         ORDER BY major DESC, minor DESC, patch DESC
         LIMIT 1",
    )
    .bind(listing_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(row.as_ref().map(row_to_version))
}

/// Insert the listing row + its initial version row in one transaction. The
/// caller has already validated the `SemVer` + kind/body invariants; this
/// function performs the DB-level CHECK enforcement as a backstop only.
pub async fn create_listing(pool: &PgPool, n: NewListing, now: i64) -> DbResult<ListingRow> {
    let (major, minor, patch) = validate_semver(&n.initial_version)?;
    let id = Uuid::new_v4();

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;

    query(
        "INSERT INTO market_listings (
           id, slug, kind, publisher_id, publisher_tier, display_name, description,
           domain, category, intents, severity, status, current_version, forked_from,
           created_at, updated_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'published', $12, $13, $14, $14)",
    )
    .bind(id)
    .bind(&n.slug)
    .bind(&n.kind)
    .bind(&n.publisher_id)
    .bind(&n.publisher_tier)
    .bind(&n.display_name)
    .bind(n.description.as_ref())
    .bind(n.domain.as_deref())
    .bind(n.category.as_deref())
    .bind(n.intents.as_ref())
    .bind(n.severity.as_deref())
    .bind(&n.initial_version)
    .bind(n.forked_from)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;

    insert_version_row(
        &mut tx,
        id,
        &n.initial_version,
        major,
        minor,
        patch,
        &n.initial_body,
        now,
    )
    .await?;

    tx.commit()
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;

    get_listing_by_id(pool, id, None)
        .await?
        .ok_or_else(|| DbError::Invariant("listing not found after insert".into()))
}

/// Publish a new version on an existing listing. Updates `current_version`
/// to point at the newly inserted row.
pub async fn create_version(
    pool: &PgPool,
    listing_id: Uuid,
    version: &str,
    body: VersionBody,
    now: i64,
) -> DbResult<VersionRow> {
    let (major, minor, patch) = validate_semver(version)?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;

    insert_version_row(
        &mut tx, listing_id, version, major, minor, patch, &body, now,
    )
    .await?;

    query(
        "UPDATE market_listings
         SET current_version = $1, updated_at = $2
         WHERE id = $3",
    )
    .bind(version)
    .bind(now)
    .bind(listing_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;

    tx.commit()
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;

    get_version(pool, listing_id, version)
        .await?
        .ok_or_else(|| DbError::Invariant("version not found after insert".into()))
}

async fn insert_version_row(
    tx: &mut sqlx_core::transaction::Transaction<'_, sqlx_postgres::Postgres>,
    listing_id: Uuid,
    version: &str,
    major: i32,
    minor: i32,
    patch: i32,
    body: &VersionBody,
    now: i64,
) -> DbResult<()> {
    query(
        "INSERT INTO market_listing_versions (
           listing_id, version, major, minor, patch,
           cedar_text, manifest, policy_tree, members, changelog, published_at
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(listing_id)
    .bind(version)
    .bind(major)
    .bind(minor)
    .bind(patch)
    .bind(body.cedar_text.as_deref())
    .bind(body.manifest.as_ref())
    .bind(body.policy_tree.as_deref())
    .bind(body.members.as_ref())
    .bind(body.changelog.as_ref())
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(())
}

/// Record one install event. The same user installing twice writes two rows
/// (event log, not state).
pub async fn record_install(
    pool: &PgPool,
    listing_id: Uuid,
    version: &str,
    user_id: &str,
    now: i64,
) -> DbResult<Uuid> {
    let id = Uuid::new_v4();
    query(
        "INSERT INTO market_installs (id, listing_id, version, user_id, installed_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(listing_id)
    .bind(version)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(id)
}

pub async fn list_reviews(pool: &PgPool, listing_id: Uuid, limit: i64) -> DbResult<Vec<ReviewRow>> {
    let limit = limit.clamp(1, 200);
    let rows = query(
        "SELECT id, listing_id, user_id, version, rating, body, helpful_count, created_at
         FROM market_reviews
         WHERE listing_id = $1
         ORDER BY helpful_count DESC, created_at DESC
         LIMIT $2",
    )
    .bind(listing_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(rows.iter().map(row_to_review).collect())
}

/// Upsert review (one per user per listing). Re-submitting overwrites the
/// previous body / rating; the `helpful_count` is preserved across edits.
pub async fn upsert_review(
    pool: &PgPool,
    listing_id: Uuid,
    user_id: &str,
    version: &str,
    rating: i16,
    body: &Value,
    now: i64,
) -> DbResult<ReviewRow> {
    let id = Uuid::new_v4();
    query(
        "INSERT INTO market_reviews (id, listing_id, user_id, version, rating, body, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (listing_id, user_id) DO UPDATE
         SET version = excluded.version,
             rating = excluded.rating,
             body = excluded.body,
             created_at = excluded.created_at",
    )
    .bind(id)
    .bind(listing_id)
    .bind(user_id)
    .bind(version)
    .bind(rating)
    .bind(body)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;

    let row = query(
        "SELECT id, listing_id, user_id, version, rating, body, helpful_count, created_at
         FROM market_reviews
         WHERE listing_id = $1 AND user_id = $2",
    )
    .bind(listing_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(row_to_review(&row))
}

/// Vote "helpful" on a review. Returns `true` if the vote was newly inserted
/// (caller hadn't voted yet); `false` if it was already there.
pub async fn vote_helpful(
    pool: &PgPool,
    review_id: Uuid,
    user_id: &str,
    now: i64,
) -> DbResult<bool> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;
    let res = query(
        "INSERT INTO market_review_helpful (review_id, user_id, voted_at)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
    )
    .bind(review_id)
    .bind(user_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;

    let inserted = res.rows_affected() > 0;
    if inserted {
        query(
            "UPDATE market_reviews
             SET helpful_count = helpful_count + 1
             WHERE id = $1",
        )
        .bind(review_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;
    }

    tx.commit()
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(inserted)
}

pub async fn watch(pool: &PgPool, user_id: &str, listing_id: Uuid, now: i64) -> DbResult<()> {
    query(
        "INSERT INTO market_watches (user_id, listing_id, subscribed_at)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(listing_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(())
}

pub async fn unwatch(pool: &PgPool, user_id: &str, listing_id: Uuid) -> DbResult<()> {
    query("DELETE FROM market_watches WHERE user_id = $1 AND listing_id = $2")
        .bind(user_id)
        .bind(listing_id)
        .execute(pool)
        .await
        .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(())
}

pub async fn list_watches(pool: &PgPool, user_id: &str) -> DbResult<Vec<ListingRow>> {
    let rows = query(
        "SELECT l.id, l.slug, l.kind, l.publisher_id, l.publisher_tier,
                l.display_name, l.description, l.domain, l.category, l.intents, l.severity,
                l.status, l.current_version, l.forked_from, l.created_at, l.updated_at,
                stats.install_count, stats.rating_avg, stats.rating_count,
                stats.is_installed,
                u.email AS publisher_email
         FROM market_watches w
         JOIN market_listings l ON l.id = w.listing_id
         LEFT JOIN users u ON u.user_id = l.publisher_id
         CROSS JOIN LATERAL (
           SELECT
             (SELECT COUNT(*) FROM market_installs i WHERE i.listing_id = l.id) AS install_count,
             (SELECT AVG(rating)::float8 FROM market_reviews r WHERE r.listing_id = l.id) AS rating_avg,
             (SELECT COUNT(*) FROM market_reviews r WHERE r.listing_id = l.id) AS rating_count,
             EXISTS (
                SELECT 1 FROM market_installs i
                WHERE i.listing_id = l.id AND i.user_id = $1
             ) AS is_installed
         ) stats
         WHERE w.user_id = $1
         ORDER BY w.subscribed_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| DbError::Invariant(e.to_string()))?;
    Ok(rows.iter().map(row_to_listing).collect())
}

fn row_to_listing(row: &PgRow) -> ListingRow {
    ListingRow {
        id: row.get("id"),
        slug: row.get("slug"),
        kind: row.get("kind"),
        publisher_id: row.get("publisher_id"),
        publisher_tier: row.get("publisher_tier"),
        display_name: row.get("display_name"),
        description: row.get("description"),
        domain: row.get("domain"),
        category: row.get("category"),
        intents: row.get("intents"),
        severity: row.get("severity"),
        status: row.get("status"),
        current_version: row.get("current_version"),
        forked_from: row.get("forked_from"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        install_count: row.get("install_count"),
        rating_avg: row.get("rating_avg"),
        rating_count: row.get("rating_count"),
        is_installed: row.get("is_installed"),
        publisher_email: row.get("publisher_email"),
    }
}

fn row_to_version(row: &PgRow) -> VersionRow {
    VersionRow {
        listing_id: row.get("listing_id"),
        version: row.get("version"),
        major: row.get("major"),
        minor: row.get("minor"),
        patch: row.get("patch"),
        cedar_text: row.get("cedar_text"),
        manifest: row.get("manifest"),
        policy_tree: row.get("policy_tree"),
        members: row.get("members"),
        changelog: row.get("changelog"),
        published_at: row.get("published_at"),
    }
}

fn row_to_review(row: &PgRow) -> ReviewRow {
    ReviewRow {
        id: row.get("id"),
        listing_id: row.get("listing_id"),
        user_id: row.get("user_id"),
        version: row.get("version"),
        rating: row.get("rating"),
        body: row.get("body"),
        helpful_count: row.get("helpful_count"),
        created_at: row.get("created_at"),
    }
}
