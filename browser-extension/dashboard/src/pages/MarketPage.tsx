import { useMemo, useRef, useState } from "react";
import { useQueries, useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useSearchParams } from "react-router-dom";

import {
  getActivitySummary,
  getListing,
  listListings,
  pickI18n,
  type ListingSort,
  type ListingSummary,
} from "../server-api";
import { publisherDisplay } from "../server-api/market";
import { MarketInstallModal } from "./MarketInstallModal";
import { MarketPagehead, useMarketContentClass } from "./MarketPagehead";

import {
  CATEGORY_COLOR,
  CATEGORY_ORDER,
  CategoryGlyph,
  categoryNameOf,
  categoryOf,
  isCategoryKey,
  type CategoryKey,
} from "./market-domain";
import { policyCopy } from "./market-copy";
import { useMarketLocale, type MarketLocale } from "./market-locale";

import "./market.css";

/**
 * `/market` — discovery landing by default; `?view=list` swaps in the full
 * filter-and-search grid.
 *
 * Landing layout (matches the reference Phantom/MagicEden hero +
 * trending + sidebar shape):
 *   ┌─ main ────────────────────────┬─ sidebar ─┐
 *   │ Hero: 3 official packages     │  Toggle   │
 *   │ Categories: 12 domain tiles   │  Top 10   │
 *   │                               │  더보기 → │
 *   └───────────────────────────────┴───────────┘
 *
 * Clicking a category navigates to `?view=list&domain=<d>`; clicking the
 * sidebar 더보기 navigates to `?view=list&kind=<k>&sort=popular`. The list
 * view reads those URL params on first render so deep links work.
 */
export function MarketPage() {
  // Locale is fixed to the stored preference here; the 한/EN toggle moved out
  // of the market header (language belongs in user Settings). Default is `ko`.
  const [locale] = useMarketLocale();
  const [params] = useSearchParams();
  const view = params.get("view") === "list" ? "list" : "landing";

  // Market route owns its frame: kill the shell padding so `.rm-page` is the
  // sole content frame (prototype `.app-content { padding: 0 }`), scoped here.
  useMarketContentClass();

  // Prototype `shell()` rule: the landing calls shell("", null, …) so it has
  // NO page header — only the body's `.rm-shead-ttl "Market"`. The list view
  // calls shell("전체 목록", {act:home}, …), so it gets the `.rm-pagehead`
  // crumb + back. Reproduce that here instead of an always-on global Topbar.
  return (
    <>
      {view === "list" ? (
        <>
          <MarketPagehead
            crumb={locale === "ko" ? "전체 목록" : "All listings"}
            back={{ to: "/market", label: locale === "ko" ? "← 마켓 홈" : "← Market home" }}
          />
          <ListView locale={locale} initialParams={params} />
        </>
      ) : (
        <LandingView locale={locale} />
      )}
    </>
  );
}

// ─────────────────────────────────────────────────────────────────────────
// Market-scoped search (replaces the global jump-search in the topbar)
// ─────────────────────────────────────────────────────────────────────────

/**
 * Top-bar search for the market. Unlike the shared `GlobalSearch` (which jumps
 * to wallets / installed policies / verdicts), this searches the marketplace
 * itself: submitting routes to the list view with the query applied.
 */
const POPULAR_QUERIES = ["무제한 승인", "드레이너", "슬리피지", "블라인드 서명", "에어드랍", "Permit2"];
const RECENT_KEY = "market:recent-searches";

function readRecent(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    const arr = raw ? (JSON.parse(raw) as unknown) : [];
    return Array.isArray(arr) ? arr.filter((x): x is string => typeof x === "string").slice(0, 6) : [];
  } catch {
    return [];
  }
}
function writeRecent(list: string[]): void {
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, 6)));
  } catch {
    /* storage blocked — recent search is best-effort */
  }
}


// ─────────────────────────────────────────────────────────────────────────
// Landing view
// ─────────────────────────────────────────────────────────────────────────

