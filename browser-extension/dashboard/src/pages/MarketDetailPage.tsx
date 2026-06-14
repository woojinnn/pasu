import { useEffect, useState } from "react";
import { useMutation, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "react-router-dom";

import {
  createReview,
  getListing,
  listListings,
  pickI18n,
  type ListingDetail,
  type SetMember,
} from "../server-api";
import { formatYmd, publisherDisplay } from "../server-api/market";
import { MarketPagehead, useMarketContentClass } from "./MarketPagehead";

import {
  CATEGORY_COLOR,
  CategoryGlyph,
  categoryNameOf,
  categoryOf,
  type CategoryKey,
} from "./market-domain";
import { CodeTabs, leadingComment } from "./market-code";
import { textToBlocks } from "../cedar";
import { PolicyDiagram } from "../cedar/diagram/PolicyDiagram";
import type { PolicyIR } from "../cedar/blocks";
import { policyCopy } from "./market-copy";
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

  const detailQ = useQuery({
    queryKey: ["market-listing", slug],
    queryFn: () => getListing(slug),
    enabled: slug.length > 0,
  });

  // 설치는 공용 MarketInstallModal(범위 선택 + ps2:install-market)이 수행한다.
  const [installOpen, setInstallOpen] = useState(false);

  // Market route owns its frame (prototype `.app-content { padding: 0 }`).
  useMarketContentClass();

  // Prototype detail = shell(<listing name>, {act:home, "← 마켓 목록"}, …):
  // `.rm-pagehead` crumb is the listing's display name, back returns to the
  // list view (label says 목록, so route to ?view=list, not the bare home).
  const crumb = detailQ.data
    ? pickI18n(detailQ.data.display_name, locale) || detailQ.data.slug
    : slug || "…";
  return (
    <>
      <MarketPagehead
        crumb={crumb}
        back={{ to: "/market?view=list", label: locale === "ko" ? "← 마켓 목록" : "← Market" }}
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
  const ko = locale === "ko";
  const name = pickI18n(detail.display_name, locale) || detail.slug;
  const isSet = detail.kind === "set";
  const members = isSet ? detail.latest_version?.members ?? [] : [];
  const cat = !isSet ? categoryOf(detail.slug) : null;
  const catColor = cat ? CATEGORY_COLOR[cat] : null;

  return (
    <div className="rm-detail">
      <div className="rm-md-head">
        <div className="rm-md-icon" style={catColor ? { background: catColor.soft } : { background: "var(--warn-100)" }}>
          {isSet ? (
            <PackageGlyphLg />
          ) : cat ? (
            <CategoryGlyph category={cat} size={26} color={catColor!.hex} />
          ) : null}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <h1>{name}</h1>
          <div className="rm-md-pub">
            <span className="name">
              {publisherDisplay(detail.publisher_tier, detail.publisher_email, locale)}
              {detail.publisher_tier === "official" && (
                <span className="rm-vf" title="Verified" aria-label="verified">✓</span>
              )}
            </span>
            {detail.publisher_tier === "verified" && (
              <span className="mc-tier tier-verified">{ko ? "검증" : "Verified"}</span>
            )}
            <span>·</span>
            <span>
              {ko ? `${formatYmd(detail.created_at)} 발행` : `Published ${formatYmd(detail.created_at)}`}
            </span>
            {detail.updated_at > detail.created_at && (
              <>
                <span>·</span>
                <span>
                  {ko ? `${formatYmd(detail.updated_at)} 갱신` : `Updated ${formatYmd(detail.updated_at)}`}
                </span>
              </>
            )}
          </div>
          <div className="rm-md-meta">
            <span>{isSet ? (ko ? "패키지" : "Package") : ko ? "정책" : "Policy"}</span>
            {!isSet && cat && <span>{categoryNameOf(cat, locale)}</span>}
            {detail.current_version && <span>v{detail.current_version}</span>}
            <span className="rm-installs">
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
              <span className="rm-rating"><span className="s">★</span> {detail.rating_avg.toFixed(1)} ({detail.rating_count})</span>
            )}
          </div>
        </div>
        <div className="rm-md-actions">
          <button
            type="button"
            className={`rm-badge${detail.is_installed ? " installed" : ""}`}
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
          <IncludedPolicies members={members} locale={locale} />
        </>
      ) : (
        <PolicyDetailBody detail={detail} locale={locale} />
      )}

      <Reviews detail={detail} locale={locale} />
    </div>
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
  const [revOpen, setRevOpen] = useState(false);
  const avg = detail.rating_avg;
  const hasReviews = detail.rating_count > 0 && avg != null;
  // 시드(폴백) listing 은 서버에 실제 row 가 없어 createReview 가 400/404 가 된다
  // (id 가 UUID 가 아닌 "seed-…"). 이 경우 작성란을 읽기 전용으로 막아 에러를
  // 원천 차단한다. 백엔드에 실제 시드가 올라오면 id 가 UUID 라 자동 활성화.
  const isSeed = detail.id.startsWith("seed-");
  const canSubmit = !isSeed && rating > 0 && text.trim().length > 0;
  const submit = () => {
    if (canSubmit && !mut.isPending) mut.mutate();
  };

  // 별점 분포 — 실제 recent_reviews 표본 우선, 없으면 평균 기반 합성.
  const dist = hasReviews ? ratingDist(avg!, detail.recent_reviews.map((r) => r.rating)) : [];
  const pctOf = new Map(dist.map((d) => [d.star, d.pct]));

  return (
    <div className="rm-sec rm-reviews">
      <div className="rm-rev-head">
        <h2>Review</h2>
        <span className="div">|</span>
        <span className="sub">
          {hasReviews
            ? ko ? `평가 ${detail.rating_count.toLocaleString()}명` : `${detail.rating_count.toLocaleString()} ratings`
            : ko ? "아직 평가 없음" : "No ratings yet"}
        </span>
      </div>

      <div className="rm-rev-bar">
        {hasReviews ? (
          <button
            type="button"
            className={`rm-rev-toggle${revOpen ? " open" : ""}`}
            onClick={() => setRevOpen((o) => !o)}
            title={ko ? "평점을 눌러 리뷰 보기" : "Toggle reviews"}
          >
            <span className="sc">{avg!.toFixed(1)}</span>
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} aria-hidden="true">
              <path d="M6 9l6 6 6-6" />
            </svg>
          </button>
        ) : (
          <span className="rm-rev-toggle empty"><span className="sc">{ko ? "신규" : "New"}</span></span>
        )}

        {hasReviews ? (
          <span className="rm-rev-stars" aria-label={`${avg!.toFixed(1)} / 5`}>
            {[1, 2, 3, 4, 5].map((i) => {
              const fill = Math.max(0, Math.min(1, avg! - (i - 1))) * 100;
              return (
                <span key={i} className="s" data-tip={`${i}${ko ? "점" : "★"} · ${pctOf.get(i) ?? 0}%`}>
                  <span className="bg">★</span>
                  <span className="fg" style={{ width: `${fill}%` }}>★</span>
                </span>
              );
            })}
          </span>
        ) : (
          <span className="rm-rev-stars muted">
            {[1, 2, 3, 4, 5].map((i) => (
              <span key={i} className="s"><span className="bg">★</span></span>
            ))}
          </span>
        )}

        {/* 리뷰 작성 — 별점 picker 를 input 앞쪽에 통합해(rm-rev-input-wrap)
            원본 rm-rev-bar 의 4요소 리듬([토글][stars][입력][등록])을 지키면서
            실제 createReview 별점 입력 기능을 살린다. */}
        <span className="rm-review-input rm-rev-input-wrap">
          <span className="rm-rev-starpick" role="radiogroup" aria-label={ko ? "별점" : "rating"}>
            {[1, 2, 3, 4, 5].map((s) => (
              <button
                type="button"
                key={s}
                className={`rm-starpick-st${!isSeed && s <= (hover || rating) ? " on" : ""}`}
                onClick={() => !isSeed && setRating(s)}
                onMouseEnter={() => !isSeed && setHover(s)}
                onMouseLeave={() => setHover(0)}
                disabled={isSeed}
                aria-label={`${s}`}
              >
                ★
              </button>
            ))}
          </span>
          <input
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") submit(); }}
            placeholder={
              isSeed
                ? ko ? "데모 패키지 — 리뷰는 게시 후 작성할 수 있어요" : "Demo package — reviews open after publish"
                : ko ? "한 줄 리뷰를 남겨보세요" : "Leave a short review"
            }
            readOnly={isSeed}
            maxLength={280}
            autoComplete="off"
            name="market-review"
          />
        </span>
        {!isSeed && (
          <button type="button" className="rm-badge" onClick={submit} disabled={mut.isPending || !canSubmit}>
            {mut.isPending ? (ko ? "등록 중…" : "Posting…") : ko ? "등록" : "Post"}
          </button>
        )}
      </div>

      {mut.isError && (
        <div className="publish-error" style={{ marginTop: 8 }}>
          {ko ? "등록 실패" : "Failed"}: {(mut.error as Error).message}
        </div>
      )}

      {revOpen && detail.recent_reviews.length > 0 && (
        <div className="rm-rev-list">
          {detail.recent_reviews.map((r) => {
            const who = r.user_id.slice(0, 6);
            return (
              <div className="rm-rev-item" key={r.id}>
                <span className="rm-rev-av" style={{ background: "var(--slate-500)" }}>
                  {who.slice(0, 1).toUpperCase()}
                </span>
                <div className="bd">
                  <div className="hd">
                    <span className="nm">{who}</span>
                    <span className="stars">
                      <span className="on">{"★".repeat(r.rating)}</span>
                      <span className="off">{"★".repeat(5 - r.rating)}</span>
                    </span>
                    <span className="dt">{formatYmd(r.created_at)}</span>
                    <span className="vc">v{r.version}</span>
                  </div>
                  <div className="tx">{pickI18n(r.body, locale)}</div>
                </div>
              </div>
            );
          })}
          {detail.rating_count > detail.recent_reviews.length && (
            <button type="button" className="rm-rev-allbtn">
              {ko
                ? `리뷰 ${detail.rating_count.toLocaleString()}개 모두 보기 →`
                : `See all ${detail.rating_count.toLocaleString()} reviews →`}
            </button>
          )}
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
  const counts = new Map<CategoryKey, number>();
  members.forEach((m) => {
    const c = categoryOf(m.slug);
    counts.set(c, (counts.get(c) ?? 0) + 1);
  });
  const entries = [...counts.entries()].sort((a, b) => b[1] - a[1]);
  return (
    <div className="rm-summary">
      <span className="eyebrow">{ko ? "이 패키지가 막는 것" : "What this package blocks"}</span>
      {(copy?.intro || why) && <p className="why">{copy?.intro || why}</p>}
      {copy && copy.blocks.length > 0 && (
        <ul className="rm-blocklist">
          {copy.blocks.map((b, i) => (
            <li key={i}>
              <span className="x" aria-hidden="true">✕</span>
              <span>
                <strong>{b.t}</strong>
                {b.d && <span> — {b.d}</span>}
              </span>
            </li>
          ))}
        </ul>
      )}
      <div className="rm-summary-stats">
        <span className="rm-stat">
          <strong style={{ color: "var(--slate-900)" }}>{members.length}</strong> {ko ? "개 정책" : "policies"}
        </span>
        {entries.length > 0 && (
          <span className="rm-cov-inline">
            {entries.map(([c, n]) => (
              <span
                key={c}
                className="rm-cov"
                style={{ background: CATEGORY_COLOR[c].soft, color: CATEGORY_COLOR[c].ink }}
              >
                <CategoryGlyph category={c} size={12} color={CATEGORY_COLOR[c].hex} />
                {categoryNameOf(c, locale)} {n}
              </span>
            ))}
          </span>
        )}
      </div>
    </div>
  );
}

function IncludedPolicies({ members, locale }: { members: SetMember[]; locale: MarketLocale }) {
  const ko = locale === "ko";
  return (
    <div className="rm-sec">
      <div className="rm-sec-head">
        <h2>
          {ko ? "포함된 정책" : "Policies in this package"}{" "}
          <span style={{ color: "var(--slate-400)" }}>({members.length})</span>
        </h2>
        <span className="sub">{ko ? "정책을 누르면 소개가 펼쳐져요" : "Tap a policy to expand"}</span>
      </div>
      <div className="rm-members" style={{ marginTop: 11 }}>
        {members.map((m, i) => (
          <MemberRow key={`${m.slug}-${i}`} member={m} locale={locale} />
        ))}
      </div>
    </div>
  );
}

/**
 * cedar 원문 → IR (흐름도 입력). editor / ListingConditionTree 와 같은 파이프라인
 * (textToBlocks). 비동기(확장 브리지)라 로딩/실패 상태를 구분해서 반환한다.
 * 실패(브리지 없음·파싱 불가)면 null — 호출부가 흐름도 섹션을 숨긴다.
 */
function usePolicyIr(cedarText: string): PolicyIR | null | "loading" {
  const [ir, setIr] = useState<PolicyIR | null | "loading">(cedarText ? "loading" : null);
  useEffect(() => {
    if (!cedarText) {
      setIr(null);
      return;
    }
    let alive = true;
    setIr("loading");
    textToBlocks(cedarText)
      .then((irs) => alive && setIr(irs[0] ?? null))
      .catch(() => alive && setIr(null));
    return () => {
      alive = false;
    };
  }, [cedarText]);
  return ir;
}

/**
 * Policy detail body — MK_v3 detailPol():
 *   summary → "폼·흐름도"(읽기전용 PolicyDiagram) → "정책 원문"(CodeTabs:
 *   조건/cedar/manifest) → (조건부) 포함된 패키지.
 * 흐름도·원문 모두 실제 컴포넌트(editor PolicyDiagram, market CodeTabs)에
 * getListing 의 cedar_text/manifest 를 그대로 연결 — mock 아님.
 */
function PolicyDetailBody({ detail, locale }: { detail: ListingDetail; locale: MarketLocale }) {
  const ko = locale === "ko";
  const cedar = detail.latest_version?.cedar_text ?? "";
  const manifest = detail.latest_version?.manifest;
  const copy = policyCopy(detail.slug);
  const summary = copy?.title || pickI18n(detail.description, locale) || (cedar ? leadingComment(cedar) : "");
  const sev = cedar ? severityFromCedar(cedar) : detail.severity ?? "deny";
  const cat = categoryOf(detail.slug);
  const inPkgs = usePackagesContaining(detail.slug);
  const ir = usePolicyIr(cedar);
  return (
    <>
      <div className="rm-summary">
        <span className="eyebrow">{ko ? "이 정책이 막는 것" : "What this blocks"}</span>
        {summary && <p className="why">{summary}</p>}
        <div className="rm-summary-stats">
          <span className={`rm-sev ${sev}`}>{sevLabel(sev, locale)}</span>
          <span className="rm-stat">{categoryNameOf(cat, locale)}</span>
        </div>
      </div>

      {cedar && (
        <div className="rm-sec">
          <div className="rm-sec-head">
            <h2>{ko ? "폼 · 흐름도" : "Form · Diagram"}</h2>
            <span className="sub">{ko ? "정책을 구조 다이어그램으로 봅니다" : "Policy as a structure diagram"}</span>
          </div>
          <div className="rm-pv-diagram" style={{ marginTop: 11 }}>
            {ir === "loading" ? (
              <p className="rm-mdesc">{ko ? "불러오는 중…" : "Loading…"}</p>
            ) : ir ? (
              <PolicyDiagram ir={ir} />
            ) : (
              <p className="rm-mdesc">{ko ? "이 정책의 흐름도는 준비 중이에요. 아래 원문을 확인하세요." : "Diagram unavailable — see the source below."}</p>
            )}
          </div>
        </div>
      )}

      {cedar && (
        <div className="rm-sec">
          <div className="rm-sec-head"><h2>{ko ? "정책 원문" : "Policy source"}</h2></div>
          <p style={{ color: "var(--slate-500)", fontSize: 13, marginTop: 4 }}>
            {ko ? "이 지갑에 적용되는 실제 Cedar 규칙입니다." : "The actual Cedar rule applied to your wallet."}
          </p>
          <div style={{ marginTop: 11 }}>
            <CodeTabs cedar={cedar} manifest={manifest} locale={locale} hideComments />
          </div>
        </div>
      )}

      {inPkgs.length > 0 && (
        <div className="rm-sec">
          <div className="rm-sec-head"><h2>{ko ? "포함된 패키지" : "Included in packages"}</h2></div>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginTop: 11 }}>
            {inPkgs.map((pk) => (
              <Link key={pk.slug} to={`/market/${encodeURIComponent(pk.slug)}`} className="rm-pkgchip">
                <PackageGlyphSm /> {pk.name} <span className="n">{pk.memberCount}</span>
              </Link>
            ))}
          </div>
        </div>
      )}
    </>
  );
}

