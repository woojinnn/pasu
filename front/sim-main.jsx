/* sim-main.jsx v2 — Scopeball simulation root.
   Global nav + sim shell · causal-spine resolution · independent/sequence modes ·
   floating-panel manager. */
const { FloatingPanel, TXPanel, PolicyPanel, BalancePanel, VerdictPanel } = window.SimPanels;

const PANEL_DEFS = {
  tx:      { title: "TX 입력", sub: "decode · form · sample", icon: "form",   width: 332 },
  policy:  { title: "정책",     sub: "preset · 3-tier",        icon: "shield", width: 356 },
  balance: { title: "잔고 · 포지션", sub: "S → S′ · 인과",      icon: "wallet", width: 348 },
  verdict: { title: "판정 · 위반", sub: "verdict",             icon: "gavel",  width: 332 },
};
const DEFAULT_POS = { tx: { x: 86, y: 86 }, policy: { x: 1024, y: 14 }, balance: { x: 86, y: 80 }, verdict: { x: 1024, y: 470 } };

function gv(globalVerdict, sim) {
  switch (globalVerdict) {
    case "pass": return { cls: "pass", ic: "check", txt: "전체 통과" };
    case "warn": return { cls: "warn", ic: "warn", txt: "검토 필요" };
    case "partial": return { cls: "partial", ic: "alert", txt: "부분 차단", where: sim.blocked[0]?.id };
    case "all-blocked": return { cls: "all-blocked", ic: "x", txt: sim.bundleRejected ? "번들 거부" : "전부 차단" };
    default: return { cls: "raw", ic: "dot", txt: "정책 없음" };
  }
}

/* causal-spine resolver — seed → unified highlight set */
function resolveCausal(sim, seed) {
  const A = new Set(), P = new Set(), S = new Set(), SC = new Set();
  const addAction = (aid) => {
    const r = sim.results.find((x) => x.id === aid); if (!r || A.has(aid)) return;
    A.add(aid); r.applied.forEach((x) => P.add(x.id)); r.eff.forEach((e) => S.add(e.sym));
    sim.scalars.forEach((s) => { if (s.drivers.includes(aid)) SC.add(s.key); });
  };
  if (seed.kind === "action") addAction(seed.id);
  else if (seed.kind === "policy") {
    P.add(seed.id);
    sim.results.forEach((r) => { if (r.applied.some((x) => x.id === seed.id)) addAction(r.id); });
    const pol = SIM.byId(seed.id); if (pol) { if (pol.scalar) SC.add(pol.scalar); pol.touches.forEach(addAction); }
  } else if (seed.kind === "sym") {
    S.add(seed.id);
    sim.results.forEach((r) => { if (r.eff.some((e) => e.sym === seed.id)) addAction(r.id); });
  } else if (seed.kind === "scalar") {
    SC.add(seed.id);
    const sc = sim.scalars.find((s) => s.key === seed.id); if (sc) { P.add(sc.policy); sc.drivers.forEach(addAction); }
  }
  return { actions: A, policies: P, syms: S, scalars: SC, active: true };
}

const NAV = [
  { id: "home", icon: "home", label: "Home" },
  { id: "editor", icon: "blocks", label: "Editor" },
  { id: "sim", icon: "bolt", label: "Simulation", active: true },
  { id: "monitor", icon: "monitor", label: "Monitoring" },
  { id: "audit", icon: "audit", label: "Audit" },
  { id: "settings", icon: "settings", label: "Settings" },
];

function GlobalNav() {
  return (
    <nav className="gnav">
      <div className="gnav-logo"><div className="mk"><Icon name="bolt" size={16} /></div><span className="word">scopeball</span></div>
      <div className="gnav-ws">
        <div className="ws-av">A</div>
        <div className="ws-label">Acme<span className="sub">4 wallets · 14 policies</span></div>
        <span className="ws-caret"><Icon name="chevron" size={13} /></span>
      </div>
      <div className="gnav-wallet">
        <span className="dot"></span>
        <div className="wm"><span className="nm">메인 지갑</span><span className="ad">0xA1c4··7e29</span></div>
      </div>
      <div className="gnav-div"></div>
      <div className="gnav-group">
        {NAV.map((n) => (
          <a key={n.id} className={"gnav-item" + (n.active ? " active" : "")}>
            <span className="icon"><Icon name={n.icon} size={18} /></span><span className="label">{n.label}</span>
          </a>
        ))}
      </div>
      <div className="gnav-bottom">
        <div className="gnav-user"><div className="av">SK</div><div className="meta"><div className="nm">Soyeon K.</div><div className="em">soyeon@acme.io</div></div></div>
      </div>
    </nav>
  );
}