function LandingView({ locale }: { locale: MarketLocale }) {
  const ko = locale === "ko";
  const [installTarget, setInstallTarget] = useState<ListingSummary | null>(null);
  // Single 100-item fetch powers everything on the landing page (counts,
  // coverage, official packages, popular policies) — minimal backend calls,
  // all real data. 35 seed policies + a few packages fit under the ceiling.
  const allQ = useQuery({
    queryKey: ["market-all-for-landing"],
    queryFn: () => listListings({ limit: 100 }),
  });
  const all = allQ.data ?? [];

  // Per-category policy counts + how many of them are already installed.
  const { counts, installed } = useMemo(() => {
    const counts = new Map<CategoryKey, number>();
    const installed = new Map<CategoryKey, number>();
    all.forEach((l) => {
      const c = listingCategoryKey(l);
      if (!c) return;
      counts.set(c, (counts.get(c) ?? 0) + 1);
      if (l.is_installed) installed.set(c, (installed.get(c) ?? 0) + 1);
    });
    return { counts, installed };
  }, [all]);

  const officialPkgs = useMemo(
    () =>
      all
        .filter((l) => l.kind === "set" && l.publisher_tier === "official")
        .sort((a, b) => b.install_count - a.install_count)
        .slice(0, 4),
    [all],
  );
  // Fetch each official package's members so the landing cards show the same
  // policy-count + #category tags as the list view (prototype pkgCard()).
  const officialDetailQs = useQueries({
    queries: officialPkgs.map((s) => ({
      queryKey: ["market-listing", s.slug],
      queryFn: () => getListing(s.slug),
      staleTime: 60_000,
    })),
  });
  const officialMetaFor = (i: number) => {
    const members = officialDetailQs[i]?.data?.latest_version?.members ?? [];
    const catCount = new Map<CategoryKey, number>();
    members.forEach((m) => {
      const c = categoryOf(m.slug);
      catCount.set(c, (catCount.get(c) ?? 0) + 1);
    });
    return { count: members.length, catCount, ready: members.length > 0 };
  };
  const topPolicies = useMemo(
    () =>
      all
        .filter((l) => l.kind === "policy")
        .sort((a, b) => b.install_count - a.install_count)
        .slice(0, 5),
    [all],
  );

  // Recommendation hero — REAL data, two honest modes (no mock):
  //  • activity mode: GET /market/activity-summary gives per-listing install
  //    events in the last 7 days; we bucket by categoryOf(slug) → "최근 인기"
  //    categories by recent install demand. This is real marketplace demand,
  //    NOT "your activity" (the server has no per-wallet action history), so
  //    the copy says 인기, not 활동.
  //  • coverage fallback: when nothing was installed in the window (empty
  //    entries), fall back to "still-uninstalled categories" — also real data.
  const activityQ = useQuery({
    queryKey: ["market-activity-summary", 7],
    queryFn: () => getActivitySummary({ days: 7, limit: 100 }),
    staleTime: 60_000,
  });
  const recentByCat = useMemo(() => {
    const m = new Map<CategoryKey, number>();
    (activityQ.data?.entries ?? []).forEach((e) => {
      const c = categoryOf(e.slug);
      m.set(c, (m.get(c) ?? 0) + e.recent_installs);
    });
    return m;
  }, [activityQ.data]);
  const activityDays = activityQ.data?.days ?? 7;
  const hasActivity = recentByCat.size > 0;

  const recoCats = useMemo(() => {
    if (hasActivity) {
      // 최근 7일 설치가 있는 카테고리, 설치 많은 순 (단 이미 다 깐 카테고리는 제외).
      return [...recentByCat.entries()]
        .filter(([c]) => {
          const n = counts.get(c) ?? 0;
          const on = installed.get(c) ?? 0;
          return n > 0 && on < n;
        })
        .sort((a, b) => b[1] - a[1])
        .map(([c]) => c)
        .slice(0, 3);
    }
    // coverage fallback — 미설치 정책이 남은 카테고리, 정책 수 많은 순.
    return CATEGORY_ORDER.filter((c) => {
      const n = counts.get(c) ?? 0;
      const on = installed.get(c) ?? 0;
      return n > 0 && on < n;
    })
      .sort((a, b) => (counts.get(b) ?? 0) - (counts.get(a) ?? 0))
      .slice(0, 3);
  }, [hasActivity, recentByCat, counts, installed]);
  const leadCat = recoCats[0] ?? null;
  const railCats = recoCats.slice(1);

  // Per-category reason line: activity mode shows "최근 7일 설치 N건", coverage
  // mode shows "미설치 N개". Both are real; the eyebrow label matches the mode.
  const reasonFor = (c: CategoryKey): string => {
    if (hasActivity) {
      const n = recentByCat.get(c) ?? 0;
      return ko ? `최근 ${activityDays}일 설치 ${n}건` : `${n} installs · ${activityDays}d`;
    }
    const left = (counts.get(c) ?? 0) - (installed.get(c) ?? 0);
    return ko ? `미설치 ${left}개` : `${left} uninstalled`;
  };

  return (
    <div className="rm-page">
      <div className="rm-shead">
        <div>
          <div className="rm-shead-ttl">Market</div>
          <div className="rm-shead-sub">
            {ko ? "지갑을 지키는 정책과 패키지를 둘러보세요" : "Browse policies and packages that protect your wallet"}
          </div>
        </div>
      </div>

      {leadCat && (
        <div className="rm-hero">
          <Link to={`/market?view=list&category=${leadCat}`} className="rm-hero-lead">
            <div className="rm-hero-eg"><span className="dot" />{hasActivity ? (ko ? "최근 인기 카테고리" : "Trending now") : ko ? "내 지갑에 빈 카테고리" : "Gaps in your coverage"}</div>
            <div className="rm-hero-cat">
              <span className="ic" style={{ background: CATEGORY_COLOR[leadCat].hex }}>
                <CategoryGlyph category={leadCat} size={26} color="#fff" />
              </span>
              <span className="nm">{categoryNameOf(leadCat, locale)}</span>
              <span className="cnt">{ko ? `정책 ${counts.get(leadCat) ?? 0}` : `${counts.get(leadCat) ?? 0} policies`}</span>
            </div>
            <div className="rm-hero-reason">
              <span className="act">
                <PulseGlyph />
                {reasonFor(leadCat)}
              </span>
              {" — "}
              {hasActivity
                ? ko
                  ? `${categoryNameOf(leadCat, locale)} 정책을 많이들 받고 있어요`
                  : `${categoryNameOf(leadCat, locale)} is popular this week`
                : ko
                  ? `${categoryNameOf(leadCat, locale)} 정책이 아직 비어 있어요`
                  : `${categoryNameOf(leadCat, locale)} coverage is still open`}
            </div>
            <span className="rm-hero-cta">
              {ko ? `${categoryNameOf(leadCat, locale)} 정책 둘러보기` : `Browse ${categoryNameOf(leadCat, locale)}`}
              <Chevron />
            </span>
          </Link>
          {railCats.length > 0 && (
            <div className="rm-hero-rail">
              <div className="rm-hero-rail-eg">{ko ? "이 카테고리도 살펴보세요" : "Also worth a look"}</div>
              {railCats.map((c) => (
                <Link key={c} to={`/market?view=list&category=${c}`} className="rm-hero-rrow">
                  <span className="ic" style={{ background: CATEGORY_COLOR[c].soft }}>
                    <CategoryGlyph category={c} size={16} color={CATEGORY_COLOR[c].hex} />
                  </span>
                  <div className="meta">
                    <div className="nm">
                      {categoryNameOf(c, locale)}{" "}
                      <span className="cnt">{ko ? `정책 ${counts.get(c) ?? 0}` : `${counts.get(c) ?? 0}`}</span>
                    </div>
                    <div className="why">
                      <span className="act">{reasonFor(c)}</span>
                    </div>
                  </div>
                  <span className="go"><Chevron /></span>
                </Link>
              ))}
            </div>
          )}
        </div>
      )}

      <CategoryCoverage counts={counts} installed={installed} locale={locale} />

      <div className="rm-cols">
        <div className="rm-sec">
          <div className="rm-sec-head between">
            <div>
              <h2>{ko ? "공식 패키지" : "Official packages"}</h2>
              <div className="sub">{ko ? "Dambi가 검증한 정책 패키지 — 공식 인증" : "Verified by Dambi"}</div>
            </div>
          </div>
          <div className="rm-rgrid two" style={{ marginTop: 12 }}>
            {officialPkgs.map((pk, i) => (
              <PackageListCard
                key={pk.id}
                listing={pk}
                meta={officialMetaFor(i)}
                categories={[]}
                locale={locale}
                onInstall={setInstallTarget}
              />
            ))}
            {officialPkgs.length === 0 && (
              <div className="ml-status">{ko ? "공식 패키지가 없습니다." : "No official packages yet."}</div>
            )}
          </div>
        </div>

        <aside className="rm-sec">
          <div className="rm-sec-head">
            <div>
              <h2>{ko ? "다운로드 많은 정책" : "Popular policies"}</h2>
              <div className="sub">{ko ? "최근 7일 설치 기준 인기 정책" : "Top by recent installs"}</div>
            </div>
          </div>
          <div className="rm-trend" style={{ marginTop: 12 }}>
            {topPolicies.map((p, i) => {
              const c = listingCategoryKey(p);
              const color = c ? CATEGORY_COLOR[c] : null;
              return (
                <Link key={p.id} to={`/market/${encodeURIComponent(p.slug)}`} className="rm-rrow">
                  <span className={`rk${i < 3 ? " top" : ""}`}>{i + 1}</span>
                  <span className="ic" style={color ? { background: color.soft } : undefined}>
                    {c && <CategoryGlyph category={c} size={14} color={color!.hex} />}
                  </span>
                  <span className="meta">
                    <span className="nm">{pickI18n(p.display_name, locale) || p.slug}</span>
                    <span className="pub">
                      <InstallCount n={p.install_count} /> {ko ? "다운로드" : "installs"}
                    </span>
                  </span>
                  {c && color && (
                    <span className="ct">
                      <span className="rm-catmini" style={{ background: color.soft, color: color.ink }}>
                        {categoryNameOf(c, locale)}
                      </span>
                    </span>
                  )}
                </Link>
              );
            })}
            <Link to="/market?view=list&kind=policy&sort=popular" className="rm-more">
              {ko ? "전체 정책 보기 →" : "View all policies →"}
            </Link>
          </div>
        </aside>
      </div>

      {installTarget && (
        <MarketInstallModal
          listing={installTarget}
          locale={locale}
          onClose={() => setInstallTarget(null)}
        />
      )}
    </div>
  );
}

