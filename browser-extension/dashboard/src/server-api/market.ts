/**
 * Marketplace API client.
 *
 * Mirrors the Rust DTOs in `crates/policy-server/server/src/market_dto.rs`.
 * Listings come in two kinds: 'policy' (a single Cedar policy) and 'set'
 * (a bundle whose member policies are snapshotted inline into each set
 * version). Install is copy-to-editor — receivers get the full body in one
 * payload and write it into their local extension store via the SW bridge.
 */

import { request } from "./client";
import { seedDetail, seedListings } from "./market-seed-beginner";

export type ListingKind = "policy" | "set";
export type PublisherTier = "official" | "verified" | "community";
export type ListingStatus = "pending" | "published" | "archived" | "rejected";
export type Severity = "deny" | "warn";
export type ListingSort = "popular" | "new" | "rating";

/** Two-locale display string. `en` is the canonical fallback. */
export interface I18nText {
  en: string;
  ko?: string;
}

export interface SetMember {
  slug: string;
  display_name: string;
  cedar_text: string;
  manifest?: unknown;
}

export interface ListingSummary {
  id: string;
  slug: string;
  kind: ListingKind;
  publisher_id: string;
  publisher_tier: PublisherTier;
  display_name: I18nText;
  description?: I18nText;
  domain?: string;
  /** Action-based taxonomy (approvals, swap, perps, …); see market-domain. */
  category?: string;
  intents?: string[];
  severity?: Severity;
  status: ListingStatus;
  current_version: string | null;
  created_at: number;
  updated_at: number;
  install_count: number;
  rating_avg: number | null;
  rating_count: number;
  /** True when the currently-authenticated user has installed this listing
   *  at least once (event log row, not state). Drives the 설치/설치됨 badge. */
  is_installed: boolean;
  /** Publisher's email, joined from `users` on read. Use `publisherDisplay`
   *  to derive a human-friendly label (handles the official tier fallback). */
  publisher_email?: string;
}

export interface ListingVersion {
  listing_id: string;
  version: string;
  major: number;
  minor: number;
  patch: number;
  cedar_text?: string;
  manifest?: unknown;
  policy_tree?: string;
  members?: SetMember[];
  changelog?: I18nText;
  published_at: number;
}

export interface Review {
  id: string;
  listing_id: string;
  user_id: string;
  version: string;
  rating: number;
  body: I18nText;
  helpful_count: number;
  created_at: number;
}

export type ReportReason =
  | "unsafe_policy"
  | "misleading"
  | "spam"
  | "abuse"
  | "other";

export type ReportStatus = "open" | "resolved";

export interface MarketReport {
  id: string;
  listing_id?: string;
  review_id?: string;
  reporter_id: string;
  reason: ReportReason;
  details?: string;
  status: ReportStatus;
  resolved_by?: string;
  resolved_at?: number;
  created_at: number;
}

export interface ListingDetail extends ListingSummary {
  latest_version: ListingVersion | null;
  recent_reviews: Review[];
}

export interface ListListingsParams {
  kind?: ListingKind;
  domain?: string;
  category?: string;
  publisher_id?: string;
  publisher_tier?: PublisherTier;
  q?: string;
  sort?: ListingSort;
  limit?: number;
  offset?: number;
}

export interface CreatePolicyListingBody {
  slug: string;
  kind: "policy";
  display_name: I18nText;
  description?: I18nText;
  domain: string;
  intents?: string[];
  severity: Severity;
  version?: string;
  cedar_text: string;
  manifest?: unknown;
  policy_tree?: string;
  changelog?: I18nText;
}

export interface CreateSetListingBody {
  slug: string;
  kind: "set";
  display_name: I18nText;
  description?: I18nText;
  version?: string;
  members: SetMember[];
  changelog?: I18nText;
}

export type CreateListingBody = CreatePolicyListingBody | CreateSetListingBody;

export interface CreateVersionBody {
  version: string;
  cedar_text?: string;
  manifest?: unknown;
  policy_tree?: string;
  members?: SetMember[];
  changelog?: I18nText;
}

export interface CreateReviewBody {
  version: string;
  rating: number;
  body: I18nText;
}

export interface CreateReportBody {
  reason: ReportReason;
  details?: string;
}

export interface ListReportsParams {
  status?: ReportStatus;
  limit?: number;
}

export interface UpdateReportStatusBody {
  status: ReportStatus;
}

/** `GET /market/listings` — browse + filter + sort. */
export async function listListings(
  params: ListListingsParams = {},
): Promise<ListingSummary[]> {
  const search = new URLSearchParams();
  if (params.kind) search.set("kind", params.kind);
  if (params.domain) search.set("domain", params.domain);
  if (params.category) search.set("category", params.category);
  if (params.publisher_id) search.set("publisher_id", params.publisher_id);
  if (params.publisher_tier) search.set("publisher_tier", params.publisher_tier);
  if (params.q) search.set("q", params.q);
  if (params.sort) search.set("sort", params.sort);
  if (params.limit != null) search.set("limit", String(params.limit));
  if (params.offset != null) search.set("offset", String(params.offset));
  const qs = search.toString();
  const path = qs ? `/market/listings?${qs}` : "/market/listings";
  const rows = await request<ListingSummary[]>(path);
  return mergeSeedListings(rows, params); // ⚠️ 임시 시드 폴백 — market-seed-beginner.ts
}

