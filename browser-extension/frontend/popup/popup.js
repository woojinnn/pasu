/* ============================================================
   DAMBI popup — 정책 관리 (패키지 우선 · 주소별 스코프)
   본문 토글 = appliedByAddress[activeAddress] 읽기/쓰기
   전체 오버레이 지갑 스위처 · 알림 강도 퀵 시트 · 설정 오버레이
   온보딩(4-step)도 이 팝업 안에서 렌더 — 별도 welcome 탭 없음(R4 §1).

   ── 빌드 분리 ───────────────────────────────────────────────
     window.DambiStore        : 데모(localStorage) ↔ 연동(service-worker) 교체
     window.DAMBI_ASSET_BASE  : 에셋 경로 ("picture/" | "../picture/")
     window.DAMBI_DASHBOARD_URL: 대시보드 배포 URL (없으면 기본값)
   ============================================================ */
const S = window.DambiStore;
function esc(s) { return String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c])); }
const ASSET = (typeof window !== "undefined" && window.DAMBI_ASSET_BASE) || "picture/";
// §2.2 — dev 하드코딩 제거. 배포 URL 은 빌드시 window.DAMBI_DASHBOARD_URL 로 주입.
const DASHBOARD_URL = (typeof window !== "undefined" && window.DAMBI_DASHBOARD_URL) || "https://app.dambi.xyz/";

/* ---------- icons ---------- */
const I = {
  chev: '<svg class="pc-chev" viewBox="0 0 24 24" fill="none"><path d="M9 6l6 6-6 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  caretDown: '<svg class="chev" viewBox="0 0 24 24" fill="none" width="15" height="15"><path d="M6 9l6 6 6-6" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  copy: '<svg viewBox="0 0 24 24" fill="none" width="14" height="14"><rect x="9" y="9" width="11" height="11" rx="2.5" stroke="currentColor" stroke-width="1.8"/><path d="M5 15V5a2 2 0 0 1 2-2h8" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>',
  check: '<svg viewBox="0 0 24 24" fill="none" width="13" height="13"><path d="M20 6 9 17l-5-5" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  back: '<svg viewBox="0 0 24 24" fill="none" width="18" height="18"><path d="M15 18l-6-6 6-6" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  plus: '<svg viewBox="0 0 24 24" fill="none" width="18" height="18"><path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/></svg>',
  trash: '<svg viewBox="0 0 24 24" fill="none" width="15" height="15"><path d="M5 7h14M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  pin: '<svg viewBox="0 0 24 24" fill="none" width="15" height="15"><path d="M9 3h6l-1 6 3 3v2H7v-2l3-3-1-6z" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/><path d="M12 14v7" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/></svg>',
  sort: '<svg viewBox="0 0 24 24" fill="none" width="16" height="16"><path d="M7 4v16M7 20l-3-3M7 4l3 3M17 20V4M17 4l3 3M17 20l-3-3" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  go: '<svg class="ac-go" viewBox="0 0 24 24" fill="none" width="16" height="16"><path d="M9 6l6 6-6 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  ext: '<svg viewBox="0 0 24 24" fill="none" width="12" height="12"><path d="M7 17 17 7M9 7h8v8" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  expand: '<svg viewBox="0 0 24 24" fill="none" width="16" height="16"><path d="M9 4H4v5M15 4h5v5M9 20H4v-5M15 20h5v-5" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  shrink: '<svg viewBox="0 0 24 24" fill="none" width="16" height="16"><path d="M4 9h5V4M20 9h-5V4M4 15h5v5M20 15h-5v5" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/></svg>',
};
const G_SVG = '<svg viewBox="0 0 48 48" width="18" height="18"><path fill="#4285F4" d="M45.12 24.5c0-1.56-.14-3.06-.4-4.5H24v8.51h11.84c-.51 2.75-2.06 5.08-4.39 6.64v5.52h7.11c4.16-3.83 6.56-9.47 6.56-16.17z"/><path fill="#34A853" d="M24 46c5.94 0 10.92-1.97 14.56-5.33l-7.11-5.52c-1.97 1.32-4.49 2.1-7.45 2.1-5.73 0-10.58-3.87-12.31-9.07H4.34v5.7C7.96 41.07 15.4 46 24 46z"/><path fill="#FBBC05" d="M11.69 28.18c-.44-1.32-.69-2.73-.69-4.18s.25-2.86.69-4.18v-5.7H4.34A21.99 21.99 0 0 0 2 24c0 3.55.85 6.91 2.34 9.88l7.35-5.7z"/><path fill="#EA4335" d="M24 9.75c3.23 0 6.13 1.11 8.41 3.29l6.31-6.31C34.91 2.97 29.93 1 24 1 15.4 1 7.96 5.93 4.34 13.12l7.35 5.7c1.73-5.2 6.58-9.07 12.31-9.07z"/></svg>';

const state = {
  view: "main", // 'main' | 'onboarding'
  onb: null,    // 온보딩 진행 상태
  account: null, activeAddress: null, wallets: [], appliedByAddress: {},
  settings: null,
  search: "", catFilter: "all", expanded: new Set(["pkg.baseline", "pkg.day1-safety"]),
  ovlOpen: false, ovlSearch: "", manage: false, addOpen: false, pinnedFirst: true,
  confirmAddr: null, sheetOpen: false, justSaved: false,
  expanded2: false, // popup 크게 보기 토글
  applyStatus: "idle", applyError: "", appliedServer: [], loadError: "",
};

/* ============================================================
   §2.1 — 카테고리 매핑 (서버 category/action 우선, 없으면 id 휴리스틱)
   ============================================================ */
const CAT_ORDER = ["swap", "approvals", "transfer", "lending", "perps", "liquidity", "rewards", "governance", "intents", "others"];
const CAT_LABEL = { all: "전체", swap: "swap", approvals: "approvals", transfer: "transfer", lending: "lending", perps: "perps", liquidity: "liquidity", rewards: "rewards", governance: "governance", intents: "intents", others: "others" };
function policyCategory(id) {
  const p = S.POLICIES[id] || {};
  if (p.category) return p.category;
  const s = String(id).toLowerCase();
  if (/^swap[.\-]/.test(s) || s.includes("swap")) return "swap";
  if (/^(approve|permit2)[.\-]/.test(s) || s.includes("approval") || s.includes("permit2")) return "approvals";
  if (/^(send|transfer)[.\-]/.test(s) || s.includes("transfer") || s.includes("recipient") || s.includes("burn")) return "transfer";
  if (/^lending[.\-]/.test(s) || s.includes("lending") || s.includes("collateral") || s.includes("health")) return "lending";
  if (/^perps[.\-]/.test(s) || s.includes("perps") || s.includes("leverage") || s.includes("liquidation")) return "perps";
  if (/^liquidity[.\-]/.test(s) || s.includes("liquidity") || s.includes("impermanent")) return "liquidity";
  if (/^rewards[.\-]/.test(s) || s.includes("reward") || s.includes("claim")) return "rewards";
  if (s.includes("governance") || /^vote[.\-]/.test(s)) return "governance";
  if (/^intents[.\-]/.test(s) || s.includes("intent") || s.includes("solver")) return "intents";
  return "others";
}
function availableCats() {
  const present = new Set();
  (S.PACKAGES || []).forEach((p) => p.members.forEach((id) => present.add(policyCategory(id))));
  return CAT_ORDER.filter((c) => present.has(c));
}
function pkgInCat(pkg) {
  if (state.catFilter === "all") return true;
  return pkg.members.some((id) => policyCategory(id) === state.catFilter);
}

