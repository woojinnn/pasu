//! Marketplace seed — populates `market_listings` + `market_listing_versions`
//! from the bundled phase1A Cedar seed JSON, plus three curated sets that
//! group related policies.
//!
//! Idempotent: each listing is `INSERT … ON CONFLICT (slug) DO NOTHING`.
//! Re-running the binary leaves an already-seeded DB unchanged.
//!
//! Run:
//!   cargo run -p policy-server --bin seed-market
//!
//! Requires the same `DATABASE_URL` as the server itself.

#![allow(clippy::too_many_arguments)]

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{json, Value};
use sqlx_core::query::query;
use sqlx_postgres::PgPool;
use uuid::Uuid;

use policy_db::stores::postgres::connect_pool;
use policy_server::config::ServerConfig;
use tracing_subscriber::EnvFilter;

/// Synthetic user that owns seed listings. `publisher_tier = 'official'` on
/// each row carries the badge; this row in `users` only exists so the FK is
/// satisfied.
const OFFICIAL_USER_ID: &str = "u_seed_official";
const OFFICIAL_EMAIL: &str = "official@scopeball.seed";

#[derive(Deserialize)]
struct SeedEntry {
    id: String,
    cedar: String,
    manifest: serde_json::Value,
}

/// Embed the editor seed JSON at compile time so the binary stays
/// self-contained — no runtime path arg, no cwd dependence.
const PHASE1A_JSON: &str =
    include_str!("../../../../../browser-extension/dashboard/src/pages/editor/phase1A-seed.json");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,policy_server=info")),
        )
        .init();

    let config = ServerConfig::from_env();
    let database_url = config.database_url.as_deref().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "DATABASE_URL is required")
    })?;

    let pool = connect_pool(database_url, 10, std::time::Duration::from_secs(10)).await?;
    tracing::info!("seeding marketplace into {database_url}");

    ensure_official_user(&pool).await?;
    let entries: Vec<SeedEntry> = serde_json::from_str(PHASE1A_JSON)?;
    tracing::info!("loaded {} phase1A entries", entries.len());

    // ── Insert each policy as its own listing ────────────────────────────
    let mut inserted_policies = 0usize;
    for entry in &entries {
        let domain = infer_domain(&entry.id);
        let severity = infer_severity(&entry.id);
        let (en, ko) = derive_display_name(&entry.id);
        let intents = infer_intents(&entry.id);
        let inserted = insert_policy_listing(
            &pool,
            &entry.id,
            domain,
            severity,
            &en,
            &ko,
            &intents,
            &entry.cedar,
            &entry.manifest,
        )
        .await?;
        if inserted {
            inserted_policies += 1;
        }
    }
    tracing::info!("inserted {inserted_policies} new policy listings");

    // ── Insert three curated sets ────────────────────────────────────────
    let known: BTreeSet<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    let curations = [
        Curation {
            slug: "compliance-essentials",
            name_en: "Compliance Essentials",
            name_ko: "컴플라이언스 셋",
            desc_en: "Recipient checks + sanctions baseline for every wallet.",
            desc_ko: "수신자 검증 + 제재 주소 차단 기본 세트.",
            member_slugs: &[
                "air-recipient-not-self-deny",
                "swap-recipient-not-self-deny",
                "bridge-recipient-not-self-deny",
                "values-recipient-denylist-deny",
                "send-first-time-or-burn-recipient-warn",
            ],
        },
        Curation {
            slug: "risk-limits",
            name_en: "Risk Limits",
            name_ko: "리스크 한도",
            desc_en: "Slippage, gas and leverage ceilings to stop runaway losses.",
            desc_ko: "슬리피지·가스·레버리지 상한으로 폭주를 막는 세트.",
            member_slugs: &[
                "swap-slippage-high-warn",
                "gas-cost-usd-cap-deny",
                "gas-cost-ratio-warn",
                "perp-leverage-cap-deny",
                "perp-leverage-increase-warn",
                "holding-pct-outflow-warn",
            ],
        },
        Curation {
            slug: "drainer-shield",
            name_en: "Drainer & Phishing Shield",
            name_ko: "드레이너·피싱 차단",
            desc_en: "Block malicious approvals, signature spoofing, and permit drains.",
            desc_ko: "악성 승인, 서명 위조, permit 드레인을 입구에서 막는 세트.",
            member_slugs: &[
                "air-permit-on-held-token-deny",
                "unlimited-approval-deny",
                "increase-allowance-cap-warn",
                "multicall-hidden-approval-warn",
                "unknown-blind-sign-warning",
                "signature-chain-mismatch-permit-warn",
                "permit-allowance-horizon-warn",
            ],
        },
    ];

    let entries_by_id: std::collections::HashMap<&str, &SeedEntry> =
        entries.iter().map(|e| (e.id.as_str(), e)).collect();

    let mut inserted_sets = 0usize;
    for cur in &curations {
        let members: Vec<Value> = cur
            .member_slugs
            .iter()
            .filter(|s| known.contains(*s))
            .filter_map(|s| {
                entries_by_id.get(*s).map(|entry| {
                    let (en, _) = derive_display_name(&entry.id);
                    json!({
                        "slug": entry.id,
                        "display_name": en,
                        "cedar_text": entry.cedar,
                        "manifest": entry.manifest,
                    })
                })
            })
            .collect();
        if members.is_empty() {
            tracing::warn!("curation '{}' resolved to 0 members; skipping", cur.slug);
            continue;
        }
        let inserted = insert_set_listing(&pool, cur, &members).await?;
        if inserted {
            inserted_sets += 1;
        }
    }
    tracing::info!("inserted {inserted_sets} new set listings");

    tracing::info!("marketplace seed complete");
    Ok(())
}

