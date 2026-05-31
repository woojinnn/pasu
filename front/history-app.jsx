// history-app.jsx — Scopeball 히스토리(History) screen. Rev.1.
// Job unchanged from Audit: inspect *why* an individual verdict was reached. NO time-series.
// Rev.1: rename, origin/policy dropdowns removed (search covers them), scroll containment
// (fixed screen → only the verdict list scrolls), severity hierarchy (FAIL/WARN emphasised,
// PASS dimmed + consecutive PASS collapsed), why-panel quasi-hero (top emphasised row auto-open),
// grouping/sort toggle (time / verdict / origin / rule), evidence cues (seq # + second-precise ts).

const { useState: useSH, useMemo: useMH, useEffect: useEH, useRef: useRH } = React;

/* ── icons ── */
const I = {
  download: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M12 3v12m0 0 4-4m-4 4-4-4"/><path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/></svg>,
  caretD: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="m6 9 6 6 6-6"/></svg>,
  caretR: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="m9 6 6 6-6 6"/></svg>,
  search: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="11" cy="11" r="7"/><path d="m20 20-3.2-3.2"/></svg>,
  x: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M6 6l12 12M18 6 6 18"/></svg>,
  check: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M20 6 9 17l-5-5"/></svg>,
  clock: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>,
  pass: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M20 6 9 17l-5-5"/></svg>,
  warn: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M10.3 3.8 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.8a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4M12 17h.01"/></svg>,
  fail: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="12" cy="12" r="9"/><path d="M5.6 5.6 18.4 18.4"/></svg>,
  reset: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/></svg>,
  quote: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M9 7H6a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h2v2a2 2 0 0 1-2 2"/><path d="M19 7h-3a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h2v2a2 2 0 0 1-2 2"/></svg>,
  info: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="12" cy="12" r="9"/><path d="M12 11v5M12 8h.01"/></svg>,
  lock: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/></svg>,
  sleep: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M5 13a7 7 0 0 0 13 2 6 6 0 0 1-8-8 7 7 0 0 0-5 6Z"/></svg>,
  layers: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="m12 3 9 5-9 5-9-5 9-5Z"/><path d="m3 13 9 5 9-5"/></svg>,
};

const VMETA = {
  pass: { Ic: I.pass, ko: 'PASS', en: 'Pass' },
  warn: { Ic: I.warn, ko: 'WARN', en: 'Warn' },
  fail: { Ic: I.fail, ko: 'FAIL', en: 'Fail' },
};
const RANGES = [
  { id: '1h', ms: 60 * 60 * 1000, label: '1h' },
  { id: '6h', ms: 6 * 60 * 60 * 1000, label: '6h' },
  { id: '24h', ms: 24 * 60 * 60 * 1000, label: '24h' },
  { id: '7d', ms: 7 * 24 * 60 * 60 * 1000, label: { ko: '7일', en: '7d' } },
];
const GROUPS = [
  { id: 'time', ko: '시간순', en: 'Time' },
  { id: 'verdict', ko: 'verdict별', en: 'By verdict' },
  { id: 'origin', ko: 'dApp별', en: 'By dApp' },
  { id: 'rule', ko: 'rule별', en: 'By rule' },
];

function L({ ko, en }) { return (<>{ko != null && <span lang="ko">{ko}</span>}{en != null && <span lang="en">{en}</span>}</>); }
function shortAddr(a) { return a.length > 14 ? `${a.slice(0, 6)}···${a.slice(-4)}` : a; }
function ago(ts) {
  const d = Date.now() - ts, m = Math.floor(d / 60000), h = Math.floor(d / 3600000), day = Math.floor(d / 86400000);
  if (m < 1) return { ko: '방금', en: 'just now' };
  if (m < 60) return { ko: `${m}분 전`, en: `${m}m ago` };
  if (h < 24) return { ko: `${h}시간 전`, en: `${h}h ago` };
  return { ko: `${day}일 전`, en: `${day}d ago` };
}
// second-precise stamp — evidence record, not a minute log
function stamp(ts) {
  const dt = new Date(ts), p = (n) => String(n).padStart(2, '0');
  return { date: `${p(dt.getMonth() + 1)}/${p(dt.getDate())}`, time: `${p(dt.getHours())}:${p(dt.getMinutes())}:${p(dt.getSeconds())}` };
}