/* ---------- store glue ---------- */
function enabledSet() { return new Set((state.activeAddress && state.appliedByAddress[state.activeAddress]) || []); }
// ps2 토글 한 건 적용 → 활성 지갑 상태 재조회 → 캐시/화면 갱신. 실패는 풋터에.
async function applyMut(fn) {
  if (!state.activeAddress) return;
  state.applyStatus = "applying"; state.applyError = "";
  renderFooter();
  try {
    await fn();
    const enabled = await S.refreshActivePolicies(state.activeAddress);
    state.appliedByAddress[state.activeAddress] = enabled;
    state.appliedServer = enabled.slice();
    state.applyStatus = "idle"; state.applyError = "";
  } catch (e) {
    state.applyStatus = "error";
    state.applyError = String((e && e.message) || e);
  }
  renderFooter(); renderHero(); renderMain();
}
// 로컬 프로필(활성 주소/핀/별칭) 저장 — 정책 적용은 applyMut(ps2)이 담당.
async function persist() {
  try {
    await S.saveState({ account: state.account, activeAddress: state.activeAddress, wallets: state.wallets, appliedByAddress: state.appliedByAddress });
  } catch (e) {
    console.warn("[dambi] profile save failed:", e);
  }
}

function activeWallet() { return state.wallets.find((w) => w.address === state.activeAddress) || state.wallets[0] || null; }
// 고정 슬롯은 하나 — 항상 현재(활성) 주소가 맨 위에 고정
function normalizePins() {
  state.wallets.forEach((w) => { w.pinned = (w.address === state.activeAddress); });
}
function walletName(w) { return w && w.nickname ? w.nickname : ""; }
function addrStatus(addr) {
  const set = new Set(state.appliedByAddress[addr] || []);
  const pkgCount = S.PACKAGES.filter((p) => S.pkgState(p, set) === "on").length;
  return { pkgCount, polCount: set.size };
}

/* ---------- entry points ---------- */
function openOptions() { openSettings(); } // 설정 = popup 내부 오버레이
// 대시보드 = 확장 옵션 페이지(chrome-extension://<id>/options.html).
// 이 페이지는 SW 토큰(chrome.storage.local)을 localStorage 로 자동 mirror 하므로
// popup 과 같은 계정으로 자동 로그인된다(별도 OAuth 불필요).
function openDashboard() {
  if (typeof chrome !== "undefined" && chrome.runtime && chrome.runtime.getURL) {
    const url = chrome.runtime.getURL("options.html");
    if (chrome.tabs && chrome.tabs.create) chrome.tabs.create({ url });
    else window.open(url, "_blank");
    return;
  }
  window.open(DASHBOARD_URL, "_blank"); // 폴백(확장 런타임 없음)
}

/* ============================================================
   SHELL
   ============================================================ */
function buildShell() {
  document.body.classList.remove("onb-mode");
  const root = document.getElementById("root");
  root.innerHTML =
    '<div class="pc-hero" id="hero"></div>' +
    '<div class="pc-stick" id="stick"></div>' +
    '<main class="pc-main" id="main"></main>' +
    '<footer class="pc-footer" id="footer"></footer>' +
    '<div class="pc-ovl" id="ovl"></div>' +
    '<div class="pc-ovl" id="settingsOvl"></div>' +
    '<div class="pc-sheet-scrim" id="sheetScrim"></div>' +
    '<div class="pc-sheet" id="sheet"></div>' +
    '<div class="pc-modal" id="modal"></div>';
  document.getElementById("sheetScrim").addEventListener("click", () => toggleSheet(false));
}

/* ============================================================
   HERO
   ============================================================ */
function renderHero() {
  const hero = document.getElementById("hero");
  const w = activeWallet();
  const set = enabledSet();
  const on = set.size;
  const pkgOn = S.PACKAGES.filter((p) => S.pkgState(p, set) === "on").length;
  const nm = walletName(w);
  hero.innerHTML =
    '<div class="pc-hero-top">' +
      '<button class="pc-switch" id="switchBtn">' +
        '<span class="av">' + (w ? S.identiconSVG(w.address, 26) : "") + '</span>' +
        '<span class="sw-tx">' +
          (nm ? '<span class="sw-name">' + esc(nm) + '</span><span class="sw-addr">' + S.shortAddr(w ? w.address : "") + '</span>'
              : '<span class="sw-name only">' + S.shortAddr(w ? w.address : "") + '</span>') +
        '</span>' +
        '<span class="chev">' + I.caretDown + '</span>' +
      '</button>' +
      '<button class="pc-sizebtn" id="sizeBtn" title="창 크기 전환" aria-label="창 크기 전환">' + (state.expanded2 ? I.shrink : I.expand) + '</button>' +
      '<button class="pc-iconbtn" id="dashBtn" title="웹 대시보드 열기">대시보드 ' + I.ext + '</button>' +
    '</div>' +
    '<div class="pc-hero-main">' +
      '<div class="pc-guard ' + (on ? "" : "off") + '"><div class="ring"></div><div class="face"><img src="' + ASSET + 'dambi-mark-navy.png" alt=""></div><div class="badge">' + I.check + '</div></div>' +
      '<div><div class="pc-hero-h">' + (on ? "보호 중" : "보호 꺼짐") + '</div>' +
        '<div class="pc-hero-s">' + (on ? "정책 " + on + "개 활성 · 패키지 " + pkgOn + "개" : "이 주소에 켜진 정책이 없어요") + '</div></div>' +
    '</div>' +
    '<div class="pc-hero-acct"><span class="pc-acct-dot"></span><span class="pc-acct-t">' + (state.account ? esc(state.account.email) : "로그아웃됨") + '</span><span class="pc-acct-tag">지갑 ' + state.wallets.length + '개</span></div>';
  document.getElementById("switchBtn").addEventListener("click", () => openOverlay());
  document.getElementById("dashBtn").addEventListener("click", openDashboard);
  document.getElementById("sizeBtn").addEventListener("click", toggleSize);
}

/* ---------- popup 크기 전환 (기본 ↔ 크게) ---------- */
function applySize() {
  document.documentElement.classList.toggle("pc-big", !!state.expanded2);
  document.body.classList.toggle("pc-big", !!state.expanded2);
  const root = document.getElementById("root");
  if (root) root.classList.toggle("big", !!state.expanded2);
}
function toggleSize() {
  state.expanded2 = !state.expanded2;
  applySize();
  try { if (typeof chrome !== "undefined" && chrome.storage) chrome.storage.local.set({ "dambi.popup.big": state.expanded2 }); } catch (e) {}
  renderHero();
}

/* ============================================================
   STICKY CONTROLS (search + bulk + 카테고리 칩)
   ============================================================ */
function renderStick() {
  const stick = document.getElementById("stick");
  const cats = availableCats();
  let chips = '<button class="pc-chip ' + (state.catFilter === "all" ? "on" : "") + '" data-cat="all">전체</button>';
  cats.forEach((c) => { chips += '<button class="pc-chip ' + (state.catFilter === c ? "on" : "") + '" data-cat="' + c + '">' + CAT_LABEL[c] + '</button>'; });
  stick.innerHTML =
    '<div class="pc-toolbar">' +
      '<input class="pc-search" id="search" type="text" placeholder="패키지·정책 검색" />' +
      '<div class="pc-bulk"><button class="pc-mini" id="allOn">전체 켜기</button><button class="pc-mini" id="allOff">전체 끄기</button></div>' +
    '</div>' +
    '<div class="pc-chips" id="chips">' + chips + '</div>';
  const s = document.getElementById("search");
  s.value = state.search;
  s.addEventListener("input", () => { state.search = s.value; renderMain(); });
  document.getElementById("allOn").addEventListener("click", () => { void applyMut(() => S.setAllBindings(state.activeAddress, true)); });
  document.getElementById("allOff").addEventListener("click", () => { void applyMut(() => S.setAllBindings(state.activeAddress, false)); });
  // 카테고리 칩 — 단일 선택, 검색어와 AND
  stick.querySelectorAll(".pc-chip").forEach((b) => b.addEventListener("click", () => {
    state.catFilter = b.dataset.cat;
    stick.querySelectorAll(".pc-chip").forEach((x) => x.classList.toggle("on", x.dataset.cat === state.catFilter));
    renderMain();
  }));
}

