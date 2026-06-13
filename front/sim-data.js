/* ════════════════════════════════════════════════════════════════════
   sim-data.js v2 — Scopeball Policy × TX simulation
   Real Cedar-category scenario · provenance engine · counterfactuals ·
   dual-mode (independent / sequence+atomicity) · causal maps.
   No staking primitives (not in schema). Exposed as window.SIM.
   ════════════════════════════════════════════════════════════════════ */
(function () {
  "use strict";

  const fmt = (n, d = 2) =>
    n == null || isNaN(n) ? "—"
    : Number(n).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
  const fmtAmt = (n) => { const a = Math.abs(n); return a >= 1000 ? fmt(n, 0) : fmt(n, n % 1 === 0 ? 0 : a < 10 ? 3 : 2); };
  const usd = (n) => (n < 0 ? "-$" : "$") + fmt(Math.abs(n), n >= 1000 || n <= -1000 ? 0 : 2);
  const signed = (n) => (n > 0 ? "+" : n < 0 ? "−" : "") + fmtAmt(Math.abs(n));

  const ORACLE = { ETH: 3200, WETH: 3200, USDC: 1, ARB: 0.82 };
  const LIQ = 0.825; // Aave WETH liquidation threshold

  // 13-chain registry (caip → label) — same symbol lives on many chains,
  // so (chain, address) is the real key. Used for the per-row chain badge.
  const CHAINS = {
    "eip155:1": "Ethereum", "eip155:10": "Optimism", "eip155:56": "BNB", "eip155:130": "Unichain",
    "eip155:137": "Polygon", "eip155:480": "World Chain", "eip155:8453": "Base", "eip155:42161": "Arbitrum",
    "eip155:42220": "Celo", "eip155:43114": "Avalanche", "eip155:57073": "Ink", "eip155:7777777": "Zora", "eip155:81457": "Blast",
  };
  // balance_amount is a U256 string → BigInt, never float. Returns a Number for
  // display-formatting (safe ≤ ~1e15) plus keeps the raw string available.
  function fromRaw(raw, decimals) {
    if (raw == null) return null;
    const s = String(raw); const neg = s.startsWith("-"); const digits = (neg ? s.slice(1) : s).padStart(decimals + 1, "0");
    const intPart = digits.slice(0, digits.length - decimals) || "0";
    const fracPart = decimals ? digits.slice(digits.length - decimals) : "";
    return (neg ? -1 : 1) * Number(intPart + (fracPart ? "." + fracPart : ""));
  }

  // ── S₀ : snapshot of the active wallet ─────────────────────────────
  const state0 = {
    address: "0xA1c4f7e2b9d83a6c5e1f0d4b8a2c7e96f3d15a4b",
    short: "0xA1c4··7e29",
    walletName: "메인 지갑",
    chain: { caip: "eip155:1", name: "Ethereum" },
    oracle: ORACLE,
    // token_holdings ⋈ tokens catalog — display = holdings, meta(symbol/decimals/name)=catalog.
    // 핵심은 "임의 토큰 1행"이 절대 안 깨지게: kind 분기 + (chain,address) 키 + fallback.
    balances: [
      { id: "usdc-1", sym: "USDC", name: "USD Coin", chain: "eip155:1", kind: "erc20", decimals: 6,
        address: "0xA0b8··6EB48", raw: "12000000000", amt: 12000, price: 1, floor: null, syncedAgo: "방금", stale: false },
      { id: "weth-1", sym: "WETH", name: "Wrapped Ether", chain: "eip155:1", kind: "erc20", decimals: 18,
        address: "0xC02a··6Cc2", amt: 3.5, price: 3200, floor: null, syncedAgo: "14분 전", stale: true },
      { id: "eth-1", sym: "ETH", name: "Ethereum", chain: "eip155:1", kind: "native", decimals: 18,
        amt: 1.8, price: 3200, floor: 0.3, floorLabel: "gas reserve", syncedAgo: "방금", stale: false },
      // 멀티체인: 같은 USDC 심볼이 Base에도 → 행마다 체인 뱃지로 구분 (display-only, TX 미접촉)
      { id: "usdc-8453", sym: "USDC", name: "USD Coin", chain: "eip155:8453", kind: "erc20", decimals: 6,
        address: "0x8335··2913", amt: 4200, price: 1, syncedAgo: "방금", stale: false },
      { id: "arb-42161", sym: "ARB", name: "Arbitrum", chain: "eip155:42161", kind: "erc20", decimals: 18,
        address: "0x912C··0548", amt: 1500, price: 0.82, syncedAgo: "3분 전", stale: false },
      // 엣지: 미등록 토큰(레지스트리에 없음) + 가격 없음 → Unknown Token fallback, USD '—'
      { id: "unk-56", sym: null, name: null, chain: "eip155:56", kind: "erc20", decimals: 18, unknown: true,
        address: "0x3f9a··B71e", amt: 880000, price: null, syncedAgo: "방금", stale: false },
    ],
    // NFT / non-fungible — quantity-less, separate Collectibles section
    collectibles: [
      { id: "NFT1", name: "Pudgy Penguin #4021", collection: "Pudgy Penguins", chain: "eip155:1", kind: "erc721", floor: 12.4, floorSym: "ETH" },
      { id: "NFT2", name: "soyeon.eth", collection: "ENS Names", chain: "eip155:1", kind: "erc721", floor: null },
    ],
    lending: { venue: "Aave v3", market: "ETH-USDC", collSym: "WETH", coll: 2.0, debtSym: "USDC", debt: 1000, liq: LIQ },
    lp: { venue: "Uniswap v3", pair: "ETH/USDC", value: 6400 },
    // [권장] approvals — "이 지갑이 외부 컨트랙트에 허용한 토큰 사용 권한". is_unlimited = 최우선 빨강.
    approvals: [
      { id: "AP1", token: "USDC", type: "erc20", spender: "Uniswap v3 Router", spenderShort: "0x68b3··fD45", chain: "eip155:1", amount: null, isUnlimited: true },
      { id: "AP2", token: "WETH", type: "permit2", spender: "Aave v3 Pool", spenderShort: "0xae7a··0C2b", chain: "eip155:1", amount: 5.0, isUnlimited: false, permitExpiry: "06-14", permitExpSoon: false },
      { id: "AP3", token: "USDC", type: "permit2", spender: "Permit2", spenderShort: "0x0000··0022", chain: "eip155:1", amount: 2000, isUnlimited: false, permitExpiry: "06-02", permitExpSoon: true },
      { id: "AP4", token: "Pudgy Penguins", type: "setApprovalForAll", spender: "OpenSea Seaport", spenderShort: "0x0000··00C0", chain: "eip155:1", amount: null, isUnlimited: true, nft: true },
    ],
  };
  const hf0 = (state0.lending.coll * ORACLE.WETH * LIQ) / state0.lending.debt;
  const ltv0 = state0.lending.debt / (state0.lending.coll * ORACLE.WETH);

  // ── Actions A1–A4 (decoded TXs) — Cedar categories only ────────────
  const actions = [
    {
      id: "A1", n: 1, cat: "amm", verb: "스왑", icon: "swap", angle: -34,
      chip: "스왑 USDC→WETH", full: "AMM swap · USDC → WETH · Uniswap v3",
      flowIn: { sym: "USDC", amt: 5000 }, flowOut: { sym: "WETH", amt: 1.5 },
      target: "Uniswap v3", targetShort: "0x68b3··fD45", slippagePct: 1.2,
      effects: [{ sym: "USDC", d: -5000 }, { sym: "WETH", d: +1.5, out: true }],
    },
    {
      id: "A2", n: 2, cat: "lending", verb: "공급", icon: "supply", angle: 30,
      chip: "공급 WETH→Aave", full: "Lending supply · 2.0 WETH → Aave v3",
      flowIn: { sym: "WETH", amt: 2.0 }, target: "Aave v3", targetShort: "0xae7a··0C2b",
      effects: [{ sym: "WETH", d: -2.0 }], lend: { coll: +2.0 },
    },
    {
      id: "A3", n: 3, cat: "lending", verb: "차입", icon: "borrow", angle: 150,
      chip: "차입 USDC", full: "Lending borrow · 6,000 USDC ← Aave v3",
      flowOut: { sym: "USDC", amt: 6000 }, target: "Aave v3", targetShort: "0xae7a··0C2b",
      effects: [{ sym: "USDC", d: +6000 }], lend: { debt: +6000 },
    },
    {
      id: "A4", n: 4, cat: "token", verb: "전송", icon: "transfer", angle: 214,
      chip: "전송 →외부 EOA", full: "Token transfer · 4,000 USDC → 외부 EOA",
      flowOut: { sym: "USDC", amt: 4000, external: true }, target: "외부 EOA", targetShort: "0x9c4f··A7e1", recipientUnknown: true,
      effects: [{ sym: "USDC", d: -4000 }],
    },
  ];

  // ── Policy library (Cedar names), TX-scoped subset ─────────────────
  const policies = [
    { id: "P1", rule: "allowlist.recipient", cat: "token", kind: "constraint", preset: "compliance",
      name: "수신자 화이트리스트", touches: ["A4"], desc: "전송 수신자가 승인 목록에 없으면 차단" },
    { id: "P2", rule: "sanctions.ofac", cat: "token", kind: "constraint", preset: "compliance",
      name: "제재 주소 차단", touches: ["A1", "A4"], desc: "상대 주소가 OFAC 제재 목록이면 차단" },
    { id: "P3", rule: "limit.daily_transfer", cat: "token", kind: "constraint", preset: "risk",
      name: "일일 전송 한도", touches: ["A4"], desc: "전송액 ≤ $5,000", threshold: 5000 },
    { id: "P4", rule: "guard.slippage", cat: "amm", kind: "constraint", preset: "risk",
      name: "슬리피지 가드", touches: ["A1"], desc: "스왑 슬리피지 > 1.0% 면 검토", threshold: 1.0 },
    { id: "P5", rule: "swap.min_output", cat: "amm", kind: "constraint", preset: "risk",
      name: "최소 수령량", touches: ["A1"], desc: "스왑 수령 ≥ 1.45 WETH", threshold: 1.45 },
    { id: "P6", rule: "fee.protocol_cut", cat: "amm", kind: "transform", preset: "fees",
      name: "프로토콜 수수료", touches: ["A1"], desc: "스왑 수령에서 0.3% 차감", rate: 0.003 },
    { id: "P7", rule: "limit.health_factor", cat: "lending", kind: "constraint", preset: "risk",
      name: "헬스팩터 하한", touches: ["A3"], desc: "차입 후 HF ≥ 1.50", scalar: "hf", threshold: 1.5 },
    { id: "P8", rule: "lending.max_ltv", cat: "lending", kind: "constraint", preset: "risk",
      name: "최대 LTV", touches: ["A3"], desc: "차입 후 LTV ≤ 80%", scalar: "ltv", threshold: 0.8 },
    // related / hidden — search-only
    { id: "P9", rule: "approve.no_unlimited", cat: "token", kind: "constraint", preset: null,
      name: "무제한 승인 금지", touches: ["A1"], desc: "unlimited approve 차단", hidden: true },
    { id: "P10", rule: "amm.lp_exposure_cap", cat: "amm", kind: "constraint", preset: null,
      name: "LP 노출 한도", touches: ["A1"], desc: "LP 노출 ≤ 35%", scalar: "lp", hidden: true },
    { id: "P11", rule: "collateral.no_disable_if_borrow", cat: "lending", kind: "constraint", preset: null,
      name: "담보 비활성 금지", touches: ["A3"], desc: "부채 있으면 담보 해제 불가", hidden: true },
    { id: "P12", rule: "multicall.atomic_only", cat: "multicall", kind: "constraint", preset: null,
      name: "원자적 멀티콜만", touches: [], desc: "비원자 멀티콜 차단", hidden: true },
    { id: "P13", rule: "floor.min_transfer", cat: "token", kind: "constraint", preset: null,
      name: "최소 전송액", touches: ["A4"], desc: "전송액 ≥ $6,000", threshold: 6000, hidden: true },
  ];

  const presets = [
    { id: "compliance", name: "컴플라이언스 셋", en: "Compliance set", members: ["P1", "P2"] },
    { id: "risk",       name: "리스크 한도",     en: "Risk limits",    members: ["P3", "P4", "P5", "P7", "P8"] },
    { id: "fees",       name: "수수료·라우팅",   en: "Fees & routing", members: ["P6"] },
  ];

  const conflictRules = [
    { id: "C1", type: "중복 적용", typeEn: "redundant", grade: "warn", pair: ["P7", "P8"], hop: "A3",
      why: "헬스팩터 하한과 최대 LTV가 같은 차입 홉(A3)에서 같은 리스크를 중복 평가 — 하나는 죽은 규칙이 될 수 있음." },
    { id: "C2", type: "충족 불가", typeEn: "unsatisfiable", grade: "error", pair: ["P3", "P13"], hop: "A4",
      why: null, hiddenPair: true },
  ];

  const byId = (id) => policies.find((p) => p.id === id);
  const presetOf = (id) => presets.find((p) => p.id === id);
  const catLabel = { amm: "amm", lending: "lending", token: "token", airdrop: "airdrop", perp: "perp", launchpad: "launchpad", multicall: "multicall" };

  // ── ENGINE ─────────────────────────────────────────────────────────
  // evaluate(activeIds, txIds) — always sequential in user-defined TX order.
  // Each TX applies to the state the previous TX produced. A blocked TX is
  // marked blocked; the rest proceed (no bundle atomicity, no mode split).
  function evaluate(activeIds, txIds = null) {
    const active = new Set(activeIds);
    const on = (id) => active.has(id);
    // respect the user's order (drag-reorder = execution order)
    const order = txIds || actions.map((a) => a.id);
    const acts = order.map((id) => actions.find((a) => a.id === id)).filter(Boolean);

    // transform-adjusted output per action
    const adj = (a) => {
      let cuts = [];
      const eff = a.effects.map((e) => ({ ...e }));
      if (a.cat === "amm" && on("P6")) {
        const o = eff.find((e) => e.out);
        if (o) { const cut = +(o.d * 0.003).toFixed(4); o.d = +(o.d - cut).toFixed(4); o.cut = { rule: "fee.protocol_cut", id: "P6", d: -cut };
          cuts.push(o.cut); }
      }
      return { eff, cuts };
    };

    // constraint evaluation of one action against a lending context
    const evalAction = (a, lendCtx) => {
      const applied = [];
      // post-action lending (for scalar policies)
      const postColl = lendCtx.coll + (a.lend?.coll || 0);
      const postDebt = lendCtx.debt + (a.lend?.debt || 0);
      const postHF = postDebt > 0 ? (postColl * ORACLE.WETH * LIQ) / postDebt : 99;
      const postLTV = postColl > 0 ? postDebt / (postColl * ORACLE.WETH) : 0;

      policies.forEach((p) => {
        if (!on(p.id) || !p.touches.includes(a.id)) return;
        if (p.kind === "transform") { applied.push({ id: p.id, kind: "transform", outcome: "transform", reason: "−0.3% 수수료" }); return; }
        let outcome = "permit", reason = "조건 충족", val = null;
        switch (p.rule) {
          case "allowlist.recipient":
            if (a.recipientUnknown) { outcome = "forbid"; reason = "수신자가 화이트리스트에 없음"; }
            else reason = "승인된 수신자"; break;
          case "sanctions.ofac": reason = "제재 목록 해당 없음"; break;
          case "limit.daily_transfer": {
            const v = a.flowOut ? a.flowOut.amt * (a.flowOut.sym === "USDC" ? 1 : ORACLE[a.flowOut.sym]) : 0;
            if (v > p.threshold) { outcome = "forbid"; reason = `전송액 $${fmt(v,0)} > 한도 $${fmt(p.threshold,0)}`; }
            else reason = `$${fmt(v,0)} ≤ $${fmt(p.threshold,0)}`; break;
          }
          case "guard.slippage":
            if (a.slippagePct > p.threshold) { outcome = "warn"; reason = `슬리피지 ${a.slippagePct}% > ${p.threshold}%`; }
            else reason = `슬리피지 ${a.slippagePct}% ≤ ${p.threshold}%`; break;
          case "swap.min_output": {
            const o = a.flowOut ? a.flowOut.amt : 0;
            if (o < p.threshold) { outcome = "forbid"; reason = `수령 ${o} < 최소 ${p.threshold}`; }
            else reason = `수령 ${fmtAmt(o)} ≥ ${p.threshold} WETH`; break;
          }
          case "floor.min_transfer": {
            const v = a.flowOut ? a.flowOut.amt * (a.flowOut.sym === "USDC" ? 1 : ORACLE[a.flowOut.sym]) : 0;
            if (v < p.threshold) { outcome = "forbid"; reason = `전송액 $${fmt(v,0)} < 최소 $${fmt(p.threshold,0)}`; }
            else reason = `$${fmt(v,0)} ≥ $${fmt(p.threshold,0)}`; break;
          }
          case "limit.health_factor":
            val = postHF;
            if (postHF < p.threshold) { outcome = "forbid"; reason = `차입 후 HF ${postHF.toFixed(2)} < ${p.threshold.toFixed(2)}`; }
            else if (postHF < p.threshold * 1.12) { outcome = "warn"; reason = `차입 후 HF ${postHF.toFixed(2)} — 하한 근접`; }
            else reason = `차입 후 HF ${postHF.toFixed(2)} ≥ ${p.threshold.toFixed(2)}`; break;
          case "lending.max_ltv":
            val = postLTV;
            if (postLTV > p.threshold) { outcome = "forbid"; reason = `LTV ${(postLTV*100).toFixed(0)}% > ${(p.threshold*100).toFixed(0)}%`; }
            else reason = `LTV ${(postLTV*100).toFixed(0)}% ≤ ${(p.threshold*100).toFixed(0)}%`; break;
          default: reason = "조건 충족";
        }
        applied.push({ id: p.id, kind: "constraint", outcome, reason, val });
      });
      const forbid = applied.find((x) => x.outcome === "forbid");
      const warn = applied.find((x) => x.outcome === "warn");
      const verdict = forbid ? "forbid" : warn ? "warn" : "permit";
      return { applied, verdict, firedBy: forbid ? forbid.id : warn ? warn.id : null,
        firedReason: forbid ? forbid.reason : warn ? warn.reason : null, postHF, postLTV };
    };

    // ── evaluate sequentially in user order ──────────────────────────
    const S0lend = { coll: state0.lending.coll, debt: state0.lending.debt };
    const lendCtx = { ...S0lend };
    const results = acts.map((a, i) => {
      const n = i + 1;                       // sequence position = order index
      const { eff, cuts } = adj(a);
      const ev = evalAction(a, lendCtx);     // evaluated against accumulated state
      const executed = ev.verdict !== "forbid";
      if (executed) { lendCtx.coll += a.lend?.coll || 0; lendCtx.debt += a.lend?.debt || 0; }
      return { ...a, n, eff, cuts, ...ev, executed, inStep: true };
    });
    const bundleRejected = false;

    // ── fold balances with provenance (keyed by token id, not symbol) ─
    const bal = {}; const prov = {};
    state0.balances.forEach((b) => { bal[b.id] = b.amt; prov[b.id] = { sources: [], ghosts: [] }; });
    // TX effects use a symbol; they target the active-chain (eip155:1) token of that symbol
    const symToId = {};
    state0.balances.forEach((b) => { if (b.chain === "eip155:1" && b.sym && symToId[b.sym] == null) symToId[b.sym] = b.id; });

    let lend = { coll: state0.lending.coll, debt: state0.lending.debt };
    const lendProv = { coll: [], debt: [] };

    results.forEach((r) => {
      r.eff.forEach((e) => {
        const id = symToId[e.sym]; if (!id) return;
        if (r.executed) {
          bal[id] += e.d;
          prov[id].sources.push({ aid: r.id, cat: r.cat, verb: r.verb, d: e.d, cut: e.cut || null });
        } else {
          prov[id].ghosts.push({ aid: r.id, cat: r.cat, verb: r.verb, d: e.d,
            blockedBy: r.firedBy, reason: r.firedReason });
        }
      });
      if (r.lend) {
        if (r.executed) {
          if (r.lend.coll) { lend.coll += r.lend.coll; lendProv.coll.push({ aid: r.id, d: r.lend.coll }); }
          if (r.lend.debt) { lend.debt += r.lend.debt; lendProv.debt.push({ aid: r.id, d: r.lend.debt }); }
        } else {
          if (r.lend.coll) lendProv.coll.push({ aid: r.id, d: r.lend.coll, ghost: true, blockedBy: r.firedBy });
          if (r.lend.debt) lendProv.debt.push({ aid: r.id, d: r.lend.debt, ghost: true, blockedBy: r.firedBy });
        }
      }
    });

    const balances = state0.balances.map((b) => {
      const after = bal[b.id];
      const breach = b.floor != null && after < b.floor;
      const price = b.price != null ? b.price : null;
      return { id: b.id, sym: b.sym, name: b.name, chain: b.chain, chainName: CHAINS[b.chain] || b.chain,
        kind: b.kind, decimals: b.decimals, address: b.address, unknown: !!b.unknown,
        before: b.amt, after, delta: after - b.amt, floor: b.floor, floorLabel: b.floorLabel,
        price, usd: price != null ? after * price : null,
        breach, near: b.floor != null && !breach && after < b.floor * 1.6,
        stale: b.stale, syncedAgo: b.syncedAgo,
        sources: prov[b.id].sources, ghosts: prov[b.id].ghosts };
    }).sort((a, b) => (b.usd != null ? b.usd : -1) - (a.usd != null ? a.usd : -1)); // USD 내림차순, 가격없음 마지막

    // ── derived metrics — now PROPERTIES of the lending position ─────
    // Always computed (intrinsic to the Aave position). The gating policy,
    // when active, only adds a reference badge + feeds the verdict.
    const hfAfter = lend.debt > 0 ? (lend.coll * ORACLE.WETH * LIQ) / lend.debt : 99;
    const ltvAfter = lend.coll > 0 ? lend.debt / (lend.coll * ORACLE.WETH) : 0;
    const drivers = results.filter((r) => r.lend).map((r) => r.id);
    const scalars = [
      { key: "hf", label: "Health factor", policy: "P7", rule: "limit.health_factor", policyActive: on("P7"),
        before: hf0, after: hfAfter, floor: 1.5, max: 4, fmtv: (v) => v.toFixed(2), isCap: false,
        info: "담보 대비 부채 안전도. 1.0 아래로 가면 청산.",
        breach: hfAfter < 1.5, near: hfAfter >= 1.5 && hfAfter < 1.5 * 1.12, drivers },
      { key: "ltv", label: "Loan-to-value", policy: "P8", rule: "lending.max_ltv", policyActive: on("P8"),
        before: ltv0, after: ltvAfter, floor: 0.8, max: 1, isCap: true, fmtv: (v) => (v * 100).toFixed(0) + "%",
        info: "담보 가치 대비 빌린 비율. 상한을 넘으면 차입 차단.",
        breach: ltvAfter > 0.8, near: ltvAfter <= 0.8 && ltvAfter > 0.8 * 0.88, drivers },
    ];

    const sc = (k) => scalars.find((s) => s.key === k);
    const positions = {
      lending: { venue: state0.lending.venue, market: state0.lending.market,
        collSym: state0.lending.collSym, debtSym: state0.lending.debtSym,
        coll: state0.lending.coll, collAfter: lend.coll, collDelta: lend.coll - state0.lending.coll, collProv: lendProv.coll,
        debt: state0.lending.debt, debtAfter: lend.debt, debtDelta: lend.debt - state0.lending.debt, debtProv: lendProv.debt,
        collUsd: lend.coll * ORACLE.WETH, debtUsd: lend.debt, hf: sc("hf"), ltv: sc("ltv") },
      lp: { ...state0.lp, scoped: on("P10") },
    };

    // ── conflicts ────────────────────────────────────────────────────
    const conflicts = conflictRules.filter((c) => c.pair.every((id) => active.has(id)))
      .map((c) => ({ ...c, why: c.why || `${byId(c.pair[0]).desc} / ${byId(c.pair[1]).desc} — 동시에 참일 수 없음.` }));

    // ── verdict tallies ──────────────────────────────────────────────
    const blocked = results.filter((r) => r.verdict === "forbid");
    const warned = results.filter((r) => r.verdict === "warn");
    const allBlocked = activeIds.length > 0 && results.length > 0 && blocked.length >= results.length;
    const globalVerdict = activeIds.length === 0 ? "raw"
      : allBlocked ? "all-blocked"
      : blocked.length ? "partial"
      : warned.length ? "warn" : "pass";

    const appliedPolicyIds = policies.filter((p) => active.has(p.id) &&
      results.some((r) => r.applied.some((x) => x.id === p.id))).map((p) => p.id);

    return { results, balances, positions, scalars, conflicts, blocked, warned, globalVerdict,
      bundleRejected, appliedPolicyIds, hf0, ltv0 };
  }

  window.SIM = {
    state0, actions, policies, presets, conflictRules, ORACLE, CHAINS, catLabel,
    byId, presetOf, evaluate, fmt, fmtAmt, usd, signed, fromRaw, hf0, ltv0,
    defaultActive: ["P1", "P2", "P3", "P4", "P5", "P7", "P8"],
    defaultTx: ["A1", "A2", "A3", "A4"],
  };
})();