/* ── nav rail (Cloudy Pond markup, 히스토리 active) ── */
function NavRail() {
  return (
    <nav className="nav-rail" aria-label="Global">
      <div className="nav-logo"><span className="mark">LOGO</span><span className="word">Scopeball</span></div>
      <div className="nav-cta"><div className="main"><svg className="plus" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M12 5v14M5 12h14"/></svg><span className="label"><L ko="새 정책" en="New policy" /></span></div><div className="caret"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg></div></div>
      <div className="nav-ws"><span className="ws-av">A</span><div className="ws-label">Acme<span className="sub">메인 지갑 · 0xA1c4···7e29</span></div><svg className="ws-caret" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg></div>
      <div className="nav-divider"></div>
      <div className="nav-group">
        <a className="nav-item" href="Home.html"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 11.5 12 4l9 7.5"/><path d="M5 10v10h14V10"/></svg></span><span className="label">Home</span></a>
        <a className="nav-item" href="Editor v6.html"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></svg></span><span className="label">Editor</span></a>
        <a className="nav-item" href="#"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="9"/><path d="m10 8.5 5 3.5-5 3.5z"/></svg></span><span className="label">Simulation</span></a>
        <a className="nav-item" href="#"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 12h4l3 8 4-16 3 8h4"/></svg></span><span className="label">Monitoring</span></a>
      </div>
      <div className="nav-divider"></div>
      <div className="nav-group">
        <a className="nav-item active" href="#" aria-current="page"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 3v18h18"/><path d="m7 14 4-4 4 3 5-7"/></svg></span><span className="label"><L ko="History" en="History" /></span></a>
        <a className="nav-item" href="#"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9c0 .7.4 1.31 1 1.51H21a2 2 0 0 1 0 4h-.09c-.7 0-1.31.4-1.51 1z"/></svg></span><span className="label">Settings</span></a>
      </div>
      <div className="nav-bottom"><div className="nav-user"><span className="av">TY</span><div className="meta"><div className="nm">Taeyoon Kim</div><div className="em">ty@acme.xyz</div></div></div></div>
    </nav>
  );
}

/* ── verdict badge ── */
function Badge({ v }) {
  const m = VMETA[v]; const Ic = m.Ic;
  return <span className={`vbadge ${v}`}><Ic /><L ko={m.ko} en={m.en} /></span>;
}

/* ── decision chip (warn only) ── */
function DecisionChip({ d }) {
  if (d === 'trusted') return <span className="dchip trusted"><span className="dc-dot"></span><L ko="Accept" en="Accept" /></span>;
  return <span className="dchip cancelled"><span className="dc-dot"></span><L ko="Cancel" en="Cancel" /></span>;
}

/* ── expanded detail (the why panel — quasi-hero of this screen) ── */
function Detail({ r }) {
  return (
    <div className="detail-body">
      <dl className="dprops">
        <dt><L ko="매칭 정책" en="Matched policy" /></dt>
        <dd data-k="matched policy"><span className={`tag-pol ${r.policy.severity}`}>{r.policy.name}<span className="tp-v">{r.policy.severity}</span></span></dd>

        <dt><L ko="RPC method" en="RPC method" /></dt>
        <dd data-k="rpc method"><span className="mono">{r.method}</span></dd>

        <dt><L ko="대상 컨트랙트" en="Target contract" /></dt>
        <dd data-k="target contract"><span className="addr-pill"><span className="mono">{shortAddr(r.contract.addr)}</span><span className="sym">{r.contract.symbol}</span></span></dd>

        <dt><L ko="셀렉터" en="Selector" /></dt>
        <dd data-k="selector"><span className="mono">{r.selector.sig}</span> <span className="mono" style={{ color: 'var(--slate-400)' }}>· {r.selector.decoded}</span></dd>

        <dt><L ko="사유" en="Reason" /></dt>
        <dd data-k="reason" className="span2">
          <span className="reason-line"><I.quote className="rl-q" /><span className="reason-text"><L ko={r.reason.ko} en={r.reason.en} /></span></span>
        </dd>
      </dl>
    </div>
  );
}