/** Coverage-aware category grid — each tile shows installed/total + a bar. */
function CategoryCoverage({
  counts,
  installed,
  locale,
}: {
  counts: Map<CategoryKey, number>;
  installed: Map<CategoryKey, number>;
  locale: MarketLocale;
}) {
  const ko = locale === "ko";
  return (
    <div className="rm-sec">
      <div className="rm-sec-head between">
        <div>
          <h2>Category</h2>
          <div className="sub">
            {ko
              ? "찾는 정책 카테고리가 있나요? 자산·프로토콜 유형별로 골라 둘러보고 빈틈을 채우세요."
              : "Browse by asset/protocol type and fill the gaps."}
          </div>
        </div>
      </div>
      <div className="rm-defense" style={{ marginTop: 14 }}>
        {CATEGORY_ORDER.map((c) => {
          const color = CATEGORY_COLOR[c];
          const n = counts.get(c) ?? 0;
          const on = installed.get(c) ?? 0;
          const full = n > 0 && on >= n;
          const pct = n ? Math.round((on / n) * 100) : 0;
          return (
            <Link key={c} to={`/market?view=list&category=${c}`} className={`rm-deftile${on ? " has" : ""}`}>
              <div className="rm-deftile-top">
                <span className="ic" style={{ background: color.soft }}>
                  <CategoryGlyph category={c} size={19} color={color.hex} />
                </span>
                {full ? (
                  <span className="rm-defok">
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="var(--warn-700)" strokeWidth={2.4} aria-hidden="true">
                      <path d="M5 12l4 4L19 7" />
                    </svg>
                  </span>
                ) : on ? (
                  <span className="rm-defpart">{on}/{n}</span>
                ) : (
                  <span className="rm-defempty">{ko ? "미설치" : "none"}</span>
                )}
              </div>
              <div className="rm-deftile-nm" style={{ color: color.ink }}>{categoryNameOf(c, locale)}</div>
              <div className="rm-defbar"><span style={{ width: `${pct}%`, background: color.hex }} /></div>
            </Link>
          );
        })}
      </div>
    </div>
  );
}

