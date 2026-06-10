/* Pasu onboarding — 4-step flow
   1 Google 로그인 → 2 지갑 등록(다중) → 3 베이스라인 정책 → 4 완료
   전진 게이팅 · 뒤로만 허용 · 입력 보존 · 완료 시 store 영속화 */

const CHECK = '<svg viewBox="0 0 24 24" fill="none" width="13" height="13"><path d="M20 6 9 17l-5-5" stroke="currentColor" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round"/></svg>';
const G_SVG = '<svg class="gg" viewBox="0 0 48 48"><path fill="#4285F4" d="M45.12 24.5c0-1.56-.14-3.06-.4-4.5H24v8.51h11.84c-.51 2.75-2.06 5.08-4.39 6.64v5.52h7.11c4.16-3.83 6.56-9.47 6.56-16.17z"/><path fill="#34A853" d="M24 46c5.94 0 10.92-1.97 14.56-5.33l-7.11-5.52c-1.97 1.32-4.49 2.1-7.45 2.1-5.73 0-10.58-3.87-12.31-9.07H4.34v5.7C7.96 41.07 15.4 46 24 46z"/><path fill="#FBBC05" d="M11.69 28.18c-.44-1.32-.69-2.73-.69-4.18s.25-2.86.69-4.18v-5.7H4.34A21.99 21.99 0 0 0 2 24c0 3.55.85 6.91 2.34 9.88l7.35-5.7z"/><path fill="#EA4335" d="M24 9.75c3.23 0 6.13 1.11 8.41 3.29l6.31-6.31C34.91 2.97 29.93 1 24 1 15.4 1 7.96 5.93 4.34 13.12l7.35 5.7c1.73-5.2 6.58-9.07 12.31-9.07z"/></svg>';

const S = window.PasuStore;

const dotsEls = [...document.querySelectorAll("#dots .dot")];
const stsEls = [...document.querySelectorAll(".ob-st")];
const eyebrow = document.getElementById("eyebrow");
const titleEl = document.getElementById("title");
const ledeEl = document.getElementById("lede");
const card = document.getElementById("card");
const backBtn = document.getElementById("back");

/* ---- preserved state across steps ---- */
const flow = {
  step: 1,
  reached: 1,            // 가장 멀리 도달한 스텝(앞으로는 게이팅, 뒤로는 자유)
  email: null,
  wallets: [],           // { address, nickname }
  baseline: new Set(), // loadState 후 S.BASELINE(builtin)으로 시드
};

const COPY = {
  1: { eb: "설치 완료 · 1 / 4", t: "지갑이 이제 보호받기 시작해요", l: "먼저 Google 계정으로 로그인하면, 이 계정에 지갑을 묶어 정책을 동기화해요." },
  2: { eb: "지갑 등록 · 2 / 4", t: "어느 주소를 지킬까요?", l: "이 Google 계정에서 사용할 지갑 주소를 등록하세요. 여러 개 추가할 수 있어요." },
  3: { eb: "베이스라인 정책 · 3 / 4", t: "기본 보호를 켤게요", l: "권장 스왑 가드예요. 최소 한 개는 켜야 검사가 동작해요 — 언제든 팝업에서 바꿀 수 있어요." },
  4: { eb: "준비 완료 · 4 / 4", t: "이제 보호받고 있어요", l: "트랜잭션에 서명할 때 Pasu가 검토·차단을 페이지 위에서 바로 알려줘요." },
};

function paintChrome() {
  const n = flow.step;
  dotsEls.forEach((d, i) => d.classList.toggle("on", i < n));
  stsEls.forEach((s) => {
    const k = Number(s.dataset.step);
    s.classList.remove("done", "cur", "pend");
    s.classList.add(k < n ? "done" : k === n ? "cur" : "pend");
    s.querySelector(".num").innerHTML = k < n ? CHECK : String(k);
  });
  const c = COPY[n];
  eyebrow.textContent = c.eb; titleEl.textContent = c.t; ledeEl.textContent = c.l;
  backBtn.hidden = n === 1;
}

function goTo(n) {
  if (n > flow.reached) return;         // 점프 금지(앞으로 게이팅)
  flow.step = n;
  flow.reached = Math.max(flow.reached, n);
  paintChrome();
  RENDER[n]();
}
function advance(n) { flow.reached = Math.max(flow.reached, n); goTo(n); }

