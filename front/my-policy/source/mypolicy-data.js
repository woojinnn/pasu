/* mypolicy-data.js — Scopeball "My Policy" 프론트 추출본 (예시 데이터 2개)
   상태 모델: state.life = 'draft' | 'publish'.  publish 안에서만 on/off 의미.
   예시 정책 2개:
     1) p-slip-wide   — 스왑 와이드 슬리피지 경고 (폼 편집 가능 · warn)
     2) p-permit-held — 보유 토큰 승인 위장 차단 (블록 전용 · fail)
   패키지는 비워 두었고, 필요 시 PACKAGES 배열에 채워 넣으면 됩니다. */
(function () {
  // ── 카테고리 액센트 ──
  var CAT = {
    swap:     { ko: "스왑",    en: "Swap",      fam: "cyan",  hex: "#688186", soft: "#DCEAED", ink: "#2B3639" },
    amm:      { ko: "AMM·LP",  en: "AMM·LP",    fam: "cyan",  hex: "#85A4AB", soft: "#EDF4F6", ink: "#2B3639" },
    perp:     { ko: "퍼프",    en: "Perp",      fam: "cyan",  hex: "#485A5E", soft: "#CAE0E4", ink: "#2B3639" },
    bridge:   { ko: "브릿지",  en: "Bridge",    fam: "cyan",  hex: "#A4C9D1", soft: "#EDF4F6", ink: "#485A5E" },
    security: { ko: "보안",    en: "Security",  fam: "sage",  hex: "#637E59", soft: "#EBF3E8", ink: "#283523" },
    airdrop:  { ko: "에어드랍", en: "Airdrop",  fam: "sage",  hex: "#44583D", soft: "#D9E9D3", ink: "#283523" },
    lending:  { ko: "렌딩",    en: "Lending",   fam: "slate", hex: "#384455", soft: "#D7DBDF", ink: "#0D1118" },
    nft:      { ko: "NFT",     en: "NFT",       fam: "slate", hex: "#697485", soft: "#EFF0F2", ink: "#1B222C" },
    core:     { ko: "코어",    en: "Core",      fam: "slate", hex: "#2A3441", soft: "#D7DBDF", ink: "#0D1118" },
    token:    { ko: "토큰",    en: "Token",     fam: "slate", hex: "#9099A5", soft: "#EFF0F2", ink: "#2A3441" }
  };
  var CAT_ORDER = ["security", "swap", "lending", "airdrop", "perp", "bridge", "nft", "amm", "core", "token"];

  // ── i18n ──
  function t(node, loc) { if (!node) return ""; return node[loc] != null ? node[loc] : (node.ko || node.en || ""); }

  // ── 단일 정책 (예시 2개) ──
  var SINGLES = [
    { id: "p-slip-wide", cat: "swap", method: "form", formCapable: true,
      name: { ko: "와이드 슬리피지 경고", en: "Wide-slippage warning" },
      slug: "swap-slippage-wide-warn", sev: "warn",
      state: { life: "publish", on: true } },

    { id: "p-permit-held", cat: "security", method: "block", formCapable: false,
      name: { ko: "보유 토큰 승인 위장 차단", en: "Spoofed approval on held token" },
      slug: "air-permit-on-held-token-deny", sev: "fail",
      dupKey: "permit-held", threshold: { ko: "보유 잔액 > 0 전부", en: "any held balance > 0" },
      state: { life: "publish", on: true } }
  ];

  // ── 패키지 (예시에서는 비움) ──
  var PACKAGES = [];

  // ── NLG + Cedar 거울: 에디터에서 여는 정책의 풀 텍스트 ──
  var DETAIL = {
    "p-slip-wide": {
      action: { ko: "스왑(Amm::Swap)", en: "Swap (Amm::Swap)" }, actionTag: "swap", actionEnt: "Amm::Swap",
      nlg: {
        ko: "지갑이 스왑할 때, 슬리피지 허용폭이 1.5%를 넘으면 경고합니다.",
        en: "When the wallet swaps, warn if the slippage tolerance exceeds 1.5%."
      },
      cedar: [
        { t: "// swap-slippage-wide-warn · manifest #fc20a91", k: "cmt" },
        { t: "forbid (", k: "kw" },
        { t: "  principal, action == Action::\"Amm::Swap\", resource" },
        { t: ") when {" },
        { t: "  context.slippageBp > 150", g: "c1" },
        { t: "};" }
      ],
      manifest: '{\n  "id": "swap-slippage-wide-warn",\n  "effect": "forbid",\n  "trigger": {\n    "where": { "action.tag": { "eq": "swap" } }\n  },\n  "enrichment": [],\n  "severity": "warn"\n}',
      params: [
        { key: "slip", role: "numeric", label: { ko: "슬리피지 임계값", en: "Slippage threshold" }, canon: "context.slippageBp", hole: "?slippageBp", recommended: "150", unit: "bp" }
      ],
      meta: { id: "swap-slippage-wide-warn", sev: "warn", reason: { ko: "슬리피지 허용폭이 넓어 샌드위치 MEV에 노출", en: "Wide slippage band → exposed to sandwich MEV" } },
      nlgParts: { verb: { ko: "경고", en: "warn" },
        lead: { ko: "지갑이 스왑할 때,", en: "When the wallet swaps," },
        base: [{ ko: "슬리피지 허용폭이 1.5%를 넘으면", en: "if the slippage tolerance exceeds 1.5%" }], or: null },
      conds: [
        { id: "c1", field: { ko: "슬리피지 허용폭", en: "Slippage tolerance" }, canon: "context.slippageBp", op: ">", val: "150", unit: "bp", role: "numeric", or: null,
          chip: "context.slippageBp > 150", recommended: ["100", "150", "300"] }
      ]
    },
    // 블록 정책 — 보유 토큰 승인 위장 (has·OR 구조, 블록으로만)
    "p-permit-held": {
      action: { ko: "토큰 승인(ERC20::Permit)", en: "Token permit (ERC20::Permit)" },
      nlg: {
        ko: "지갑이 보유 중인 토큰에 대해 permit 서명할 때, (다음 중 하나라도: spender가 미확인 컨트랙트이거나 · allowance가 무제한이면) 차단합니다.",
        en: "When permit-signing a held token, block if (any of: spender is an unknown contract, or allowance is unlimited)."
      },
      cedar: [
        { t: "// air-permit-on-held-token-deny · manifest #b1902fe", k: "cmt" },
        { t: "forbid (", k: "kw" },
        { t: "  principal, action == Action::\"ERC20::Permit\", resource" },
        { t: ") when {" },
        { t: "  context has heldBalance && context.heldBalance > 0", g: "g0" },
        { t: "  && ( !context.allowedSpenders.contains(context.spender)", g: "g1" },
        { t: "       || context.allowance == MAX_UINT )", g: "g2" },
        { t: "};" }
      ],
      manifest: '{\n  "id": "air-permit-on-held-token-deny",\n  "effect": "forbid",\n  "trigger": {\n    "where": { "action.tag": { "eq": "erc20_permit" } }\n  },\n  "enrichment": [],\n  "severity": "block"\n}',
      meta: { id: "air-permit-on-held-token-deny", sev: "fail", reason: { ko: "보유 토큰에 위장 승인 → 드레인", en: "Spoofed approval on held token → drain" } },
      nlgParts: { verb: { ko: "차단", en: "block" },
        lead: { ko: "지갑이 보유 중인 토큰에 permit 서명할 때,", en: "When permit-signing a held token," },
        base: [], or: { head: { ko: "다음 중 하나라도 해당하면:", en: "any of the following is true:" },
          items: [{ ko: "spender가 미확인 컨트랙트", en: "spender is an unknown contract" }, { ko: "allowance가 무제한", en: "allowance is unlimited" }] } }
    }
  };

  // slug 별칭 (멤버 slug → DETAIL)
  DETAIL["swap-slippage-wide-warn"] = DETAIL["p-slip-wide"];
  DETAIL["air-permit-on-held-token-deny"] = DETAIL["p-permit-held"];

  window.MP = {
    CAT: CAT, CAT_ORDER: CAT_ORDER, t: t,
    SINGLES: SINGLES, PACKAGES: PACKAGES, DETAIL: DETAIL,
    detailFor: function (row) {
      if (!row) return null;
      return DETAIL[row.id] || (row.slug && DETAIL[row.slug]) || null;
    }
  };
})();