struct Curation {
    slug: &'static str,
    name_en: &'static str,
    name_ko: &'static str,
    desc_en: &'static str,
    desc_ko: &'static str,
    member_slugs: &'static [&'static str],
}

async fn ensure_official_user(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let now: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX);
    query(
        "INSERT INTO users (user_id, email, provider, created_at, last_login_at)
         VALUES ($1, $2, 'seed', $3, $3)
         ON CONFLICT(email) DO NOTHING",
    )
    .bind(OFFICIAL_USER_ID)
    .bind(OFFICIAL_EMAIL)
    .bind(now)
    .execute(pool)
    .await?;
    tracing::info!("ensured official seed user {OFFICIAL_USER_ID}");
    Ok(())
}

async fn insert_policy_listing(
    pool: &PgPool,
    slug: &str,
    domain: &str,
    severity: &str,
    name_en: &str,
    name_ko: &str,
    intents: &[&str],
    cedar_text: &str,
    manifest: &Value,
) -> Result<bool, Box<dyn std::error::Error>> {
    let id = Uuid::new_v4();
    let now: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX);

    let display_name = json!({ "en": name_en, "ko": name_ko });
    let intents_json = json!(intents);

    let mut tx = pool.begin().await?;
    let inserted = query(
        "INSERT INTO market_listings (
           id, slug, kind, publisher_id, publisher_tier, display_name, description,
           domain, intents, severity, status, current_version, forked_from,
           created_at, updated_at
         ) VALUES ($1, $2, 'policy', $3, 'official', $4, NULL,
                   $5, $6, $7, 'published', '1.0.0', NULL,
                   $8, $8)
         ON CONFLICT (slug) DO NOTHING",
    )
    .bind(id)
    .bind(slug)
    .bind(OFFICIAL_USER_ID)
    .bind(&display_name)
    .bind(domain)
    .bind(&intents_json)
    .bind(severity)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    if inserted.rows_affected() == 0 {
        // Already present — leave as-is.
        tx.commit().await?;
        return Ok(false);
    }

    query(
        "INSERT INTO market_listing_versions (
           listing_id, version, major, minor, patch,
           cedar_text, manifest, policy_tree, members, changelog, published_at
         ) VALUES ($1, '1.0.0', 1, 0, 0, $2, $3, NULL, NULL, NULL, $4)",
    )
    .bind(id)
    .bind(cedar_text)
    .bind(manifest)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

async fn insert_set_listing(
    pool: &PgPool,
    cur: &Curation,
    members: &[Value],
) -> Result<bool, Box<dyn std::error::Error>> {
    let id = Uuid::new_v4();
    let now: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX);

    let display_name = json!({ "en": cur.name_en, "ko": cur.name_ko });
    let description = json!({ "en": cur.desc_en, "ko": cur.desc_ko });
    let members_json = Value::Array(members.to_vec());

    let mut tx = pool.begin().await?;
    let inserted = query(
        "INSERT INTO market_listings (
           id, slug, kind, publisher_id, publisher_tier, display_name, description,
           domain, intents, severity, status, current_version, forked_from,
           created_at, updated_at
         ) VALUES ($1, $2, 'set', $3, 'official', $4, $5,
                   NULL, NULL, NULL, 'published', '1.0.0', NULL,
                   $6, $6)
         ON CONFLICT (slug) DO NOTHING",
    )
    .bind(id)
    .bind(cur.slug)
    .bind(OFFICIAL_USER_ID)
    .bind(&display_name)
    .bind(&description)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    if inserted.rows_affected() == 0 {
        tx.commit().await?;
        return Ok(false);
    }

    query(
        "INSERT INTO market_listing_versions (
           listing_id, version, major, minor, patch,
           cedar_text, manifest, policy_tree, members, changelog, published_at
         ) VALUES ($1, '1.0.0', 1, 0, 0, NULL, NULL, NULL, $2, NULL, $3)",
    )
    .bind(id)
    .bind(&members_json)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// Infer the listing's `domain` from the slug prefix. Mirrors the