/* ============================================================
   MAIN — package-first
   ============================================================ */
function pkgMatches(pkg, term) {
  if (!term) return true;
  const t = term.toLowerCase();
  if (pkg.name.toLowerCase().includes(t) || pkg.id.toLowerCase().includes(t)) return true;
  return pkg.members.some((id) => {
    const p = S.POLICIES[id] || {};
    if ((p.title || "").toLowerCase().includes(t) || id.toLowerCase().includes(t)) return true;
    return (p.reasons || []).some((r) => r.toLowerCase().includes(t));
  });
}
function renderMain() {
  const main = document.getElementById("main");
  const set = enabledSet();
  const w = activeWallet();
  let html = "";
  html += '<div class="pc-scopebar">이 주소의 정책 — <span class="a">' + S.shortAddr(w ? w.address : "") + (walletName(w) ? " · " + esc(walletName(w)) : "") + '</span></div>';
  if (set.size === 0) html += '<div class="pc-banner">이 주소는 모든 정책이 꺼져 있어요 — 트랜잭션이 검사 없이 통과돼요. 최소 한 개는 켜두세요.</div>';

  const pkgs = S.PACKAGES.filter((p) => pkgInCat(p) && pkgMatches(p, state.search));
  if (!pkgs.length) {
    const why = state.catFilter !== "all" ? "이 카테고리에 해당하는 패키지가 없어요." : "해당하는 패키지가 없어요.";
    main.innerHTML = html + '<p class="pc-empty">' + why + '</p>'; return;
  }

  for (const pkg of pkgs) {
    const st = S.pkgState(pkg, set);
    const open = state.expanded.has(pkg.id);
    const onCount = pkg.members.filter((id) => set.has(id)).length;
    html += '<div class="pc-pkg ' + (st === "off" ? "dim" : "") + (open ? " open" : "") + '" data-pkg="' + pkg.id + '">';
    html +=   '<div class="pc-pkg-head" data-act="expand">' + I.chev +
                '<div class="pc-pkg-main"><div class="pc-pkg-titlerow">' +
                  '<span class="pc-pkg-name">' + pkg.name + '</span>' +
                  '<span class="pc-src ' + pkg.source.kind + '">' + pkg.source.label + '</span>' +
                '</div>' +
                '<div class="pc-pkg-meta"><b>' + onCount + '/' + pkg.members.length + '</b> 정책 활성</div></div>' +
                '<button class="pc-tog ' + (st === "on" ? "" : st === "off" ? "off" : "mixed") + '" data-act="master" role="switch" aria-checked="' + (st === "on") + '"></button>' +
              '</div>';
    if (open) {
      html += '<div class="pc-pkg-members">';
      for (const id of pkg.members) {
        const p = S.POLICIES[id];
        const memOn = set.has(id);
        html += '<div class="pc-mem">' +
                  '<span class="pc-mem-sev ' + p.sev + '"></span>' +
                  '<span class="pc-mem-name">' + p.title + '</span>' +
                  '<button class="pc-tog sm ' + (memOn ? "" : "off") + '" data-act="member" data-id="' + id + '" role="switch" aria-checked="' + memOn + '"></button>' +
                '</div>';
      }
      html += '</div>';
    }
    html += '</div>';
  }
  main.innerHTML = html;

  main.querySelectorAll(".pc-pkg").forEach((cardEl) => {
    const pkgId = cardEl.dataset.pkg;
    const pkg = S.PACKAGES.find((p) => p.id === pkgId);
    cardEl.querySelector('[data-act="expand"]').addEventListener("click", (e) => {
      if (e.target.closest('[data-act="master"]')) return;
      state.expanded.has(pkgId) ? state.expanded.delete(pkgId) : state.expanded.add(pkgId);
      renderMain();
    });
    cardEl.querySelector('[data-act="master"]').addEventListener("click", (e) => {
      e.stopPropagation();
      // 하이브리드: 끄기 = 게이트 off(부분 상태 보존), 켜기 = 게이트 on +
      // (전부 꺼져 있으면) 멤버 일괄 on.
      const set3 = enabledSet();
      const displayedOn = S.pkgState(pkg, set3) === "on";
      void applyMut(() =>
        displayedOn
          ? S.setPackageOn(state.activeAddress, pkg.id, false)
          : S.enablePackage(state.activeAddress, pkg, set3),
      );
    });
    cardEl.querySelectorAll('[data-act="member"]').forEach((btn) => {
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        if (btn.disabled) return;
        const id = btn.dataset.id;
        const on = enabledSet().has(id);
        void applyMut(() => S.setBindingEnabled(state.activeAddress, id, !on));
      });
    });
  });
}

/* ============================================================
   FOOTER
   ============================================================ */
function renderFooter() {
  const f = document.getElementById("footer");
  if (!f) return;
  let txt = "변경 즉시 이 주소에 적용";
  let cls = "";
  if (state.applyStatus === "applying") { txt = "적용 중…"; cls = "busy"; }
  else if (state.applyStatus === "error") { txt = "적용 실패: " + state.applyError; cls = "error"; }
  else {
    const cur = [...enabledSet()].sort().join(",");
    const srv = [...(state.appliedServer || [])].sort().join(",");
    if (cur !== srv) { txt = "적용 중…"; cls = "busy"; }
  }
  f.innerHTML =
    '<span class="pc-status ' + cls + '"><span class="pc-dot"></span>' + esc(txt) + '</span>' +
    '<div class="pc-foot-actions">' +
      '<button class="pc-link" id="notifBtn">알림 강도</button>' +
      '<button class="pc-link accent" id="optBtn">설정 ' + I.ext + '</button>' +
    '</div>';
  document.getElementById("notifBtn").addEventListener("click", () => toggleSheet(true));
  document.getElementById("optBtn").addEventListener("click", openOptions);
}

/* ============================================================
   OVERLAY — 지갑 스위처
   ============================================================ */
function openOverlay() { state.ovlOpen = true; state.manage = false; state.addOpen = false; state.ovlSearch = ""; renderOverlay(); document.getElementById("ovl").classList.add("show"); }
function closeOverlay() { const o = document.getElementById("ovl"); o.classList.remove("show"); state.ovlOpen = false; }

/* ============================================================
   SETTINGS OVERLAY (popup 내부 — 별도 페이지 X)
   ============================================================ */
function openSettings() { renderSettingsOverlay(); document.getElementById("settingsOvl").classList.add("show"); }
function closeSettings() { document.getElementById("settingsOvl").classList.remove("show"); }