/* ── verdict row (2-line) with severity hierarchy + evidence cues ── */
function Row({ r, open, dim, onToggle }) {
  const s = stamp(r.ts);
  return (
    <article className={`vrow ${r.verdict} ${open ? 'open' : ''} ${dim ? 'dim' : ''}`} data-screen-label={`verdict ${r.id}`}>
      <header className="vrow-head" role="button" tabIndex={0} aria-expanded={open}
        onClick={onToggle} onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onToggle(); } }}>
        <div className="vrow-l"><Badge v={r.verdict} /></div>
        <div className="vrow-main">
          <div className="vrow-r1">
            <span className="vrow-origin">{r.origin}</span>
            {r.verdict === 'warn' && r.decision && <DecisionChip d={r.decision} />}
          </div>
          <div className="vrow-r2">
            <span className="vrow-fn">{r.fn}</span>
            <span className="vrow-dot2">·</span>
            <span className="vrow-pol">{r.policy.name}</span>
          </div>
        </div>
        <div className="vrow-r">
          <div className="vrow-tsb" title={ago(r.ts)[document.body.dataset.locale] || ''}>
            <span className="vrow-time"><span className="td">{s.date}</span> {s.time}</span>
            <span className="vrow-seq">#{r.seq}</span>
          </div>
          <I.caretR className="vrow-chev" />
        </div>
      </header>
      <div className="vrow-detail"><div className="vrow-detail-inner">{open && <Detail r={r} />}</div></div>
    </article>
  );
}

/* ── collapsed run of consecutive PASS (severity hierarchy — PASS recedes) ── */
function PassGroup({ records, open, openId, onToggleGroup, onToggleRow }) {
  const origins = [...new Set(records.map(r => r.origin))];
  return (
    <div className={`passgroup ${open ? 'open' : ''}`}>
      <button className="pg-head" aria-expanded={open} onClick={onToggleGroup}>
        <span className="pg-badge"><I.pass /><L ko="PASS" en="Pass" /></span>
        <span className="pg-count"><L ko={<><b>{records.length}</b>건</>} en={<><b>{records.length}</b> records</>} /></span>
        <span className="pg-desc"><L ko="정책 통과 · 묶음" en="passed · collapsed" /></span>
        <span className="pg-origins">{origins.slice(0, 3).join(' · ')}{origins.length > 3 ? ` +${origins.length - 3}` : ''}</span>
        <I.caretR className="pg-chev" />
      </button>
      {open && (
        <div className="pg-body">
          {records.map(r => <Row key={r.id} r={r} dim open={openId === r.id} onToggle={() => onToggleRow(r.id)} />)}
        </div>
      )}
    </div>
  );
}

/* ── group section header (verdict / origin / rule modes) ── */
function GroupHeader({ group }) {
  const c = { pass: 0, warn: 0, fail: 0 };
  group.records.forEach(r => c[r.verdict]++);
  let title;
  if (group.kind === 'verdict') { const m = VMETA[group.verdict]; title = <span className={`gh-vlabel ${group.verdict}`}><m.Ic /><L ko={m.ko} en={m.en} /></span>; }
  else title = <span className="gh-key">{group.label}</span>;
  return (
    <div className={`group-head ${group.kind === 'verdict' ? group.verdict : ''}`}>
      {title}
      <span className="gh-n"><L ko={<><b>{group.records.length}</b>건</>} en={<><b>{group.records.length}</b></>} /></span>
      {group.kind !== 'verdict' && (
        <span className="gh-mini">
          {c.warn > 0 && <span style={{ color: 'var(--warn-700)' }}>{c.warn} warn</span>}
          {c.fail > 0 && <span style={{ color: 'var(--fail-600)' }}>{c.fail} fail</span>}
          {c.pass > 0 && <span style={{ color: 'var(--pass-700)' }}>{c.pass} pass</span>}
        </span>
      )}
    </div>
  );
}