backBtn.addEventListener("click", () => { if (flow.step > 1) goTo(flow.step - 1); });

/* ============================================================
   STEP 1 — Google 로그인
   ============================================================ */
function renderStep1() {
  card.innerHTML =
    '<div class="ob-ghead">' +
      '<div class="ob-guard ' + (flow.email ? "" : "idle") + '" id="g1"><div class="ring"></div><div class="face"><img src="picture/pasu-mark-navy.png" alt="" /></div><div class="badge">' + CHECK + '</div></div>' +
      '<div class="gt">' + (flow.email ? "로그인됐어요" : "Google 계정으로 시작") + '</div>' +
      '<div class="gs">' + (flow.email ? "이 계정에 지갑과 정책이 동기화돼요." : "키는 받지 않아요 — 읽기 전용으로 정책만 묶어요.") + '</div>' +
    '</div>' +
    (flow.email
      ? '<div class="ob-authed"><div class="chk">' + CHECK + '</div><div><div class="at">Google 계정 연결됨</div><div class="ae" id="authedEmail">' + flow.email + '</div></div></div>' +
        '<button class="ob-cta" id="next1">계속 →</button>'
      : '<button class="ob-google" id="glogin">' + G_SVG + '<span>Google 계정으로 계속</span></button>' +
        '<div class="ob-note">// chrome.identity.launchWebAuthFlow (client type = chromeExtension). OAuth Client ID 무료 · Identity Platform 비활성화.</div>');

  const gbtn = document.getElementById("glogin");
  if (gbtn) gbtn.addEventListener("click", async () => {
    gbtn.classList.add("loading");
    gbtn.innerHTML = '<span class="spin"></span><span>로그인 중…</span>';
    try {
      const acct = await S.signIn(); // 실제 Google OAuth (SW 가 수행)
      flow.email = (acct && acct.email) || "";
      flow.reached = Math.max(flow.reached, 2); // 스텝 2 활성(전진 가능)
    } catch (e) {
      gbtn.classList.remove("loading");
      gbtn.innerHTML = G_SVG + '<span>다시 시도 — Google 계정으로 계속</span>';
      return;
    }
    renderStep1();
    paintChrome();
  });
  const n1 = document.getElementById("next1");
  if (n1) n1.addEventListener("click", () => advance(2));
}

/* ============================================================
   STEP 2 — 지갑 주소 등록 (다중)
   ============================================================ */
function renderStep2() {
  card.innerHTML =
    '<div class="ob-shead"><div class="st">지갑 주소 등록</div><div class="ss">주소를 추가하면 목록에 쌓여요. 별칭은 비워둬도 괜찮아요.</div></div>' +
    '<div class="ob-walform">' +
      '<div class="ob-field"><label for="waddr">지갑 주소</label><input class="ob-input mono" id="waddr" placeholder="0x…" autocomplete="off" spellcheck="false" /></div>' +
      '<div class="ob-field"><label for="walias">별칭<span class="opt">선택</span></label>' +
        '<div style="display:flex;gap:8px"><input class="ob-input" id="walias" placeholder="예: 메인 지갑" autocomplete="off" />' +
        '<button class="ob-add" id="waddBtn">추가</button></div></div>' +
      '<div class="ob-msg" id="wmsg"></div>' +
    '</div>' +
    '<div class="ob-wallets-list" id="wlist"></div>' +
    '<button class="ob-cta" id="next2">계속 →</button>' +
    '<div class="ob-cta-help" id="help2"></div>';

  const addr = document.getElementById("waddr");
  const alias = document.getElementById("walias");
  const msg = document.getElementById("wmsg");
  const addBtn = document.getElementById("waddBtn");

  function setMsg(text, kind) { msg.className = "ob-msg" + (kind ? " " + kind : ""); msg.textContent = text || ""; }

  function tryAdd() {
    const a = addr.value.trim();
    setMsg("");
    addr.classList.remove("err");
    if (!a) { addr.classList.add("err"); setMsg("주소를 입력하세요.", "err"); addr.focus(); return; }
    if (!S.isAddressShape(a)) { addr.classList.add("err"); setMsg("0x로 시작하는 40자리 16진수 주소를 입력하세요.", "err"); addr.focus(); return; }
    if (flow.wallets.some((w) => w.address.toLowerCase() === a.toLowerCase())) { addr.classList.add("err"); setMsg("이미 등록된 주소예요.", "err"); addr.focus(); return; }
    const warn = S.checksumWarn(a);
    flow.wallets.push({ address: a, nickname: alias.value.trim() });
    addr.value = ""; alias.value = "";
    if (warn) setMsg(warn, "warn"); // 경고 수준 — 등록은 허용
    renderWalletList(); paintGate2(); addr.focus();
  }

  addBtn.addEventListener("click", tryAdd);
  addr.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); tryAdd(); } });
  alias.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); tryAdd(); } });

  document.getElementById("next2").addEventListener("click", () => { if (flow.wallets.length) advance(3); });

  renderWalletList(); paintGate2();
  setTimeout(() => addr.focus(), 30);
}