function renderSettingsOverlay() {
  const ovl = document.getElementById("settingsOvl");
  const cur = currentPreset();
  const s = state.settings || S.SETTINGS_DEFAULT;
  const presets = [["quiet", "조용히", "차단만 막고 조용히"], ["std", "표준", "권장 균형"], ["loud", "적극적", "모든 신호 표시"]];
  const segBtn = (key, opts) =>
    '<div class="set-seg" data-key="' + key + '">' +
    opts.map(([v, t]) => '<button class="' + (s[key] === v ? "on" : "") + '" data-v="' + v + '">' + t + '</button>').join("") +
    '</div>';
  const tog = (key) => '<button class="pc-tog ' + (s[key] ? "" : "off") + '" data-key="' + key + '" role="switch" aria-checked="' + !!s[key] + '"></button>';

  ovl.innerHTML =
    '<div class="pc-ovl-head">' +
      '<button class="pc-ovl-back" id="setBack" aria-label="뒤로">' + I.back + '</button>' +
      '<div class="pc-ovl-title">설정</div>' +
      '<span style="width:34px"></span>' +
    '</div>' +
    '<div class="pc-ovl-body pc-set-body">' +
      '<div class="pc-set-sec">알림 강도</div>' +
      '<div class="pc-presets">' +
        presets.map(([k, t, d]) => '<button class="pc-preset ' + (cur === k ? "on" : "") + '" data-p="' + k + '"><div class="pt"><span class="pd ' + k + '"></span>' + t + '</div><div class="ps">' + d + '</div></button>').join("") +
      '</div>' +
      '<div class="pc-set-rows">' +
        '<div class="pc-set-row"><div class="rl"><div class="rt">데스크톱 알림</div><div class="rs">브라우저 밖 통지</div></div>' + segBtn("desk", [["block", "차단만"], ["both", "차단+검토"], ["all", "모두"]]) + '</div>' +
        '<div class="pc-set-row"><div class="rl"><div class="rt">페이지 인터셉트 모달</div><div class="rs">서명 직전 개입</div></div>' + segBtn("modal", [["block", "차단"], ["both", "차단+검토"]]) + '</div>' +
        '<div class="pc-set-row"><div class="rl"><div class="rt">상단 리본 배너</div><div class="rs">위험 사이트 경고</div></div>' + tog("ribbon") + '</div>' +
        '<div class="pc-set-row"><div class="rl"><div class="rt">알림음</div><div class="rs">차단 시 소리</div></div>' + tog("sound") + '</div>' +
      '</div>' +
      '<div class="pc-set-sec">계정</div>' +
      '<div class="pc-set-acct">' +
        '<span class="av">' + S.identiconSVG(state.account ? state.account.email : "0x", 32) + '</span>' +
        '<div class="acct-main"><div class="acct-email">' + (state.account ? esc(state.account.email) : "로그아웃됨") + '</div>' +
          '<div class="acct-sub">지갑·정책 동기화 앵커 · 읽기 전용</div></div>' +
        '<button class="pc-set-out" id="setLogout">로그아웃</button>' +
      '</div>' +
      '<div class="pc-set-sec">데이터</div>' +
      '<button class="pc-set-reset" id="setReset">이 계정 데이터 초기화</button>' +
      '<div class="pc-set-foot">Dambi — 트랜잭션 가드 · watch-only</div>' +
    '</div>';

  ovl.querySelector("#setBack").addEventListener("click", closeSettings);
  ovl.querySelectorAll(".pc-preset").forEach((b) => b.addEventListener("click", () => {
    state.settings = { preset: b.dataset.p, ...S.SETTINGS_PRESETS[b.dataset.p] };
    S.saveSettings(state.settings); renderSettingsOverlay();
  }));
  ovl.querySelectorAll(".set-seg").forEach((seg) => seg.querySelectorAll("button").forEach((b) => b.addEventListener("click", () => {
    state.settings = { ...state.settings, [seg.dataset.key]: b.dataset.v };
    state.settings.preset = currentPreset();
    S.saveSettings(state.settings); renderSettingsOverlay();
  })));
  ovl.querySelectorAll(".pc-tog[data-key]").forEach((t) => t.addEventListener("click", () => {
    state.settings = { ...state.settings, [t.dataset.key]: !state.settings[t.dataset.key] };
    state.settings.preset = currentPreset();
    S.saveSettings(state.settings); renderSettingsOverlay();
  }));
  ovl.querySelector("#setLogout").addEventListener("click", async () => {
    const btn = ovl.querySelector("#setLogout");
    btn.disabled = true; btn.textContent = "로그아웃 중…";
    try { if (S.signOut) await S.signOut(); } catch (e) {}
    state.account = null; state.wallets = []; state.activeAddress = null; state.appliedByAddress = {};
    state.onb = null;
    closeSettings();
    route(); // → 온보딩 스텝1(로그인)
  });
  ovl.querySelector("#setReset").addEventListener("click", () => { openConfirmReset(); });
}

function openConfirmReset() {
  const m = document.getElementById("modal");
  m.innerHTML = '<div class="pc-dialog">' +
    '<h3>이 계정 데이터를 초기화할까요?</h3>' +
    '<p>등록 지갑과 주소별 정책 적용 상태가 모두 비워져요. 되돌릴 수 없어요.</p>' +
    '<div class="pc-dialog-foot"><button class="pc-dlg-btn ghost" id="rstCancel">취소</button><button class="pc-dlg-btn danger" id="rstOk">초기화</button></div>' +
    '</div>';
  m.classList.add("show");
  m.querySelector("#rstCancel").addEventListener("click", () => m.classList.remove("show"));
  m.querySelector("#rstOk").addEventListener("click", async () => {
    state.wallets = []; state.activeAddress = null; state.appliedByAddress = {};
    try { await S.saveState({ account: state.account, activeAddress: null, wallets: [], appliedByAddress: {} }); } catch (e) {}
    state.onb = null;
    m.classList.remove("show"); closeSettings();
    route(); // 계정은 유지 + 지갑 없음 → 온보딩 스텝2(지갑 등록)
  });
}

function sortedOthers() {
  let list = state.wallets.filter((w) => w.address !== state.activeAddress);
  if (state.ovlSearch) {
    const t = state.ovlSearch.toLowerCase();
    list = list.filter((w) => w.address.toLowerCase().includes(t) || (w.nickname || "").toLowerCase().includes(t));
  }
  // 고정된 주소는 항상 최상단 — 풀면 자연 순서로 내려감
  list = [...list].sort((a, b) => (b.pinned ? 1 : 0) - (a.pinned ? 1 : 0));
  return list;
}

function renderOverlay() {
  const ovl = document.getElementById("ovl");
  ovl.innerHTML =
    '<div class="pc-ovl-head">' +
      '<button class="pc-ovl-back" id="ovlBack" aria-label="뒤로">' + I.back + '</button>' +
      '<div class="pc-ovl-title">현재 주소</div>' +
      '<button class="pc-ovl-add" id="ovlAddIcon" aria-label="새 주소 추가">' + I.plus + '</button>' +
    '</div>' +
    '<div class="pc-ovl-body" id="ovlBody"></div>' +
    '<div class="pc-ovl-foot"><button class="pc-addbtn" id="ovlAddBtn">' + I.plus + ' 새 주소 추가</button></div>';

  document.getElementById("ovlBack").addEventListener("click", closeOverlay);
  document.getElementById("ovlAddIcon").addEventListener("click", () => { state.addOpen = true; renderOvlBody(); setTimeout(() => { const i = document.getElementById("afAddr"); if (i) i.focus(); }, 20); });
  document.getElementById("ovlAddBtn").addEventListener("click", () => { state.addOpen = !state.addOpen; renderOvlBody(); if (state.addOpen) setTimeout(() => { const i = document.getElementById("afAddr"); if (i) i.focus(); }, 20); });
  renderOvlBody();
}

