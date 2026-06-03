/* Scopeball Market — 앱 셸 + 상태 + 라우팅 + Tweaks */
const { useEffect, useRef } = React;

const NAV_ICONS = {
  home: "M3 11.5 12 4l9 7.5 M5 10v10h14V10",
  editor: ["M3 3h7v7H3z", "M14 3h7v7h-7z", "M14 14h7v7h-7z", "M3 14h7v7H3z"],
  sim: ["M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18", "M10 8.5 16 12l-6 3.5z"],
  monitor: "M3 12h4l3 8 4-16 3 8h4",
  market: ["M3 9h18l-1.5 11H4.5z", "M3 9l2-5h14l2 5", "M9 13a3 3 0 0 0 6 0"],
  history: ["M3 3v18h18", "M7 14l4-4 4 3 5-7"],
  gear: ICONS.gear,
};

function NavRail({ locale }) {
  const items = [
    { key: "home", label: { ko: "홈", en: "Home" }, href: "Home.html", icon: NAV_ICONS.home },
    { key: "editor", label: { ko: "에디터", en: "Editor" }, href: "Editor v7.html", icon: NAV_ICONS.editor },
    { key: "sim", label: { ko: "시뮬레이션", en: "Simulation" }, href: "Simulation.html", icon: NAV_ICONS.sim },
    { key: "monitor", label: { ko: "모니터링", en: "Monitoring" }, href: "Monitoring.html", icon: NAV_ICONS.monitor },
    { key: "market", label: { ko: "마켓", en: "Market" }, href: null, icon: NAV_ICONS.market, active: true, pill: Market.POLICIES.length },
  ];
  const lower = [
    { key: "history", label: { ko: "히스토리", en: "History" }, href: "History.html", icon: NAV_ICONS.history },
  ];
  function NavItem({ it }) {
    const inner = (
      <React.Fragment>
        <span className="icon"><Ico d={it.icon} w={19} /></span>
        <span className="label">{it.label[locale === "en" ? "en" : "ko"]}</span>
        {it.pill != null && <span className="pill">{it.pill}</span>}
      </React.Fragment>
    );
    if (it.active) return <a className="nav-item active" aria-current="page">{inner}</a>;
    return <a className="nav-item" href={it.href}>{inner}</a>;
  }
  return (
    <nav className="nav-rail">
      <div className="nav-logo">
        <span className="mark">S</span>
        <span className="word">Scopeball</span>
      </div>
      <div className="nav-divider"></div>
      <div className="nav-group">
        {items.map((it) => <NavItem key={it.key} it={it} />)}
      </div>
      <div className="nav-divider"></div>
      <div className="nav-group">
        {lower.map((it) => <NavItem key={it.key} it={it} />)}
        <a className="nav-item is-disabled" aria-disabled="true">
          <span className="icon"><Ico d={NAV_ICONS.gear} w={19} /></span>
          <span className="label">{locale === "en" ? "Settings" : "설정"}</span>
          <span className="soon">soon</span>
        </a>
      </div>
    </nav>
  );
}

const MK_TABS = [
  { key: "popular", label: { ko: "인기", en: "Popular" } },
  { key: "market", label: { ko: "마켓", en: "Market" } },
  { key: "community", label: { ko: "커뮤니티", en: "Community" } },
  { key: "updates", label: { ko: "업데이트", en: "Updates" } },
];

function Placeholder({ tab, locale }) {
  const txt = {
    community: { ko: "커뮤니티 — 검증된 평가 & 자유 토론", en: "Community — verified reviews & discussion" },
    updates: { ko: "업데이트 — 신규 공개 & 버전 갱신 피드", en: "Updates — new releases & version feed" },
  }[tab];
  return (
    <div className="mk-placeholder">
      <div className="pico"><Ico d={tab === "community" ? "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" : "M12 8v4l3 3M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0"} w={30} /></div>
      <h2>{txt[locale === "en" ? "en" : "ko"]}</h2>
      <p><span data-lang="ko">이번 범위에는 포함되지 않았습니다 (준비 중).</span><span data-lang="en">Not part of this scope yet (coming soon).</span></p>
    </div>
  );
}

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "density": "regular",
  "domainColor": "tonal",
  "dimSoon": 0.5,
  "packageStack": true
}/*EDITMODE-END*/;