/**
 * Reverse lookup "which packages include this policy?" — the server has no
 * back-reference, so we fetch the set listings and inspect their members
 * (cached 60s). Returns [] until loaded; the section is hidden when empty, so
 * a missing back-reference degrades gracefully (prototype `inPkgs.length ?`).
 */
function usePackagesContaining(
  slug: string,
): { slug: string; name: string; memberCount: number }[] {
  const setsQ = useQuery({
    queryKey: ["market-sets-for-reverse"],
    queryFn: () => listListings({ kind: "set", limit: 50 }),
    staleTime: 60_000,
  });
  const sets = setsQ.data ?? [];
  const detailQs = useQueries({
    queries: sets.map((s) => ({
      queryKey: ["market-listing", s.slug],
      queryFn: () => getListing(s.slug),
      staleTime: 60_000,
    })),
  });
  return sets.flatMap((s, i) => {
    const members = detailQs[i]?.data?.latest_version?.members ?? [];
    if (!members.some((m) => m.slug === slug)) return [];
    return [{ slug: s.slug, name: pickI18n(s.display_name, "ko"), memberCount: members.length }];
  });
}

function PackageGlyphSm() {
  return (
    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="var(--warn-600)" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3 8l9-5 9 5-9 5-9-5zM3 8v8l9 5 9-5V8" />
    </svg>
  );
}