/** One listing's recent-install rollup from `GET /market/activity-summary`.
 * Real install demand within the look-back window — never mocked. The landing
 * hero buckets these by `categoryOf(slug)` to surface "최근 인기" categories. */
export interface InstallActivityEntry {
  slug: string;
  kind: ListingKind;
  display_name: I18nText;
  /** Server action-based category (differs from the dashboard taxonomy). */
  category?: string;
  recent_installs: number;
}

export interface ActivitySummary {
  days: number;
  /** Unix-seconds cutoff actually used (now − days·86400). */
  since: number;
  entries: InstallActivityEntry[];
}

/** `GET /market/activity-summary` — per-listing install counts in the last
 * `days` (default 7), most-installed first. Powers the "최근 인기" hero with
 * real demand data. Returns an empty `entries` list when nothing was installed
 * in the window (the caller then falls back to coverage-based suggestions). */
export async function getActivitySummary(
  params: { days?: number; limit?: number } = {},
): Promise<ActivitySummary> {
  const search = new URLSearchParams();
  if (params.days != null) search.set("days", String(params.days));
  if (params.limit != null) search.set("limit", String(params.limit));
  const qs = search.toString();
  const path = qs ? `/market/activity-summary?${qs}` : "/market/activity-summary";
  try {
    return await request<ActivitySummary>(path);
  } catch {
    // Server unreachable / endpoint absent → no activity signal. The hero
    // falls back to coverage ("미설치 N개"), which is honest with no data.
    const days = params.days ?? 7;
    return { days, since: 0, entries: [] };
  }
}

/** `GET /market/listings/:slug` — listing detail + latest version + recent reviews. */
export async function getListing(slug: string): Promise<ListingDetail> {
  try {
    return await request<ListingDetail>(`/market/listings/${encodeURIComponent(slug)}`);
  } catch (e) {
    // ⚠️ 임시 시드 폴백 — 서버에 없는 slug 면 데모 시드로 본다.
    const seeded = getSeedDetail(slug);
    if (seeded) return seeded;
    throw e;
  }
}

// ════════════════════════════════════════════════════════════════════
// ⚠️ 임시 시드 폴백 (PASU Beginner Pack V1) — 실제 데이터 올라오면 제거.
//    market-seed-beginner.ts 와 이 두 헬퍼, 그리고 listListings/getListing
//    의 호출 지점만 지우면 원복된다.
// ════════════════════════════════════════════════════════════════════

/** 서버 결과가 시드 slug 를 아직 포함하지 않을 때만 시드를 끼워 넣고,
 *  요청 파라미터(kind/category/q/sort/limit)로 시드도 동일하게 거른다. */
function mergeSeedListings(
  rows: ListingSummary[],
  params: ListListingsParams,
): ListingSummary[] {
  const have = new Set(rows.map((r) => r.slug));
  let seed = seedListings().filter((s) => !have.has(s.slug));
  if (seed.length === 0) return rows;

  if (params.kind) seed = seed.filter((s) => s.kind === params.kind);
  if (params.category) seed = seed.filter((s) => s.category === params.category);
  if (params.publisher_tier) seed = seed.filter((s) => s.publisher_tier === params.publisher_tier);
  if (params.q) {
    const q = params.q.toLowerCase();
    seed = seed.filter(
      (s) =>
        s.slug.includes(q) ||
        (s.display_name.ko ?? "").toLowerCase().includes(q) ||
        s.display_name.en.toLowerCase().includes(q),
    );
  }

  let merged = [...rows, ...seed];
  if (params.sort === "new") merged = merged.sort((a, b) => b.created_at - a.created_at);
  else if (params.sort === "rating") merged = merged.sort((a, b) => (b.rating_avg ?? 0) - (a.rating_avg ?? 0));
  else merged = merged.sort((a, b) => b.install_count - a.install_count); // popular(기본)
  if (params.limit != null) merged = merged.slice(0, params.limit);
  return merged;
}

/** 시드 slug 의 상세를 돌려준다(없으면 null). getListing 의 404 폴백. */
function getSeedDetail(slug: string): ListingDetail | null {
  return seedDetail(slug);
}

/** `GET /market/listings/id/:id/versions/:ver` — fetch a specific version body. */
export async function getListingVersion(
  listingId: string,
  version: string,
): Promise<ListingVersion> {
  return request<ListingVersion>(
    `/market/listings/id/${listingId}/versions/${encodeURIComponent(version)}`,
  );
}

/** `POST /market/listings` — publish a new listing + v1.0.0 atomically. */
export async function createListing(
  body: CreateListingBody,
): Promise<ListingSummary> {
  return request<ListingSummary>("/market/listings", { method: "POST", body });
}