function App() {
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const [locale, setLocale] = useState("ko");
  const [route, setRoute] = useState({ screen: "popular" });
  const [items, setItems] = useState([]);
  const [query, setQuery] = useState("");
  const [filters, setFilters] = useState({});
  const [sort, setSort] = useState("ready_first");
  const [setOpen, setSetOpen] = useState(false);
  const [toast, setToast] = useState(null);
  const searchRef = useRef(null);
  const toastTimer = useRef(null);

  // body attrs for locale + tweak classes
  useEffect(() => { document.body.setAttribute("data-locale", locale); }, [locale]);
  useEffect(() => {
    document.body.className = [
      "density-" + t.density,
      t.domainColor === "mono" ? "dcolor-mono" : "",
      t.packageStack ? "" : "nostack",
    ].filter(Boolean).join(" ");
    document.documentElement.style.setProperty("--dim", t.dimSoon);
  }, [t.density, t.domainColor, t.packageStack, t.dimSoon]);

  // "/" focus search
  useEffect(() => {
    function onKey(e) {
      if (e.key === "/" && document.activeElement.tagName !== "INPUT") {
        e.preventDefault(); searchRef.current && searchRef.current.focus();
      }
      if (e.key === "Escape") setSetOpen(false);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  function fireToast(msg) {
    setToast(msg);
    clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 2200);
  }

  function isInSet(type, id) { return items.some((i) => i.type === type && i.id === id); }
  function toggleItem(type, id, action) {
    if (action === "notify") {
      fireToast(locale === "en" ? "We'll notify you on release" : "출시되면 알림을 보내드릴게요");
      return;
    }
    setItems((prev) => {
      const exists = prev.some((i) => i.type === type && i.id === id);
      if (action === "remove" || (action !== "add" && exists)) {
        return prev.filter((i) => !(i.type === type && i.id === id));
      }
      if (exists) return prev;
      return prev.concat({ type, id });
    });
    if (action === "add") {
      fireToast(locale === "en" ? "Added to your set" : "세트에 담았어요");
    }
  }

  const ctx = {
    isInSet, toggleItem,
    openPolicy: (slug) => { setRoute({ screen: "policy", slug }); scrollTop(); },
    openPackage: (id) => { setRoute({ screen: "package", pkgId: id }); scrollTop(); },
    goBrowse: (opts) => {
      opts = opts || {};
      setFilters(opts.domain ? { domain: [opts.domain] } : {});
      setRoute({ screen: "browse" }); scrollTop();
    },
    goPopular: () => { setRoute({ screen: "popular" }); scrollTop(); },
    goCommunity: (slug) => { setRoute({ screen: "community", slug: slug || null }); scrollTop(); },
    commitDraft: () => { fireToast(locale === "en" ? "Added to your wallet draft" : "지갑 Draft에 일괄 추가했어요"); setSetOpen(false); },
    saveSet: () => fireToast(locale === "en" ? "Saved as your set" : "내 세트로 저장했어요"),
    shareSet: () => fireToast(locale === "en" ? "Share link copied" : "공유 링크를 복사했어요"),
  };
  const bodyRef = useRef(null);
  function scrollTop() { setTimeout(() => { bodyRef.current && bodyRef.current.scrollTo({ top: 0 }); }, 0); }

  // tab → screen sync
  const activeTab = (route.screen === "popular") ? "popular"
    : (route.screen === "community" || route.screen === "updates") ? route.screen : "market";

  function onTab(key) {
    if (key === "popular") ctx.goPopular();
    else if (key === "market") { setRoute({ screen: "browse" }); scrollTop(); }
    else { setRoute({ screen: key }); scrollTop(); }
  }

  let screen;
  if (route.screen === "popular") screen = <PopularScreen locale={locale} ctx={ctx} />;
  else if (route.screen === "browse") screen = <BrowseScreen locale={locale} query={query} setQuery={setQuery} filters={filters} setFilters={setFilters} sort={sort} setSort={setSort} ctx={ctx} />;
  else if (route.screen === "policy") screen = <PolicyDetail slug={route.slug} locale={locale} ctx={ctx} />;
  else if (route.screen === "package") screen = <PackageDetail pkgId={route.pkgId} locale={locale} ctx={ctx} />;
  else if (route.screen === "community") screen = <CommunityScreen locale={locale} ctx={ctx} fireToast={fireToast} initialSlug={route.slug} />;
  else if (route.screen === "updates") screen = <UpdatesScreen locale={locale} ctx={ctx} fireToast={fireToast} />;
  else screen = <Placeholder tab={route.screen} locale={locale} />;

  const setCount = items.length;

  return (
    <div className="app">
      <NavRail locale={locale} />
      <div className="market-main">
        <header className="mk-header">
          <span className="mk-title">Market</span>
          <div className="mk-tabs">
            {MK_TABS.map((tab) => (
              <button key={tab.key} className={"mk-tab" + (activeTab === tab.key ? " active" : "")} onClick={() => onTab(tab.key)}>
                {tab.label[locale === "en" ? "en" : "ko"]}
                {tab.soon && <span className="tab-soon">soon</span>}
              </button>
            ))}
          </div>
          <div className="mk-actions">
            <div className="mk-search">
              <svg className="s-ico" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><circle cx="11" cy="11" r="7" /><path d="m21 21-4.3-4.3" /></svg>
              <input ref={searchRef} value={query}
                placeholder={Market.tChrome("search.placeholder", locale)}
                onChange={(e) => { setQuery(e.target.value); if (route.screen !== "browse") { setRoute({ screen: "browse" }); } }}
                onFocus={() => { if (route.screen !== "browse") setRoute({ screen: "browse" }); }} />
              <span className="s-kbd">/</span>
            </div>
            <button className="mk-iconbtn" onClick={() => setSetOpen(true)} title="Set">
              <Ico d={"M3 7v13h18V7M3 7l3-4h12l3 4M3 7h18M9 11a3 3 0 0 0 6 0"} w={19} />
              {setCount > 0 && <span className="count">{setCount}</span>}
            </button>
            <div className="mk-locale">
              <button className={locale === "ko" ? "on" : ""} onClick={() => setLocale("ko")}>KO</button>
              <button className={locale === "en" ? "on" : ""} onClick={() => setLocale("en")}>EN</button>
            </div>
          </div>
        </header>
        <div className="mk-body" ref={bodyRef}>
          {screen}
        </div>
      </div>

      <SetPanel open={setOpen} onClose={() => setSetOpen(false)} locale={locale} items={items} ctx={ctx} />

      <div className={"mk-toast" + (toast ? " show" : "")}>
        {toast && <React.Fragment><Ico d={ICONS.check} w={16} />{toast}</React.Fragment>}
      </div>

      <TweaksPanel title="Tweaks">
        <TweakSection label={locale === "en" ? "Cards" : "카드"} />
        <TweakRadio label={locale === "en" ? "Density" : "밀도"} value={t.density}
          options={["compact", "regular", "comfy"]} onChange={(v) => setTweak("density", v)} />
        <TweakToggle label={locale === "en" ? "Package stack" : "패키지 스택 비주얼"} value={t.packageStack}
          onChange={(v) => setTweak("packageStack", v)} />
        <TweakSection label={locale === "en" ? "Domain color" : "도메인 색"} />
        <TweakRadio label={locale === "en" ? "Mapping" : "매핑 방식"} value={t.domainColor}
          options={["tonal", "mono"]} onChange={(v) => setTweak("domainColor", v)} />
        <TweakSection label={locale === "en" ? "Readiness" : "작동 상태"} />
        <TweakSlider label={locale === "en" ? "Coming-soon dim" : "준비중 흐림"} value={t.dimSoon}
          min={0.25} max={0.75} step={0.05} onChange={(v) => setTweak("dimSoon", v)} />
      </TweaksPanel>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<App />);