function PackageGlyphSm() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="var(--warn-700)" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3 8l9-5 9 5-9 5-9-5zM3 8v8l9 5 9-5V8" />
    </svg>
  );
}

function Chevron() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M9 6l6 6-6 6" />
    </svg>
  );
}

/** Pulse/activity glyph used in the recommendation hero reason. */
function PulseGlyph({ size = 14, color = "var(--warn-400)" }: { size?: number; color?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3 12h4l3 8 4-16 3 8h4" />
    </svg>
  );
}

// ─────────────────────────────────────────────────────────────────────────
// List view (`?view=list`)
// ─────────────────────────────────────────────────────────────────────────

/**
 * 목록 뷰 검색 팔레트 — 프로토타입 searchPanelHtml() + setupLandingSearch()
 * 1:1 포팅. 패널은 세 상태로 갈린다(원본 동일):
 *   - hover 만(빈 입력)          → 카테고리 그리드만
 *   - focus + 빈 입력            → 최근 검색 + 많이 찾는 검색 (카테고리 없음)
 *   - focus + 입력값             → 패키지(≤3)·정책(≤5) 라이브 hit, 없으면
 *                                  rm-srch-empty, 있으면 rm-srch-enter
 * 카테고리 클릭은 즉시 필터(toggleCat), hit 클릭은 상세로, Enter 는 전체 결과.
 */