/** A package member — MK_v3 detailPkg 의 펼침(accordion) 행(rm-member). 헤더를
 * 누르면 한 줄 소개가 펼쳐지고, "자세히 살펴보기 →"로 그 정책 상세로 이동한다. */
function MemberRow({ member, locale }: { member: SetMember; locale: MarketLocale }) {
  const ko = locale === "ko";
  const sev = severityFromCedar(member.cedar_text);
  const cat = categoryOf(member.slug);
  const color = CATEGORY_COLOR[cat];
  const copy = policyCopy(member.slug);
  const oneLine = copy?.title || leadingComment(member.cedar_text);
  const desc = copy?.what || oneLine;
  const [open, setOpen] = useState(false);

  return (
    <div className={`rm-member${open ? " open" : ""}`}>
      <button type="button" className="rm-mhead" onClick={() => setOpen((o) => !o)}>
        <span className="rm-mchev">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <path d="M9 6l6 6-6 6" />
          </svg>
        </span>
        <span className="rm-mrow-ic" style={{ background: color.soft }}>
          <CategoryGlyph category={cat} size={14} color={color.hex} />
        </span>
        <span className="rm-mmain">
          <span className="rm-mtitlerow">
            <span className="rm-mrow-name">{member.display_name || member.slug}</span>
            <span className="rm-mchip">{categoryNameOf(cat, locale)}</span>
          </span>
          {oneLine && <span className="rm-moneline">{oneLine}</span>}
        </span>
        <span className={`rm-sev ${sev}`}>{sevLabel(sev, locale)}</span>
      </button>
      <div className="rm-mbody">
        <div className="rm-mbody-in">
          {desc && <p className="rm-mdesc">{desc}</p>}
          <Link to={`/market/${encodeURIComponent(member.slug)}`} className="rm-msource">
            {ko ? "자세히 살펴보기 →" : "View details →"}
          </Link>
        </div>
      </div>
    </div>
  );
}

