import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router-dom";

import {
  dashboardId,
  dashboardSetId,
  getListing,
  installListing,
  listManagedPolicies,
  pickI18n,
  putPolicy,
  putPolicySet,
  type ListingDetail,
  type SetMember,
} from "../server-api";
import { formatYmd, publisherDisplay } from "../server-api/market";
import { Topbar } from "../shell/Topbar";

import { DomainGlyph, colorOf, domainNameOf } from "./market-domain";
import { useMarketLocale, type MarketLocale } from "./market-locale";

import "./market.css";

/**
 * `/market/:slug` — detail page for a single listing.
 *
 * Install flow (locked design: copy-to-editor):
 *   1. POST /market/listings/id/:id/install → server returns the version body.
 *   2. Client copies the cedar/manifest into chrome.storage.local via the SW
 *      bridge (putPolicy for `policy`, putPolicy×N + putPolicySet for `set`).
 *   3. Navigate to /editor so the user lands on their local copy.
 *
 * Slug collisions on the local side are resolved by suffixing `-2`, `-3`, …
 * until an unused dashboard:: id is found. The user can rename freely after.
 */
export function MarketDetailPage() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const params = useParams<{ slug: string }>();
  const slug = params.slug ? decodeURIComponent(params.slug) : "";
  const [locale, setLocale] = useMarketLocale();

  const detailQ = useQuery({
    queryKey: ["market-listing", slug],
    queryFn: () => getListing(slug),
    enabled: slug.length > 0,
  });

  const [installMsg, setInstallMsg] = useState<string | null>(null);

  const installMut = useMutation({
    mutationFn: async (detail: ListingDetail) => {
      if (!detail.latest_version || !detail.current_version) {
        throw new Error(
          locale === "ko"
            ? "이 listing에는 발행된 버전이 없습니다."
            : "This listing has no published version.",
        );
      }
      const body = await installListing(detail.id, detail.current_version);
      const existing = await listManagedPolicies();
      const existingIds = new Set(existing.map((p) => p.id));

      if (detail.kind === "policy") {
        if (!body.cedar_text) {
          throw new Error("server returned policy version without cedar_text");
        }
        const id = freshLocalId(detail.slug, existingIds, "policy");
        await putPolicy({
          id,
          cedarText: body.cedar_text,
          manifest: body.manifest,
          displayName: pickI18n(detail.display_name, locale) || detail.slug,
          source: "market",
          sourceListingId: detail.id,
          sourceVersion: detail.current_version,
          cat: detail.domain ?? undefined,
          life: "publish",
        });
        return { kind: "policy" as const, id };
      }

      // kind === 'set'
      const members = body.members ?? [];
      if (members.length === 0) {
        throw new Error("server returned set version without members");
      }
      const memberIds: string[] = [];
      for (const m of members) {
        const id = freshLocalId(m.slug, existingIds, "policy");
        await putPolicy({
          id,
          cedarText: m.cedar_text,
          manifest: m.manifest,
          displayName: m.display_name || m.slug,
          source: "market",
          sourceListingId: detail.id,
          sourceVersion: detail.current_version,
          cat: detail.domain ?? undefined,
          life: "publish",
        });
        existingIds.add(id);
        memberIds.push(id);
      }
      const setId = freshLocalId(detail.slug, new Set(), "set");
      await putPolicySet({
        id: setId,
        displayName: pickI18n(detail.display_name, locale) || detail.slug,
        description: pickI18n(detail.description, locale) || undefined,
        memberIds,
        source: "market",
        readOnly: true,
        sourceListingId: detail.id,
        sourceVersion: detail.current_version,
        cat: detail.domain ?? undefined,
      });
      return { kind: "set" as const, id: setId };
    },
    onSuccess: async (result) => {
      await qc.invalidateQueries({ queryKey: ["managed-policies"] });
      await qc.invalidateQueries({ queryKey: ["policy-sets"] });
      await qc.invalidateQueries({ queryKey: ["market-listing", slug] });
      setInstallMsg(
        result.kind === "policy"
          ? locale === "ko"
            ? "정책을 받았습니다. 에디터로 이동합니다."
            : "Policy installed. Heading to editor."
          : locale === "ko"
            ? "패키지와 멤버 정책을 받았습니다. 에디터로 이동합니다."
            : "Package + members installed. Heading to editor.",
      );
      window.setTimeout(() => navigate("/editor"), 800);
    },
  });

  return (
    <>
      <Topbar
        here="Market"
        subtitle={detailQ.data ? pickI18n(detailQ.data.display_name, locale) || detailQ.data.slug : slug || "…"}
        right={
          <>
            <div className="locale-switch" role="group" aria-label="locale">
              <button
                type="button"
                className={`locale-btn${locale === "ko" ? " is-active" : ""}`}
                onClick={() => setLocale("ko")}
              >
                한
              </button>
              <button
                type="button"
                className={`locale-btn${locale === "en" ? " is-active" : ""}`}
                onClick={() => setLocale("en")}
              >
                EN
              </button>
            </div>
            <Link to="/market" className="back-link">
              ← {locale === "ko" ? "마켓 목록" : "Market"}
            </Link>
          </>
        }
      />

      <div className="market-detail-wrap">
        {detailQ.isLoading && <div className="market-status">{locale === "ko" ? "불러오는 중…" : "Loading…"}</div>}
        {detailQ.isError && (
          <div className="market-status market-error">
            {locale === "ko" ? "로드 실패" : "Load failed"}: {(detailQ.error as Error).message}
          </div>
        )}

        {detailQ.data && (
          <DetailBody
            detail={detailQ.data}
            locale={locale}
            installing={installMut.isPending}
            installError={
              installMut.isError ? (installMut.error as Error).message : null
            }
            installMessage={installMsg}
            onInstall={() => installMut.mutate(detailQ.data!)}
          />
        )}
      </div>
    </>
  );
}