/* ── build render items: severity hierarchy + grouping ── */
function buildItems(recs, mode) {
  if (mode === 'time') {
    const out = []; let run = [];
    const flush = () => {
      if (run.length >= 2) out.push({ type: 'passgroup', key: 'pg-' + run[0].id, records: run.slice() });
      else run.forEach(r => out.push({ type: 'row', record: r, dim: true }));
      run = [];
    };
    recs.forEach(r => {
      if (r.verdict === 'pass') run.push(r);
      else { flush(); out.push({ type: 'row', record: r }); }
    });
    flush();
    return out;
  }
  let groups = [];
  if (mode === 'verdict') {
    ['fail', 'warn', 'pass'].forEach(v => {
      const rs = recs.filter(r => r.verdict === v);
      if (rs.length) groups.push({ key: v, kind: 'verdict', verdict: v, records: rs });
    });
  } else {
    const keyFn = mode === 'origin' ? (r => r.origin) : (r => r.policy.name);
    const map = new Map();
    recs.forEach(r => { const k = keyFn(r); if (!map.has(k)) map.set(k, []); map.get(k).push(r); });
    groups = [...map.entries()].map(([k, rs]) => ({ key: k, kind: mode, label: k, records: rs }));
    const rank = g => g.records.some(r => r.verdict === 'fail') ? 0 : g.records.some(r => r.verdict === 'warn') ? 1 : 2;
    groups.sort((a, b) => rank(a) - rank(b) || b.records.length - a.records.length);
  }
  const out = [];
  groups.forEach(g => {
    out.push({ type: 'header', key: 'h-' + g.key, group: g });
    g.records.forEach(r => out.push({ type: 'row', record: r, dim: r.verdict === 'pass' }));
  });
  return out;
}

/* ── filter bar (search + verdict toggles + grouping) — no origin/policy dropdowns ── */
function FilterBar({ q, setQ, verdicts, toggleVerdict, group, setGroup, anyFilter, onReset }) {
  return (
    <div className="filter-row">
      <div className="aud-search">
        <I.search />
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="주소 · dApp 출처 · 함수명 · 정책명 검색" aria-label="search" />
        {q && <button className="clear" onClick={() => setQ('')}><I.x style={{ width: 13, height: 13 }} /></button>}
      </div>

      <div className="verdict-toggles" role="group" aria-label="verdict filter">
        {['pass', 'warn', 'fail'].map(v => {
          const m = VMETA[v]; const Ic = m.Ic; const on = verdicts.includes(v);
          return <button key={v} className={`vt ${v} ${on ? 'on' : ''}`} aria-pressed={on} onClick={() => toggleVerdict(v)}><Ic className="vt-ic" /><L ko={m.ko} en={m.en} /></button>;
        })}
      </div>

      <span className="filter-sep"></span>

      <span className="group-label"><I.layers /><L ko="그룹" en="Group" /></span>
      <div className="segmented group" role="tablist" aria-label="grouping">
        {GROUPS.map(g => (
          <button key={g.id} role="tab" aria-selected={group === g.id} className={`seg ${group === g.id ? 'on' : ''}`} onClick={() => setGroup(g.id)}>
            <L ko={g.ko} en={g.en} />
          </button>
        ))}
      </div>

      {anyFilter && <button className="filter-reset" onClick={onReset}><I.reset /><L ko="초기화" en="Reset" /></button>}
    </div>
  );
}

