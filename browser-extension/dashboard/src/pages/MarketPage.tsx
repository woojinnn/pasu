import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import {
  listListings,
  pickI18n,
  type ListingKind,
  type ListingSort,
  type ListingSummary,
} from "../server-api";
import { Topbar } from "../shell/Topbar";

import { DomainGlyph, colorOf, domainNameOf } from "./market-domain";
import { useMarketLocale } from "./market-locale";

import "./market.css";

/**
 * `/market` — browse marketplace listings. Kind toggle (전체 / 정책 / 셋),
 * sort dropdown (인기순 / 신규순 / 별점순), search box, ko/en locale
 * switcher. Selecting a card navigates to `/market/:slug`.
 */
export function MarketPage() {
  const [locale, setLocale] = useMarketLocale();
  const [kind, setKind] = useState<ListingKind | "all">("all");
  const [sort, setSort] = useState<ListingSort>("popular");
  const [q, setQ] = useState("");
  const [search, setSearch] = useState("");

  const listingsQ = useQuery({
    queryKey: ["market-listings", { kind, sort, q: search }],
    queryFn: () =>
      listListings({
        kind: kind === "all" ? undefined : kind,
        sort,
        q: search.trim() || undefined,
        limit: 60,
      }),
  });

  return (
    <>
      <Topbar
        here="Market"
        subtitle={listingsQ.data ? `${listingsQ.data.length} listings` : "…"}
        right={
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
        }
      />

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
              {locale === "ko" ? "셋" : "Set"}
            </KindTab>
          </div>
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
            {locale === "ko" ? "마켓 로드 실패" : "Market load failed"}: {(listingsQ.error as Error).message}
          </div>
        )}

        {listingsQ.data && listingsQ.data.length === 0 && (
          <div className="market-empty">
            <h2>{locale === "ko" ? "마켓이 비어 있습니다" : "Market is empty"}</h2>
            <p>
              {locale === "ko"
                ? "아직 공개된 정책이 없습니다. 에디터에서 정책을 만들고 publish하면 이 자리에 나타납니다."
                : "No policies have been published yet. Build one in the editor and publish to see it here."}
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
    </>
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
  locale: "ko" | "en";
}) {
  const name = pickI18n(listing.display_name, locale);
  const desc = pickI18n(listing.description, locale);
  const color = colorOf(listing.domain);
  const domainLabel = domainNameOf(listing.domain, locale);

  // Family-keyed left border accent — same trick the original used so a
  // glance at the grid tells the user "this is a trading policy" etc.
  const accentStyle: React.CSSProperties = color
    ? {
        borderLeft: `3px solid ${color.hex}`,
      }
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
            ? locale === "ko" ? "셋" : "Set"
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
        {listing.current_version && (
          <span className="mc-stat mc-ver">v{listing.current_version}</span>
        )}
      </div>
    </Link>
  );
}
