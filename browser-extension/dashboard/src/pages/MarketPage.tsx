import { useMemo, useRef, useState } from "react";
import { useQueries, useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useTranslation } from "react-i18next";

import {
  getListing,
  listListings,
  pickI18n,
  type ListingKind,
  type ListingSort,
  type ListingSummary,
} from "../server-api";
import { publisherDisplay } from "../server-api/market";
import { Topbar } from "../shell/Topbar";
import { MarketInstallModal } from "./MarketInstallModal";

import {
  CATEGORY_COLOR,
  CATEGORY_ORDER,
  CategoryGlyph,
  categoryNameOf,
  categoryOf,
  isCategoryKey,
  type CategoryKey,
} from "./market-domain";
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
  const { t } = useTranslation("market");
  const [params] = useSearchParams();
  const view = params.get("view") === "list" ? "list" : "landing";

  return (
    <>
      <Topbar
        here="Market"
        subtitle={view === "list" ? t("page.allListings") : undefined}
        showNotifications={false}
        showSearch={false}
        right={
          view === "list" ? (
            <Link to="/market" className="back-link">
              ← {t("page.marketHome")}
            </Link>
          ) : undefined
        }
      />

      {view === "list" ? (
        <ListView locale={locale} initialParams={params} />
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
function MarketSearch() {
  const { t } = useTranslation("market");
  const [q, setQ] = useState("");
  const navigate = useNavigate();
  return (
    <form
      className="market-hero-search"
      role="search"
      onSubmit={(e) => {
        e.preventDefault();
        const term = q.trim();
        navigate(term ? `/market?view=list&q=${encodeURIComponent(term)}` : "/market?view=list");
      }}
    >
      <svg
        width="17"
        height="17"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <circle cx="11" cy="11" r="7" />
        <path d="m20 20-3.5-3.5" />
      </svg>
      <input
        type="text"
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder={t("search.placeholderHero")}
        aria-label={t("search.aria")}
      />
      {q && (
        <button
          type="button"
          className="market-hero-search-clear"
          onClick={() => setQ("")}
          aria-label="clear"
        >
          ×
        </button>
      )}
    </form>
  );
}

// ─────────────────────────────────────────────────────────────────────────
// Landing view
// ─────────────────────────────────────────────────────────────────────────

function LandingView({ locale }: { locale: MarketLocale }) {
  const heroQ = useQuery({
    queryKey: ["market-latest-packages"],
    queryFn: () =>
      listListings({
        kind: "set",
        sort: "new",
        limit: 6,
      }),
  });

  // Category counts are computed client-side from a single 100-item fetch;
  // 35 seed policies + 3 seed packages fits comfortably under that ceiling.
  const allForCountsQ = useQuery({
    queryKey: ["market-all-for-categories"],
    queryFn: () => listListings({ limit: 100 }),
  });

  // Category counts derive from each policy's slug (see `categoryOf`). Sets are
  // packages, not single-action policies, so they don't count toward a tile.
  const categoryCounts = useMemo(() => {
    const map = new Map<CategoryKey, number>();
    (allForCountsQ.data ?? []).forEach((l) => {
      const c = listingCategoryKey(l);
      if (c) map.set(c, (map.get(c) ?? 0) + 1);
    });
    return map;
  }, [allForCountsQ.data]);

  return (
    <div className="market-landing-v2">
      <MarketSearch />
      <div className="market-cols">
        <div className="market-col-main">
          <HeroPackages items={heroQ.data ?? []} loading={heroQ.isLoading} locale={locale} />
          <CategoryGrid counts={categoryCounts} locale={locale} />
        </div>
        <aside className="market-col-side">
          <RankingSidebar locale={locale} />
        </aside>
      </div>
    </div>
  );
}

function HeroPackages({
  items,
  loading,
  locale,
}: {
  items: ListingSummary[];
  loading: boolean;
  locale: MarketLocale;
}) {
  const { t } = useTranslation("market");
  const scrollerRef = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(0);
  const page = (dir: 1 | -1) => {
    const el = scrollerRef.current;
    if (el) el.scrollBy({ left: dir * el.clientWidth, behavior: "smooth" });
  };
  const goTo = (i: number) => {
    const el = scrollerRef.current;
    if (el) el.scrollTo({ left: i * el.clientWidth, behavior: "smooth" });
  };
  const onScroll = () => {
    const el = scrollerRef.current;
    if (el) setActive(Math.round(el.scrollLeft / Math.max(1, el.clientWidth)));
  };
  return (
    <section className="ml-section">
      <header className="ml-section-head">
        <span className="ml-eyebrow">LATEST PACKAGES</span>
      </header>
      {loading && <div className="ml-status">{t("common:loading")}</div>}
      {!loading && items.length === 0 && (
        <div className="ml-status">{t("landing.noPackages")}</div>
      )}
      {items.length > 0 && (
        <div className="pkg-carousel-wrap">
          <div className="pkg-carousel-viewport">
            {items.length > 1 && (
              <button
                type="button"
                className="carousel-arrow left"
                onClick={() => page(-1)}
                aria-label={t("carousel.prev")}
              >
                ‹
              </button>
            )}
            <div className="pkg-carousel" ref={scrollerRef} onScroll={onScroll}>
              {items.map((l) => (
                <PackageCard key={l.id} listing={l} locale={locale} />
              ))}
            </div>
            {items.length > 1 && (
              <button
                type="button"
                className="carousel-arrow right"
                onClick={() => page(1)}
                aria-label={t("carousel.next")}
              >
                ›
              </button>
            )}
          </div>
          {items.length > 1 && (
            <div className="carousel-dots" role="tablist">
              {items.map((l, i) => (
                <button
                  key={l.id}
                  type="button"
                  className={`carousel-dot${i === active ? " is-active" : ""}`}
                  aria-label={`${i + 1}`}
                  aria-selected={i === active}
                  onClick={() => goTo(i)}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

/** Full-width lead package card — one fills the carousel viewport; the
 * carousel pages through them one at a time (Four-Pillars hero feel). */
function PackageCard({ listing, locale }: { listing: ListingSummary; locale: MarketLocale }) {
  const { t } = useTranslation("market");
  const name = pickI18n(listing.display_name, locale) || listing.slug;
  const desc = pickI18n(listing.description, locale);
  return (
    <Link to={`/market/${encodeURIComponent(listing.slug)}`} className="featured-card">
      <div className="featured-card-glyph" aria-hidden="true">
        <PackageGlyph size={168} color="rgba(99, 126, 89, 0.18)" />
      </div>
      <div className="featured-card-body">
        <span className="featured-card-tag">
          {t("kind.package")}
          {listing.publisher_tier === "official" && (
            <span className="featured-card-official"> · {t("kind.official")}</span>
          )}
        </span>
        <h3 className="featured-card-name">{name}</h3>
        {desc && <p className="featured-card-desc">{desc}</p>}
        <div className="featured-card-foot">
          <InstallCount n={listing.install_count} />
          <Rating avg={listing.rating_avg} count={listing.rating_count} />
          <span className={`mc-install-badge featured-card-cta${listing.is_installed ? " is-installed" : ""}`}>
            {listing.is_installed ? t("install.installed") : t("install.get")}
          </span>
        </div>
      </div>
    </Link>
  );
}

function CategoryGrid({
  counts,
  locale,
}: {
  counts: Map<CategoryKey, number>;
  locale: MarketLocale;
}) {
  const { t } = useTranslation("market");
  return (
    <section className="ml-section">
      <header className="ml-section-head">
        <h2>{t("landing.categories")}</h2>
        <p className="ml-section-sub">{t("landing.categoriesSub")}</p>
      </header>
      <div className="cat-grid">
        {CATEGORY_ORDER.map((c) => {
          const color = CATEGORY_COLOR[c];
          const count = counts.get(c) ?? 0;
          return (
            <Link
              key={c}
              to={`/market?view=list&category=${c}`}
              className={`cat-tile family-${color.family}`}
              style={{ background: color.soft }}
            >
              <div className="cat-tile-icon">
                <CategoryGlyph category={c} size={22} color={color.hex} />
              </div>
              <div className="cat-tile-name" style={{ color: color.ink }}>
                {categoryNameOf(c, locale)}
              </div>
              <div className="cat-tile-count">{count}</div>
            </Link>
          );
        })}
      </div>
    </section>
  );
}

function RankingSidebar({ locale }: { locale: MarketLocale }) {
  const { t } = useTranslation("market");
  const [tab, setTab] = useState<ListingKind>("set");
  const topQ = useQuery({
    queryKey: ["market-top", tab],
    queryFn: () =>
      listListings({
        kind: tab,
        sort: "popular",
        limit: 10,
      }),
  });

  return (
    <section className="ml-section">
      <header className="ml-section-head">
        <span className="ml-eyebrow">TRENDING</span>
      </header>
      <div className="ranking-sidebar">
        <div className="rs-toggle-wrap">
          <div className="rs-toggle" role="group" aria-label="kind">
            <button
              type="button"
              className={`rs-tab${tab === "set" ? " is-active" : ""}`}
              onClick={() => setTab("set")}
            >
              {t("kind.package")}
            </button>
            <button
              type="button"
              className={`rs-tab${tab === "policy" ? " is-active" : ""}`}
              onClick={() => setTab("policy")}
            >
              {t("kind.policy")}
            </button>
          </div>
        </div>

        {topQ.isLoading && <div className="ml-status">{t("common:loading")}</div>}

      <ol className="rs-list">
        {(topQ.data ?? []).map((l, i) => {
          const color = listingColor(l);
          return (
            <li key={l.id} className="rs-row">
              <span className={`rs-rank rs-rank-${i < 3 ? i + 1 : "n"}`}>{i + 1}</span>
              <Link to={`/market/${encodeURIComponent(l.slug)}`} className="rs-link">
                <div className="rs-icon" style={color ? { background: color.soft } : undefined}>
                  <ListingIcon listing={l} size={14} />
                </div>
                <div className="rs-meta">
                  <div className="rs-name">{pickI18n(l.display_name, locale) || l.slug}</div>
                  <div className="rs-sub">
                    {publisherDisplay(l.publisher_tier, l.publisher_email, locale)}
                    {l.publisher_tier === "official" && <span className="mc-verified"> ✓</span>}
                  </div>
                </div>
                <div className="rs-count">
                  <InstallCount n={l.install_count} />
                  {l.rating_count > 0 && l.rating_avg != null && (
                    <Rating avg={l.rating_avg} count={l.rating_count} showCount={false} />
                  )}
                </div>
              </Link>
            </li>
          );
        })}
      </ol>

        <Link to={`/market?view=list&kind=${tab}&sort=popular`} className="rs-more">
          {t("landing.viewFullRanking")}
        </Link>
      </div>
    </section>
  );
}

// ─────────────────────────────────────────────────────────────────────────
// List view (`?view=list`)
// ─────────────────────────────────────────────────────────────────────────

function ListView({
  locale,
  initialParams,
}: {
  locale: MarketLocale;
  initialParams: URLSearchParams;
}) {
  const { t } = useTranslation("market");
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
    <div className="market-wrap">
      {/* Category rail — multi-select, above the search bar */}
      <div className="lv-catrail">
        {CATEGORY_ORDER.map((c) => {
          const on = cats.has(c);
          const col = CATEGORY_COLOR[c];
          const n = polCatCounts.get(c) ?? 0;
          return (
            <button
              key={c}
              type="button"
              className={`lv-catrail-chip${on ? " on" : ""}`}
              onClick={() => toggleCat(c)}
              style={on ? { color: col.hex, borderBottomColor: col.hex, background: col.soft } : undefined}
            >
              {categoryNameOf(c, locale)}
              {n > 0 && <span className="lv-catrail-n">{n}</span>}
            </button>
          );
        })}
      </div>

      <header className="market-controls">
        <form
          className="market-search"
          onSubmit={(e) => {
            e.preventDefault();
            setSearch(q);
          }}
        >
          <input
            type="text"
            placeholder={t("search.placeholder")}
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
          {search && (
            <button
              type="button"
              className="market-search-clear"
              onClick={() => {
                setQ("");
                setSearch("");
              }}
            >
              ×
            </button>
          )}
        </form>
        <select
          className="market-sort"
          value={sort}
          onChange={(e) => setSort(e.target.value as ListingSort)}
          aria-label="sort"
        >
          <option value="popular">{t("sort.popular")}</option>
          <option value="new">{t("sort.new")}</option>
          <option value="rating">{t("sort.rating")}</option>
        </select>
      </header>

      {/* Selected category chips — below the search bar */}
      {selected.length > 0 && (
        <div className="lv-selected">
          {selected.map((c) => (
            <span
              key={c}
              className="lv-sel-chip"
              style={{ background: CATEGORY_COLOR[c].soft, color: CATEGORY_COLOR[c].ink }}
            >
              {t("list.selectedCategory", { name: categoryNameOf(c, locale) })}
              <button type="button" onClick={() => toggleCat(c)} aria-label="remove">
                ×
              </button>
            </span>
          ))}
          <button type="button" className="lv-sel-clear" onClick={() => setCats(new Set())}>
            {t("list.clearAll")}
          </button>
        </div>
      )}

      {loading && <div className="market-status">{t("common:loading")}</div>}
      {policiesQ.isError && (
        <div className="market-status market-error">{t("list.loadFailed")}</div>
      )}

      {!loading && (
        <>
          <section className="lv-section">
            <div className="lv-section-head">
              <h2>
                {t("list.packagesHeading")} <span className="lv-count">{pkgList.length}</span>
              </h2>
              <p className="lv-section-sub">{t("list.packagesSub")}</p>
            </div>
            {pkgList.length === 0 ? (
              <p className="lv-empty">{t("list.noPackages")}</p>
            ) : (
              <>
                <div className="lv-grid">
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
                  <button type="button" className="lv-more" onClick={() => setPkgAll((v) => !v)}>
                    {pkgAll ? t("list.showLess") : t("list.showMore", { n: pkgList.length - 8 })}
                  </button>
                )}
              </>
            )}
          </section>

          <section className="lv-section">
            <div className="lv-section-head">
              <h2>
                {t("list.policiesHeading")} <span className="lv-count">{polList.length}</span>
              </h2>
              <p className="lv-section-sub">{t("list.policiesSub")}</p>
            </div>
            {polList.length === 0 ? (
              <p className="lv-empty">{t("list.noPolicies")}</p>
            ) : (
              <>
                <div className="lv-grid">
                  {polShown.map((l) => (
                    <PolicyListCard key={l.id} listing={l} locale={locale} onInstall={setInstallTarget} />
                  ))}
                </div>
                {polList.length > 12 && (
                  <button type="button" className="lv-more" onClick={() => setPolAll((v) => !v)}>
                    {polAll ? t("list.showLess") : t("list.showMore", { n: polList.length - 12 })}
                  </button>
                )}
              </>
            )}
          </section>
        </>
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
function SeveritySymbol({ sev }: { sev: "deny" | "warn" }) {
  const { t } = useTranslation("market");
  const label = sev === "deny" ? t("severity.deny") : t("severity.warn");
  return (
    <span className={`lv-sev-sym sev-${sev}`} title={label} aria-label={label}>
      {sev === "deny" ? (
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round">
          <circle cx="12" cy="12" r="9" />
          <line x1="6.6" y1="6.6" x2="17.4" y2="17.4" />
        </svg>
      ) : (
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
          <path d="M12 3.2 21 19H3z" />
          <line x1="12" y1="10" x2="12" y2="14" />
          <circle cx="12" cy="16.6" r="0.7" fill="currentColor" stroke="none" />
        </svg>
      )}
    </span>
  );
}

/** Compact rating: ★ avg (count). Shows "★ 신규" when there are no reviews. */
function Rating({
  avg,
  count,
  showCount = true,
}: {
  avg: number | null;
  count: number;
  showCount?: boolean;
}) {
  const { t } = useTranslation("market");
  if (!count || avg == null) {
    return <span className="lv-rating is-none">{t("rating.new")}</span>;
  }
  return (
    <span className="lv-rating" title={`${avg.toFixed(1)} / 5 · ${count}`}>
      <span className="lv-rating-star">★</span> {avg.toFixed(1)}
      {showCount && <span className="lv-rating-n"> ({count})</span>}
    </span>
  );
}

/** Install count as a download glyph + number (replaces "설치 N" text). */
function InstallCount({ n }: { n: number }) {
  return (
    <span className="lv-installs" title="installs">
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
      {n}
    </span>
  );
}

function InstallBadge({
  installed,
  onClick,
}: {
  installed: boolean;
  onClick: () => void;
}) {
  const { t } = useTranslation("market");
  return (
    <button
      type="button"
      className={`mc-install-badge mc-install-btn${installed ? " is-installed" : ""}`}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();
        onClick();
      }}
    >
      {t("install.get")}
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
  const { t, i18n } = useTranslation("market");
  const name = pickI18n(listing.display_name, locale) || listing.slug;
  const cat = listingCategoryKey(listing);
  const color = listingColor(listing);
  const sev = listing.severity;
  const titleKey = `policy.${listing.slug}.title`;
  const oneLine = i18n.exists(`market:${titleKey}`)
    ? t(titleKey)
    : pickI18n(listing.description, locale) || "";
  return (
    <Link
      to={`/market/${encodeURIComponent(listing.slug)}`}
      className="market-card lv-card"
      style={color ? { borderLeft: `3px solid ${color.hex}` } : undefined}
    >
      <div className="lv-card-top">
        {cat && (
          <span
            className="lv-cat-chip"
            style={{ background: CATEGORY_COLOR[cat].soft, color: CATEGORY_COLOR[cat].ink }}
          >
            {categoryNameOf(cat, locale)}
          </span>
        )}
        {sev && <SeveritySymbol sev={sev} />}
      </div>
      <h3 className="lv-card-name">{name}</h3>
      {oneLine && <p className="lv-card-line">{oneLine}</p>}
      <div className="lv-card-foot">
        <InstallCount n={listing.install_count} />
        <Rating avg={listing.rating_avg} count={listing.rating_count} showCount={false} />
        <InstallBadge installed={listing.is_installed} onClick={() => onInstall(listing)} />
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
  const { t } = useTranslation("market");
  const name = pickI18n(listing.display_name, locale) || listing.slug;
  const desc = pickI18n(listing.description, locale);
  // How many of this package's policies match the active category filter.
  const match = categories.reduce((s, c) => s + (meta.catCount.get(c) ?? 0), 0);
  const matchColor = categories.length === 1 ? CATEGORY_COLOR[categories[0]] : null;
  const matchLabel =
    categories.length === 1
      ? t("list.matchSingle", { category: categoryNameOf(categories[0], locale), n: match })
      : t("list.matchMulti", { n: match });
  return (
    <Link
      to={`/market/${encodeURIComponent(listing.slug)}`}
      className="market-card lv-card lv-pkg-card"
    >
      <div className="lv-card-top">
        <span className="lv-kind">{t("kind.package")}</span>
        {match > 0 && (
          <span
            className="lv-match"
            style={
              matchColor
                ? { background: matchColor.soft, color: matchColor.ink }
                : { background: "var(--sage-50, #f1f6ee)", color: "var(--sage-700, #44583d)" }
            }
          >
            {matchLabel}
          </span>
        )}
      </div>
      <h3 className="lv-card-name">{name}</h3>
      {desc && <p className="lv-card-line">{desc}</p>}
      {meta.ready && (
        <div className="lv-compose">
          <span>
            <strong>{meta.count}</strong> {t("unit.policies")}
          </span>
        </div>
      )}
      <div className="lv-card-foot">
        <InstallCount n={listing.install_count} />
        <Rating avg={listing.rating_avg} count={listing.rating_count} showCount={false} />
        <InstallBadge installed={listing.is_installed} onClick={() => onInstall(listing)} />
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

/** Box/package line glyph for set listings. */
function PackageGlyph({ size = 18, color }: { size?: number; color?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color ?? "var(--slate-400)"}
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 8l9-5 9 5-9 5-9-5zM3 8v8l9 5 9-5V8M12 13v8" />
    </svg>
  );
}

/** Unified listing icon: category glyph for policies, package glyph for sets.
 * Never renders an empty box (the old `domain` glyph was null for sets). */
function ListingIcon({ listing, size = 18 }: { listing: ListingSummary; size?: number }) {
  const cat = listingCategoryKey(listing);
  if (cat) return <CategoryGlyph category={cat} size={size} color={CATEGORY_COLOR[cat].hex} />;
  return <PackageGlyph size={size} />;
}

function parseSortParam(raw: string | null): ListingSort {
  if (raw === "new" || raw === "rating" || raw === "popular") return raw;
  return "popular";
}
