/* Scopeball Market — 화면: Popular / Browse / Detail / SetPanel */

// 공통: 카드 그리드 렌더 (정책 + 패키지 혼합)
function CardGrid({ items, locale, ctx }) {
  return (
    <div className="mk-grid">
      {items.map((it) =>
        it._kind === "pkg" ? (
          <PackageCard key={"p" + it.id} pkg={it} locale={locale}
            inSet={ctx.isInSet("package", it.id)}
            onToggle={(a) => ctx.toggleItem("package", it.id, a)}
            onOpen={ctx.openPackage} />
        ) : (
          <PolicyCard key={it.slug} policy={it} locale={locale}
            inSet={ctx.isInSet("policy", it.slug)}
            onToggle={(a) => ctx.toggleItem("policy", it.slug, a)}
            onOpen={ctx.openPolicy} />
        )
      )}
    </div>
  );
}

function SecHead({ title, sub, moreLabel, onMore }) {
  return (
    <div className="mk-sec-head">
      <h2>{title}</h2>
      {sub && <span className="sub">{sub}</span>}
      {moreLabel && (
        <button className="more" onClick={onMore}>{moreLabel}<Ico d={ICONS.arrow} w={14} /></button>
      )}
    </div>
  );
}

/* ════════════ POPULAR (홈) ════════════ */
function PopularScreen({ locale, ctx }) {
  const hero = Market.PKG_BY_ID["liq-pack"] || Market.PACKAGES[1];
  const essentials = Market.PKG_BY_ID["essentials"];
  const top3 = Market.PACKAGES.filter((p) => !p.anchor && p.id !== hero.id)
    .sort((a, b) => b.count - a.count).slice(0, 3);
  const essCards = Market.sortForDisplay(essentials.members).slice(0, 4);
  const fresh = Market.POLICIES.slice(-3).reverse();

  return (
    <div className="mk-canvas">
      {/* Hero */}
      <div className="mk-hero">
        <div className="glow"></div>
        <div className="hcol">
          <div className="kicker">{Market.tChrome("section.pick_of_day", locale)}</div>
          <h1>{Market.pick(hero.name, locale)}</h1>
          <div className="lede">{Market.pick(hero.tagline, locale)}</div>
          <div className="hmeta">
            <span>{Market.G.chrome.publisher.official.icon} <span data-lang="ko">지갑방위대 공식</span><span data-lang="en">By Wallet Defense Force</span></span>
            <span className="dot"></span>
            <span><span data-lang="ko">정책 {hero.count}개</span><span data-lang="en">{hero.count} policies</span></span>
            <span className="dot"></span>
            <span><span data-lang="ko">{hero.readyCount}개 즉시작동</span><span data-lang="en">{hero.readyCount} ready now</span></span>
          </div>
          <div className="hactions">
            <button className="addbtn lg" onClick={(e) => { e.stopPropagation(); ctx.toggleItem("package", hero.id, ctx.isInSet("package", hero.id) ? "remove" : "add"); }}>
              {ctx.isInSet("package", hero.id)
                ? <span><Ico d={ICONS.check} w={16} /> <span data-lang="ko">담김</span><span data-lang="en">In set</span></span>
                : <span>{Market.tChrome("action.add_package", locale)}</span>}
            </button>
            <button className="addbtn lg ghost" onClick={() => ctx.openPackage(hero.id)}>
              <span data-lang="ko">자세히</span><span data-lang="en">Details</span>
            </button>
          </div>
        </div>
        <div className="hstack">
          {Market.sortForDisplay(hero.members).slice(0, 4).map((m) => (
            <div className="hpol" key={m.slug}>
              <DomainGlyph domain={m.domain} size={16} />
              <span className="nm">{Market.pick(m.name, locale)}</span>
            </div>
          ))}
          {hero.count > 4 && <div className="more">+{hero.count - 4} <span data-lang="ko">더</span><span data-lang="en">more</span></div>}
        </div>
      </div>

      {/* Realtime Top 3 */}
      <div className="mk-section">
        <SecHead title={Market.tChrome("market_mode.top3", locale)} />
        <div className="mk-row3">
          {top3.map((pkg, i) => (
            <div key={pkg.id} style={{ position: "relative" }}>
              <span className={"badge rank r" + (i + 1)} style={{ position: "absolute", top: 12, left: 12, zIndex: 2 }}>{i + 1}</span>
              <PackageCard pkg={pkg} locale={locale}
                inSet={ctx.isInSet("package", pkg.id)}
                onToggle={(a) => ctx.toggleItem("package", pkg.id, a)}
                onOpen={ctx.openPackage} />
            </div>
          ))}
        </div>
      </div>

      {/* Essentials anchor */}
      <div className="mk-section">
        <div className="anchor-band">
          <div className="ab-head">
            <span className="ab-ico"><Ico d={ICONS.shield} w={19} /></span>
            <div>
              <h2>{Market.tChrome("section.essentials", locale)}</h2>
              <div className="sub"><span data-lang="ko">도메인과 무관하게 모든 지갑에 권장</span><span data-lang="en">Recommended for every wallet, whatever you do</span></div>
            </div>
            <button className="more" style={{ marginLeft: "auto" }} onClick={() => ctx.openPackage("essentials")}>
              <span data-lang="ko">세트 전체</span><span data-lang="en">Full set</span><Ico d={ICONS.arrow} w={14} />
            </button>
          </div>
          <CardGrid items={essCards} locale={locale} ctx={ctx} />
        </div>
      </div>

      {/* Domain quick grid */}
      <div className="mk-section">
        <SecHead title={<span><span data-lang="ko">도메인 바로가기</span><span data-lang="en">Browse by domain</span></span>} />
        <div className="dgrid">
          {Market.DOMAIN_ORDER.map((d) => {
            const c = Market.DOMAIN_COLOR[d];
            return (
              <button className="dtile" key={d} onClick={() => ctx.goBrowse({ domain: d })}>
                <span className="dt-ico" style={{ background: c.soft, color: c.hex }}><DomainGlyph domain={d} size={22} /></span>
                <span className="dt-txt">
                  <span className="dt-name">{Market.domainName(d, locale)}</span>
                  <span className="dt-count">{Market.domainCount(d)} <span data-lang="ko">정책</span><span data-lang="en">policies</span></span>
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {/* New this week */}
      <div className="mk-section">
        <SecHead title={Market.tChrome("section.new_this_week", locale)}
          moreLabel={<span><span data-lang="ko">전체 보기</span><span data-lang="en">See all</span></span>}
          onMore={() => ctx.goBrowse({})} />
        <CardGrid items={fresh} locale={locale} ctx={ctx} />
      </div>
    </div>
  );
}

/* ════════════ BROWSE (둘러보기) ════════════ */
const READINESS_KEYS = ["ready", "external", "soon"];
const SEVERITY_KEYS = ["deny", "warn"];

function FilterRail({ locale, filters, setFilters, activeCount, onClear }) {
  function toggle(axis, val) {
    setFilters((f) => {
      const cur = f[axis] || [];
      const next = cur.indexOf(val) >= 0 ? cur.filter((x) => x !== val) : cur.concat(val);
      return Object.assign({}, f, { [axis]: next });
    });
  }
  return (
    <div className="frail">
      <div className="fgroup">
        <div className="fg-title">{Market.tChrome("filter.readiness", locale)}{activeCount ? <span className="cnt">{activeCount}</span> : null}</div>
        <div className="fchips">
          {READINESS_KEYS.map((k) => {
            const m = Market.readinessMeta(k);
            return (
              <button key={k} className={"fchip" + ((filters.readiness || []).indexOf(k) >= 0 ? " on" : "")} onClick={() => toggle("readiness", k)}>
                <span style={{ fontSize: 11 }}>{m.icon}</span>{locale === "en" ? m.en : m.ko}
              </button>
            );
          })}
        </div>
      </div>
      <div className="fgroup">
        <div className="fg-title">{Market.tChrome("filter.strength", locale)}</div>
        <div className="fchips">
          {SEVERITY_KEYS.map((k) => {
            const m = Market.severityMeta(k);
            return (
              <button key={k} className={"fchip" + ((filters.severity || []).indexOf(k) >= 0 ? " on" : "")} onClick={() => toggle("severity", k)}>
                {locale === "en" ? m.en : m.ko}
              </button>
            );
          })}
        </div>
      </div>
      <div className="fgroup">
        <div className="fg-title">{Market.tChrome("filter.publisher", locale)}</div>
        <div className="fchips">
          <button className="fchip on" style={{ cursor: "default" }}>{Market.G.chrome.publisher.official.icon} {locale === "en" ? "Official" : "공식"}</button>
        </div>
        <div style={{ fontSize: 11, color: "var(--slate-300)", marginTop: 7 }}>
          <span data-lang="ko">카탈로그 전체가 검증된 공식 정책입니다</span>
          <span data-lang="en">Every catalog policy is official &amp; verified</span>
        </div>
      </div>
      {(activeCount > 0) && <button className="fclear" onClick={onClear}><span data-lang="ko">필터 초기화</span><span data-lang="en">Clear filters</span></button>}
    </div>
  );
}

function BrowseScreen({ locale, query, setQuery, filters, setFilters, sort, setSort, ctx }) {
  const res = Market.search(query, locale);
  let pols = Market.applyFilters(res.policies, filters);
  pols = Market.sortForDisplay(pols, sort);
  const pkgs = res.packages.map((p) => Object.assign({ _kind: "pkg" }, p));
  // 둘러보기: 패키지 먼저 약간, 그다음 정책 (혼합 그리드)
  const mixed = pkgs.slice(0, 3).concat(pols);
  const activeCount = (filters.readiness || []).length + (filters.severity || []).length + (filters.domain || []).length;
  const total = pols.length + pkgs.length;

  return (
    <div className="mk-canvas">
      {/* matched intent tags */}
      {res.matchedIntents.length > 0 && (
        <div className="matched">
          <span className="ml">{Market.tChrome("search.matched_tags", locale)}</span>
          {res.matchedIntents.map((i) => (
            <button key={i} className="htag on" onClick={() => setQuery("")}>{Market.intentTag(i, locale)}</button>
          ))}
        </div>
      )}
      <div className="browse">
        <FilterRail locale={locale} filters={filters} setFilters={setFilters} activeCount={activeCount}
          onClear={() => setFilters({})} />
        <div>
          <div className="results-head">
            <span className="rc"><b>{total}</b> <span data-lang="ko">개 결과</span><span data-lang="en">results</span>
              {(filters.domain && filters.domain.length === 1) && <span> · {Market.domainName(filters.domain[0], locale)}</span>}
            </span>
            <div className="sortsel">
              <label style={{ fontSize: 12, color: "var(--slate-400)" }} data-lang="ko">정렬</label>
              <label style={{ fontSize: 12, color: "var(--slate-400)" }} data-lang="en">Sort</label>
              <select value={sort} onChange={(e) => setSort(e.target.value)}>
                <option value="ready_first">{Market.tChrome("sort.ready_first", locale)}</option>
                <option value="rating">{Market.tChrome("sort.rating", locale)}</option>
                <option value="new">{Market.tChrome("sort.new", locale)}</option>
              </select>
            </div>
          </div>
          {total === 0 ? (
            <div className="empty">
              <Ico d={ICONS.x} w={48} />
              <h3>{Market.tChrome("search.no_results", locale)}</h3>
              <div className="sugg">
                {["slippage", "drainer", "liquidation", "approval"].map((i) => (
                  <button key={i} className="htag" onClick={() => { setQuery(Market.intentLabel(i, locale)); }}>{Market.intentTag(i, locale)}</button>
                ))}
              </div>
            </div>
          ) : (
            <CardGrid items={query ? mixed : pkgs.concat(pols)} locale={locale} ctx={ctx} />
          )}
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { CardGrid, SecHead, PopularScreen, FilterRail, BrowseScreen, READINESS_KEYS, SEVERITY_KEYS });
