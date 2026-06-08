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
  return request<ListingSummary[]>(path);
}

/** `GET /market/listings/:slug` — listing detail + latest version + recent reviews. */
export async function getListing(slug: string): Promise<ListingDetail> {
  return request<ListingDetail>(`/market/listings/${encodeURIComponent(slug)}`);
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
 * (the seed user's email is `official@pasu.seed`, ugly to render);
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