function sevLabel(sev: "deny" | "warn" | "info", locale: MarketLocale): string {
  const ko = locale === "ko";
  return sev === "deny" ? (ko ? "차단" : "DENY") : sev === "warn" ? (ko ? "경고" : "WARN") : ko ? "정보" : "INFO";
}

/** 별점 분포(%) — 실제 리뷰 표본이 있으면 그걸로 버킷, 없으면 평균에서 합성(cosmetic).
 *  프로토타입 rmRatingDist 와 동일한 평균-기반 합성 + 실데이터 우선. */
function ratingDist(avg: number, sample: number[]): { star: number; pct: number }[] {
  if (sample.length > 0) {
    const counts = [0, 0, 0, 0, 0]; // index 0 = 1점
    sample.forEach((r) => {
      const i = Math.min(5, Math.max(1, Math.round(r))) - 1;
      counts[i] += 1;
    });
    const total = sample.length;
    const out: { star: number; pct: number }[] = [];
    for (let s = 5; s >= 1; s--) out.push({ star: s, pct: Math.round((counts[s - 1] / total) * 100) });
    return out;
  }
  const p5 = Math.max(0, Math.min(1, (avg - 3) / 2));
  const rem = 1 - p5;
  const shares: Record<number, number> = { 5: p5, 4: rem * 0.55, 3: rem * 0.2, 2: rem * 0.13, 1: rem * 0.12 };
  const out: { star: number; pct: number }[] = [];
  for (let s = 5; s >= 1; s--) out.push({ star: s, pct: Math.round(shares[s] * 100) });
  out[0].pct += 100 - out.reduce((a, b) => a + b.pct, 0);
  return out;
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
