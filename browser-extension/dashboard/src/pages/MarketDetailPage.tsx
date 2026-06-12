import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "react-router-dom";
import { useTranslation } from "react-i18next";

import {
  createReview,
  getListing,
  pickI18n,
  type ListingDetail,
  type SetMember,
} from "../server-api";
import { formatYmd, publisherDisplay } from "../server-api/market";
import { Topbar } from "../shell/Topbar";

import {
  CATEGORY_COLOR,
  CategoryGlyph,
  categoryNameOf,
  categoryOf,
  type CategoryKey,
} from "./market-domain";
import { CodeTabs, leadingComment } from "./market-code";
import { packageCopy } from "./market-package-copy";
import { MarketInstallModal } from "./MarketInstallModal";
import { severityFromCedar } from "./editor/policy-meta";
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
  const params = useParams<{ slug: string }>();
  const slug = params.slug ? decodeURIComponent(params.slug) : "";
  const [locale] = useMarketLocale();
  const { t } = useTranslation("market");

  const detailQ = useQuery({
    queryKey: ["market-listing", slug],
    queryFn: () => getListing(slug),
    enabled: slug.length > 0,
  });

  // 설치는 공용 MarketInstallModal(범위 선택 + ps2:install-market)이 수행한다.
  const [installOpen, setInstallOpen] = useState(false);

  return (
    <>
      <Topbar
        here="Market"
        subtitle={detailQ.data ? pickI18n(detailQ.data.display_name, locale) || detailQ.data.slug : slug || "…"}
        showNotifications={false}
        showSearch={false}
        right={
          <Link to="/market" className="back-link">
            ← {t("detail.backToMarket")}
          </Link>
        }
      />

      <div className="market-detail-wrap">
        {detailQ.isLoading && <div className="market-status">{t("common:loading")}</div>}
        {detailQ.isError && (
          <div className="market-status market-error">
            {t("detail.loadFailed")}: {(detailQ.error as Error).message}
          </div>
        )}

        {detailQ.data && (
          <DetailBody
            detail={detailQ.data}
            locale={locale}
            installing={false}
            installError={null}
            installMessage={null}
            onInstall={() => setInstallOpen(true)}
          />
        )}
      </div>

      {installOpen && detailQ.data && (
        <MarketInstallModal
          listing={detailQ.data}
          locale={locale}
          onClose={() => setInstallOpen(false)}
        />
      )}
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
  const { t } = useTranslation("market");
  const name = pickI18n(detail.display_name, locale) || detail.slug;
  const isSet = detail.kind === "set";
  const members = isSet ? detail.latest_version?.members ?? [] : [];
  const cat = !isSet ? categoryOf(detail.slug) : null;
  const catColor = cat ? CATEGORY_COLOR[cat] : null;

  return (
    <>
      <div className="md-header">
        <div className="md-icon-large" style={catColor ? { background: catColor.soft } : undefined}>
          {isSet ? (
            <PackageGlyphLg />
          ) : cat ? (
            <CategoryGlyph category={cat} size={26} color={catColor!.hex} />
          ) : null}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <h1>{name}</h1>
          <div className="md-publisher-line">
            <span className="md-publisher-name">
              {publisherDisplay(detail.publisher_tier, detail.publisher_email, locale)}
              {detail.publisher_tier === "official" && (
                <span className="mc-verified" title="Verified" aria-label="verified">✓</span>
              )}
            </span>
            {detail.publisher_tier === "verified" && (
              <span className="mc-tier tier-verified">{t("detail.verifiedTier")}</span>
            )}
            <span className="md-publisher-dot">·</span>
            <span className="md-publisher-date">
              {t("detail.published", { date: formatYmd(detail.created_at) })}
            </span>
            {detail.updated_at > detail.created_at && (
              <>
                <span className="md-publisher-dot">·</span>
                <span className="md-publisher-date">
                  {t("detail.updated", { date: formatYmd(detail.updated_at) })}
                </span>
              </>
            )}
          </div>
          <div className="md-meta">
            <span>{isSet ? t("kind.package") : t("kind.policy")}</span>
            {!isSet && cat && <span>{categoryNameOf(cat, locale)}</span>}
            {detail.current_version && <span>v{detail.current_version}</span>}
            <span className="md-installs">
              <svg
                width="13"
                height="13"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
                strokeLinecap="round"
                strokeLinejoin="round"
                aria-hidden="true"
              >
                <path d="M12 3v12M7 10l5 5 5-5M5 21h14" />
              </svg>
              {detail.install_count}
            </span>
            {detail.rating_count > 0 && detail.rating_avg != null && (
              <span>★ {detail.rating_avg.toFixed(1)} ({detail.rating_count})</span>
            )}
          </div>
        </div>
        <div className="md-actions">
          <button
            type="button"
            className={detail.is_installed ? "btn-secondary" : "btn-primary"}
            onClick={onInstall}
            disabled={installing || !detail.current_version}
            title={detail.is_installed ? t("detail.alreadyInstalledTitle") : undefined}
          >
            {installing
              ? t("install.installing")
              : detail.is_installed
                ? t("install.installed")
                : t("install.get")}
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
          {t("install.failed")}: {installError}
        </div>
      )}

      {isSet ? (
        <>
          <SetSummary detail={detail} members={members} locale={locale} />
          <SetDetail members={members} locale={locale} />
          <IncludedPolicies members={members} locale={locale} />
        </>
      ) : (
        <PolicyDetailBody detail={detail} locale={locale} />
      )}

      <Reviews detail={detail} locale={locale} />
    </>
  );
}

