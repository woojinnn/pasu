/* ============================================================
   DAMBI — backend-wired store
   Claude 디자인 popup UI(popup.js)가 기대하는 DambiStore 인터페이스를
   그대로 유지하되, 데모 데이터 대신 service-worker 메시지에 연결한다.

     loadState()   → policy-catalog + dambi-auth-status + dambi-list-wallets
     saveState()   → set-enabled-ids  (활성 주소 = 단일 계정 enabled)
     PACKAGES/POLICIES → catalog.policies 로부터 동적 생성 (sourceLabel 그룹핑)

   백엔드 모델은 "단일 사용자 단위 평면 enabled 목록"이고, zip UI 는
   "주소별 appliedByAddress"이다. 단일 계정이므로 모든 지갑 주소가 같은
   enabled 집합을 공유하도록 매핑한다(주소 전환 시 같은 토글 상태).
   ============================================================ */
(function (global) {
  "use strict";

  /* ---------- service-worker 메시지 ---------- */
  const rt =
    typeof chrome !== "undefined" && chrome.runtime ? chrome.runtime : null;

  // 멈춘(wedged) 서비스워커 방어: timeoutMs 안에 응답이 없으면 reject.
  // 원본 index.ts 의 fetchCatalog 5초 가드와 동일한 의도.
  function send(type, extra, timeoutMs) {
    const ms = timeoutMs == null ? 5000 : timeoutMs;
    return new Promise((resolve, reject) => {
      if (!rt || !rt.sendMessage) {
        reject(new Error("no extension runtime"));
        return;
      }
      let settled = false;
      const timer = setTimeout(() => {
        if (settled) return;
        settled = true;
        reject(new Error("no response from service worker (timeout " + Math.round(ms / 1000) + "s): " + type));
      }, ms);
      const done = (fn, val) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        fn(val);
      };
      try {
        rt.sendMessage(Object.assign({ type }, extra || {}), (res) => {
          const lastErr = rt.lastError || (chrome.runtime && chrome.runtime.lastError);
          if (lastErr) { done(reject, new Error(lastErr.message || "sendMessage failed")); return; }
          if (!res) { done(reject, new Error("empty response from service worker")); return; }
          if (res.ok === false) {
            const e = res.error || {};
            done(reject, new Error((e.kind || "error") + ": " + (e.message || "")));
            return;
          }
          done(resolve, res.data);
        });
      } catch (e) {
        done(reject, e);
      }
    });
  }

  // 콜드부팅 방어: popup이 SW를 막 깨운 직후에는 부팅 시퀀스(스토리지
  // 마이그레이션·wasm·번들 설치)가 ps2 응답을 5초 넘게 잡아둘 수 있다.
  // 첫 호출이 실패하면 잠깐 기다렸다 한 번 더 — 두 번째는 여유 있게.
  // (이게 없으면 catch 폴백이 "빈 라이브러리"를 그대로 그려서, 한 번씩
  // 정책이 하나도 안 뜨는 popup이 된다.)
  async function sendRetry(type, extra) {
    try {
      return await send(type, extra);
    } catch (e) {
      await new Promise((r) => setTimeout(r, 1200));
      return send(type, extra, 12000);
    }
  }

  /* ---------- 동적 정책/패키지 (catalog 에서 채워짐) ---------- */
  // popup.js 가 `S.POLICIES[id].title` / `S.PACKAGES` 를 동기적으로 읽으므로,
  // loadState() 가 catalog 를 받아 아래 객체를 in-place 로 갱신한다.
  const POLICIES = {}; // id -> { title, sev }
  let PACKAGES = []; // [{ id, name, source:{kind,label}, members:[id] }]

  // ps2 라이브러리(정의/패키지) — loadState 가 채우고, 온보딩/재조회가 쓴다.
  let LIBRARY = null;

  function pkgSource(id) {
    if (id.indexOf("pkg::builtin") === 0) return { kind: "builtin", label: "기본 제공" };
    if (id.indexOf("pkg::market") === 0) return { kind: "market", label: "마켓" };
    return { kind: "org", label: "내 패키지" };
  }

  /** ps2 라이브러리 + 활성 지갑 상태 → POLICIES(bindingId 키)/PACKAGES 갱신.
   *  반환값: enabled 인 bindingId 배열(appliedByAddress 캐시에 들어간다). */
  function rebuildFromPs2(lib, walletState) {
    LIBRARY = lib;
    api.BASELINE = window.DambiPs2.deriveBaseline(lib);
    for (const k of Object.keys(POLICIES)) delete POLICIES[k];
    const pkgs = window.DambiPs2.derivePopupPackages(lib, walletState || { bindings: {}, packageEnabled: {} });
    const enabled = [];
    for (const pkg of pkgs) {
      for (const m of pkg.members) {
        POLICIES[m.bindingId] = { title: m.name, sev: m.sev, reasons: [], category: undefined };
        if (m.enabled) enabled.push(m.bindingId);
      }
    }
    PACKAGES = pkgs.map((p) => ({
      id: p.id,
      name: p.name,
      on: p.on,
      source: pkgSource(p.id),
      members: p.members.map((m) => m.bindingId),
    }));
    api.PACKAGES = PACKAGES;
    return enabled;
  }

  /* ---------- 설정 프리셋 (options 공유) ---------- */
  const SETTINGS_PRESETS = {
    quiet: { desk: "block", modal: "block", ribbon: false, sound: false },
    std: { desk: "both", modal: "both", ribbon: true, sound: false },
    loud: { desk: "all", modal: "both", ribbon: true, sound: true },
  };
  const SETTINGS_DEFAULT = { preset: "std", ...SETTINGS_PRESETS.std };


  /* ---------- 계정별 로컬 프로필 (지갑·별칭·핀·주소별 정책) ----------
     지갑 모델 = "로컬 수동 주소". 단, 계정(uid)별로 격리 저장해서
     다른 Google 계정으로 로그인하면 이전 계정 지갑이 안 보이게 한다.
     키: chrome.storage.local["dambi.profile.<uid>"]
       = { activeAddress, wallets:[{address,nickname,pinned}], appliedByAddress:{addr:[ids]} }
     uid: 로그인 시 me.user_id, 로그아웃 시 null(=프로필 없음). */
  const hasLocal =
    typeof chrome !== "undefined" && chrome.storage && chrome.storage.local;
  function profileKey(uid) {
    return "dambi.profile." + (uid || "__anon__");
  }
  async function readProfile(uid) {
    if (!hasLocal) return null;
    try {
      const key = profileKey(uid);
      const r = await chrome.storage.local.get(key);
      return r[key] || null;
    } catch (e) {
      return null;
    }
  }
  async function writeProfile(uid, profile) {
    if (!hasLocal) return;
    try {
      await chrome.storage.local.set({ [profileKey(uid)]: profile });
    } catch (e) {
      /* ignore */
    }
  }

  // 현재 로그인된 계정(uid + email). 로그아웃이면 null.
  let currentUid = null;

  /* ---------- 실제 인증 (dambi auth) ---------- */
  // 로그인: Google OAuth 플로우를 SW 가 수행하고 Me 를 반환.
  // 반환에 isFirstLogin 포함 — 이 계정(uid)의 로컬 프로필이 아직 없으면 true
  // (= 이 계정으로 처음 로그인 → 온보딩 welcome 을 띄울 신호).
  async function signIn() {
    // 인터랙티브 구글 OAuth(계정 선택·비밀번호·2FA)는 기본 5초 가드를 거의
    // 항상 넘긴다. 5초에 reject 되면 팝업이 "실패"로 오인해 재시도를 유도하고,
    // 그 사이 서비스워커는 토큰을 저장 → 두 번째 클릭에 "성공"하는 것처럼 보여
    // "매번 2번 로그인" 증상이 생겼다. OAuth 에 충분한 3분으로 가드를 늘린다.
    const me = await send("dambi-auth-sign-in", null, 180000);
    if (!me || !me.email) return null;
    const uid = me.user_id || me.email;
    currentUid = uid;
    const prof = await readProfile(uid);
    const isFirstLogin = !(prof && Array.isArray(prof.wallets) && prof.wallets.length);
    return { email: me.email, isFirstLogin };
  }
  // 로그아웃: 저장된 토큰 제거. (프로필은 uid 별로 남으므로 재로그인 시 복구)
  async function signOut() {
    await send("dambi-auth-sign-out");
    currentUid = null;
  }

  // 서버에 지갑 등록 (대시보드와 공유). 실패해도 로컬 프로필에는 남는다.
  async function addWallet(address, label) {
    try {
      await send("dambi-add-wallet", { address, label }, 30000);
      return true;
    } catch (e) {
      console.warn("[dambi] add-wallet(server) failed:", e);
      return false;
    }
  }

  // 서버에서 지갑 삭제 (대시보드와 즉시 동기화). throw 로 호출자에 결과 전달.
  async function removeWallet(address) {
    await send("dambi-delete-wallet", { address });
  }
  // 서버 닉네임(label) 변경. label="" 이면 서버에서 라벨 제거.
  async function renameWallet(address, label) {
    await send("dambi-update-wallet", { address, label });
  }

  // 온보딩(welcome) 탭 열기 — 첫 로그인 계정에서 호출.
  function openWelcome() {
    try {
      if (typeof chrome !== "undefined" && chrome.tabs && chrome.runtime) {
        chrome.tabs.create({ url: chrome.runtime.getURL("welcome.html") });
      }
    } catch (e) {
      /* ignore */
    }
  }

  /* ---------- 상태 로드/저장 ---------- */

  async function loadState() {
    // 인증 상태(uid + 이메일)
    let account = null;
    let uid = null;
    try {
      const me = await send("dambi-auth-status");
      if (me && me.email) {
        account = { email: me.email };
        uid = me.user_id || me.email; // user_id 우선, 없으면 email 로 격리
      }
    } catch (e) {
      /* 서버 다운/로그아웃 → null */
    }
    currentUid = uid;

    // 로그아웃 상태면 빈 상태 반환 (지갑 없음 → popup 이 로그인 유도)
    if (!account) {
      return { account: null, activeAddress: null, wallets: [], appliedByAddress: {} };
    }

    // 로컬 프로필은 "보조"로만 사용한다: 핀(pinned) + 주소별 정책(appliedByAddress).
    // 지갑의 존재 여부와 닉네임(label)은 서버가 단일 진실(source of truth)이다.
    const prof = (await readProfile(uid)) || {};
    const localByAddr = new Map();
    for (const w of (Array.isArray(prof.wallets) ? prof.wallets : [])) {
      localByAddr.set((w.address || "").toLowerCase(), w);
    }
    const appliedByAddress = prof.appliedByAddress || {};

    // 서버 지갑 목록 (label 포함). 이게 진실 — 삭제/이름변경이 즉시 반영된다.
    // reconcile(로컬→서버 백필)은 제거: 삭제한 지갑이 옛 닉네임으로 부활하는
    // 원인이었다. 서버에 없으면 popup 에도 없다.
    let summaries = [];
    try {
      summaries = (await sendRetry("dambi-list-wallet-summaries")) || [];
    } catch (e) {
      // summary 실패 시 주소만이라도 (label 없이)
      try {
        const list = await send("dambi-list-wallets");
        summaries = (list || []).map((w) => ({ address: w.address, label: null }));
      } catch (e2) {
        summaries = [];
      }
    }

    // 서버 지갑 → 표시 지갑. 닉네임은 서버 label, 핀은 로컬 보조.
    const wallets = summaries.map((s) => {
      const a = (s.address || "").toLowerCase();
      const local = localByAddr.get(a);
      return {
        address: a,
        nickname: s.label || "",
        pinned: !!(local && local.pinned),
      };
    });

    // ps2 라이브러리 — 패키지 카드/베이스라인 목록의 원천. 실패 시 빈 라이브러리.
    let lib = { defs: {}, packages: {} };
    try {
      const r = await sendRetry("ps2:get-library");
      lib = (r && r.library) || lib;
    } catch (e) {
      /* SW 미부팅 등 — 빈 카드로 폴백 */
    }

    if (!wallets.length) {
      // 서버에 지갑 없음 → popup 에서 "새 주소 추가" 유도.
      rebuildFromPs2(lib, null);
      return { account, activeAddress: null, wallets: [], appliedByAddress: {} };
    }

    const byAddr = new Map(wallets.map((w) => [w.address, w]));
    const active =
      (prof.activeAddress && byAddr.has(prof.activeAddress.toLowerCase()) && prof.activeAddress.toLowerCase()) ||
      wallets[0].address;
    // 활성 주소의 진짜 per-wallet 상태(ps2) — enabled 캐시는 표시용.
    let walletState = null;
    try {
      walletState = await sendRetry("ps2:get-wallet-state", { address: active });
    } catch (e) {
      /* 폴백: 빈 상태 */
    }
    const enabled = rebuildFromPs2(lib, walletState);
    appliedByAddress[active] = enabled.slice();
    return { account, activeAddress: active, wallets, appliedByAddress, appliedServer: enabled.slice() };
  }

  /** 활성 지갑 재조회(토글/지갑 전환 후) — enabled bindingId 배열 반환. */
  async function refreshActivePolicies(address) {
    const walletState = await sendRetry("ps2:get-wallet-state", { address });
    return rebuildFromPs2(LIBRARY || { defs: {}, packages: {} }, walletState);
  }

  async function setBindingEnabled(address, bindingId, on) {
    await send("ps2:update-binding", { address, bindingId, patch: { enabled: on } });
  }

  async function setPackageOn(address, packageId, on) {
    await send("ps2:set-package-enabled", { address, packageId, enabled: on });
  }

  /** 전체 켜기/끄기 — 활성 지갑의 모든 바인딩 + 패키지 토글. */
  async function setAllBindings(address, on) {
    const ws = await send("ps2:get-wallet-state", { address });
    for (const pkgId of Object.keys(ws.packageEnabled || {})) {
      if (ws.packageEnabled[pkgId] === false && on) {
        await send("ps2:set-package-enabled", { address, packageId: pkgId, enabled: true });
      }
    }
    for (const b of Object.values(ws.bindings || {})) {
      if (b.enabled !== on) {
        await send("ps2:update-binding", { address, bindingId: b.id, patch: { enabled: on } });
      }
    }
  }

  /** 온보딩 베이스라인 적용: 체크 해제된 builtin def 는 defaults.enabled=false 로
   *  내리고(앞으로의 지갑), 프로비저닝 후 기존 바인딩도 동기 off. */
  async function applyBaseline(addresses, offDefIds) {
    for (const defId of offDefIds) {
      const def = LIBRARY && LIBRARY.defs && LIBRARY.defs[defId];
      if (!def) continue;
      await send("ps2:put-def", {
        def: Object.assign({}, def, {
          defaults: Object.assign({}, def.defaults, { enabled: false }),
          updatedAtMs: Date.now(),
        }),
      });
    }
    if (addresses.length) await send("ps2:provision-wallets", { addresses });
    for (const a of addresses) {
      let ws;
      try {
        ws = await send("ps2:get-wallet-state", { address: a });
      } catch (e) {
        continue;
      }
      for (const b of Object.values((ws && ws.bindings) || {})) {
        if (offDefIds.indexOf(b.defId) >= 0 && b.enabled) {
          await send("ps2:update-binding", { address: a, bindingId: b.id, patch: { enabled: false } });
        }
      }
    }
  }

  async function saveState(state) {
    // 1) 계정별 로컬 프로필 저장 (지갑·별칭·핀·주소별 정책)
    if (state.account && currentUid) {
      await writeProfile(currentUid, {
        activeAddress: state.activeAddress,
        wallets: state.wallets || [],
        appliedByAddress: state.appliedByAddress || {},
      });
    }
    // 정책 적용은 ps2 토글(setBindingEnabled/setPackageOn)이 즉시 수행한다 —
    // 여기는 로컬 프로필(활성 주소/핀)만 저장한다.
  }

  /* ---------- 설정(알림 강도) — chrome.storage.sync 직접 ---------- */
  const hasSync =
    typeof chrome !== "undefined" && chrome.storage && chrome.storage.sync;

  async function loadSettings() {
    if (hasSync) {
      try {
        const r = await chrome.storage.sync.get("settings");
        return { ...SETTINGS_DEFAULT, ...(r.settings || {}) };
      } catch (e) {
        /* fall through */
      }
    }
    return { ...SETTINGS_DEFAULT };
  }
  async function saveSettings(settings) {
    if (hasSync) {
      try {
        await chrome.storage.sync.set({ settings });
      } catch (e) {
        /* ignore */
      }
    }
  }
  function onSettingsChange(cb) {
    if (
      typeof chrome !== "undefined" &&
      chrome.storage &&
      chrome.storage.onChanged
    ) {
      chrome.storage.onChanged.addListener((changes, area) => {
        if (area === "sync" && changes.settings)
          cb({ ...SETTINGS_DEFAULT, ...changes.settings.newValue });
      });
    }
  }

  /* ---------- defaults (loadState 실패 시 폴백) ---------- */
  function defaults() {
    const zero = "0x0000000000000000000000000000000000000000";
    return {
      account: null,
      activeAddress: zero,
      wallets: [{ address: zero, nickname: "내 계정", pinned: true }],
      appliedByAddress: { [zero]: [] },
    };
  }

  /* ---------- 주소 유틸 ---------- */
  function shortAddr(a) {
    return a ? a.slice(0, 6) + "…" + a.slice(-4) : "";
  }
  function isAddressShape(a) {
    return /^0x[0-9a-fA-F]{40}$/.test((a || "").trim());
  }
  function checksumWarn(a) {
    const body = a.slice(2);
    const mixed = /[a-f]/.test(body) && /[A-F]/.test(body);
    return mixed
      ? null
      : "체크섬이 적용되지 않은 주소예요. 주소를 다시 확인하세요.";
  }

  /* ---------- identicon ---------- */
  const IDENTICON_PALETTE = [
    "#22A06B", "#2775CA", "#34476C", "#DCA02C",
    "#566A91", "#1B8C5E", "#5C6B86", "#C0821C",
  ];
  function seedFrom(str) {
    let h = 2166136261;
    for (let i = 0; i < str.length; i++) {
      h ^= str.charCodeAt(i);
      h = Math.imul(h, 16777619) >>> 0;
    }
    return h >>> 0;
  }
  function identiconSVG(address, size) {
    size = size || 28;
    let s = seedFrom((address || "0x").toLowerCase());
    const rnd = () => {
      s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
      return s / 4294967296;
    };
    const color = IDENTICON_PALETTE[Math.floor(rnd() * IDENTICON_PALETTE.length)];
    let spot = IDENTICON_PALETTE[Math.floor(rnd() * IDENTICON_PALETTE.length)];
    if (spot === color)
      spot =
        IDENTICON_PALETTE[
          (IDENTICON_PALETTE.indexOf(color) + 3) % IDENTICON_PALETTE.length
        ];
    const bg = "#EEF1F7";
    const N = 5,
      cell = size / N;
    let cells = "";
    for (let y = 0; y < N; y++) {
      for (let x = 0; x < Math.ceil(N / 2); x++) {
        const r = rnd();
        const c = r > 0.66 ? spot : r > 0.42 ? color : null;
        if (!c) continue;
        const xs = [x, N - 1 - x];
        for (const xx of xs) {
          cells +=
            '<rect x="' + (xx * cell).toFixed(2) + '" y="' + (y * cell).toFixed(2) +
            '" width="' + (cell + 0.5).toFixed(2) + '" height="' + (cell + 0.5).toFixed(2) +
            '" fill="' + c + '"/>';
        }
      }
    }
    return (
      '<svg viewBox="0 0 ' + size + " " + size + '" width="' + size +
      '" height="' + size + '" xmlns="http://www.w3.org/2000/svg"><rect width="' +
      size + '" height="' + size + '" fill="' + bg + '"/>' + cells + "</svg>"
    );
  }

  /* ---------- 패키지 상태 헬퍼 ----------
     하이브리드: 게이트(packageEnabled)가 켜져 있어도 활성 멤버가 없으면
     꺼진 것으로 보여준다. enabledSet = 활성 bindingId 집합. */
  function pkgState(pkg, enabledSet) {
    return pkg.on && pkg.members.some((id) => enabledSet.has(id)) ? "on" : "off";
  }

  /** 하이브리드 켜기: 게이트 on + (전부 꺼져 있으면) 멤버 일괄 on. */
  async function enablePackage(address, pkg, enabledSet) {
    await send("ps2:set-package-enabled", { address, packageId: pkg.id, enabled: true });
    if (!pkg.members.some((id) => enabledSet.has(id))) {
      const ws = await send("ps2:get-wallet-state", { address });
      for (const b of Object.values(ws.bindings || {})) {
        if (b.packageId === pkg.id && !b.enabled) {
          await send("ps2:update-binding", { address, bindingId: b.id, patch: { enabled: true } });
        }
      }
    }
  }

  const api = {
    POLICIES,
    PACKAGES,
    BASELINE: [],
    SETTINGS_PRESETS,
    SETTINGS_DEFAULT,
    defaults,
    signIn,
    signOut,
    addWallet,
    removeWallet,
    renameWallet,
    openWelcome,
    loadState,
    saveState,
    loadSettings,
    saveSettings,
    onSettingsChange,
    shortAddr,
    isAddressShape,
    checksumWarn,
    identiconSVG,
    pkgState,
    enablePackage,
    refreshActivePolicies,
    setBindingEnabled,
    setPackageOn,
    setAllBindings,
    applyBaseline,
  };
  global.DambiStore = api;
})(typeof window !== "undefined" ? window : this);