function renderOvlBody() {
  const body = document.getElementById("ovlBody");
  const w = activeWallet();
  const st = w ? addrStatus(w.address) : { pkgCount: 0, polCount: 0 };
  const nm = walletName(w);
  let html = "";
  html += '<div class="pc-addform ' + (state.addOpen ? "show" : "") + '" id="addForm">' +
            '<input class="pc-af-input mono" id="afAddr" placeholder="0x… 지갑 주소" autocomplete="off" spellcheck="false" />' +
            '<div class="af-row"><input class="pc-af-input" id="afAlias" placeholder="별칭 (선택)" autocomplete="off" /><button class="pc-af-add" id="afAdd">추가</button></div>' +
            '<div class="pc-af-msg" id="afMsg"></div>' +
          '</div>';
  html += '<div class="pc-acard">' +
            '<span class="av">' + (w ? S.identiconSVG(w.address, 36) : "") + '</span>' +
            '<div class="ac-tx">' +
              '<div class="ac-name">' + (nm ? esc(nm) : '<span class="only">' + S.shortAddr(w ? w.address : "") + '</span>') + '<span class="pc-pin-tag">' + I.pin + '</span></div>' +
              '<div class="ac-addr"><span class="ac-short">' + S.shortAddr(w ? w.address : "") + '</span><button class="pc-copy" id="acCopy" aria-label="주소 복사">' + I.copy + '</button></div>' +
            '</div>' +
            '<div class="ac-status"><span class="pc-statline">패키지 <span class="n">' + st.pkgCount + '</span> · 정책 <span class="n">' + st.polCount + '</span></span></div>' +
          '</div>';
  html += '<div class="pc-srow">' +
            '<input class="pc-ovl-search" id="ovlSearch" type="text" placeholder="주소·별칭 검색" />' +
            '<button class="pc-manage-link ' + (state.manage ? "on" : "") + '" id="manageLink">' + (state.manage ? "완료" : "주소 관리 ›") + '</button>' +
          '</div>';
  html += '<div class="pc-seclabel">' + (state.manage ? "주소 관리 — 별칭·핀·삭제" : "전환할 주소") + '</div>';
  html += '<div class="pc-alist" id="ovlList"></div>';
  body.innerHTML = html;

  wireAddForm();
  document.getElementById("acCopy").addEventListener("click", (e) => copyAddr(e.currentTarget, w.address));
  const os = document.getElementById("ovlSearch");
  os.value = state.ovlSearch;
  os.addEventListener("input", () => { state.ovlSearch = os.value; renderOvlList(); });
  document.getElementById("manageLink").addEventListener("click", () => { state.manage = !state.manage; renderOvlBody(); });
  renderOvlList();
}

function renderOvlList() {
  const list = document.getElementById("ovlList"); if (!list) return;
  const others = sortedOthers();
  if (!others.length) { list.innerHTML = '<div class="pc-empty" style="padding:18px 0">' + (state.ovlSearch ? "검색 결과가 없어요." : "다른 주소가 없어요.") + '</div>'; return; }
  list.innerHTML = "";
  others.forEach((w) => {
    const st = addrStatus(w.address);
    const nm = walletName(w);
    const row = document.createElement("div");
    row.className = "pc-arow" + (state.manage ? " managing" : "");
    if (state.manage) {
      row.innerHTML =
        '<span class="av">' + S.identiconSVG(w.address, 30) + '</span>' +
        '<input class="pc-alias-edit" value="' + esc(nm) + '" placeholder="' + S.shortAddr(w.address) + '" />' +
        '<div class="pc-mgmt">' +
          '<button class="pin ' + (w.pinned ? "on" : "") + '" title="맨 위로 고정">' + I.pin + '</button>' +
          '<button class="trash" title="삭제"' + (state.wallets.length <= 1 ? " disabled style=\"opacity:.4;cursor:not-allowed\"" : "") + '>' + I.trash + '</button>' +
        '</div>';
      const ed = row.querySelector(".pc-alias-edit");
      ed.addEventListener("change", () => {
        const nv = ed.value.trim();
        if (nv === (w.nickname || "")) return;
        w.nickname = nv;
        persist();
        // 닉네임(label)을 서버에도 반영 → 대시보드와 즉시 동기화
        if (S.renameWallet) void S.renameWallet(w.address, nv).catch((e) => console.warn("[dambi] rename failed:", e));
      });
      ed.addEventListener("keydown", (e) => { if (e.key === "Enter") ed.blur(); });
      row.querySelector(".pin").addEventListener("click", () => {
        // 고정 = 맨 위 현재 주소 카드로 올림 — 기존 상단 주소는 리스트로 내려감
        state.activeAddress = w.address;
        state.wallets.forEach((x) => { x.pinned = (x.address === w.address); });
        persist();
        renderOverlay(); renderHero(); renderMain(); renderFooter();
      });
      const tr = row.querySelector(".trash");
      if (state.wallets.length > 1) tr.addEventListener("click", () => openConfirm(w.address));
    } else {
      row.innerHTML =
        '<span class="av">' + S.identiconSVG(w.address, 30) + '</span>' +
        '<div class="ar-tx"><div class="ar-name">' + (nm ? esc(nm) : '<span class="only">' + S.shortAddr(w.address) + '</span>') + '</div>' +
          (nm ? '<div class="ar-short">' + S.shortAddr(w.address) + '</div>' : "") + '</div>' +
        '<div class="ar-status"><div class="ar-statline">패키지 <span class="n">' + st.pkgCount + '</span> · 정책 <span class="n">' + st.polCount + '</span></div></div>';
      row.addEventListener("click", () => switchTo(w.address));
    }
    list.appendChild(row);
  });
}

function switchTo(addr) {
  state.activeAddress = addr;
  // 현재 주소가 곧 맨 위 고정 슬롯 — pinned는 항상 활성 주소를 따름
  state.wallets.forEach((w) => { w.pinned = (w.address === addr); });
  void persist();
  closeOverlay();
  // 새 활성 지갑의 ps2 상태를 다시 읽어 카드 갱신(per-wallet 뷰).
  void applyMut(() => Promise.resolve());
}

/* ---------- add form (shared validation) ---------- */
function wireAddForm() {
  const addr = document.getElementById("afAddr");
  const alias = document.getElementById("afAlias");
  const msg = document.getElementById("afMsg");
  const add = document.getElementById("afAdd");
  if (!addr) return;
  function setMsg(t, k) { msg.className = "pc-af-msg" + (k ? " " + k : ""); msg.textContent = t || ""; }
  function tryAdd() {
    const a = addr.value.trim(); setMsg(""); addr.classList.remove("err");
    if (!a) { addr.classList.add("err"); setMsg("주소를 입력하세요.", "err"); return; }
    if (!S.isAddressShape(a)) { addr.classList.add("err"); setMsg("0x로 시작하는 40자리 16진수 주소를 입력하세요.", "err"); return; }
    if (state.wallets.some((w) => w.address.toLowerCase() === a.toLowerCase())) { addr.classList.add("err"); setMsg("이미 등록된 주소예요.", "err"); return; }
    const warn = S.checksumWarn(a);
    const aliasVal = alias.value.trim();
    state.wallets.push({ address: a, nickname: aliasVal, pinned: false });
    state.appliedByAddress[a] = [];
    persist();
    if (S.addWallet) void S.addWallet(a, aliasVal || undefined);
    addr.value = ""; alias.value = "";
    state.addOpen = false;
    renderOverlay(); renderHero(); renderFooter();
    if (warn) { /* 경고 수준 — 등록은 됨 */ }
  }
  add.addEventListener("click", tryAdd);
  addr.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); tryAdd(); } });
  alias.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); tryAdd(); } });
}

function copyAddr(btn, addr) {
  try { navigator.clipboard.writeText(addr); } catch (e) {}
  btn.classList.add("done"); btn.innerHTML = I.check;
  setTimeout(() => { btn.classList.remove("done"); btn.innerHTML = I.copy; }, 1100);
}