function ListSearchPalette({
  q,
  setQ,
  onSubmit,
  onClear,
  cats,
  toggleCat,
  catCounts,
  policies,
  packages,
  onOpenDetail,
  locale,
}: {
  q: string;
  setQ: (v: string) => void;
  onSubmit: () => void;
  onClear: () => void;
  cats: Set<CategoryKey>;
  toggleCat: (c: CategoryKey) => void;
  catCounts: Map<CategoryKey, number>;
  /** Live-hit source: all policies / packages currently loaded in the list. */
  policies: ListingSummary[];
  packages: ListingSummary[];
  onOpenDetail: (slug: string) => void;
  locale: MarketLocale;
}) {
  const ko = locale === "ko";
  // Two independent triggers (prototype `focused`/`hovering`); the panel is
  // open when either is set, and the section layout depends on `focused`.
  const [focused, setFocused] = useState(false);
  const [hovering, setHovering] = useState(false);
  const [recent, setRecent] = useState<string[]>(() => readRecent());
  const blurTimer = useRef<number | null>(null);
  const open = focused || hovering;
  const v = q.trim();

  const submit = (raw: string) => {
    const t = raw.trim();
    if (t) {
      const next = [t, ...recent.filter((r) => r !== t)].slice(0, 6);
      setRecent(next);
      writeRecent(next);
    }
    setQ(raw);
    onSubmit();
    setFocused(false);
  };
  const onBlur = () => {
    if (blurTimer.current) window.clearTimeout(blurTimer.current);
    blurTimer.current = window.setTimeout(() => setFocused(false), 140);
  };
  const cancelBlur = () => {
    if (blurTimer.current) window.clearTimeout(blurTimer.current);
  };

  // Live hits — same filter the prototype's searchPanelHtml() applied: name or
  // one-line includes the query text, then category/severity narrowing. We
  // reuse the loaded list data so no extra fetch is needed.
  const text = v.toLowerCase();
  const matchT = (l: ListingSummary) => {
    if (!text) return true;
    const name = (pickI18n(l.display_name, locale) || l.slug).toLowerCase();
    const line = (policyCopy(l.slug)?.title || pickI18n(l.description, locale) || "").toLowerCase();
    return name.includes(text) || line.includes(text) || l.slug.toLowerCase().includes(text);
  };
  const hitPkgs = packages.filter(matchT).slice(0, 3);
  const hitPols = policies.filter(matchT).slice(0, 5);

  const cardCat = (l: ListingSummary) => listingCategoryKey(l);

  return (
    <div
      className={`rm-srch-wrap${open ? " open" : ""}`}
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
    >
      <div className="rm-srch-bar">
        <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="var(--slate-400)" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.5-3.5" />
        </svg>
        <input
          type="text"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onFocus={() => setFocused(true)}
          onBlur={onBlur}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit(q);
            else if (e.key === "Escape") setFocused(false);
          }}
          placeholder={ko ? "정책·패키지 검색 — 커서를 올리면 카테고리, 누르면 최근 검색" : "Search — hover for categories, focus for recent"}
        />
        {q ? (
          <button type="button" className="rm-srch-kbd" style={{ cursor: "pointer" }} onClick={onClear}>×</button>
        ) : (
          <kbd className="rm-srch-kbd">↵</kbd>
        )}
      </div>

      <div className="rm-srch-panel" onMouseDown={cancelBlur}>
        {focused && v ? (
          // ── focus + 입력: 라이브 hit (패키지·정책) / 없으면 empty ──
          <>
            {hitPkgs.length > 0 && (
              <div className="rm-srch-sec">
                <div className="rm-srch-shead">{ko ? "패키지" : "Packages"} · {hitPkgs.length}</div>
                {hitPkgs.map((pk) => (
                  <button key={pk.id} type="button" className="rm-srch-hit" onClick={() => { submit(q); onOpenDetail(pk.slug); }}>
                    <span className="hic pkg"><PackageGlyphSm /></span>
                    <span className="nm">{pickI18n(pk.display_name, locale) || pk.slug}</span>
                    <span className="k">{pk.publisher_tier === "official" ? (ko ? "공식" : "Official") : ko ? "커뮤니티" : "Community"}</span>
                    <span className="hn">{pk.install_count.toLocaleString()}</span>
                  </button>
                ))}
              </div>
            )}
            {hitPols.length > 0 && (
              <div className="rm-srch-sec">
                <div className="rm-srch-shead">{ko ? "정책" : "Policies"} · {hitPols.length}</div>
                {hitPols.map((p2) => {
                  const c = cardCat(p2);
                  return (
                    <button key={p2.id} type="button" className="rm-srch-hit" onClick={() => { submit(q); onOpenDetail(p2.slug); }}>
                      <span className={`hic ${p2.severity ?? "warn"}`}>{p2.severity && <SeveritySymbol sev={p2.severity} size={12} />}</span>
                      <span className="nm">{pickI18n(p2.display_name, locale) || p2.slug}</span>
                      <span className="k">{c ? categoryNameOf(c, locale) : ""}</span>
                      <span className="hn">{p2.install_count.toLocaleString()}</span>
                    </button>
                  );
                })}
              </div>
            )}
            {hitPkgs.length === 0 && hitPols.length === 0 ? (
              <div className="rm-srch-empty">{ko ? `"${v}"에 대한 결과가 없어요` : `No results for "${v}"`}</div>
            ) : (
              <div className="rm-srch-enter">
                <kbd>Enter</kbd> {ko ? "전체 결과 보기" : "see all results"}
              </div>
            )}
          </>
        ) : focused ? (
          // ── focus + 빈 입력: 최근 검색 + 많이 찾는 검색 ──
          <>
            {recent.length > 0 && (
              <div className="rm-srch-sec">
                <div className="rm-srch-shead">{ko ? "최근 검색" : "Recent"}</div>
                {recent.map((r) => (
                  <div key={r} className="rm-srch-recent" onClick={() => submit(r)}>
                    <span className="ic">
                      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="var(--slate-400)" strokeWidth={1.8} aria-hidden="true">
                        <path d="M12 7v5l3 2M12 3a9 9 0 100 18 9 9 0 000-18" />
                      </svg>
                    </span>
                    <span className="t">{r}</span>
                    <button type="button" className="del" onClick={(e) => { e.stopPropagation(); const next = recent.filter((x) => x !== r); setRecent(next); writeRecent(next); }} aria-label="remove">×</button>
                  </div>
                ))}
              </div>
            )}
            <div className="rm-srch-sec">
              <div className="rm-srch-shead">{ko ? "많이 찾는 검색" : "Popular"}</div>
              <div className="rm-srch-pop">
                {POPULAR_QUERIES.map((t) => (
                  <button key={t} type="button" className="rm-srch-pchip" onClick={() => submit(t)}>{t}</button>
                ))}
              </div>
            </div>
          </>
        ) : (
          // ── hover 만: 카테고리 그리드 ──
          <div className="rm-srch-sec">
            <div className="rm-srch-shead rm-srch-shead-row">
              <span>{ko ? "카테고리" : "Category"} <span className="muted">· {ko ? "여러 개 선택 가능" : "multi-select"}</span></span>
              <span className="rm-srch-allacts">
                <button type="button" onClick={() => CATEGORY_ORDER.forEach((c) => { if (!cats.has(c)) toggleCat(c); })}>{ko ? "모두 선택" : "Select all"}</button>
                <button type="button" onClick={() => [...cats].forEach(toggleCat)}>{ko ? "모두 해제" : "Clear"}</button>
              </span>
            </div>
            <div className="rm-srch-cats">
              {CATEGORY_ORDER.map((c) => {
                const on = cats.has(c);
                const col = CATEGORY_COLOR[c];
                const n = catCounts.get(c) ?? 0;
                return (
                  <button
                    key={c}
                    type="button"
                    className={`rm-srch-cat${on ? " on" : ""}`}
                    onClick={() => toggleCat(c)}
                  >
                    <span className="ic" style={{ background: on ? col.hex : col.soft }}>
                      <CategoryGlyph category={c} size={14} color={on ? "#fff" : col.hex} />
                    </span>
                    {categoryNameOf(c, locale)}
                    <span className="n">{n}</span>
                  </button>
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ListView({
  locale,
  initialParams,
}: {
  locale: MarketLocale;
  initialParams: URLSearchParams;
}) {
  const ko = locale === "ko";
  const navigate = useNavigate();
  const initialCategory = initialParams.get("category") ?? "";
  const initialSort = parseSortParam(initialParams.get("sort"));
  const initialQ = initialParams.get("q") ?? "";

  const [cats, setCats] = useState<Set<CategoryKey>>(
    () => new Set(isCategoryKey(initialCategory) ? [initialCategory] : []),
  );
  const [sort, setSort] = useState<ListingSort>(initialSort);
  const [q, setQ] = useState(initialQ);
  const [search, setSearch] = useState(initialQ);
  const [pkgAll, setPkgAll] = useState(false);
  const [polAll, setPolAll] = useState(false);
  const [installTarget, setInstallTarget] = useState<ListingSummary | null>(null);

  const selected = [...cats];
  const toggleCat = (c: CategoryKey) =>
    setCats((prev) => {
      const n = new Set(prev);
      if (n.has(c)) n.delete(c);
      else n.add(c);
      return n;
    });

  // All policies — multi-category filtering is client-side (union of selected).
  const policiesQ = useQuery({
    queryKey: ["market-listings", { kind: "policy", sort, q: search }],
    queryFn: () =>
      listListings({ kind: "policy", sort, q: search.trim() || undefined, limit: 100 }),
  });
  const allPolicies = policiesQ.data ?? [];
  const polCatCounts = useMemo(() => {
    const m = new Map<CategoryKey, number>();
    allPolicies.forEach((p) => {
      const c = listingCategoryKey(p);
      if (c) m.set(c, (m.get(c) ?? 0) + 1);
    });
    return m;
  }, [allPolicies]);

  const setsQ = useQuery({
    queryKey: ["market-sets-list", { sort, q: search }],
    queryFn: () =>
      listListings({ kind: "set", sort, q: search.trim() || undefined, limit: 50 }),
  });
  const sets = setsQ.data ?? [];
  const setDetailQs = useQueries({
    queries: sets.map((s) => ({
      queryKey: ["market-listing", s.slug],
      queryFn: () => getListing(s.slug),
      staleTime: 60_000,
    })),
  });
  const metaFor = (i: number) => {
    const members = setDetailQs[i]?.data?.latest_version?.members ?? [];
    const catCount = new Map<CategoryKey, number>();
    members.forEach((m) => {
      const c = categoryOf(m.slug);
      catCount.set(c, (catCount.get(c) ?? 0) + 1);
    });
    return { count: members.length, catCount, ready: members.length > 0 };
  };

  const polList =
    selected.length === 0
      ? allPolicies
      : allPolicies.filter((p) => {
          const c = listingCategoryKey(p);
          return c != null && cats.has(c);
        });
  // A package surfaces if any member is in any selected category.
  const pkgList =
    selected.length === 0
      ? sets
      : sets.filter((_, i) => {
          const cc = metaFor(i).catCount;
          return selected.some((c) => cc.has(c));
        });
  const pkgShown = pkgAll ? pkgList : pkgList.slice(0, 8);
  const polShown = polAll ? polList : polList.slice(0, 12);
  const loading = policiesQ.isLoading || setsQ.isLoading;

  return (
    <div className="rm-page">
      <div className="rm-controls">
        <ListSearchPalette
          q={q}
          setQ={setQ}
          onSubmit={() => setSearch(q)}
          onClear={() => { setQ(""); setSearch(""); }}
          cats={cats}
          toggleCat={toggleCat}
          catCounts={polCatCounts}
          policies={allPolicies}
          packages={sets}
          onOpenDetail={(slug) => navigate(`/market/${encodeURIComponent(slug)}`)}
          locale={locale}
        />
        <select
          value={sort}
          onChange={(e) => setSort(e.target.value as ListingSort)}
          aria-label="sort"
        >
          <option value="popular">{ko ? "인기순" : "Most popular"}</option>
          <option value="new">{ko ? "신규순" : "Newest"}</option>
          <option value="rating">{ko ? "별점순" : "Top rated"}</option>
        </select>
      </div>

      {/* Selected category chips — below the search bar */}
      {selected.length > 0 && (
        <div className="rm-selected">
          {selected.map((c) => (
            <span
              key={c}
              className="rm-selchip"
              style={{ background: CATEGORY_COLOR[c].soft, color: CATEGORY_COLOR[c].ink }}
            >
              <span className="ic"><CategoryGlyph category={c} size={13} color={CATEGORY_COLOR[c].ink} /></span>
              {categoryNameOf(c, locale)}
              <button type="button" onClick={() => toggleCat(c)} aria-label="remove">
                ×
              </button>
            </span>
          ))}
          <button type="button" className="rm-selclear" onClick={() => setCats(new Set())}>
            {ko ? "모두 해제" : "Clear all"}
          </button>
        </div>
      )}

      {loading && <div className="market-status">{ko ? "불러오는 중…" : "Loading…"}</div>}
      {policiesQ.isError && (
        <div className="market-status market-error">
          {ko ? "마켓 로드 실패" : "Market load failed"}
        </div>
      )}

      {/* 프로토타입: .rm-controls/.rm-selected 는 .rm-page 직계, 두 섹션은
          별도 .rm-results 래퍼 안(스태거 인덱스 분리). 첫 섹션은 margin-top:8px. */}
      {!loading && (
        <div className="rm-results">
          <section className="rm-sec" style={{ marginTop: 8 }}>
            <div className="rm-lv-head">
              <h2>{ko ? "패키지" : "Packages"}</h2>
              <span className="count">{pkgList.length}</span>
              <span className="sub">
                {ko ? "여러 정책을 한 번에 켜는 묶음" : "Bundles that switch on many policies at once"}
              </span>
            </div>
            {pkgList.length === 0 ? (
              <p className="lv-empty">{ko ? "해당하는 패키지가 없습니다." : "No packages."}</p>
            ) : (
              <>
                <div className="rm-grid">
                  {pkgShown.map((l) => (
                    <PackageListCard
                      key={l.id}
                      listing={l}
                      meta={metaFor(sets.indexOf(l))}
                      categories={selected}
                      locale={locale}
                      onInstall={setInstallTarget}
                    />
                  ))}
                </div>
                {pkgList.length > 8 && (
                  <button type="button" className="rm-lv-more" onClick={() => setPkgAll((v) => !v)}>
                    {pkgAll
                      ? ko ? "접기" : "Show less"
                      : ko ? `더보기 (+${pkgList.length - 8})` : `Show more (+${pkgList.length - 8})`}
                  </button>
                )}
              </>
            )}
          </section>

          <section className="rm-sec">
            <div className="rm-lv-head">
              <h2>{ko ? "정책" : "Policies"}</h2>
              <span className="count">{polList.length}</span>
              <span className="sub">
                {ko ? "개별 정책 — 직접 골라 설치" : "Individual policies"}
              </span>
            </div>
            {polList.length === 0 ? (
              <p className="lv-empty">{ko ? "해당하는 정책이 없습니다." : "No policies."}</p>
            ) : (
              <>
                <div className="rm-grid">
                  {polShown.map((l) => (
                    <PolicyListCard key={l.id} listing={l} locale={locale} onInstall={setInstallTarget} />
                  ))}
                </div>
                {polList.length > 12 && (
                  <button type="button" className="rm-lv-more" onClick={() => setPolAll((v) => !v)}>
                    {polAll
                      ? ko ? "접기" : "Show less"
                      : ko ? `더보기 (+${polList.length - 12})` : `Show more (+${polList.length - 12})`}
                  </button>
                )}
              </>
            )}
          </section>
        </div>
      )}

      {installTarget && (
        <MarketInstallModal
          listing={installTarget}
          locale={locale}
          onClose={() => setInstallTarget(null)}
        />
      )}
    </div>
  );
}

/** Severity as a colored symbol (top-right of policy cards): deny = red
 * no-entry, warn = amber triangle. Replaces the "차단"/"경고" text label. */
function SeveritySymbol({ sev, size = 18 }: { sev: "deny" | "warn"; size?: number }) {
  return sev === "deny" ? (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" aria-hidden="true">
      <circle cx="12" cy="12" r="9" />
      <line x1="6.6" y1="6.6" x2="17.4" y2="17.4" />
    </svg>
  ) : (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 3.2 21 19H3z" />
      <line x1="12" y1="10" x2="12" y2="14" />
      <circle cx="12" cy="16.6" r="0.7" fill="currentColor" stroke="none" />
    </svg>
  );
}

/** Compact rating: ★ avg (count). Shows "★ 신규" when there are no reviews. */
function Rating({
  avg,
  count,
  locale,
  showCount = true,
}: {
  avg: number | null;
  count: number;
  locale: MarketLocale;
  showCount?: boolean;
}) {
  const ko = locale === "ko";
  if (!count || avg == null) {
    return <span className="rm-rating none">{ko ? "★ 신규" : "★ New"}</span>;
  }
  return (
    <span className="rm-rating" title={`${avg.toFixed(1)} / 5 · ${count}`}>
      <span className="s">★</span> {avg.toFixed(1)}
      {showCount && <> ({count})</>}
    </span>
  );
}

/** Install count as a download glyph + number (replaces "설치 N" text). */
function InstallCount({ n }: { n: number }) {
  return (
    <span className="rm-installs" title="installs">
      <svg
        width="12"
        height="12"
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
      {n.toLocaleString()}
    </span>
  );
}

function InstallBadge({
  installed,
  locale,
  onClick,
}: {
  installed: boolean;
  locale: MarketLocale;
  onClick: () => void;
}) {
  const ko = locale === "ko";
  return (
    <button
      type="button"
      className={`rm-badge${installed ? " installed" : ""}`}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onClick();
      }}
    >
      {installed ? (ko ? "설치됨" : "Installed") : ko ? "받기" : "Install"}
    </button>
  );
}

function PolicyListCard({
  listing,
  locale,
  onInstall,
}: {
  listing: ListingSummary;
  locale: MarketLocale;
  onInstall: (l: ListingSummary) => void;
}) {
  const ko = locale === "ko";
  const name = pickI18n(listing.display_name, locale) || listing.slug;
  const cat = listingCategoryKey(listing);
  const color = listingColor(listing);
  const sev = listing.severity;
  const oneLine = policyCopy(listing.slug)?.title || pickI18n(listing.description, locale) || "";
  return (
    <Link
      to={`/market/${encodeURIComponent(listing.slug)}`}
      className="rm-rcard"
      style={color ? { borderLeft: `3px solid ${color.hex}` } : undefined}
    >
      <div className="top">
        {sev && (
          <span className={`rm-rkind ${sev}`}>
            <SeveritySymbol sev={sev} size={11} />
            {" "}
            {sev === "deny" ? (ko ? "차단" : "Deny") : ko ? "경고" : "Warn"}
          </span>
        )}
        {cat && <span className="rm-rpub" style={{ marginLeft: "auto" }}>{categoryNameOf(cat, locale)}</span>}
      </div>
      <div className="rm-rname" style={{ fontSize: 15 }}>{name}</div>
      {oneLine && <div className="rm-card-line">{oneLine}</div>}
      <div className="rm-rfoot">
        <InstallCount n={listing.install_count} />
        <Rating avg={listing.rating_avg} count={listing.rating_count} locale={locale} showCount={false} />
        <InstallBadge installed={listing.is_installed} locale={locale} onClick={() => onInstall(listing)} />
      </div>
    </Link>
  );
}

/** Package card — leads with policy count + (in a category view) how many of
 * its policies belong to the active category (why it surfaced). */
function PackageListCard({
  listing,
  meta,
  categories,
  locale,
  onInstall,
}: {
  listing: ListingSummary;
  meta: { count: number; catCount: Map<CategoryKey, number>; ready: boolean };
  categories: CategoryKey[];
  locale: MarketLocale;
  onInstall: (l: ListingSummary) => void;
}) {
  const ko = locale === "ko";
  const name = pickI18n(listing.display_name, locale) || listing.slug;
  // How many of this package's policies match the active category filter.
  const match = categories.reduce((s, c) => s + (meta.catCount.get(c) ?? 0), 0);
  const matchColor = categories.length === 1 ? CATEGORY_COLOR[categories[0]] : null;
  const matchLabel =
    categories.length === 1
      ? `${categoryNameOf(categories[0], locale)} ${match}${ko ? "개 포함" : ""}`
      : ko ? `관련 ${match}개 포함` : `${match} matching`;
  const official = listing.publisher_tier === "official";
  // 멤버 카테고리 상위 2개 → #태그.
  const topCats = [...meta.catCount.entries()].sort((a, b) => b[1] - a[1]).slice(0, 2).map(([c]) => c);
  return (
    <Link
      to={`/market/${encodeURIComponent(listing.slug)}`}
      className="rm-rcard"
    >
      <div className="top">
        <span className={`rm-rkind ${official ? "official" : "community"}`}>
          {official ? (
            <>
              <PackageGlyphSm />
              {ko ? "공식 패키지" : "Official"}
            </>
          ) : (
            ko ? "커뮤니티" : "Community"
          )}
        </span>
        <span className="rm-rpub">
          {publisherDisplay(listing.publisher_tier, listing.publisher_email, locale)}
          {official && <span className="rm-vf"> ✓</span>}
        </span>
        <span className="rm-rmeta">
          {meta.ready && <>{ko ? "정책" : ""} <b>{meta.count}</b></>}
          {listing.current_version && <> · v<b>{listing.current_version}</b></>}
        </span>
      </div>
      <div className="rm-rname">{name}</div>
      <div className="rm-rtags">
        {topCats.map((c) => (
          <span key={c} className="rm-rtag">#{categoryNameOf(c, locale)}</span>
        ))}
        {match > 0 && (
          <span
            className="rm-rmatch"
            style={
              matchColor
                ? { background: matchColor.soft, color: matchColor.ink }
                : { background: "var(--warn-50)", color: "var(--warn-700)" }
            }
          >
            {matchLabel}
          </span>
        )}
      </div>
      <div className="rm-rfoot">
        <InstallCount n={listing.install_count} />
        <Rating avg={listing.rating_avg} count={listing.rating_count} locale={locale} showCount={false} />
        <InstallBadge installed={listing.is_installed} locale={locale} onClick={() => onInstall(listing)} />
      </div>
    </Link>
  );
}

// ─────────────────────────────────────────────────────────────────────────
// Listing visuals — category-driven (sets get a package glyph)
// ─────────────────────────────────────────────────────────────────────────

/** A listing's category: server `category` if present, else slug-derived.
 * Sets (packages) span categories, so they have none. */
function listingCategoryKey(l: ListingSummary): CategoryKey | null {
  if (l.kind !== "policy") return null;
  return isCategoryKey(l.category) ? l.category : categoryOf(l.slug);
}

function listingColor(l: ListingSummary) {
  const cat = listingCategoryKey(l);
  return cat ? CATEGORY_COLOR[cat] : null;
}

function parseSortParam(raw: string | null): ListingSort {
  if (raw === "new" || raw === "rating" || raw === "popular") return raw;
  return "popular";
}