function renderWalletList() {
  const list = document.getElementById("wlist"); if (!list) return;
  list.innerHTML = "";
  if (!flow.wallets.length) {
    list.innerHTML = '<div class="ob-wlist-empty">아직 등록된 주소가 없어요</div>';
    return;
  }
  flow.wallets.forEach((w, i) => {
    const row = document.createElement("div");
    row.className = "ob-witem";
    const nameHtml = w.nickname
      ? '<span class="wn">' + escapeHtml(w.nickname) + '</span><span class="wa">' + S.shortAddr(w.address) + '</span>'
      : '<span class="wa only">' + S.shortAddr(w.address) + '</span>';
    row.innerHTML =
      '<span class="av">' + S.identiconSVG(w.address, 28) + '</span>' +
      '<span class="wtext">' + nameHtml + '</span>' +
      '<button class="wx" aria-label="삭제" data-i="' + i + '">×</button>';
    row.querySelector(".wx").addEventListener("click", () => { flow.wallets.splice(i, 1); renderWalletList(); paintGate2(); });
    list.appendChild(row);
  });
}

function paintGate2() {
  const btn = document.getElementById("next2"); const help = document.getElementById("help2");
  if (!btn) return;
  const ok = flow.wallets.length >= 1;
  btn.disabled = !ok;
  btn.textContent = ok ? "계속 → (지갑 " + flow.wallets.length + "개)" : "계속 →";
  help.textContent = ok ? "" : "유효한 지갑 주소를 최소 한 개 등록하세요.";
}

/* ============================================================
   STEP 3 — 베이스라인 정책 체크리스트 (최소 1)
   ============================================================ */
function renderStep3() {
  const items = (S.BASELINE || []).map((b) => ({ id: b.id, title: b.title, sev: b.sev }));
  card.innerHTML =
    '<div class="ob-shead"><div class="st">베이스라인 정책</div><div class="ss">스왑 트랜잭션을 검사하는 기본 가드예요. 기본으로 모두 켜져 있어요.</div></div>' +
    '<div class="ob-checks" id="checks">' +
      items.map((p) =>
        '<div class="ob-check ' + (flow.baseline.has(p.id) ? "on" : "") + '" data-id="' + p.id + '">' +
          '<span class="box">' + CHECK + '</span>' +
          '<span class="cmain"><span class="cn">' + p.title + '</span><span class="cd">' + p.id + '</span></span>' +
          '<span class="sev ' + p.sev + '" title="' + sevLabel(p.sev) + '"></span>' +
        '</div>').join("") +
    '</div>' +
    '<button class="ob-cta" id="next3">베이스라인 켜고 계속 →</button>' +
    '<div class="ob-cta-help" id="help3"></div>';

  card.querySelectorAll(".ob-check").forEach((row) =>
    row.addEventListener("click", () => {
      const id = row.dataset.id;
      flow.baseline.has(id) ? flow.baseline.delete(id) : flow.baseline.add(id);
      row.classList.toggle("on", flow.baseline.has(id));
      paintGate3();
    }));
  document.getElementById("next3").addEventListener("click", () => { if (flow.baseline.size >= 1) advance(4); });
  paintGate3();
}

function paintGate3() {
  const btn = document.getElementById("next3"); const help = document.getElementById("help3");
  if (!btn) return;
  const ok = flow.baseline.size >= 1;
  btn.disabled = !ok;
  help.textContent = ok ? "" : "최소 한 개는 켜야 검사가 동작해요.";
}