/* ---------- confirm delete ---------- */
function openConfirm(addr) {
  state.confirmAddr = addr;
  const m = document.getElementById("modal");
  const w = state.wallets.find((x) => x.address === addr);
  m.innerHTML =
    '<div class="pc-dialog"><h3>이 주소를 삭제할까요?</h3>' +
    '<p>' + (w && w.nickname ? esc(w.nickname) + " · " : "") + '등록 목록과 이 주소의 정책 적용 상태가 영구 삭제돼요. 되돌릴 수 없어요.</p>' +
    '<div class="dlg-addr">' + addr + '</div>' +
    '<div class="pc-dialog-foot"><button class="pc-dlg-btn ghost" id="dlgCancel">취소</button><button class="pc-dlg-btn danger" id="dlgDel">삭제</button></div></div>';
  m.classList.add("show");
  document.getElementById("dlgCancel").addEventListener("click", closeConfirm);
  document.getElementById("dlgDel").addEventListener("click", () => doDelete(addr));
  m.addEventListener("click", (e) => { if (e.target === m) closeConfirm(); });
}
function closeConfirm() { document.getElementById("modal").classList.remove("show"); state.confirmAddr = null; }
async function doDelete(addr) {
  // 서버에서 먼저 삭제 → 대시보드와 즉시 동기화. 로컬 상태도 같이 정리.
  if (S.removeWallet) {
    try { await S.removeWallet(addr); }
    catch (e) { console.warn("[dambi] delete-wallet(server) failed:", e); }
  }
  state.wallets = state.wallets.filter((w) => w.address !== addr);
  delete state.appliedByAddress[addr];
  if (state.activeAddress === addr) state.activeAddress = state.wallets[0] ? state.wallets[0].address : null;
  persist(); closeConfirm();
  if (!state.wallets.length) { route(); return; } // 마지막 지갑 삭제 → 지갑 등록 온보딩
  renderHero(); renderMain(); renderFooter(); renderOverlay();
}

/* ============================================================
   알림 강도 퀵 시트
   ============================================================ */
function toggleSheet(open) {
  state.sheetOpen = open;
  const sheet = document.getElementById("sheet");
  const scrim = document.getElementById("sheetScrim");
  if (open) renderSheet();
  sheet.classList.toggle("show", open);
  scrim.classList.toggle("show", open);
}
function currentPreset() {
  const s = state.settings || S.SETTINGS_DEFAULT;
  for (const [name, p] of Object.entries(S.SETTINGS_PRESETS)) {
    if (["desk", "modal", "ribbon", "sound"].every((k) => s[k] === p[k])) return name;
  }
  return "custom";
}
function renderSheet() {
  const sheet = document.getElementById("sheet");
  const cur = currentPreset();
  const presets = [["quiet", "조용히", "차단만 막고 조용히"], ["std", "표준", "권장 균형"], ["loud", "적극적", "모든 신호 표시"]];
  sheet.innerHTML =
    '<div class="pc-sheet-grip"></div>' +
    '<div class="pc-sheet-head"><h3>알림 강도</h3><button class="more" id="sheetMore">자세히 ' + I.ext + '</button></div>' +
    '<p>얼마나 적극적으로 알릴지 정해요. 설정과 실시간으로 같이 바뀌어요.</p>' +
    '<div class="pc-presets">' +
      presets.map(([k, t, d]) => '<button class="pc-preset ' + (cur === k ? "on" : "") + '" data-p="' + k + '"><div class="pt"><span class="pd ' + k + '"></span>' + t + '</div><div class="ps">' + d + '</div></button>').join("") +
    '</div>';
  sheet.querySelectorAll(".pc-preset").forEach((b) => b.addEventListener("click", () => {
    const p = b.dataset.p;
    state.settings = { preset: p, ...S.SETTINGS_PRESETS[p] };
    S.saveSettings(state.settings);
    renderSheet();
  }));
  document.getElementById("sheetMore").addEventListener("click", () => { toggleSheet(false); openOptions(); });
}

/* ============================================================
   §1 — 온보딩 (popup 내부 4-step)
   1 Google 로그인 → 2 지갑 등록 → 3 베이스라인 → 4 완료
   완료 시 saveState(베이스라인 포함) 후 같은 팝업에서 정책 화면으로 전환.
   ============================================================ */
const ONB_COPY = {
  1: { eb: "설치 완료 · 1 / 4", t: "지갑이 이제 보호받기 시작해요", l: "먼저 Google 계정으로 로그인하면, 이 계정에 지갑을 묶어 정책을 동기화해요." },
  2: { eb: "지갑 등록 · 2 / 4", t: "어느 주소를 지킬까요?", l: "이 계정에서 사용할 지갑 주소를 등록하세요. 여러 개 추가할 수 있어요." },
  3: { eb: "베이스라인 정책 · 3 / 4", t: "기본 보호를 켤게요", l: "권장 가드예요. 최소 한 개는 켜야 검사가 동작해요 — 언제든 팝업에서 바꿀 수 있어요." },
  4: { eb: "준비 완료 · 4 / 4", t: "이제 보호받고 있어요", l: "트랜잭션에 서명할 때 Dambi가 검토·차단을 페이지 위에서 바로 알려줘요." },
};
const ONB_STEPS = [[1, "로그인"], [2, "지갑 등록"], [3, "베이스라인"], [4, "완료"]];

function enterOnboarding(step) {
  state.view = "onboarding";
  state.onb = {
    step: step, reached: step,
    email: state.account ? state.account.email : null,
    // v2: 기본 안전팩(builtin)이 전부 체크된 상태로 시작 — 체크 해제한 것만
    // defaults.enabled=false 로 내려간다(새 지갑 비적용).
    wallets: [], baseline: new Set((S.BASELINE || []).map((b) => b.id)),
  };
  document.body.classList.add("onb-mode");
  const root = document.getElementById("root");
  root.innerHTML = '<div class="pc-onb show" id="onb"></div>';
  renderOnboarding();
}
function goStep(n) {
  if (n > state.onb.reached) return;
  state.onb.step = n; state.onb.reached = Math.max(state.onb.reached, n);
  renderOnboarding();
}
function advanceStep(n) { state.onb.reached = Math.max(state.onb.reached, n); goStep(n); }

function renderOnboarding() {
  const host = document.getElementById("onb"); if (!host) return;
  const n = state.onb.step;
  const c = ONB_COPY[n];
  const dots = [1, 2, 3, 4].map((i) => '<span class="dot ' + (i <= n ? "on" : "") + '"></span>').join("");
  host.innerHTML =
    '<div class="pc-onb-top">' +
      '<button class="pc-onb-back" id="onbBack" aria-label="뒤로"' + (n === 1 ? " hidden" : "") + '>' + I.back + '</button>' +
      '<div class="pc-onb-brand"><img src="' + ASSET + 'dambi-mark-white.png" alt=""><span class="wd">DAMBI</span></div>' +
      '<div class="pc-onb-dots">' + dots + '</div>' +
    '</div>' +
    '<div class="pc-onb-body">' +
      '<div class="pc-onb-eyebrow">' + c.eb + '</div>' +
      '<div class="pc-onb-title">' + c.t + '</div>' +
      '<div class="pc-onb-lede">' + c.l + '</div>' +
      '<div class="pc-onb-card" id="onbCard"></div>' +
      '<div class="onb-steps" id="onbSteps"></div>' +
    '</div>';
  const back = host.querySelector("#onbBack");
  if (back) back.addEventListener("click", () => { if (state.onb.step > 1) goStep(state.onb.step - 1); });
  renderOnbSteps();
  ONB_RENDER[n]();
}
function renderOnbSteps() {
  const el = document.getElementById("onbSteps"); if (!el) return;
  const n = state.onb.step;
  el.innerHTML = ONB_STEPS.map(([k, lb]) => {
    const cls = k < n ? "done" : k === n ? "cur" : "pend";
    const num = k < n ? I.check : String(k);
    return '<div class="onb-st ' + cls + '"><span class="num">' + num + '</span><span class="lb">' + lb + '</span></div>';
  }).join("");
}