/// classification the original glossary used; falls back to "security" so
/// every entry lands in a real bucket.
fn infer_domain(slug: &str) -> &'static str {
    match slug {
        s if s.starts_with("aave-") => "lending",
        s if s.starts_with("air-") || s.starts_with("claim-") => "airdrop",
        s if s.starts_with("ammlp-") || s.starts_with("curve-") => "ammlp",
        s if s.starts_with("bridge-") => "bridge",
        s if s.starts_with("gov-") => "gov",
        s if s.starts_with("hl-") || s.starts_with("perp-") => "perp",
        s if s.starts_with("lp-") => "sale",
        s if s.starts_with("nft-")
            || s.starts_with("seaport-")
            || s.starts_with("setapprovalforall-")
            || s.starts_with("market-order-") =>
        {
            "nft"
        }
        s if s.starts_with("portfolio-")
            || s.starts_with("alloc-")
            || s.starts_with("behav-")
            || s.starts_with("suitability-")
            || s.starts_with("values-") =>
        {
            "portfolio"
        }
        s if s.starts_with("stk-") => "staking",
        s if s.starts_with("swap-") || s.starts_with("intent-") || s.starts_with("large-swap-") => {
            "swap"
        }
        _ => "security",
    }
}

fn infer_severity(slug: &str) -> &'static str {
    if slug.ends_with("-deny") {
        "deny"
    } else {
        // -warn, -warning, anything else: default to warn (only deny gets the
        // hard block; the seed shouldn't surprise-deny).
        "warn"
    }
}

/// Slug heuristics for intent tags. The original glossary tagged each policy
/// with one or more intents; here we recover the obvious ones from substrings.
fn infer_intents(slug: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    if slug.contains("slippage") || slug.contains("price-impact") {
        out.push("slippage");
    }
    if slug.contains("sandwich") {
        out.push("sandwich");
    }
    if slug.contains("permit") || slug.contains("drain") {
        out.push("drainer");
    }
    if slug.contains("phishing") || slug.contains("spoof") || slug.contains("blind-sign") {
        out.push("phishing");
    }
    if slug.contains("recipient") {
        out.push("recipient");
    }
    if slug.contains("approval") || slug.contains("allowance") || slug.contains("approve") {
        out.push("approval");
    }
    if slug.contains("unlimited") {
        out.push("unlimited");
    }
    if slug.contains("liq-")
        || slug.contains("liquidation")
        || slug.contains("leverage")
        || slug.contains("hf-")
        || slug.contains("ltv")
    {
        out.push("liquidation");
    }
    if slug.contains("depeg") || slug.contains("peg") {
        out.push("depeg");
    }
    if slug.contains("denylist") || slug.contains("sanctions") || slug.contains("ofac") {
        out.push("compliance");
    }
    if slug.contains("overtrad") || slug.contains("averaging-down") || slug.contains("fomo") {
        out.push("overtrade");
    }
    out
}

/// Turn `aave-hf-floor-warn` into ("Aave HF Floor Warn", "Aave HF Floor Warn").
/// We don't have authoritative Korean names for every entry, so the ko/en
/// strings match; the locale switcher still works for the marketplace chrome.
fn derive_display_name(slug: &str) -> (String, String) {
    let parts: Vec<String> = slug
        .split('-')
        .map(|w| {
            let upper = w.to_uppercase();
            if matches!(
                upper.as_str(),
                "HF" | "LST" | "WETH" | "AAVE" | "NFT" | "USDC" | "LP" | "AMM" | "LTV" | "DEX"
            ) {
                upper
            } else {
                let mut chars = w.chars();
                match chars.next() {
                    None => String::new(),
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                }
            }
        })
        .collect();
    let name = parts.join(" ");
    (name.clone(), name)
}
