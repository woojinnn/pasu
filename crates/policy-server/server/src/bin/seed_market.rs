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
const OFFICIAL_EMAIL: &str = "official@pasu.seed";

#[derive(Deserialize)]
struct SeedEntry {
    id: String,
    cedar: String,
    manifest: serde_json::Value,
}

/// Embed the phase1 default-policy seed at compile time so the binary stays
/// self-contained — no runtime path arg, no cwd dependence. Generated from
/// `crates/policy-engine/tests/fixtures/default_policies_v2/phase1/` (35 policies,
/// each `{ id, cedar, manifest }`). The curated packages below reference these
/// slugs, so the policy set and the package set stay in lock-step.
const PHASE1_JSON: &str = include_str!("phase1-seed.json");

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
    let entries: Vec<SeedEntry> = serde_json::from_str(PHASE1_JSON)?;
    tracing::info!("loaded {} phase1 entries", entries.len());

    // ── Insert each policy as its own listing ────────────────────────────
    let mut inserted_policies = 0usize;
    for entry in &entries {
        let domain = infer_domain(&entry.id);
        let category = infer_category(&entry.id, &entry.manifest);
        let severity = infer_severity(&entry.id);
        let (en, ko) = derive_display_name(&entry.id);
        let intents = infer_intents(&entry.id);
        let inserted = insert_policy_listing(
            &pool,
            &entry.id,
            domain,
            category,
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
    // Six curated packages from `agentBase/policy-packages/` (phase1 only).
    // Members reference phase1 slugs; each set version snapshots their cedar.
    let curations = [
        Curation {
            slug: "wallet-first-shield",
            name_en: "Wallet First Shield",
            name_ko: "지갑 첫 방패",
            desc_en: "The 10 must-haves for a brand-new trading wallet.",
            desc_ko: "트레이딩 입문자가 가장 먼저 깔아야 할 필수 10종.",
            member_slugs: &[
                "unlimited-approval-deny",
                "increase-allowance-cap-warn",
                "reapprove-already-granted-warn",
                "permit2-sign-allowance-confirm",
                "multicall-hidden-approval-warn",
                "setapprovalforall-operator-warning",
                "send-first-time-or-burn-recipient-warn",
                "swap-recipient-not-self-deny",
                "holding-pct-outflow-warn",
                "unknown-blind-sign-warning",
            ],
        },
        Curation {
            slug: "no-mistake-swap",
            name_en: "No-Mistake Swap",
            name_ko: "노미스 스왑",
            desc_en: "Stop funds leaking to the wrong place in swaps & LP.",
            desc_ko: "스왑·LP에서 돈이 엉뚱한 데로 새는 실수 차단.",
            member_slugs: &[
                "multicall-hidden-approval-warn",
                "transfer-to-token-own-contract-deny",
                "swap-recipient-not-self-deny",
                "values-recipient-denylist-deny",
                "ammlp-collect-recipient-not-self-deny",
                "ammlp-remove-recipient-not-self-deny",
                "holding-pct-outflow-warn",
            ],
        },
        Curation {
            slug: "never-again",
            name_en: "Never Again",
            name_ko: "그날의 해킹",
            desc_en: "Policies that would have stopped real-world mega-hacks.",
            desc_ko: "실제 대형 탈취 사고를 막을 수 있었던 정책집.",
            member_slugs: &[
                "unlimited-approval-deny",
                "bridge-unlimited-approval-deny",
                "permit2-sign-allowance-far-expiry-warn",
                "signature-chain-mismatch-permit-warn",
                "bridge-recipient-not-self-deny",
                "bridge-refund-not-self-warn",
                "bridge-target-not-allowlisted-deny",
            ],
        },
        Curation {
            slug: "nft-vault-guard",
            name_en: "NFT Vault Guard",
            name_ko: "NFT 금고",
            desc_en: "Protect NFTs from collection-wide approvals and burns.",
            desc_ko: "컬렉션 전체 위임·소각 분실로부터 NFT 보호.",
            member_slugs: &[
                "nft-bid-weth-unlimited-warn",
                "multicall-hidden-approval-warn",
                "setapprovalforall-operator-warning",
                "nft-setapprovalforall-conduit-warn",
                "nft-transfer-burn-recipient-deny",
            ],
        },
        Curation {
            slug: "leverage-safety",
            name_en: "Leverage Safety",
            name_ko: "레버리지 세이프티",
            desc_en: "Gate risky moves on Hyperliquid perps.",
            desc_ko: "Hyperliquid 선물 거래의 위험 동작 게이트.",
            member_slugs: &[
                "hl-confirm-approve-agent",
                "hl-confirm-high-leverage",
                "hl-confirm-unknown",
                "hl-confirm-usd-send",
                "hl-confirm-withdraw",
                "hl-no-short-perp",
            ],
        },
        Curation {
            slug: "claim-and-vote-guard",
            name_en: "Claim & Vote Guard",
            name_ko: "클레임 & 보트 가드",
            desc_en: "Block airdrop-claim and governance-delegation phishing.",
            desc_ko: "에어드롭 클레임·거버넌스 위임 피싱 차단.",
            member_slugs: &[
                "air-recipient-not-self-deny",
                "air-delegatee-not-self-deny",
                "air-claim-locks-received-warn",
                "air-merkle-without-proof-warn",
                "gov-delegatee-allowlist-deny",
                "aave-delegate-borrow-allowlist-deny",
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
    category: &str,
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
           domain, category, intents, severity, status, current_version, forked_from,
           created_at, updated_at
         ) VALUES ($1, $2, 'policy', $3, 'official', $4, NULL,
                   $5, $6, $7, $8, 'published', '1.0.0', NULL,
                   $9, $9)
         ON CONFLICT (slug) DO NOTHING",
    )
    .bind(id)
    .bind(slug)
    .bind(OFFICIAL_USER_ID)
    .bind(&display_name)
    .bind(domain)
    .bind(category)
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

/// Infer the action-based `category` (12-value taxonomy) from the policy
/// manifest's `trigger.where.action.tag` (or `action.domain`). This is the
/// authoritative server-side mirror of the dashboard's `categoryOf(slug)`;
/// both must agree so grid counts and filtered lists match.
fn infer_category(slug: &str, manifest: &Value) -> &'static str {
    let where_ = manifest.get("trigger").and_then(|t| t.get("where"));

    // action.tag may be `{ "eq": "x" }` or `{ "in": ["x", …] }`.
    let tag = where_.and_then(|w| w.get("action.tag")).and_then(|t| {
        t.get("eq").and_then(Value::as_str).or_else(|| {
            t.get("in")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(Value::as_str)
        })
    });

    if let Some(t) = tag {
        return match t {
            "erc20_approve" | "nft_set_approval_for_all" => "approvals",
            "permit2_sign_allowance" | "erc20_permit" => "signing",
            "erc20_transfer" | "nft_transfer" => "transfer",
            "swap" => "swap",
            "remove_liquidity" | "collect_fees" => "liquidity",
            "delegate_borrow" => "lending",
            "claim" => "rewards",
            "delegate" => "governance",
            // perp split: position trading vs account ops
            "hl_update_leverage" | "hl_order" | "hl_twap_order" | "hl_unknown" => "derivatives",
            "hl_approve_agent" | "hl_usd_send" | "hl_withdraw" => "perps",
            _ => "others",
        };
    }

    // No action.tag — fall back to action.domain (multicall / unknown).
    let domain = where_
        .and_then(|w| w.get("action.domain"))
        .and_then(|d| d.get("eq"))
        .and_then(Value::as_str);
    match domain {
        Some("multicall") => "approvals",
        Some("unknown") if slug.contains("blind-sign") => "intents",
        _ => "others",
    }
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
    // Named "-warn" but actually a hard block (burn-address sends are permanent
    // loss). See policy-packages README.
    if slug == "send-first-time-or-burn-recipient-warn" {
        return "deny";
    }
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