/** `DELETE /market/listings/id/:id` — remove a listing the caller published.
 *  Only the publisher can delete; the server cascades versions/installs/reviews. */
export async function deleteListing(listingId: string): Promise<void> {
  await request<void>(`/market/listings/id/${listingId}`, { method: "DELETE" });
}

/** `POST /market/listings/id/:id/versions` — release a new SemVer version. */
export async function createVersion(
  listingId: string,
  body: CreateVersionBody,
): Promise<ListingVersion> {
  return request<ListingVersion>(`/market/listings/id/${listingId}/versions`, {
    method: "POST",
    body,
  });
}

/** `POST /market/listings/id/:id/install` — record install + return version body. */
export async function installListing(
  listingId: string,
  version: string,
): Promise<ListingVersion> {
  return request<ListingVersion>(`/market/listings/id/${listingId}/install`, {
    method: "POST",
    body: { version },
  });
}

/** `GET /market/listings/id/:id/reviews` — full review list. */
export async function listReviews(listingId: string): Promise<Review[]> {
  return request<Review[]>(`/market/listings/id/${listingId}/reviews`);
}

/** `POST /market/listings/id/:id/reviews` — write or replace caller's review. */
export async function createReview(
  listingId: string,
  body: CreateReviewBody,
): Promise<Review> {
  return request<Review>(`/market/listings/id/${listingId}/reviews`, {
    method: "POST",
    body,
  });
}

/** `POST /market/listings/id/:id/report` — report a listing. */
export async function reportListing(
  listingId: string,
  body: CreateReportBody,
): Promise<MarketReport> {
  return request<MarketReport>(`/market/listings/id/${listingId}/report`, {
    method: "POST",
    body,
  });
}

/** `POST /market/reviews/:id/report` — report a review. */
export async function reportReview(
  reviewId: string,
  body: CreateReportBody,
): Promise<MarketReport> {
  return request<MarketReport>(`/market/reviews/${reviewId}/report`, {
    method: "POST",
    body,
  });
}

/** `GET /market/reports/mine` — reports submitted by the caller. */
export async function listMyReports(): Promise<MarketReport[]> {
  return request<MarketReport[]>("/market/reports/mine");
}

/** `GET /market/reports` — admin moderation queue. */
export async function listReports(
  params: ListReportsParams = {},
): Promise<MarketReport[]> {
  const search = new URLSearchParams();
  if (params.status) search.set("status", params.status);
  if (params.limit != null) search.set("limit", String(params.limit));
  const qs = search.toString();
  return request<MarketReport[]>(qs ? `/market/reports?${qs}` : "/market/reports");
}

/** `PATCH /market/reports/:id` — admin moderation status update. */
export async function updateReportStatus(
  reportId: string,
  body: UpdateReportStatusBody,
): Promise<MarketReport> {
  return request<MarketReport>(`/market/reports/${reportId}`, {
    method: "PATCH",
    body,
  });
}

/** `POST /market/reviews/:id/helpful` — idempotent helpful vote. */
export async function voteHelpful(
  reviewId: string,
): Promise<{ newly_voted: boolean }> {
  return request<{ newly_voted: boolean }>(
    `/market/reviews/${reviewId}/helpful`,
    { method: "POST" },
  );
}

/** `POST /market/listings/id/:id/watch` — subscribe to new-version events. */
export async function watchListing(listingId: string): Promise<void> {
  await request<void>(`/market/listings/id/${listingId}/watch`, {
    method: "POST",
  });
}

/** `DELETE /market/listings/id/:id/watch` — cancel subscription. */
export async function unwatchListing(listingId: string): Promise<void> {
  await request<void>(`/market/listings/id/${listingId}/watch`, {
    method: "DELETE",
  });
}

/** `GET /market/watches` — caller's watched listings with stats. */
export async function listWatches(): Promise<ListingSummary[]> {
  return request<ListingSummary[]>("/market/watches");
}

/** Locale-aware fallback for I18nText. Falls back to en when locale is missing. */
export function pickI18n(t: I18nText | undefined, locale: "en" | "ko" = "ko"): string {
  if (!t) return "";
  if (locale === "ko" && t.ko) return t.ko;
  return t.en;
}

/**
 * Human-friendly publisher label. Official listings get a fixed brand name
 * (the seed user's email is `official@dambi.seed`, ugly to render);
 * everyone else gets the email's local part (`alice@example.com` → `alice`).
 */
export function publisherDisplay(
  tier: PublisherTier,
  email: string | undefined,
  locale: "ko" | "en" = "ko",
): string {
  if (tier === "official") {
    return "Wallet Guardians";
  }
  if (!email) return locale === "ko" ? "익명" : "anonymous";
  const at = email.indexOf("@");
  return at > 0 ? email.slice(0, at) : email;
}

/** Format a Unix-seconds timestamp as YYYY-MM-DD in the user's local TZ. */
export function formatYmd(unixSeconds: number): string {
  const d = new Date(unixSeconds * 1000);
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}
