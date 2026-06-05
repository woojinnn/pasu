import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useSearchParams } from "react-router-dom";

import {
  listListings,
  pickI18n,
  type ListingKind,
  type ListingSort,
  type ListingSummary,
} from "../server-api";
import { formatYmd, publisherDisplay } from "../server-api/market";
import { Topbar } from "../shell/Topbar";

import {
  DOMAIN_COLOR,
  DOMAIN_NAME,
  DOMAIN_ORDER,
  DomainGlyph,
  colorOf,
  domainNameOf,
  type DomainKey,
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
  const [locale, setLocale] = useMarketLocale();
  const [params] = useSearchParams();
  const view = params.get("view") === "list" ? "list" : "landing";

  return (
    <>
      <Topbar
        here="Market"
        subtitle={view === "list" ? (locale === "ko" ? "전체 목록" : "All listings") : undefined}
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
            {view === "list" && (
              <Link to="/market" className="back-link">
                ← {locale === "ko" ? "마켓 홈" : "Market home"}
              </Link>
            )}
          </>
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
// Landing view
// ─────────────────────────────────────────────────────────────────────────

function LandingView({ locale }: { locale: MarketLocale }) {
  const heroQ = useQuery({
    queryKey: ["market-hero-packages"],
    queryFn: () =>
      listListings({
        kind: "set",
        publisher_tier: "official",
        sort: "popular",
        limit: 3,
      }),
  });

  // Category counts are computed client-side from a single 100-item fetch;
  // 35 seed policies + 3 seed packages fits comfortably under that ceiling.
  const allForCountsQ = useQuery({
    queryKey: ["market-all-for-categories"],
    queryFn: () => listListings({ limit: 100 }),
  });

  const domainCounts = useMemo(() => {
    const map = new Map<string, number>();
    (allForCountsQ.data ?? []).forEach((l) => {
      if (l.domain) map.set(l.domain, (map.get(l.domain) ?? 0) + 1);
    });
    return map;
  }, [allForCountsQ.data]);

  return (
    <div className="market-landing">
      <main className="ml-main">
        <HeroPackages items={heroQ.data ?? []} loading={heroQ.isLoading} locale={locale} />
        <CategoryGrid counts={domainCounts} locale={locale} />
      </main>
      <aside className="ml-sidebar">
        <RankingSidebar locale={locale} />
      </aside>
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
  return (
    <section className="ml-section">
      <header className="ml-section-head">
        <h2>{locale === "ko" ? "오늘의 추천 패키지" : "Today's package picks"}</h2>
        <p className="ml-section-sub">
          {locale === "ko"
            ? "공식 운영팀이 큐레이션한 핵심 패키지부터 시작해 보세요."
            : "Start with the official packages curated by the operations team."}
        </p>
      </header>
      {loading && <div className="ml-status">{locale === "ko" ? "불러오는 중…" : "Loading…"}</div>}
      {!loading && items.length === 0 && (
        <div className="ml-status">
          {locale === "ko" ? "공개된 공식 패키지가 없습니다." : "No official packages yet."}
        </div>
      )}
      {items.length > 0 && (
        <div className="hero-row">
          {items.map((l) => (
            <HeroCard key={l.id} listing={l} locale={locale} />
          ))}
        </div>
      )}
    </section>
  );
}

function HeroCard({ listing, locale }: { listing: ListingSummary; locale: MarketLocale }) {
  const color = colorOf(listing.domain);
  const name = pickI18n(listing.display_name, locale) || listing.slug;
  const desc = pickI18n(listing.description, locale);
  return (
    <Link
      to={`/market/${encodeURIComponent(listing.slug)}`}
      className={`hero-card${color ? ` family-${color.family}` : ""}`}
      style={color ? { borderTopColor: color.hex } : undefined}
    >
      <div className="hero-card-head">
        <div className="hero-card-icon" style={color ? { background: color.soft } : undefined}>
          <DomainGlyph domain={listing.domain} size={28} />
        </div>
        <span className="mc-tier tier-official">{locale === "ko" ? "공식" : "Official"}</span>
      </div>
      <h3 className="hero-card-name">{name}</h3>
      {desc && <p className="hero-card-desc">{desc}</p>}
      <div className="hero-card-foot">
        <span className="hero-card-stat">
          <strong>{listing.install_count}</strong> {locale === "ko" ? "설치" : "installs"}
        </span>
        {listing.rating_count > 0 && listing.rating_avg != null && (
          <span className="hero-card-stat">
            ★ {listing.rating_avg.toFixed(1)}
            <span className="mc-stat-mute"> ({listing.rating_count})</span>
          </span>
        )}
        <span
          className={`mc-install-badge${listing.is_installed ? " is-installed" : ""}`}
        >
          {listing.is_installed
            ? locale === "ko" ? "설치됨" : "Installed"
            : locale === "ko" ? "받기" : "Install"}
        </span>
      </div>
    </Link>
  );
}

function CategoryGrid({
  counts,
  locale,
}: {
  counts: Map<string, number>;
  locale: MarketLocale;
}) {
  return (
    <section className="ml-section">
      <header className="ml-section-head">
        <h2>{locale === "ko" ? "카테고리" : "Categories"}</h2>
        <p className="ml-section-sub">
          {locale === "ko"
            ? "도메인별로 정리된 정책·패키지를 탐색하세요."
            : "Browse policies and packages organized by domain."}
        </p>
      </header>
      <div className="cat-grid">
        {DOMAIN_ORDER.map((d: DomainKey) => {
          const color = DOMAIN_COLOR[d];
          const count = counts.get(d) ?? 0;
          return (
            <Link
              key={d}
              to={`/market?view=list&domain=${d}`}
              className={`cat-tile family-${color.family}`}
              style={{ background: color.soft }}
            >
              <div className="cat-tile-icon">
                <DomainGlyph domain={d} size={22} color={color.hex} />
              </div>
              <div className="cat-tile-name" style={{ color: color.ink }}>
                {DOMAIN_NAME[d][locale]}
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
    <div className="ranking-sidebar">
      <header className="rs-head">
        <span className="rs-title">{locale === "ko" ? "다운로드 순위" : "Top downloads"}</span>
        <div className="rs-toggle" role="group" aria-label="kind">
          <button
            type="button"
            className={`rs-tab${tab === "set" ? " is-active" : ""}`}
            onClick={() => setTab("set")}
          >
            {locale === "ko" ? "패키지" : "Package"}
          </button>
          <button
            type="button"
            className={`rs-tab${tab === "policy" ? " is-active" : ""}`}
            onClick={() => setTab("policy")}
          >
            {locale === "ko" ? "정책" : "Policy"}
          </button>
        </div>
      </header>

      {topQ.isLoading && <div className="ml-status">{locale === "ko" ? "불러오는 중…" : "Loading…"}</div>}

      <ol className="rs-list">
        {(topQ.data ?? []).map((l, i) => {
          const color = colorOf(l.domain);
          return (
            <li key={l.id} className="rs-row">
              <span className={`rs-rank rs-rank-${i < 3 ? i + 1 : "n"}`}>{i + 1}</span>
              <Link to={`/market/${encodeURIComponent(l.slug)}`} className="rs-link">
                <div className="rs-icon" style={color ? { background: color.soft } : undefined}>
                  <DomainGlyph domain={l.domain} size={14} />
                </div>
                <div className="rs-meta">
                  <div className="rs-name">{pickI18n(l.display_name, locale) || l.slug}</div>
                  <div className="rs-sub">
                    {publisherDisplay(l.publisher_tier, l.publisher_email, locale)}
                  </div>
                </div>
                <div className="rs-count">
                  <span className="rs-count-num">{l.install_count}</span>
                  <span className="rs-count-label">
                    {locale === "ko" ? "설치" : "inst."}
                  </span>
                </div>
              </Link>
            </li>
          );
        })}
      </ol>

      <Link to={`/market?view=list&kind=${tab}&sort=popular`} className="rs-more">
        {locale === "ko" ? "전체 순위 보기 →" : "View full ranking →"}
      </Link>
    </div>
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
  const initialKind = parseKindParam(initialParams.get("kind"));
  const initialDomain = initialParams.get("domain") ?? "";
  const initialSort = parseSortParam(initialParams.get("sort"));
  const initialQ = initialParams.get("q") ?? "";

  const [kind, setKind] = useState<ListingKind | "all">(initialKind);
  const [domain, setDomain] = useState<string>(initialDomain);
  const [sort, setSort] = useState<ListingSort>(initialSort);
  const [q, setQ] = useState(initialQ);
  const [search, setSearch] = useState(initialQ);

  const listingsQ = useQuery({
    queryKey: ["market-listings", { kind, domain, sort, q: search }],
    queryFn: () =>
      listListings({
        kind: kind === "all" ? undefined : kind,
        domain: domain || undefined,
        sort,
        q: search.trim() || undefined,
        limit: 60,
      }),
  });

  return (
    <div className="market-wrap">
      <header className="market-controls">
        <div className="market-tabs">
          <KindTab active={kind === "all"} onClick={() => setKind("all")}>
            {locale === "ko" ? "전체" : "All"}
          </KindTab>
          <KindTab active={kind === "policy"} onClick={() => setKind("policy")}>
            {locale === "ko" ? "정책" : "Policy"}
          </KindTab>
          <KindTab active={kind === "set"} onClick={() => setKind("set")}>
            {locale === "ko" ? "패키지" : "Package"}
          </KindTab>
        </div>
        {domain && (
          <div className="market-active-filter">
            <span className="map-label">
              {locale === "ko" ? "도메인" : "Domain"}:
            </span>
            <span className="map-value">{domainNameOf(domain, locale)}</span>
            <button
              type="button"
              className="map-clear"
              onClick={() => setDomain("")}
              aria-label="clear domain"
            >
              ×
            </button>
          </div>
        )}
        <form
          className="market-search"
          onSubmit={(e) => {
            e.preventDefault();
            setSearch(q);
          }}
        >
          <input
            type="text"
            placeholder={locale === "ko" ? "정책 이름으로 검색" : "Search by name"}
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
          <option value="popular">{locale === "ko" ? "인기순" : "Most popular"}</option>
          <option value="new">{locale === "ko" ? "신규순" : "Newest"}</option>
          <option value="rating">{locale === "ko" ? "별점순" : "Top rated"}</option>
        </select>
      </header>

      {listingsQ.isLoading && (
        <div className="market-status">{locale === "ko" ? "불러오는 중…" : "Loading…"}</div>
      )}

      {listingsQ.isError && (
        <div className="market-status market-error">
          {locale === "ko" ? "마켓 로드 실패" : "Market load failed"}:{" "}
          {(listingsQ.error as Error).message}
        </div>
      )}

      {listingsQ.data && listingsQ.data.length === 0 && (
        <div className="market-empty">
          <h2>{locale === "ko" ? "결과가 없습니다" : "No matches"}</h2>
          <p>
            {locale === "ko"
              ? "필터 조건을 바꾸거나 검색어를 비워보세요."
              : "Try a different filter or clear the search."}
          </p>
        </div>
      )}

      {listingsQ.data && listingsQ.data.length > 0 && (
        <div className="market-grid">
          {listingsQ.data.map((l) => (
            <ListingCard key={l.id} listing={l} locale={locale} />
          ))}
        </div>
      )}
    </div>
  );
}

function KindTab({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      className={`market-tab${active ? " is-active" : ""}`}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function ListingCard({
  listing,
  locale,
}: {
  listing: ListingSummary;
  locale: MarketLocale;
}) {
  const name = pickI18n(listing.display_name, locale);
  const desc = pickI18n(listing.description, locale);
  const color = colorOf(listing.domain);
  const domainLabel = domainNameOf(listing.domain, locale);

  const accentStyle: React.CSSProperties = color
    ? { borderLeft: `3px solid ${color.hex}` }
    : {};

  return (
    <Link
      to={`/market/${encodeURIComponent(listing.slug)}`}
      className={`market-card kind-${listing.kind}${color ? ` family-${color.family}` : ""}`}
      style={accentStyle}
    >
      <div className="mc-head">
        <div className="mc-icon-wrap" style={color ? { background: color.soft } : undefined}>
          <DomainGlyph domain={listing.domain} size={18} />
        </div>
        <span className={`mc-kind kind-${listing.kind}`}>
          {listing.kind === "set"
            ? locale === "ko" ? "패키지" : "Package"
            : locale === "ko" ? "정책" : "Policy"}
        </span>
        {listing.severity && (
          <span className={`mc-sev sev-${listing.severity}`}>
            {listing.severity === "deny"
              ? locale === "ko" ? "차단" : "Block"
              : locale === "ko" ? "경고" : "Warn"}
          </span>
        )}
        {listing.publisher_tier !== "community" && (
          <span className={`mc-tier tier-${listing.publisher_tier}`}>
            {listing.publisher_tier === "official"
              ? locale === "ko" ? "공식" : "Official"
              : locale === "ko" ? "검증" : "Verified"}
          </span>
        )}
      </div>
      <h3 className="mc-name">{name || listing.slug}</h3>
      {desc && <p className="mc-desc">{desc}</p>}
      <div className="mc-publisher">
        <span className="mc-publisher-name">
          {publisherDisplay(listing.publisher_tier, listing.publisher_email, locale)}
        </span>
        <span className="mc-publisher-dot">·</span>
        <span className="mc-publisher-date">{formatYmd(listing.created_at)}</span>
      </div>
      {domainLabel && <div className="mc-domain">{domainLabel}</div>}
      <div className="mc-foot">
        <span className="mc-stat">
          <span className="mc-stat-num">{listing.install_count}</span>{" "}
          {locale === "ko" ? "설치" : "installs"}
        </span>
        {listing.rating_count > 0 && listing.rating_avg != null && (
          <span className="mc-stat">
            ★ {listing.rating_avg.toFixed(1)}
            <span className="mc-stat-mute"> ({listing.rating_count})</span>
          </span>
        )}
        <span
          className={`mc-install-badge${listing.is_installed ? " is-installed" : ""}`}
        >
          {listing.is_installed
            ? locale === "ko" ? "설치됨" : "Installed"
            : locale === "ko" ? "설치" : "Install"}
        </span>
      </div>
    </Link>
  );
}

function parseKindParam(raw: string | null): ListingKind | "all" {
  if (raw === "policy" || raw === "set") return raw;
  return "all";
}

function parseSortParam(raw: string | null): ListingSort {
  if (raw === "new" || raw === "rating" || raw === "popular") return raw;
  return "popular";
}