function App() {
  const [active, setActive] = useState(SIM.defaultActive.slice());
  const [mode, setMode] = useState("independent");
  const [atomic, setAtomic] = useState("bundle");
  const [step, setStep] = useState(4);
  const [selectedTx, setSelectedTx] = useState(null);
  const [seed, setSeed] = useState(null);
  const [recalcKey, setRecalcKey] = useState(0);
  const [toast, setToast] = useState(null);

  const [openP, setOpenP] = useState({ policy: true, balance: true, verdict: false, tx: false });
  const [pos, setPos] = useState({ ...DEFAULT_POS });
  const [pinned, setPinned] = useState({});
  const [translucent, setTranslucent] = useState({});
  const [zTop, setZTop] = useState(30);
  const [zMap, setZMap] = useState({ policy: 22, balance: 21, verdict: 20, tx: 19 });

  const sim = useMemo(() => SIM.evaluate(active, mode, atomic, step), [active, mode, atomic, step]);
  const hl = useMemo(() => (seed ? resolveCausal(sim, seed) : null), [seed, sim]);
  const onSeed = useCallback((s) => setSeed(s), []);

  const flash = (msg, warn) => { setToast({ msg, warn }); setTimeout(() => setToast(null), 2200); };
  const bump = () => setRecalcKey((k) => k + 1);
  const toggle = (pid) => { setActive((a) => (a.includes(pid) ? a.filter((x) => x !== pid) : [...a, pid])); bump(); };
  const togglePreset = (preId) => { const pre = SIM.presetOf(preId); setActive((a) => { const allOn = pre.members.every((m) => a.includes(m)); return allOn ? a.filter((x) => !pre.members.includes(x)) : [...new Set([...a, ...pre.members])]; }); bump(); };

  const focusPanel = (id) => { setZMap((m) => ({ ...m, [id]: zTop + 1 })); setZTop((z) => z + 1); };
  const openPanel = (id) => { setOpenP((o) => ({ ...o, [id]: !o[id] })); if (!openP[id]) focusPanel(id); };
  const ensureOpen = (id) => { setOpenP((o) => ({ ...o, [id]: true })); focusPanel(id); };
  const closePanel = (id) => setOpenP((o) => ({ ...o, [id]: false }));
  const move = (id, p) => setPos((s) => ({ ...s, [id]: p }));
  const pinP = (id) => setPinned((s) => ({ ...s, [id]: !s[id] }));
  const transP = (id) => setTranslucent((s) => ({ ...s, [id]: !s[id] }));

  const onBreachClick = (aid) => { ensureOpen("verdict"); setSelectedTx(aid); setSeed({ kind: "action", id: aid }); setTimeout(() => setSeed(null), 1600); };
  const onSelectCanvas = (aid) => { setSelectedTx(aid); setSeed({ kind: "action", id: aid }); setTimeout(() => setSeed(null), 1400); };
  const onJumpScalar = (pid) => { ensureOpen("policy"); setSeed({ kind: "policy", id: pid }); setTimeout(() => setSeed(null), 1600); };

  const vinfo = gv(sim.globalVerdict, sim);
  const conflictN = sim.conflicts.length;
  const dockItems = [
    { id: "tx", icon: "form", label: "TX 입력", k: "1" },
    { id: "policy", icon: "shield", label: "정책", k: "2", badge: active.length, badgeWarn: conflictN > 0 },
    { id: "balance", icon: "wallet", label: "잔고 · 포지션", k: "3" },
    { id: "verdict", icon: "gavel", label: "판정 · 위반", k: "4", badge: sim.blocked.length || null, badgeFail: true },
  ];

  return (
    <React.Fragment>
      <GlobalNav />
      <div className="sim-root">
        <div className="sim-top">
          <div className="top-brand"><div className="mk"><Icon name="bolt" size={15} /></div><div className="wm">Simulation<span className="sub">policy × tx</span></div></div>
          <div className="tx-rail">
            <span className="rk">{mode === "sequence" ? "SEQ" : "TX"}</span>
            {sim.results.map((r) => (
              <button key={r.id} className={"tx-chip" + (selectedTx === r.id ? " sel" : "")} onClick={() => setSelectedTx(selectedTx === r.id ? null : r.id)}
                onMouseEnter={() => setSeed({ kind: "action", id: r.id })} onMouseLeave={() => setSeed(null)}>
                <span className="tc-ix">{r.id}</span>{r.chip}
                <span className="tc-tag">{SIM.catLabel[r.cat]}</span>
                <span className={"tc-dot " + (r.verdict === "rejected" ? "forbid" : r.verdict)}></span>
              </button>
            ))}
            <button className="tx-add" title="TX 추가" onClick={() => openPanel("tx")}><Icon name="plus" size={15} /></button>
          </div>
          <div className="top-spacer"></div>
          <div className="mode-seg">
            <button className={mode === "independent" ? "on" : ""} onClick={() => setMode("independent")}><Icon name="single" size={13} />독립</button>
            <button className={mode === "sequence" ? "on" : ""} onClick={() => setMode("sequence")}><Icon name="layers" size={13} />시퀀스</button>
          </div>
          <div className={"gverdict " + vinfo.cls}><span className="gv-ic"><Icon name={vinfo.ic} size={11} /></span>{vinfo.txt}{vinfo.where && <span className="gv-where">{vinfo.where}</span>}</div>
          <span className="gcount"><span className="gc-n">{active.length}</span> 활성</span>
          {conflictN > 0 && <button className="gcount conflict" onClick={() => ensureOpen("policy")}><Icon name="conflict" size={13} /><span className="gc-n">{conflictN}</span> 충돌</button>}
        </div>

        <div className="sim-body">
          <div className="sim-dock">
            {dockItems.map((d) => (
              <button key={d.id} className={"dock-btn" + (openP[d.id] ? " on" : "")} onClick={() => openPanel(d.id)}>
                <Icon name={d.icon} size={19} />
                {d.badge ? <span className={"dk-badge" + (d.badgeFail ? "" : d.badgeWarn ? " warn" : "")}>{d.badge}</span> : null}
                <span className="dock-tip">{d.label}<span className="dt-k">{d.k}</span></span>
              </button>
            ))}
            <div className="dock-div"></div>
            <button className="dock-btn" onClick={() => flash("S₀ 스냅샷 로드됨 · " + SIM.state0.short)}><Icon name="reset" size={19} /><span className="dock-tip">S₀ 구성<span className="dt-k">snapshot</span></span></button>
            <div className="dock-spacer"></div>
          </div>

          <SimCanvas sim={sim} mode={mode} selectedTx={selectedTx} hl={hl} onSelectTx={setSelectedTx} onBreachClick={onBreachClick} onSeed={onSeed} recalcKey={recalcKey} />

          {openP.tx && <FloatingPanel id="tx" {...PANEL_DEFS.tx} pos={pos.tx} z={zMap.tx} pinned={pinned.tx} translucent={translucent.tx} onMove={move} onFocus={focusPanel} onClose={closePanel} onPin={pinP} onTranslucent={transP}><TXPanel actions={SIM.actions} selectedTx={selectedTx} onSelectTx={setSelectedTx} /></FloatingPanel>}
          {openP.policy && <FloatingPanel id="policy" {...PANEL_DEFS.policy} pos={pos.policy} z={zMap.policy} pinned={pinned.policy} translucent={translucent.policy} onMove={move} onFocus={focusPanel} onClose={closePanel} onPin={pinP} onTranslucent={transP}><PolicyPanel sim={sim} active={active} onToggle={toggle} onTogglePreset={togglePreset} onSeed={onSeed} hl={hl} /></FloatingPanel>}
          {openP.balance && <FloatingPanel id="balance" {...PANEL_DEFS.balance} pos={pos.balance} z={zMap.balance} pinned={pinned.balance} translucent={translucent.balance} onMove={move} onFocus={focusPanel} onClose={closePanel} onPin={pinP} onTranslucent={transP}><BalancePanel sim={sim} onSeed={onSeed} hl={hl} onJumpScalar={onJumpScalar} /></FloatingPanel>}
          {openP.verdict && <FloatingPanel id="verdict" {...PANEL_DEFS.verdict} pos={pos.verdict} z={zMap.verdict} pinned={pinned.verdict} translucent={translucent.verdict} onMove={move} onFocus={focusPanel} onClose={closePanel} onPin={pinP} onTranslucent={transP}><VerdictPanel sim={sim} onSelectCanvas={onSelectCanvas} onSeed={onSeed} hl={hl} /></FloatingPanel>}

          {mode === "sequence" && (
            <div className="seq-bar">
              <button className="seq-play"><Icon name="play" size={13} /></button>
              <div className="seq-steps">{sim.results.map((r, i) => (
                <div className="seq-step" key={r.id}>
                  {i > 0 && <div className={"seq-link" + (i <= step - 1 ? " done" : "")}></div>}
                  <div className={"seq-dot" + (r.n === step ? " cur" : r.n < step ? ((r.verdict === "forbid" || r.verdict === "rejected") ? " blocked" : " done") : "")} onClick={() => setStep(r.n)}>{r.n}</div>
                </div>
              ))}</div>
              <div className="seq-atomic">
                <button className={atomic === "bundle" ? "on" : ""} onClick={() => setAtomic("bundle")}>번들</button>
                <button className={atomic === "sequential" ? "on" : ""} onClick={() => setAtomic("sequential")}>순차</button>
              </div>
            </div>
          )}

          {toast && <div className={"sim-toast" + (toast.warn ? " warn" : "")}><Icon name={toast.warn ? "warn" : "check"} size={15} />{toast.msg}</div>}
        </div>
      </div>
    </React.Fragment>
  );
}

ReactDOM.createRoot(document.getElementById("app")).render(<App />);