/* ----- step 1: Google 로그인 ----- */
function onbStep1() {
  const card = document.getElementById("onbCard");
  const authed = !!state.onb.email && !!state.account;
  card.innerHTML =
    '<div class="onb-ghead">' +
      '<div class="onb-guard ' + (authed ? "" : "idle") + '"><div class="ring"></div><div class="face"><img src="' + ASSET + 'dambi-mark-navy.png" alt=""></div><div class="badge">' + I.check + '</div></div>' +
      '<div class="gt">' + (authed ? "로그인됐어요" : "Google 계정으로 시작") + '</div>' +
      '<div class="gs">' + (authed ? "이 계정에 지갑과 정책이 동기화돼요." : "키는 받지 않아요 — 읽기 전용으로 정책만 묶어요.") + '</div>' +
    '</div>' +
    (authed
      ? '<div class="onb-authed"><div class="chk">' + I.check + '</div><div><div class="at">Google 계정 연결됨</div><div class="ae">' + esc(state.onb.email) + '</div></div></div><button class="onb-cta" id="onbNext1">계속 →</button>'
      : '<button class="onb-google" id="onbGoogle">' + G_SVG + '<span>Google 계정으로 계속</span></button><div class="onb-note" id="onbNote">로그인은 백그라운드(서비스 워커)에서 처리돼요 — 팝업이 닫히지 않아요.</div>');
  const g = document.getElementById("onbGoogle");
  if (g) g.addEventListener("click", onbDoLogin);
  const n1 = document.getElementById("onbNext1");
  if (n1) n1.addEventListener("click", () => advanceStep(2));
}
async function onbDoLogin() {
  const g = document.getElementById("onbGoogle");
  g.classList.add("loading");
  g.innerHTML = '<span class="spin"></span><span>로그인 중…</span>';
  let res = null;
  try { res = S.signIn ? await S.signIn() : { email: "dev@team.xyz", isFirstLogin: true }; }
  catch (e) { res = null; }
  if (!res || !res.email) {
    g.classList.remove("loading");
    g.innerHTML = G_SVG + '<span>Google 계정으로 계속</span>';
    g.addEventListener("click", onbDoLogin);
    const note = document.getElementById("onbNote");
    if (note) { note.textContent = "로그인에 실패했어요. 다시 시도해 주세요."; note.style.color = "var(--fail-400)"; }
    return;
  }
  state.onb.email = res.email;
  // 계정 + 카탈로그(PACKAGES/POLICIES) 로드
  try {
    const st = await S.loadState();
    state.account = st.account || { email: res.email };
    state.appliedServer = st.appliedServer || [];
    if (!res.isFirstLogin && st.wallets && st.wallets.length) {
      // 기존 사용자 → 온보딩 건너뛰고 정책 화면으로 (§1.1 / §5-3)
      state.wallets = st.wallets;
      state.activeAddress = st.activeAddress || st.wallets[0].address;
      state.appliedByAddress = st.appliedByAddress || {};
      state.view = "main"; buildShell(); applySize(); renderAll();
      return;
    }
  } catch (e) {
    state.account = state.account || { email: res.email };
  }
  advanceStep(2); // 첫 로그인 → 지갑 등록
}

/* ----- step 2: 지갑 등록 ----- */
function onbStep2() {
  const card = document.getElementById("onbCard");
  card.innerHTML =
    '<div class="onb-shead"><div class="st">지갑 주소 등록</div><div class="ss">주소를 추가하면 목록에 쌓여요. 별칭은 비워둬도 괜찮아요.</div></div>' +
    '<div class="onb-field"><label>지갑 주소</label><input class="onb-input mono" id="onbAddr" placeholder="0x…" autocomplete="off" spellcheck="false"></div>' +
    '<div class="onb-field"><label>별칭<span class="opt">선택</span></label><div class="onb-inrow"><input class="onb-input" id="onbAlias" placeholder="예: 메인 지갑" autocomplete="off"><button class="onb-add" id="onbAdd">추가</button></div></div>' +
    '<div class="onb-msg" id="onbMsg"></div>' +
    '<div class="onb-wlist" id="onbWlist"></div>' +
    '<button class="onb-cta" id="onbNext2">계속 →</button>' +
    '<div class="onb-help" id="onbHelp2"></div>';
  const addr = document.getElementById("onbAddr");
  const alias = document.getElementById("onbAlias");
  const msg = document.getElementById("onbMsg");
  function setMsg(t, k) { msg.className = "onb-msg" + (k ? " " + k : ""); msg.textContent = t || ""; }
  function tryAdd() {
    const a = addr.value.trim(); setMsg(""); addr.classList.remove("err");
    if (!a) { addr.classList.add("err"); setMsg("주소를 입력하세요.", "err"); addr.focus(); return; }
    if (!S.isAddressShape(a)) { addr.classList.add("err"); setMsg("0x로 시작하는 40자리 16진수 주소를 입력하세요.", "err"); addr.focus(); return; }
    if (state.onb.wallets.some((w) => w.address.toLowerCase() === a.toLowerCase())) { addr.classList.add("err"); setMsg("이미 등록된 주소예요.", "err"); addr.focus(); return; }
    const warn = S.checksumWarn(a);
    state.onb.wallets.push({ address: a, nickname: alias.value.trim() });
    addr.value = ""; alias.value = "";
    if (warn) setMsg(warn, "warn");
    renderOnbWallets(); paintOnbGate2(); addr.focus();
  }
  document.getElementById("onbAdd").addEventListener("click", tryAdd);
  addr.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); tryAdd(); } });
  alias.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); tryAdd(); } });
  document.getElementById("onbNext2").addEventListener("click", () => { if (state.onb.wallets.length) advanceStep(3); });
  renderOnbWallets(); paintOnbGate2();
  setTimeout(() => addr.focus(), 30);
}
function renderOnbWallets() {
  const list = document.getElementById("onbWlist"); if (!list) return;
  if (!state.onb.wallets.length) { list.innerHTML = '<div class="onb-wempty">아직 등록된 주소가 없어요</div>'; return; }
  list.innerHTML = "";
  state.onb.wallets.forEach((w, i) => {
    const row = document.createElement("div");
    row.className = "onb-witem";
    const nameHtml = w.nickname
      ? '<span class="wn">' + esc(w.nickname) + '</span><span class="wa">' + S.shortAddr(w.address) + '</span>'
      : '<span class="wa only">' + S.shortAddr(w.address) + '</span>';
    row.innerHTML =
      '<span class="av">' + S.identiconSVG(w.address, 28) + '</span>' +
      '<span class="wt">' + nameHtml + '</span>' +
      '<button class="wx" aria-label="삭제">×</button>';
    row.querySelector(".wx").addEventListener("click", () => { state.onb.wallets.splice(i, 1); renderOnbWallets(); paintOnbGate2(); });
    list.appendChild(row);
  });
}
function paintOnbGate2() {
  const btn = document.getElementById("onbNext2"); const help = document.getElementById("onbHelp2");
  if (!btn) return;
  const ok = state.onb.wallets.length >= 1;
  btn.disabled = !ok;
  btn.textContent = ok ? "계속 → (지갑 " + state.onb.wallets.length + "개)" : "계속 →";
  help.textContent = ok ? "" : "유효한 지갑 주소를 최소 한 개 등록하세요.";
}

