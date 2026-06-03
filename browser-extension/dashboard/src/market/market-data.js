/* Scopeball Market — 데이터 레이어 (window.MARKET_GLOSSARY 위에서 동작)
   - 정책 121개 정규화
   - 도메인 색 매핑 (Cloudy Pond 베이스 3색 → 명도 단계로 12 구분)
   - 공식 패키지 큐레이션 (의도태그 기준, 런타임 계산 → 카운트 정직)
   - i18n / 검색 / 필터 / 정렬 / 게이팅
   주의: glossary엔 publisher·rating·install 필드가 없음 → 소셜 지표는 만들지 않음.
   카탈로그 전체가 default_policies 큐레이션이므로 작성자=공식(official)로 정직 표기. */
(function () {
  var G = window.MARKET_GLOSSARY;

  // ── 도메인 표시 순서 (security 앵커 최상단, 이후 큰 도메인 순) ──
  var DOMAIN_ORDER = ["security", "swap", "perp", "lending", "nft", "airdrop",
                      "portfolio", "ammlp", "bridge", "sale", "staking", "gov"];

  // ── 도메인 액센트색: 베이스 팔레트(Cyan/Sage/Slate)를 명도 단계로 ──
  // family: trading=Cyan, safety/holding=Sage, assets/infra=Slate
  var DOMAIN_COLOR = {
    // Cyan family — 거래(trading)
    swap:      { family: "cyan",  hex: "#688186", soft: "#DCEAED", ink: "#2B3639" },
    perp:      { family: "cyan",  hex: "#485A5E", soft: "#CAE0E4", ink: "#2B3639" },
    ammlp:     { family: "cyan",  hex: "#85A4AB", soft: "#EDF4F6", ink: "#2B3639" },
    bridge:    { family: "cyan",  hex: "#A4C9D1", soft: "#EDF4F6", ink: "#485A5E" },
    // Sage family — 보안·보유(safety / holding)
    security:  { family: "sage",  hex: "#637E59", soft: "#EBF3E8", ink: "#283523" },
    portfolio: { family: "sage",  hex: "#7FA172", soft: "#EBF3E8", ink: "#283523" },
    staking:   { family: "sage",  hex: "#9CC58D", soft: "#F8F9F6", ink: "#44583D" },
    airdrop:   { family: "sage",  hex: "#44583D", soft: "#D9E9D3", ink: "#283523" },
    // Slate family — 자산·인프라(assets / infra)
    lending:   { family: "slate", hex: "#384455", soft: "#D7DBDF", ink: "#0D1118" },
    nft:       { family: "slate", hex: "#697485", soft: "#EFF0F2", ink: "#1B222C" },
    sale:      { family: "slate", hex: "#2A3441", soft: "#D7DBDF", ink: "#0D1118" },
    gov:       { family: "slate", hex: "#9099A5", soft: "#EFF0F2", ink: "#2A3441" }
  };

  // 도메인 아이콘 (단순 라인 패스, 24x24 viewBox)
  var DOMAIN_ICON = {
    swap:      "M7 7h11l-3-3M17 17H6l3 3",
    perp:      "M3 17l5-6 4 3 5-7 4 4",
    lending:   "M3 10h18M5 10v8h14v-8M9 14h6",
    security:  "M12 3l7 3v5c0 4-3 7-7 9-4-2-7-5-7-9V6z",
    nft:       "M4 4h16v16H4zM8 10a1.5 1.5 0 100-3 1.5 1.5 0 000 3M4 16l5-4 4 3 3-2 4 3",
    airdrop:   "M12 3a6 6 0 016 6c0 3-6 9-6 9S6 12 6 9a6 6 0 016-6M12 21v-3",
    portfolio: "M21 12a9 9 0 11-9-9v9z",
    ammlp:     "M12 3c3 4 6 7 6 10a6 6 0 01-12 0c0-3 3-6 6-10z",
    bridge:    "M3 16c0-4 3-7 9-7s9 3 9 7M3 16v3M21 16v3M8 13v6M16 13v6",
    sale:      "M4 8l8-4 8 4-8 4zM4 8v8l8 4 8-4V8",
    staking:   "M4 18h16M6 18V9M10 18V6M14 18V11M18 18V8",
    gov:       "M5 21h14M6 21V9M18 21V9M4 9l8-5 8 5M9 13v4M15 13v4"
  };

  // ── i18n ──
  function pick(node, locale) {
    if (!node) return "";
    return node[locale] != null ? node[locale] : (node.ko != null ? node.ko : node.en || "");
  }
  function tChrome(path, locale) {
    var parts = path.split("."), n = G.chrome;
    for (var i = 0; i < parts.length; i++) { n = n && n[parts[i]]; }
    return pick(n, locale);
  }
  function domainName(key, locale) { return pick(G.domains[key], locale); }
  function intentMeta(key) { return G.intents[key]; }
  function intentTag(key, locale) {
    var m = G.intents[key]; if (!m) return "#" + key;
    return locale === "en" ? m.tag_en : m.tag_ko;
  }
  function intentLabel(key, locale) { var m = G.intents[key]; return m ? pick(m, locale) : key; }
  function readinessMeta(key) { return G.chrome.readiness[key]; }
  function severityMeta(key) { return G.chrome.severity[key]; }

  // ── 정책 정규화 ──
  var POLICIES = Object.keys(G.policies).map(function (slug) {
    var p = G.policies[slug];
    return {
      slug: slug,
      domain: p.domain,
      intents: p.intents || [],
      severity: p.severity,
      evalClass: p.evalClass,
      readiness: p.readiness,
      name: p.display_name,        // {en, ko}
      publisher: "official"        // 카탈로그 전체가 공식 큐레이션
    };
  });
  var BY_SLUG = {};
  POLICIES.forEach(function (p) { BY_SLUG[p.slug] = p; });

  function policiesByDomain(d) { return POLICIES.filter(function (p) { return p.domain === d; }); }
  function domainCount(d) { return policiesByDomain(d).length; }

  // ── 공식 패키지 큐레이션 (의도/도메인 기준, 카운트는 실제 매칭으로) ──
  function matchPolicies(crit) {
    return POLICIES.filter(function (p) {
      if (crit.domain && p.domain !== crit.domain) return false;
      if (crit.intents && !crit.intents.some(function (i) { return p.intents.indexOf(i) >= 0; })) return false;
      if (crit.domains && crit.domains.indexOf(p.domain) < 0) return false;
      return true;
    });
  }
  var PACKAGE_DEFS = [
    { id: "essentials", anchor: true,
      name: { ko: "지갑 보안 기본 세트", en: "Wallet Essentials" },
      tagline: { ko: "모든 지갑에 권장하는 필수 방어선", en: "The baseline defense every wallet should run" },
      crit: { domain: "security" }, intents: ["drainer", "phishing", "unlimited"] },
    { id: "swap-kit",
      name: { ko: "스왑 안전 키트", en: "Swap Safety Kit" },
      tagline: { ko: "슬리피지·샌드위치로부터 모든 스왑을 보호", en: "Shield every swap from slippage and sandwiches" },
      crit: { domain: "swap", intents: ["slippage", "sandwich"] }, intents: ["slippage", "sandwich"] },
    { id: "liq-pack",
      name: { ko: "청산 방어팩", en: "Liquidation Defense Pack" },
      tagline: { ko: "레버리지·담보 포지션을 급락에서 지킨다", en: "Keep leveraged positions safe through a crash" },
      crit: { intents: ["liquidation"] }, intents: ["liquidation"] },
    { id: "drainer-shield",
      name: { ko: "드레이너·피싱 차단팩", en: "Drainer & Phishing Shield" },
      tagline: { ko: "악성 승인과 위장 서명을 입구에서 차단", en: "Block malicious approvals and spoofed signatures at the door" },
      crit: { intents: ["drainer", "phishing"] }, intents: ["drainer", "phishing"] },
    { id: "approval-hygiene",
      name: { ko: "승인 위생팩", en: "Approval Hygiene Pack" },
      tagline: { ko: "무제한·방치 승인이라는 공격면을 정리", en: "Trim the attack surface of unlimited and stale approvals" },
      crit: { intents: ["approval", "unlimited"] }, intents: ["approval", "unlimited"] },
    { id: "stable-guard",
      name: { ko: "디페그·컴플라이언스 가드", en: "De-peg & Compliance Guard" },
      tagline: { ko: "스테이블 붕괴와 제재 위반을 동시에 감시", en: "Watch for de-pegs and sanctions in one set" },
      crit: { intents: ["depeg", "compliance"] }, intents: ["depeg", "compliance"] },
    { id: "discipline-guard",
      name: { ko: "수령자·거래규율 가드", en: "Recipient & Discipline Guard" },
      tagline: { ko: "잘못된 수령자와 충동 거래로부터 보호", en: "Guard against wrong recipients and impulsive trades" },
      crit: { intents: ["recipient", "overtrade"] }, intents: ["recipient", "overtrade"] }
  ];
  var PACKAGES = PACKAGE_DEFS.map(function (def) {
    var members = matchPolicies(def.crit);
    // readiness 비중
    var ready = members.filter(function (m) { return m.readiness === "ready"; }).length;
    // 대표 도메인 (가장 많이 등장)
    var domCount = {};
    members.forEach(function (m) { domCount[m.domain] = (domCount[m.domain] || 0) + 1; });
    var primaryDomain = Object.keys(domCount).sort(function (a, b) { return domCount[b] - domCount[a]; })[0] || "security";
    return {
      id: def.id, anchor: !!def.anchor, name: def.name, tagline: def.tagline,
      intents: def.intents, members: members, count: members.length,
      readyCount: ready, primaryDomain: primaryDomain, publisher: "official"
    };
  }).filter(function (p) { return p.count > 0; });
  var PKG_BY_ID = {};
  PACKAGES.forEach(function (p) { PKG_BY_ID[p.id] = p; });

  // ── 검색: 의도태그가 도메인 경계를 넘어 결과를 모은다 ──
  function search(q, locale) {
    var query = (q || "").trim().toLowerCase();
    if (!query) return { policies: POLICIES.slice(), packages: PACKAGES.slice(), matchedIntents: [] };
    var matchedIntents = Object.keys(G.intents).filter(function (k) {
      var m = G.intents[k];
      var hay = [m.en, m.ko, m.tag_en, m.tag_ko, k].join(" ").toLowerCase();
      return query.split(/\s+/).some(function (tok) { return hay.indexOf(tok) >= 0; });
    });
    function policyMatch(p) {
      if (matchedIntents.length && p.intents.some(function (i) { return matchedIntents.indexOf(i) >= 0; })) return true;
      var hay = [pick(p.name, "en"), pick(p.name, "ko"), p.slug, domainName(p.domain, "en"), domainName(p.domain, "ko")].join(" ").toLowerCase();
      return query.split(/\s+/).every(function (tok) { return hay.indexOf(tok) >= 0; });
    }
    var pols = POLICIES.filter(policyMatch);
    var pkgs = PACKAGES.filter(function (pk) {
      if (matchedIntents.length && pk.intents.some(function (i) { return matchedIntents.indexOf(i) >= 0; })) return true;
      var hay = [pick(pk.name, "en"), pick(pk.name, "ko")].join(" ").toLowerCase();
      return query.split(/\s+/).some(function (tok) { return hay.indexOf(tok) >= 0; });
    });
    return { policies: pols, packages: pkgs, matchedIntents: matchedIntents };
  }

  // ── 필터 (데이터에 있는 축만: readiness, severity) ──
  function applyFilters(list, f) {
    return list.filter(function (p) {
      if (f.readiness && f.readiness.length && f.readiness.indexOf(p.readiness) < 0) return false;
      if (f.severity && f.severity.length && f.severity.indexOf(p.severity) < 0) return false;
      if (f.intent && f.intent.length && !f.intent.some(function (i) { return p.intents.indexOf(i) >= 0; })) return false;
      if (f.domain && f.domain.length && f.domain.indexOf(p.domain) < 0) return false;
      return true;
    });
  }

  // ── 정렬: 즉시작동 우선 ──
  var READY_RANK = { ready: 0, external: 1, soon: 2 };
  function sortForDisplay(list, mode) {
    var arr = list.slice();
    if (mode === "new") { arr.reverse(); return arr; }
    if (mode === "rating") {
      // 리뷰 있는 항목만 별점 내림차순, 리뷰 없는 항목은 뒤로 (0점으로 끌어올리지 않음)
      arr.sort(function (a, b) {
        var ra = ratingForPolicy(a.slug), rb = ratingForPolicy(b.slug);
        if (ra && rb) { if (rb.avg !== ra.avg) return rb.avg - ra.avg; return rb.count - ra.count; }
        if (ra && !rb) return -1;
        if (!ra && rb) return 1;
        return READY_RANK[a.readiness] - READY_RANK[b.readiness];
      });
      return arr;
    }
    arr.sort(function (a, b) {
      var d = READY_RANK[a.readiness] - READY_RANK[b.readiness];
      if (d) return d;
      return pick(a.name, "ko").localeCompare(pick(b.name, "ko"));
    });
    return arr;
  }

  // ── 게이팅: 준비중은 세트에 담을 수 없음 ──
  function canAddToSet(readiness) { return readiness !== "soon"; }

  // ── 포함 패키지 조회 ──
  function packagesContaining(slug) {
    return PACKAGES.filter(function (pk) {
      return pk.members.some(function (m) { return m.slug === slug; });
    });
  }

  // ── 별점 집계 (단일 출처: window.SEED_REVIEWS, 런타임 참조) ──
  // 리뷰 없으면 null 반환 — 숫자를 만들지 않는다.
  function aggregateRating(slugs) {
    var seed = window.SEED_REVIEWS || [];
    var revs = seed.filter(function (r) { return r.rating && slugs.indexOf(r.policySlug) >= 0; });
    if (!revs.length) return null;
    var sum = revs.reduce(function (a, r) { return a + r.rating; }, 0);
    return { avg: sum / revs.length, count: revs.length };
  }
  function ratingForPolicy(slug) { return aggregateRating([slug]); }
  function ratingForPackage(pkg) {
    return aggregateRating(pkg.members.map(function (m) { return m.slug; }));
  }

  // ── 버전 (단일 출처 시드 맵) — 정책 slug / 패키지 id → "vX.Y" ──
  // 값 없는 항목은 버전 미표시. detail·카드·업데이트가 전부 이 맵에서만 읽는다.
  var VERSIONS = {
    // packages
    "essentials": "v2.3", "swap-kit": "v1.6", "liq-pack": "v2.0", "drainer-shield": "v2.1",
    "approval-hygiene": "v1.4", "stable-guard": "v1.1", "discipline-guard": "v1.0",
    // policies
    "aave-hf-floor-warn": "v1.5", "aave-borrow-fraction-warn": "v1.2", "aave-emode-leverage-warn": "v1.1",
    "aave-withdraw-hf-floor-deny": "v1.3", "aave-utilization-high-warn": "v1.0", "aave-oracle-stale-borrow-warn": "v1.1",
    "air-permit-on-held-token-deny": "v2.0", "air-merkle-without-proof-warn": "v1.2", "air-recipient-not-self-deny": "v1.4",
    "air-unknown-token-warn": "v1.1", "air-upfront-payment-warn": "v1.0",
    "ammlp-remove-exit-asymmetry-warn": "v1.0", "ammlp-uni-v3v4-out-of-range-warn": "v1.2", "ammlp-collect-recipient-not-self-deny": "v1.1",
    "bridge-target-not-allowlisted-deny": "v1.7", "bridge-min-out-haircut-warn": "v1.0", "bridge-permission-change-deny": "v1.3",
    "nft-untrusted-blur-root-deny": "v1.6", "nft-bid-weth-unlimited-warn": "v1.3", "nft-setapprovalforall-conduit-warn": "v1.2",
    "nft-seaport-wildcard-zone-deny": "v1.1",
    "gov-delegatee-allowlist-deny": "v1.0", "gov-redelegate-large-power-warn": "v1.1",
    "gas-cost-usd-cap-deny": "v1.4", "gas-cost-ratio-warn": "v1.2", "unknown-blind-sign-warning": "v2.2",
    "swap-price-impact-warn": "v1.8", "swap-permit2-spender-not-router-deny": "v1.3",
    "signature-chain-mismatch-permit-warn": "v1.1", "permit-allowance-horizon-warn": "v1.0",
    "market-order-verifyingcontract-spoof-deny": "v1.5", "multicall-hidden-approval-warn": "v1.2",
    "lp-commit-platform-allowlist-deny": "v1.0"
  };
  function versionFor(id) { return VERSIONS[id] || null; }

  window.Market = {
    G: G,
    DOMAIN_ORDER: DOMAIN_ORDER, DOMAIN_COLOR: DOMAIN_COLOR, DOMAIN_ICON: DOMAIN_ICON,
    POLICIES: POLICIES, BY_SLUG: BY_SLUG, PACKAGES: PACKAGES, PKG_BY_ID: PKG_BY_ID,
    pick: pick, tChrome: tChrome, domainName: domainName,
    intentMeta: intentMeta, intentTag: intentTag, intentLabel: intentLabel,
    readinessMeta: readinessMeta, severityMeta: severityMeta,
    policiesByDomain: policiesByDomain, domainCount: domainCount,
    search: search, applyFilters: applyFilters, sortForDisplay: sortForDisplay,
    canAddToSet: canAddToSet, packagesContaining: packagesContaining,
    aggregateRating: aggregateRating, ratingForPolicy: ratingForPolicy, ratingForPackage: ratingForPackage,
    VERSIONS: VERSIONS, versionFor: versionFor
  };
})();