function DetailBody({
  detail,
  locale,
  installing,
  installError,
  installMessage,
  onInstall,
}: {
  detail: ListingDetail;
  locale: MarketLocale;
  installing: boolean;
  installError: string | null;
  installMessage: string | null;
  onInstall: () => void;
}) {
  const name = pickI18n(detail.display_name, locale) || detail.slug;
  const desc = pickI18n(detail.description, locale);
  const isSet = detail.kind === "set";
  const color = colorOf(detail.domain);

  return (
    <>
      <div className="md-header">
        <div className="md-icon-large" style={color ? { background: color.soft } : undefined}>
          <DomainGlyph domain={detail.domain} size={26} />
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <h1>{name}</h1>
          <div className="md-publisher-line">
            <span className={`mc-tier tier-${detail.publisher_tier}`}>
              {detail.publisher_tier === "official"
                ? locale === "ko" ? "공식" : "Official"
                : detail.publisher_tier === "verified"
                  ? locale === "ko" ? "검증" : "Verified"
                  : locale === "ko" ? "커뮤니티" : "Community"}
            </span>
            <span className="md-publisher-name">
              {publisherDisplay(detail.publisher_tier, detail.publisher_email, locale)}
            </span>
            <span className="md-publisher-dot">·</span>
            <span className="md-publisher-date">
              {locale === "ko" ? `${formatYmd(detail.created_at)} 발행` : `Published ${formatYmd(detail.created_at)}`}
            </span>
            {detail.updated_at > detail.created_at && (
              <>
                <span className="md-publisher-dot">·</span>
                <span className="md-publisher-date">
                  {locale === "ko"
                    ? `${formatYmd(detail.updated_at)} 갱신`
                    : `Updated ${formatYmd(detail.updated_at)}`}
                </span>
              </>
            )}
          </div>
          <div className="md-meta">
            <span>{isSet
              ? locale === "ko" ? "패키지" : "Package"
              : locale === "ko" ? "정책" : "Policy"}</span>
            {detail.domain && <span>{domainNameOf(detail.domain, locale)}</span>}
            {detail.severity && (
              <span>
                {detail.severity === "deny"
                  ? locale === "ko" ? "차단" : "Block"
                  : locale === "ko" ? "경고" : "Warn"}
              </span>
            )}
            {detail.current_version && <span>v{detail.current_version}</span>}
            <span>{locale === "ko" ? `설치 ${detail.install_count}` : `${detail.install_count} installs`}</span>
            {detail.rating_count > 0 && detail.rating_avg != null && (
              <span>
                ★ {detail.rating_avg.toFixed(1)} ({detail.rating_count})
              </span>
            )}
          </div>
        </div>
        <div className="md-actions">
          <button
            type="button"
            className={detail.is_installed ? "btn-secondary" : "btn-primary"}
            onClick={onInstall}
            disabled={installing || !detail.current_version}
            title={
              detail.is_installed
                ? locale === "ko"
                  ? "이미 받은 listing입니다. 다시 받으면 새 로컬 복사본이 추가됩니다."
                  : "Already installed. Receiving again adds a fresh local copy."
                : undefined
            }
          >
            {installing
              ? locale === "ko" ? "받는 중…" : "Installing…"
              : detail.is_installed
                ? locale === "ko" ? "설치됨" : "Installed"
                : locale === "ko" ? "받기" : "Install"}
          </button>
        </div>
      </div>

      {installMessage && (
        <div className="market-status" style={{ padding: "12px 0", color: "var(--sage-800)" }}>
          {installMessage}
        </div>
      )}
      {installError && (
        <div className="publish-error" style={{ marginBottom: 12 }}>
          {locale === "ko" ? "받기 실패" : "Install failed"}: {installError}
        </div>
      )}

      {desc && (
        <div className="md-section">
          <h2>{locale === "ko" ? "설명" : "Description"}</h2>
          <p style={{ margin: 0, lineHeight: 1.55, color: "var(--slate-700)" }}>{desc}</p>
        </div>
      )}

      {!isSet && detail.latest_version?.cedar_text && (
        <div className="md-section">
          <h2>{locale === "ko" ? "Cedar 원문" : "Cedar source"}</h2>
          <pre className="md-body">{detail.latest_version.cedar_text}</pre>
        </div>
      )}

      {isSet && detail.latest_version?.members && (
        <div className="md-section">
          <h2>
            {locale === "ko" ? "포함 정책" : "Policies in this package"} ({detail.latest_version.members.length})
          </h2>
          <div className="md-members">
            {detail.latest_version.members.map((m, i) => (
              <MemberRow key={`${m.slug}-${i}`} member={m} />
            ))}
          </div>
        </div>
      )}

      <div className="md-section">
        <h2>
          {locale === "ko" ? "최근 리뷰" : "Recent reviews"} ({detail.recent_reviews.length})
        </h2>
        {detail.recent_reviews.length === 0 && (
          <p style={{ color: "var(--slate-500)", fontSize: 13 }}>
            {locale === "ko" ? "아직 리뷰가 없습니다." : "No reviews yet."}
          </p>
        )}
        <div className="md-reviews">
          {detail.recent_reviews.map((r) => (
            <div className="md-review" key={r.id}>
              <div className="md-review-head">
                <span className="md-review-stars">{"★".repeat(r.rating)}</span>
                <span>v{r.version}</span>
                <span>{locale === "ko" ? `도움돼요 ${r.helpful_count}` : `${r.helpful_count} helpful`}</span>
              </div>
              <div className="md-review-body">{pickI18n(r.body, locale)}</div>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}

function MemberRow({ member }: { member: SetMember }) {
  // Members are snapshots in the set version, but if a listing with the
  // same slug exists as a standalone policy on the market, clicking
  // navigates to its detail page. When the slug isn't a standalone
  // listing the link 404s — the seed always publishes each policy as
  // its own listing so this is the common case.
  return (
    <Link
      to={`/market/${encodeURIComponent(member.slug)}`}
      className="md-member md-member-link"
    >
      <div>
        <div className="md-member-name">{member.display_name || member.slug}</div>
        <div className="md-member-slug">{member.slug}</div>
      </div>
      <span className="md-member-arrow" aria-hidden="true">→</span>
    </Link>
  );
}

/** Find an unused local dashboard id by suffixing `-2`, `-3`, … when the
 *  preferred slug already exists. Returns the full `dashboard::…` or
 *  `dashboard-set::…` id ready to pass to the SW bridge. */
function freshLocalId(
  preferredSlug: string,
  existing: Set<string>,
  kind: "policy" | "set",
): string {
  const make = kind === "policy" ? dashboardId : dashboardSetId;
  const sanitized = preferredSlug.replace(/[^A-Za-z0-9_./()-]/g, "-").slice(0, 96);
  if (!existing.has(make(sanitized))) return make(sanitized);
  for (let i = 2; i < 1000; i++) {
    const candidate = `${sanitized}-${i}`;
    if (!existing.has(make(candidate))) return make(candidate);
  }
  return make(`${sanitized}-${Date.now()}`);
}