/* ----- step 3: 베이스라인 ----- */
function onbStep3() {
  const card = document.getElementById("onbCard");
  const items = (S.BASELINE || []).map((b) => ({ id: b.id, title: b.title, sev: b.sev }));
  card.innerHTML =
    '<div class="onb-shead"><div class="st">베이스라인 정책</div><div class="ss">트랜잭션을 검사하는 기본 가드예요. 필요한 것만 켜세요 — 최소 한 개는 켜야 검사가 동작해요.</div></div>' +
    '<div class="onb-checks" id="onbChecks">' +
      items.map((p) =>
        '<div class="onb-check ' + (state.onb.baseline.has(p.id) ? "on" : "") + '" data-id="' + p.id + '">' +
          '<span class="box">' + I.check + '</span>' +
          '<span class="cmain"><span class="cn">' + p.title + '</span><span class="cd">' + p.id + '</span></span>' +
          '<span class="sev ' + p.sev + '"></span>' +
        '</div>').join("") +
    '</div>' +
    '<button class="onb-cta" id="onbNext3">베이스라인 켜고 계속 →</button>' +
    '<div class="onb-help" id="onbHelp3"></div>';
  card.querySelectorAll(".onb-check").forEach((row) => row.addEventListener("click", () => {
    const id = row.dataset.id;
    state.onb.baseline.has(id) ? state.onb.baseline.delete(id) : state.onb.baseline.add(id);
    row.classList.toggle("on", state.onb.baseline.has(id));
    paintOnbGate3();
  }));
  document.getElementById("onbNext3").addEventListener("click", () => { if (state.onb.baseline.size >= 1) advanceStep(4); });
  paintOnbGate3();
}
function paintOnbGate3() {
  const btn = document.getElementById("onbNext3"); const help = document.getElementById("onbHelp3");
  if (!btn) return;
  const ok = state.onb.baseline.size >= 1;
  btn.disabled = !ok;
  help.textContent = ok ? "" : "최소 한 개는 켜야 검사가 동작해요.";
}

/* ----- step 4: 완료 → 정책 화면 전환 ----- */
function onbStep4() {
  const card = document.getElementById("onbCard");
  card.innerHTML =
    '<div class="onb-ghead">' +
      '<div class="onb-guard"><div class="ring"></div><div class="face"><img src="' + ASSET + 'dambi-mark-navy.png" alt=""></div><div class="badge">' + I.check + '</div></div>' +
      '<div class="gt">보호가 켜졌어요</div>' +
      '<div class="gs">서명 직전마다 Dambi가 정책으로 검사해요.</div>' +
    '</div>' +
    '<div class="onb-sum">' +
      '<div class="onb-sumcard"><div class="n">' + state.onb.wallets.length + '</div><div class="l">등록한 지갑</div></div>' +
      '<div class="onb-sumcard"><div class="n">' + state.onb.baseline.size + '</div><div class="l">베이스라인 활성</div></div>' +
    '</div>' +
    '<button class="onb-cta" id="onbDone">시작하기</button>';
  document.getElementById("onbDone").addEventListener("click", onbFinish);
}
async function onbFinish() {
  const btn = document.getElementById("onbDone");
  if (btn) { btn.disabled = true; btn.textContent = "적용 중…"; }
  const wallets = state.onb.wallets.map((w, i) => ({ address: w.address, nickname: w.nickname || "", pinned: i === 0 }));
  const addresses = wallets.map((w) => w.address);
  // 1) 서버에 지갑 등록 → 2) 체크 해제된 builtin 은 defaults.enabled=false →
  // 3) ps2 프로비저닝(체크 유지 def 만 바인딩) — 전부 best-effort.
  if (S.addWallet) {
    for (const w of wallets) {
      try { await S.addWallet(w.address, w.nickname || undefined); } catch (e) {}
    }
  }
  const offDefIds = (S.BASELINE || []).map((b) => b.id).filter((id) => !state.onb.baseline.has(id));
  try { await S.applyBaseline(addresses, offDefIds); } catch (e) { console.warn("[dambi] baseline apply failed:", e); }
  // 영속화(활성 주소/핀) 후 전체 리로드 — ps2 상태가 진실.
  try {
    await S.saveState({ account: state.account || { email: state.onb.email || "dev@team.xyz" }, activeAddress: addresses[0] || null, wallets, appliedByAddress: {} });
  } catch (e) {}
  let st;
  try { st = await S.loadState(); } catch (e) { st = S.defaults(); }
  state.account = st.account; state.activeAddress = st.activeAddress;
  state.wallets = st.wallets; state.appliedByAddress = st.appliedByAddress || {};
  state.appliedServer = (st.appliedServer || []).slice();
  state.onb = null;
  state.view = "main";
  state.catFilter = "all"; state.search = "";
  state.expanded = new Set(S.PACKAGES[0] ? [S.PACKAGES[0].id] : []);
  buildShell(); applySize(); renderAll();
}

const ONB_RENDER = { 1: onbStep1, 2: onbStep2, 3: onbStep3, 4: onbStep4 };

/* ============================================================
   라우팅 / 에러 화면
   ============================================================ */
function renderLoadError() {
  const root = document.getElementById("root");
  root.innerHTML =
    '<div class="pc-signin">' +
      '<div class="pc-guard off" style="margin:0 auto 6px"><div class="ring"></div><div class="face"><img src="' + ASSET + 'dambi-mark-navy.png" alt=""></div></div>' +
      '<div class="pc-si-h">연결할 수 없어요</div>' +
      '<div class="pc-si-s">백그라운드(서비스 워커)가 응답하지 않아요. 잠시 후 다시 시도해 주세요.</div>' +
      '<div class="pc-si-note" style="color:var(--fail-600)">' + esc(state.loadError) + '</div>' +
      '<button class="pc-si-google" id="retryBtn" style="justify-content:center">다시 시도</button>' +
    '</div>';
  document.getElementById("retryBtn").addEventListener("click", () => void reloadState());
}
async function reloadState() {
  state.loadError = "";
  const root = document.getElementById("root");
  if (root) root.innerHTML = '<div class="pc-signin"><div class="pc-si-s">불러오는 중…</div></div>';
  try {
    const st = await S.loadState();
    state.account = st.account; state.activeAddress = st.activeAddress;
    state.wallets = st.wallets; state.appliedByAddress = st.appliedByAddress;
    state.appliedServer = st.appliedServer || [];
  } catch (e) {
    state.loadError = String((e && e.message) || e);
  }
  if (!state.activeAddress && state.wallets[0]) state.activeAddress = state.wallets[0].address;
  route();
}

// 메인 4분할 렌더 (셸이 이미 존재한다고 가정)
function renderAll() { renderHero(); renderStick(); renderMain(); renderFooter(); }

// 진입 게이트: 로드오류 → 에러, 로그아웃 → 온보딩1, 로그인+지갑없음 → 온보딩2, 그 외 → 정책화면
function route() {
  if (state.loadError) { renderLoadError(); return; }
  if (!state.account) { enterOnboarding(1); return; }
  if (!state.wallets.length || !state.activeAddress) { enterOnboarding(2); return; }
  normalizePins();
  state.view = "main";
  buildShell(); applySize(); renderAll();
}

/* ============================================================
   INIT
   ============================================================ */
(async function init() {
  // 저장된 popup 크기 선호 복원
  try {
    if (typeof chrome !== "undefined" && chrome.storage) {
      const r = await chrome.storage.local.get("dambi.popup.big");
      state.expanded2 = !!r["dambi.popup.big"];
    }
  } catch (e) {}
  try { state.settings = await S.loadSettings(); } catch (e) { state.settings = S.SETTINGS_DEFAULT; }
  try {
    const st = await S.loadState();
    state.account = st.account; state.activeAddress = st.activeAddress;
    state.wallets = st.wallets; state.appliedByAddress = st.appliedByAddress;
    state.appliedServer = st.appliedServer || [];
    state.loadError = "";
  } catch (e) {
    state.loadError = String((e && e.message) || e);
    state.account = null; state.wallets = []; state.activeAddress = null; state.appliedByAddress = {};
  }
  if (!state.activeAddress && state.wallets[0]) state.activeAddress = state.wallets[0].address;
  route();
  S.onSettingsChange((s) => { state.settings = s; if (state.sheetOpen) renderSheet(); });
})();