function Reviews({ detail, locale }: { detail: ListingDetail; locale: MarketLocale }) {
  const { t } = useTranslation("market");
  const qc = useQueryClient();
  const [rating, setRating] = useState(0);
  const [hover, setHover] = useState(0);
  const [text, setText] = useState("");
  const mut = useMutation({
    mutationFn: () =>
      createReview(detail.id, {
        version: detail.current_version ?? "1.0.0",
        rating,
        body: { en: text.trim(), ko: text.trim() },
      }),
    onSuccess: async () => {
      setRating(0);
      setText("");
      await qc.invalidateQueries({ queryKey: ["market-listing", detail.slug] });
    },
  });
  const avg = detail.rating_avg;

  return (
    <div className="md-section">
      <div className="md-reviews-head">
        <h2>{t("reviews.title")} ({detail.rating_count})</h2>
        {detail.rating_count > 0 && avg != null && (
          <span className="md-rating-total">
            <span className="md-rating-star">★</span>
            <strong>{avg.toFixed(1)}</strong>
            <span className="md-rating-of"> / 5</span>
          </span>
        )}
      </div>

      <form
        className="md-review-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (rating > 0 && text.trim()) mut.mutate();
        }}
      >
        <div className="md-star-pick" role="radiogroup" aria-label={t("reviews.ratingAria")}>
          {[1, 2, 3, 4, 5].map((s) => (
            <button
              type="button"
              key={s}
              className={`md-star${s <= (hover || rating) ? " on" : ""}`}
              onClick={() => setRating(s)}
              onMouseEnter={() => setHover(s)}
              onMouseLeave={() => setHover(0)}
              aria-label={`${s}`}
            >
              ★
            </button>
          ))}
        </div>
        <input
          className="md-review-input"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={t("reviews.placeholder")}
          maxLength={280}
          autoComplete="off"
          name="market-review"
        />
        <button
          type="submit"
          className="btn-primary md-review-submit"
          disabled={mut.isPending || rating === 0 || !text.trim()}
        >
          {mut.isPending ? t("reviews.posting") : t("reviews.post")}
        </button>
      </form>
      {mut.isError && (
        <div className="publish-error" style={{ marginTop: 8 }}>
          {t("reviews.failed")}: {(mut.error as Error).message}
        </div>
      )}

      {detail.recent_reviews.length === 0 ? (
        <p className="md-reviews-empty">{t("reviews.empty")}</p>
      ) : (
        <div className="md-reviews">
          {detail.recent_reviews.map((r) => (
            <div className="md-review" key={r.id}>
              <span className="md-review-stars">
                {"★".repeat(r.rating)}
                <span className="md-review-stars-off">{"★".repeat(5 - r.rating)}</span>
              </span>
              <span className="md-review-text">{pickI18n(r.body, locale)}</span>
              <span className="md-review-ver">v{r.version}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── package detail sections ───────────────────────────────────────────────

function SetSummary({
  detail,
  members,
  locale,
}: {
  detail: ListingDetail;
  members: SetMember[];
  locale: MarketLocale;
}) {
  const { t } = useTranslation("market");
  const why = pickI18n(detail.description, locale);
  const copy = packageCopy(detail.slug);
  return (
    <div className="md-summary">
      <span className="md-summary-eyebrow">{t("detail.packageBlocks")}</span>
      {(copy?.intro || why) && <p className="md-summary-why">{copy?.intro || why}</p>}
      {copy && copy.blocks.length > 0 && (
        <ul className="md-blocklist">
          {copy.blocks.map((b, i) => (
            <li key={i} className="md-block">
              <span className="md-block-x" aria-hidden="true">✕</span>
              <span>
                <strong>{b.t}</strong>
                {b.d && <span className="md-block-d"> — {b.d}</span>}
              </span>
            </li>
          ))}
        </ul>
      )}
      <div className="md-summary-stats">
        <span className="md-stat">
          <strong>{members.length}</strong> {t("unit.policies")}
        </span>
      </div>
    </div>
  );
}

function SetDetail({ members, locale }: { members: SetMember[]; locale: MarketLocale }) {
  const { t } = useTranslation("market");
  const counts = new Map<CategoryKey, number>();
  members.forEach((m) => {
    const c = categoryOf(m.slug);
    counts.set(c, (counts.get(c) ?? 0) + 1);
  });
  const entries = [...counts.entries()].sort((a, b) => b[1] - a[1]);
  return (
    <div className="md-section">
      <h2>{t("detail.details")}</h2>
      <p className="md-detail-text">{t("detail.packageDetailText", { n: members.length })}</p>
      <div className="md-cat-coverage">
        {entries.map(([c, n]) => (
          <span
            key={c}
            className="md-cov-chip"
            style={{ background: CATEGORY_COLOR[c].soft, color: CATEGORY_COLOR[c].ink }}
          >
            <CategoryGlyph category={c} size={13} color={CATEGORY_COLOR[c].hex} />
            {categoryNameOf(c, locale)} {n}
          </span>
        ))}
      </div>
    </div>
  );
}

function IncludedPolicies({ members, locale }: { members: SetMember[]; locale: MarketLocale }) {
  const { t } = useTranslation("market");
  return (
    <div className="md-section">
      <h2>
        {t("detail.includedPolicies")} ({members.length})
      </h2>
      <div className="md-members">
        {members.map((m, i) => (
          <MemberRow key={`${m.slug}-${i}`} member={m} locale={locale} />
        ))}
      </div>
    </div>
  );
}

function PolicyDetailBody({ detail, locale }: { detail: ListingDetail; locale: MarketLocale }) {
  const { t, i18n } = useTranslation("market");
  const cedar = detail.latest_version?.cedar_text ?? "";
  const hasCopy = i18n.exists(`market:policy.${detail.slug}.title`);
  const summary =
    (hasCopy ? t(`policy.${detail.slug}.title`) : "") ||
    pickI18n(detail.description, locale) ||
    (cedar ? leadingComment(cedar) : "");
  const desc = hasCopy ? t(`policy.${detail.slug}.what`) : "";
  const sev = cedar ? severityFromCedar(cedar) : detail.severity ?? "deny";
  const cat = categoryOf(detail.slug);
  const proto = protocolOf(detail.slug);
  return (
    <>
      <div className="md-summary">
        <span className="md-summary-eyebrow">{t("detail.policyBlocks")}</span>
        {summary && <p className="md-summary-why">{summary}</p>}
        <div className="md-summary-stats">
          <SeverityBadge sev={sev} />
          <span className="md-stat">{categoryNameOf(cat, locale)}</span>
          {proto && <span className="md-stat">{proto}</span>}
        </div>
      </div>
      {desc && (
        <div className="md-section">
          <h2>{t("detail.details")}</h2>
          <p className="md-detail-text">{desc}</p>
        </div>
      )}
      {cedar && (
        <div className="md-section">
          <h2>{t("detail.whatYouInstall")}</h2>
          <CodeTabs cedar={cedar} manifest={detail.latest_version?.manifest} hideComments />
        </div>
      )}
    </>
  );
}

function MemberRow({ member, locale }: { member: SetMember; locale: MarketLocale }) {
  const { t, i18n } = useTranslation("market");
  const [open, setOpen] = useState(false);
  const sev = severityFromCedar(member.cedar_text);
  const cat = categoryOf(member.slug);
  const proto = protocolOf(member.slug);
  const hasCopy = i18n.exists(`market:policy.${member.slug}.title`);
  const oneLine = (hasCopy ? t(`policy.${member.slug}.title`) : "") || leadingComment(member.cedar_text);
  const desc = hasCopy ? t(`policy.${member.slug}.what`) : "";

  return (
    <div className={`md-member-v2${open ? " is-open" : ""}`}>
      <button
        type="button"
        className={`md-member-head${open ? " is-open" : ""}`}
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
      >
        <span className="md-member-chev" aria-hidden="true">›</span>
        <span className="md-member-main">
          <span className="md-member-titlerow">
            <span className="md-member-title">{member.display_name || member.slug}</span>
            <span className="md-chip">{categoryNameOf(cat, locale)}</span>
            {proto && <span className="md-chip md-chip-proto">{proto}</span>}
          </span>
          {oneLine && <span className="md-member-oneline">{oneLine}</span>}
        </span>
        <SeverityBadge sev={sev} />
      </button>
      <div className={`md-member-bodywrap${open ? " is-open" : ""}`}>
        <div className="md-member-bodyinner">
          <div className="md-member-body">
            {desc && <p className="md-member-desc">{desc}</p>}
            <CodeTabs cedar={member.cedar_text} manifest={member.manifest} hideComments />
            <Link to={`/market/${encodeURIComponent(member.slug)}`} className="md-member-source">
              {t("detail.viewPolicyAlone")}
            </Link>
          </div>
        </div>
      </div>
    </div>
  );
}

function SeverityBadge({ sev }: { sev: "deny" | "warn" | "info" }) {
  const { t } = useTranslation("market");
  return <span className={`md-sev ${sev}`}>{t(`severityBadge.${sev}`)}</span>;
}

function PackageGlyphLg() {
  return (
    <svg
      width="26"
      height="26"
      viewBox="0 0 24 24"
      fill="none"
      stroke="var(--slate-500)"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 8l9-5 9 5-9 5-9-5zM3 8v8l9 5 9-5V8M12 13v8" />
    </svg>
  );
}

const PROTOCOL: Record<string, string> = {
  hl: "Hyperliquid",
  aave: "Aave",
  permit2: "Permit2",
  seaport: "Seaport",
};
function protocolOf(slug: string): string | undefined {
  return PROTOCOL[slug.split("-")[0]];
}