/* ============================================================
   STEP 4 — 완료
   ============================================================ */
function renderStep4() {
  card.innerHTML =
    '<div class="ob-ghead">' +
      '<div class="ob-guard" id="g4"><div class="ring"></div><div class="face"><img src="picture/pasu-mark-navy.png" alt="" /></div><div class="badge">' + CHECK + '</div></div>' +
      '<div class="gt">보호가 켜졌어요</div>' +
      '<div class="gs">서명 직전마다 Pasu가 정책으로 검사해요.</div>' +
    '</div>' +
    '<div class="ob-summary">' +
      '<div class="ob-sumcard"><div class="n">' + flow.wallets.length + '</div><div class="l">등록한 지갑</div></div>' +
      '<div class="ob-sumcard"><div class="n">' + flow.baseline.size + '</div><div class="l">베이스라인 활성</div></div>' +
    '</div>' +
    '<button class="ob-cta" id="done">시작하기</button>';
  document.getElementById("done").addEventListener("click", finish);
}

async function finish() {
  // v2: 서버 지갑 등록 → 체크 해제 builtin defaults off → ps2 프로비저닝.
  try {
    // loadState 로 현재 로그인 계정(uid)·라이브러리를 store 에 세팅.
    const cur = await S.loadState();
    const account = cur.account || (flow.email ? { email: flow.email } : null);
    const wallets = flow.wallets.map((w, i) => ({ address: w.address, nickname: w.nickname || "", pinned: i === 0 }));
    const addresses = wallets.map((w) => w.address);
    if (S.addWallet) {
      for (const w of wallets) { try { await S.addWallet(w.address, w.nickname || undefined); } catch (e) {} }
    }
    const offDefIds = (S.BASELINE || []).map((b) => b.id).filter((id) => !flow.baseline.has(id));
    try { await S.applyBaseline(addresses, offDefIds); } catch (e) { console.warn("[pasu] baseline apply failed:", e); }
    await S.saveState({
      account,
      activeAddress: wallets[0] ? wallets[0].address : null,
      wallets,
      appliedByAddress: {},
    });
  } catch (e) {}
  if (typeof chrome !== "undefined" && chrome.tabs) chrome.tabs.getCurrent((t) => t && chrome.tabs.remove(t.id));
  else window.close();
}

/* ---- helpers ---- */
const RENDER = { 1: renderStep1, 2: renderStep2, 3: renderStep3, 4: renderStep4 };
function sevLabel(s) { return s === "deny" ? "차단" : s === "warn" ? "검토" : "확인"; }
function escapeHtml(s) { return s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c])); }

/* ============================================================
   INIT — 현재 로그인/지갑 상태를 보고 적절한 스텝으로 자동 진입.
     · 로그인 안 됨        → step1 (Google 로그인)
     · 로그인 + 지갑 없음   → step2 (지갑 등록)  ← 로그인 스텝 스킵
     · 로그인 + 지갑 있음   → 이미 온보딩 완료 → 창 닫고 popup 으로
   ============================================================ */
(async function initWelcome() {
  let st = null;
  try { st = await S.loadState(); } catch (e) { st = null; }
  // builtin 베이스라인을 전부 체크된 상태로 시드 (loadState 가 S.BASELINE 채움)
  if (!flow.baseline.size) flow.baseline = new Set((S.BASELINE || []).map((b) => b.id));

  const loggedIn = !!(st && st.account);
  const hasWallets = !!(st && Array.isArray(st.wallets) && st.wallets.length);

  if (loggedIn && hasWallets) {
    // 기록이 있는 계정 — 재온보딩 불필요. 창을 닫는다(popup 이 본 화면을 띄움).
    if (typeof chrome !== "undefined" && chrome.tabs && chrome.tabs.getCurrent) {
      chrome.tabs.getCurrent((t) => { if (t) chrome.tabs.remove(t.id); else window.close(); });
    } else {
      window.close();
    }
    return;
  }

  if (loggedIn) {
    // 로그인은 됐는데 지갑이 없음 → 로그인 스텝 스킵하고 지갑 등록부터.
    flow.email = st.account.email;
    flow.reached = 2;
    paintChrome();
    goTo(2);
    return;
  }

  // 비로그인 → 처음부터.
  paintChrome();
  renderStep1();
})();