/* ── empty states ── */
function Empty({ kind, onReset }) {
  const filtered = kind === 'filtered';
  return (
    <div className="aud-empty">
      <div className="empty-art">
        <span className="rings"></span>
        <span className="mark">{filtered ? <I.search /> : <I.sleep />}</span>
      </div>
      <div className="ee-t">{filtered ? <L ko="조건에 맞는 verdict가 없어요" en="No verdicts match your filters" /> : <L ko="아직 기록된 verdict가 없어요" en="No verdicts recorded yet" />}</div>
      <div className="ee-d">
        {filtered
          ? <L ko="검색어·기간·필터를 조정해 보세요. 오래된 항목은 보존 기간이 지나 사라졌을 수 있어요." en="Try adjusting the search, time range, or filters. Older entries may have aged out of retention." />
          : <L ko="이 지갑에서 평가된 트랜잭션이 생기면 여기에 시간순으로 쌓입니다." en="Once transactions are evaluated for this wallet, they appear here in time order." />}
      </div>
      {filtered && <button className="ee-act" onClick={onReset}><I.reset /><L ko="필터 초기화" en="Reset filters" /></button>}
    </div>
  );
}

/* ── main app ── */
function HistoryApp() {
  const [t, setTweak] = useTweaks(HISTORY_TWEAKS);
  const [range, setRange] = useSH('1h');                 // Rev.1 default: 1h
  const [q, setQ] = useSH('');
  const [verdicts, setVerdicts] = useSH([]);             // empty = all
  const [group, setGroup] = useSH('time');
  const [openId, setOpenId] = useSH(null);
  const [openGroups, setOpenGroups] = useSH({});
  const [exportOpen, setExportOpen] = useSH(false);
  const [limit, setLimit] = useSH(12);
  const [toast, setToast] = useSH(null);
  const [simulateEmpty, setSimEmpty] = useSH(false);
  const exportRef = useRH(null);

  useEH(() => { document.body.setAttribute('data-locale', t.locale); }, [t.locale]);
  useEH(() => { document.body.setAttribute('data-detail', t.detail); }, [t.detail]);

  useEH(() => {
    if (!exportOpen) return;
    const h = (e) => { if (exportRef.current && !exportRef.current.contains(e.target)) setExportOpen(false); };
    window.addEventListener('mousedown', h); return () => window.removeEventListener('mousedown', h);
  }, [exportOpen]);

  const toggleVerdict = (v) => setVerdicts(s => s.includes(v) ? s.filter(x => x !== v) : [...s, v]);
  const anyFilter = q.trim().length > 0 || verdicts.length > 0;
  const onReset = () => { setQ(''); setVerdicts([]); };

  const rangeMs = RANGES.find(r => r.id === range).ms;
  const all = simulateEmpty ? [] : HISTORY_RECORDS;

  const filtered = useMH(() => {
    const now = Date.now(); const ql = q.trim().toLowerCase();
    return all
      .filter(r => now - r.ts <= rangeMs)
      .filter(r => !verdicts.length || verdicts.includes(r.verdict))
      .filter(r => {
        if (!ql) return true;
        return [r.origin, r.fn, r.policy.name, r.contract.addr, r.contract.symbol, r.selector.decoded]
          .some(f => String(f).toLowerCase().includes(ql));
      })
      .sort((a, b) => b.ts - a.ts);
  }, [all, rangeMs, verdicts, q]);

  const shown = filtered.slice(0, limit);
  const counts = useMH(() => {
    const c = { pass: 0, warn: 0, fail: 0 };
    filtered.forEach(r => c[r.verdict]++); return c;
  }, [filtered]);

  const items = useMH(() => buildItems(shown, group), [shown, group]);

  // why-panel quasi-hero: auto-open the top emphasised (fail→warn) row when the result set changes
  const heroId = useMH(() => {
    const emph = shown.find(r => r.verdict === 'fail') || shown.find(r => r.verdict === 'warn') || shown[0];
    return emph ? emph.id : null;
  }, [shown]);
  const shownSig = shown.map(r => r.id).join(',');
  useEH(() => {
    if (t.whyHero && heroId) setOpenId(heroId);
  }, [shownSig, group, t.whyHero]); // eslint-disable-line

  const doExport = (fmt) => { setExportOpen(false); setToast({ fmt, n: filtered.length }); setTimeout(() => setToast(null), 2600); };
  const toggleRow = (id) => setOpenId(cur => cur === id ? null : id);
  const toggleGroup = (key) => setOpenGroups(s => ({ ...s, [key]: !s[key] }));

  const isEmpty = filtered.length === 0;
  const emptyKind = all.length === 0 ? 'nodata' : 'filtered';

  return (
    <>
      <NavRail />
      <main className="content">
        {/* ── fixed region (header → list header) ── */}
        <div className="hist-fixed">
          {/* header */}
          <div className="aud-head">
            <div className="aud-title">
              <div className="aud-crumb"><span>Scopeball</span><span className="sep">·</span><span>Acme</span><span className="sep">·</span><span style={{ color: 'var(--slate-700)', fontWeight: 600 }}>0xA1c4···7e29</span></div>
              <h1 className="aud-h1"><L ko="History" en="History" /></h1>
              <div className="aud-sub"><L ko={<>지갑에서 평가된 verdict의 증거 기록. <b>방금 그 트랜잭션은 왜 이 verdict가 나왔나</b>를 건별로 들여다봅니다.</>} en={<>An evidence record of evaluated verdicts — <b>why did that transaction get this verdict</b>, one record at a time.</>} /></div>
            </div>
            <div className="aud-head-r" ref={exportRef}>
              <button className="export-btn" onClick={() => setExportOpen(o => !o)}>
                <I.download /><L ko="내보내기" en="Export" /><I.caretD className="caret" />
              </button>
              {exportOpen && (
                <div className="export-menu">
                  <div className="em-lab"><L ko="감사 증거 내보내기" en="Export audit evidence" /></div>
                  <div className="em-scope"><L ko={<>현재 필터·기간 범위의 <b>{filtered.length}건</b>을 내보냅니다.</>} en={<>Exports the <b>{filtered.length} records</b> in the current filter & range.</>} /></div>
                  <div className="export-item" onClick={() => doExport('CSV')}><span className="ei-fmt">CSV</span><span className="ei-txt"><div className="ei-t"><L ko="스프레드시트용" en="For spreadsheets" /></div><div className="ei-d">history_acme_{range}.csv</div></span></div>
                  <div className="export-item" onClick={() => doExport('JSON')}><span className="ei-fmt">JSON</span><span className="ei-txt"><div className="ei-t"><L ko="원본 레코드" en="Raw records" /></div><div className="ei-d">history_acme_{range}.json</div></span></div>
                  <div className="export-perm"><I.lock /><L ko="파일 저장 권한 확인 후 다운로드됩니다." en="Downloads after a file-save permission prompt." /></div>
                </div>
              )}
            </div>
          </div>

          {/* time range + static count summary (glance anchor) */}
          <div className="range-row">
            <span className="range-label"><I.clock className="rl-ic" /><L ko="전체기간" en="Time range" /></span>
            <div className="segmented" role="tablist" aria-label="time range">
              {RANGES.map(r => (
                <button key={r.id} role="tab" aria-selected={range === r.id} className={`seg ${range === r.id ? 'on' : ''}`} onClick={() => { setRange(r.id); setLimit(12); }}>
                  {typeof r.label === 'string' ? r.label : <L {...r.label} />}
                </button>
              ))}
            </div>
            <span className="range-hint"><L ko="현재 기준 롤링 윈도우" en="rolling window from now" /></span>
            <span className="range-spacer"></span>
            <span className="count-summary" aria-label="count summary">
              <span className="cs-total"><L ko={<><b>{filtered.length}</b>건</>} en={<><b>{filtered.length}</b></>} /></span>
              <span className="cs-sep"></span>
              <span className="cs-chip warn"><b>{counts.warn}</b> warn</span>
              <span className="cs-chip fail"><b>{counts.fail}</b> fail</span>
              <span className="cs-chip pass"><b>{counts.pass}</b> pass</span>
            </span>
          </div>

          {/* search + verdict toggle + grouping */}
          <FilterBar q={q} setQ={setQ} verdicts={verdicts} toggleVerdict={toggleVerdict}
            group={group} setGroup={setGroup} anyFilter={anyFilter} onReset={onReset} />

          {/* list header (last fixed element) */}
          {!isEmpty && (
            <div className="aud-list-head">
              <span className="lh-t"><L ko="Verdict 로그" en="Verdict log" /></span>
              <span className="lh-m"><L ko={`최신순 · ${shown.length}/${filtered.length} 표시`} en={`newest first · showing ${shown.length}/${filtered.length}`} /></span>
              <span className="lh-spacer"></span>
              <span className="lh-imm"><I.lock /><L ko="변경 불가 기록" en="Immutable record" /></span>
            </div>
          )}
        </div>

        {/* ── scroll region (verdict list only) ── */}
        <div className="hist-scroll">
          {isEmpty ? (
            <Empty kind={emptyKind} onReset={onReset} />
          ) : (
            <>
              <div className="vlist">
                {items.map(it => {
                  if (it.type === 'header') return <GroupHeader key={it.key} group={it.group} />;
                  if (it.type === 'passgroup') return (
                    <PassGroup key={it.key} records={it.records} open={!!openGroups[it.key]} openId={openId}
                      onToggleGroup={() => toggleGroup(it.key)} onToggleRow={toggleRow} />
                  );
                  const r = it.record;
                  return <Row key={r.id} r={r} dim={it.dim} open={openId === r.id} onToggle={() => toggleRow(r.id)} />;
                })}
              </div>
              {filtered.length > shown.length && (
                <div className="load-more"><button onClick={() => setLimit(n => n + 12)}><L ko={`${filtered.length - shown.length}건 더 보기`} en={`Show ${filtered.length - shown.length} more`} /></button></div>
              )}
            </>
          )}
        </div>

        {/* retention hint — pinned at the bottom of the list container */}
        {!isEmpty && (
          <div className="retention-hint"><I.info /><L ko="보존 기간(최장 7일)이 지난 항목은 자동으로 사라질 수 있어요." en="Entries past the retention window (up to 7 days) may age out automatically." /></div>
        )}
      </main>

      {toast && <div className="aud-toast"><I.check /><L ko={<>{toast.n}건을 <span className="mono">{toast.fmt}</span>로 내보냈어요</>} en={<>Exported {toast.n} records as <span className="mono">{toast.fmt}</span></>} /></div>}

      <TweaksPanel>
        <TweakSection label="글랜스" />
        <TweakToggle label="상단 항목 자동 펼침 (why 히어로)" value={t.whyHero} onChange={(v) => setTweak('whyHero', v)} />
        <TweakSection label="펼침 상세" />
        <TweakRadio label="상세 레이아웃" value={t.detail} options={['table', 'card']} onChange={(v) => setTweak('detail', v)} />
        <TweakSection label="언어" />
        <TweakRadio label="locale" value={t.locale} options={['ko', 'en']} onChange={(v) => setTweak('locale', v)} />
        <TweakSection label="상태 미리보기" />
        <TweakToggle label="데이터 없음(빈 상태)" value={simulateEmpty} onChange={setSimEmpty} />
      </TweaksPanel>
    </>
  );
}

const HISTORY_TWEAKS = /*EDITMODE-BEGIN*/{
  "whyHero": true,
  "detail": "table",
  "locale": "ko"
}/*EDITMODE-END*/;

ReactDOM.createRoot(document.getElementById('app')).render(<HistoryApp />);
