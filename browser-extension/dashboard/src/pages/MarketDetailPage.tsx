import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router-dom";

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
import { policyCopy } from "./market-copy";
import { packageCopy } from "./market-package-copy";
import { installListingToEditor } from "./market-install";
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
  const navigate = useNavigate();
  const qc = useQueryClient();
  const params = useParams<{ slug: string }>();
  const slug = params.slug ? decodeURIComponent(params.slug) : "";
  const [locale] = useMarketLocale();

  const detailQ = useQuery({
    queryKey: ["market-listing", slug],
    queryFn: () => getListing(slug),
    enabled: slug.length > 0,
  });

  const [installMsg, setInstallMsg] = useState<string | null>(null);

  const installMut = useMutation({
    mutationFn: (detail: ListingDetail) => installListingToEditor(detail, locale),
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
        showNotifications={false}
        showSearch={false}
        right={
          <Link to="/market" className="back-link">
            ← {locale === "ko" ? "마켓 목록" : "Market"}
          </Link>
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
  const ko = locale === "ko";
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
              <span className="mc-tier tier-verified">{ko ? "검증" : "Verified"}</span>
            )}
            <span className="md-publisher-dot">·</span>
            <span className="md-publisher-date">
              {ko ? `${formatYmd(detail.created_at)} 발행` : `Published ${formatYmd(detail.created_at)}`}
            </span>
            {detail.updated_at > detail.created_at && (
              <>
                <span className="md-publisher-dot">·</span>
                <span className="md-publisher-date">
                  {ko ? `${formatYmd(detail.updated_at)} 갱신` : `Updated ${formatYmd(detail.updated_at)}`}
                </span>
              </>
            )}
          </div>
          <div className="md-meta">
            <span>{isSet ? (ko ? "패키지" : "Package") : ko ? "정책" : "Policy"}</span>
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
            title={
              detail.is_installed
                ? ko
                  ? "이미 받은 listing입니다. 다시 받으면 새 로컬 복사본이 추가됩니다."
                  : "Already installed. Receiving again adds a fresh local copy."
                : undefined
            }
          >
            {installing
              ? ko ? "받는 중…" : "Installing…"
              : detail.is_installed
                ? ko ? "설치됨" : "Installed"
                : ko ? "받기" : "Install"}
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
          {ko ? "받기 실패" : "Install failed"}: {installError}
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
  const ko = locale === "ko";
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
        <h2>{ko ? "리뷰" : "Reviews"} ({detail.rating_count})</h2>
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
        <div className="md-star-pick" role="radiogroup" aria-label={ko ? "별점" : "rating"}>
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
          placeholder={ko ? "한 줄 리뷰를 남겨보세요" : "Leave a short review"}
          maxLength={280}
          autoComplete="off"
          name="market-review"
        />
        <button
          type="submit"
          className="btn-primary md-review-submit"
          disabled={mut.isPending || rating === 0 || !text.trim()}
        >
          {mut.isPending ? (ko ? "등록 중…" : "Posting…") : ko ? "등록" : "Post"}
        </button>
      </form>
      {mut.isError && (
        <div className="publish-error" style={{ marginTop: 8 }}>
          {ko ? "등록 실패" : "Failed"}: {(mut.error as Error).message}
        </div>
      )}

      {detail.recent_reviews.length === 0 ? (
        <p className="md-reviews-empty">
          {ko ? "아직 리뷰가 없습니다. 첫 리뷰를 남겨보세요." : "No reviews yet — be the first."}
        </p>
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
  const ko = locale === "ko";
  const why = pickI18n(detail.description, locale);
  const copy = packageCopy(detail.slug);
  return (
    <div className="md-summary">
      <span className="md-summary-eyebrow">{ko ? "이 패키지가 막는 것" : "What this package blocks"}</span>
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
          <strong>{members.length}</strong> {ko ? "개 정책" : "policies"}
        </span>
      </div>
    </div>
  );
}

function SetDetail({ members, locale }: { members: SetMember[]; locale: MarketLocale }) {
  const ko = locale === "ko";
  const counts = new Map<CategoryKey, number>();
  members.forEach((m) => {
    const c = categoryOf(m.slug);
    counts.set(c, (counts.get(c) ?? 0) + 1);
  });
  const entries = [...counts.entries()].sort((a, b) => b[1] - a[1]);
  return (
    <div className="md-section">
      <h2>{ko ? "상세 설명" : "Details"}</h2>
      <p className="md-detail-text">
        {ko
          ? `이 패키지는 ${members.length}개 정책으로 아래 행위(action)들을 감시합니다. 정책별 요약·코드는 "포함된 정책"에서 펼쳐 볼 수 있습니다.`
          : `This package guards the actions below across ${members.length} policies — expand each under "Policies in this package" for its summary and code.`}
      </p>
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
  const ko = locale === "ko";
  return (
    <div className="md-section">
      <h2>
        {ko ? "포함된 정책" : "Policies in this package"} ({members.length})
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
  const ko = locale === "ko";
  const cedar = detail.latest_version?.cedar_text ?? "";
  const copy = policyCopy(detail.slug);
  const summary = copy?.title || pickI18n(detail.description, locale) || (cedar ? leadingComment(cedar) : "");
  const desc = copy?.what ?? "";
  const sev = cedar ? severityFromCedar(cedar) : detail.severity ?? "deny";
  const cat = categoryOf(detail.slug);
  const proto = protocolOf(detail.slug);
  return (
    <>
      <div className="md-summary">
        <span className="md-summary-eyebrow">{ko ? "이 정책이 막는 것" : "What this blocks"}</span>
        {summary && <p className="md-summary-why">{summary}</p>}
        <div className="md-summary-stats">
          <SeverityBadge sev={sev} locale={locale} />
          <span className="md-stat">{categoryNameOf(cat, locale)}</span>
          {proto && <span className="md-stat">{proto}</span>}
        </div>
      </div>
      {desc && (
        <div className="md-section">
          <h2>{ko ? "상세 설명" : "Details"}</h2>
          <p className="md-detail-text">{desc}</p>
        </div>
      )}
      {cedar && (
        <div className="md-section">
          <h2>{ko ? "내려받는 코드" : "What you install"}</h2>
          <CodeTabs cedar={cedar} manifest={detail.latest_version?.manifest} locale={locale} hideComments />
        </div>
      )}
    </>
  );
}

function MemberRow({ member, locale }: { member: SetMember; locale: MarketLocale }) {
  const ko = locale === "ko";
  const [open, setOpen] = useState(false);
  const sev = severityFromCedar(member.cedar_text);
  const cat = categoryOf(member.slug);
  const proto = protocolOf(member.slug);
  const copy = policyCopy(member.slug);
  const oneLine = copy?.title || leadingComment(member.cedar_text);
  const desc = copy?.what ?? "";

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
        <SeverityBadge sev={sev} locale={locale} />
      </button>
      <div className={`md-member-bodywrap${open ? " is-open" : ""}`}>
        <div className="md-member-bodyinner">
          <div className="md-member-body">
            {desc && <p className="md-member-desc">{desc}</p>}
            <CodeTabs
              cedar={member.cedar_text}
              manifest={member.manifest}
              locale={locale}
              hideComments
            />
            <Link to={`/market/${encodeURIComponent(member.slug)}`} className="md-member-source">
              {ko ? "이 정책 단독 보기 →" : "View this policy →"}
            </Link>
          </div>
        </div>
      </div>
    </div>
  );
}

function SeverityBadge({ sev, locale }: { sev: "deny" | "warn" | "info"; locale: MarketLocale }) {
  const ko = locale === "ko";
  const label =
    sev === "deny" ? (ko ? "차단" : "DENY") : sev === "warn" ? (ko ? "경고" : "WARN") : ko ? "정보" : "INFO";
  return <span className={`md-sev ${sev}`}>{label}</span>;
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
