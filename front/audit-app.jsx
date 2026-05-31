// audit-app.jsx — Scopeball Audit screen. Single working prototype.
// Live client-side filtering (time range / search / verdict / origin / policy),
// inline one-at-a-time expand, Warn decision chips, dual empty states, Export menu.

const { useState: useSA, useMemo: useMA, useEffect: useEA, useRef: useRA } = React;

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

function L({ ko, en }) { return (<>{ko != null && <span lang="ko">{ko}</span>}{en != null && <span lang="en">{en}</span>}</>); }
function shortAddr(a) { return a.length > 14 ? `${a.slice(0, 6)}···${a.slice(-4)}` : a; }
function ago(ts) {
  const d = Date.now() - ts, m = Math.floor(d / 60000), h = Math.floor(d / 3600000), day = Math.floor(d / 86400000);
  if (m < 1) return { ko: '방금', en: 'just now' };
  if (m < 60) return { ko: `${m}분 전`, en: `${m}m ago` };
  if (h < 24) return { ko: `${h}시간 전`, en: `${h}h ago` };
  return { ko: `${day}일 전`, en: `${day}d ago` };
}
function clock(ts) {
  const dt = new Date(ts);
  const hh = String(dt.getHours()).padStart(2, '0'), mm = String(dt.getMinutes()).padStart(2, '0');
  const mo = String(dt.getMonth() + 1).padStart(2, '0'), dd = String(dt.getDate()).padStart(2, '0');
  return `${mo}/${dd} ${hh}:${mm}`;
}

/* ── nav rail (Cloudy Pond markup, Audit active) ── */
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
        <a className="nav-item active" href="#" aria-current="page"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 3v18h18"/><path d="m7 14 4-4 4 3 5-7"/></svg></span><span className="label">Audit</span></a>
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
  if (d === 'trusted') return <span className="dchip trusted"><span className="dc-dot"></span><L ko="신뢰함" en="Trusted" /></span>;
  return <span className="dchip cancelled"><span className="dc-dot"></span><L ko="취소함" en="Cancelled" /></span>;
}

/* ── expanded detail ── */
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
        <dd data-k="selector"><span className="mono">{r.selector.sig}</span> <span className="mono" style={{ color: 'var(--slate-500)' }}>· {r.selector.decoded}</span></dd>

        <dt><L ko="사유" en="Reason" /></dt>
        <dd data-k="reason" className="span2">
          <span className="reason-line"><I.quote className="rl-q" /><span className="reason-text"><L ko={r.reason.ko} en={r.reason.en} /></span></span>
        </dd>
      </dl>
    </div>
  );
}

/* ── verdict row ── */
function Row({ r, open, onToggle }) {
  return (
    <article className={`vrow ${r.verdict} ${open ? 'open' : ''}`} data-screen-label={`verdict ${r.id}`}>
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
          <span className="vrow-ts" title={clock(r.ts)}><L {...ago(r.ts)} /></span>
          <I.caretR className="vrow-chev" />
        </div>
      </header>
      <div className="vrow-detail"><div className="vrow-detail-inner">{open && <Detail r={r} />}</div></div>
    </article>
  );
}

/* ── dropdown filter ── */
function SelectFilter({ kLabel, value, options, onChange, fmtMono }) {
  const [open, setOpen] = useSA(false);
  const ref = useRA(null);
  useEA(() => {
    if (!open) return;
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) setOpen(false); };
    window.addEventListener('mousedown', h); return () => window.removeEventListener('mousedown', h);
  }, [open]);
  const active = value != null;
  return (
    <div className="filter-select" ref={ref}>
      <button className={`fs-btn ${active ? 'active' : ''}`} onClick={() => setOpen(o => !o)}>
        <span className="fs-k">{kLabel.ko ? <L {...kLabel} /> : kLabel}</span>
        <span className={fmtMono ? 'fs-mono' : ''}>{active ? value : <L ko="전체" en="All" />}</span>
        <I.caretD className="fs-caret" />
      </button>
      {open && (
        <div className="fs-menu">
          <div className={`fs-opt ${value == null ? 'sel' : ''}`} onClick={() => { onChange(null); setOpen(false); }}><L ko="전체" en="All" /><I.check className="fs-check" /></div>
          {options.map(o => (
            <div key={o} className={`fs-opt ${value === o ? 'sel' : ''}`} onClick={() => { onChange(o); setOpen(false); }}>
              <span className={fmtMono ? 'fs-mono' : ''}>{o}</span><I.check className="fs-check" />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/* ── filter bar (shared by top + left variants) ── */
function FilterBar({ q, setQ, verdicts, toggleVerdict, origin, setOrigin, policy, setPolicy, anyFilter, onReset }) {
  return (
    <div className="filter-row">
      <span className="filter-sidebar-h"><L ko="검색" en="Search" /></span>
      <div className="aud-search">
        <I.search />
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="주소 · dApp 출처 · 함수명 · 정책명 검색" aria-label="search" />
        {q && <button className="clear" onClick={() => setQ('')}><I.x style={{ width: 13, height: 13 }} /></button>}
      </div>

      <span className="filter-sidebar-h"><L ko="Verdict" en="Verdict" /></span>
      <div className="verdict-toggles">
        {['pass', 'warn', 'fail'].map(v => {
          const m = VMETA[v]; const Ic = m.Ic; const on = verdicts.includes(v);
          return <button key={v} className={`vt ${v} ${on ? 'on' : ''}`} aria-pressed={on} onClick={() => toggleVerdict(v)}><Ic className="vt-ic" /><L ko={m.ko} en={m.en} /></button>;
        })}
      </div>

      <span className="filter-sep"></span>

      <span className="filter-sidebar-h"><L ko="출처" en="Origin" /></span>
      <SelectFilter kLabel={{ ko: '출처', en: 'Origin' }} value={origin} options={AUDIT_ORIGINS} onChange={setOrigin} fmtMono />
      <span className="filter-sidebar-h"><L ko="정책" en="Policy" /></span>
      <SelectFilter kLabel={{ ko: '정책', en: 'Policy' }} value={policy} options={AUDIT_POLICIES} onChange={setPolicy} fmtMono />

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
function AuditApp() {
  const [t, setTweak] = useTweaks(AUDIT_TWEAKS);
  const [range, setRange] = useSA('24h');
  const [q, setQ] = useSA('');
  const [verdicts, setVerdicts] = useSA([]); // empty = all
  const [origin, setOrigin] = useSA(null);
  const [policy, setPolicy] = useSA(null);
  const [openId, setOpenId] = useSA(null);
  const [exportOpen, setExportOpen] = useSA(false);
  const [limit, setLimit] = useSA(8);
  const [toast, setToast] = useSA(null);
  const [simulateEmpty, setSimEmpty] = useSA(false);
  const exportRef = useRA(null);

  useEA(() => { document.body.setAttribute('data-locale', t.locale); }, [t.locale]);
  useEA(() => { document.body.setAttribute('data-filters', t.filters); }, [t.filters]);
  useEA(() => { document.body.setAttribute('data-detail', t.detail); }, [t.detail]);

  useEA(() => {
    if (!exportOpen) return;
    const h = (e) => { if (exportRef.current && !exportRef.current.contains(e.target)) setExportOpen(false); };
    window.addEventListener('mousedown', h); return () => window.removeEventListener('mousedown', h);
  }, [exportOpen]);

  const toggleVerdict = (v) => setVerdicts(s => s.includes(v) ? s.filter(x => x !== v) : [...s, v]);
  const anyFilter = q || verdicts.length || origin || policy;
  const onReset = () => { setQ(''); setVerdicts([]); setOrigin(null); setPolicy(null); };

  const rangeMs = RANGES.find(r => r.id === range).ms;
  const all = simulateEmpty ? [] : AUDIT_RECORDS;

  const filtered = useMA(() => {
    const now = Date.now(); const ql = q.trim().toLowerCase();
    return all
      .filter(r => now - r.ts <= rangeMs)
      .filter(r => !verdicts.length || verdicts.includes(r.verdict))
      .filter(r => !origin || r.origin === origin)
      .filter(r => !policy || r.policy.name === policy)
      .filter(r => {
        if (!ql) return true;
        return [r.origin, r.fn, r.policy.name, r.contract.addr, r.contract.symbol, r.selector.decoded]
          .some(f => String(f).toLowerCase().includes(ql));
      })
      .sort((a, b) => b.ts - a.ts);
  }, [all, rangeMs, verdicts, origin, policy, q]);

  const shown = filtered.slice(0, limit);
  const counts = useMA(() => {
    const c = { pass: 0, warn: 0, fail: 0 };
    filtered.forEach(r => c[r.verdict]++); return c;
  }, [filtered]);

  const doExport = (fmt) => { setExportOpen(false); setToast({ fmt, n: filtered.length }); setTimeout(() => setToast(null), 2600); };

  const isEmpty = filtered.length === 0;
  const emptyKind = all.length === 0 ? 'nodata' : 'filtered';

  return (
    <>
      <NavRail />
      <main className="content">
        {/* header */}
        <div className="aud-head">
          <div className="aud-title">
            <div className="aud-crumb"><span>Scopeball</span><span className="sep">·</span><span>Acme</span><span className="sep">·</span><span style={{ color: 'var(--slate-700)', fontWeight: 600 }}>0xA1c4···7e29</span></div>
            <h1 className="aud-h1">Audit</h1>
            <div className="aud-sub"><L ko={<>개별 verdict를 조회·확인하는 곳. <b>방금 그 트랜잭션은 왜 이 verdict가 나왔나</b>를 건별로 들여다봅니다.</>} en={<>Look up and inspect individual verdicts — <b>why did that transaction get this verdict</b>, one record at a time.</>} /></div>
          </div>
          <div className="aud-head-r" ref={exportRef}>
            <button className="export-btn" onClick={() => setExportOpen(o => !o)}>
              <I.download /><L ko="내보내기" en="Export" /><I.caretD className="caret" />
            </button>
            {exportOpen && (
              <div className="export-menu">
                <div className="em-lab"><L ko="감사 증거 내보내기" en="Export audit evidence" /></div>
                <div className="em-scope"><L ko={<>현재 필터·기간 범위의 <b>{filtered.length}건</b>을 내보냅니다.</>} en={<>Exports the <b>{filtered.length} records</b> in the current filter & range.</>} /></div>
                <div className="export-item" onClick={() => doExport('CSV')}><span className="ei-fmt">CSV</span><span className="ei-txt"><div className="ei-t"><L ko="스프레드시트용" en="For spreadsheets" /></div><div className="ei-d">audit_acme_{range}.csv</div></span></div>
                <div className="export-item" onClick={() => doExport('JSON')}><span className="ei-fmt">JSON</span><span className="ei-txt"><div className="ei-t"><L ko="원본 레코드" en="Raw records" /></div><div className="ei-d">audit_acme_{range}.json</div></span></div>
                <div className="export-perm"><I.lock /><L ko="파일 저장 권한 확인 후 다운로드됩니다." en="Downloads after a file-save permission prompt." /></div>
              </div>
            )}
          </div>
        </div>

        {/* time range */}
        <div className="aud-controls">
          <div className="range-row">
            <span className="range-label"><I.clock className="rl-ic" /><L ko="전체기간" en="Time range" /></span>
            <div className="segmented" role="tablist" aria-label="time range">
              {RANGES.map(r => (
                <button key={r.id} role="tab" aria-selected={range === r.id} className={`seg ${range === r.id ? 'on' : ''}`} onClick={() => { setRange(r.id); setLimit(8); }}>
                  {typeof r.label === 'string' ? r.label : <L {...r.label} />}
                </button>
              ))}
            </div>
            <span className="range-hint"><L ko="현재 기준 롤링 윈도우" en="rolling window from now" /></span>
            <span className="range-spacer"></span>
            <span className="range-count">
              <L ko={<><b>{filtered.length}</b>건</> } en={<><b>{filtered.length}</b> records</>} />
              {' · '}<span style={{ color: 'var(--pass-700)' }}>{counts.pass} pass</span>
              {' · '}<span style={{ color: 'var(--warn-700)' }}>{counts.warn} warn</span>
              {' · '}<span style={{ color: 'var(--fail-600)' }}>{counts.fail} fail</span>
            </span>
          </div>
        </div>

        {/* body: filters + list (top/left variants via body[data-filters]) */}
        <div className="aud-body">
          <FilterBar q={q} setQ={setQ} verdicts={verdicts} toggleVerdict={toggleVerdict}
            origin={origin} setOrigin={setOrigin} policy={policy} setPolicy={setPolicy}
            anyFilter={anyFilter} onReset={onReset} />

          <div className="aud-main">
            {!isEmpty && (
              <div className="aud-list-head">
                <span className="lh-t"><L ko="Verdict 로그" en="Verdict log" /></span>
                <span className="lh-m"><L ko={`시간 역순 · ${shown.length}/${filtered.length} 표시`} en={`newest first · showing ${shown.length}/${filtered.length}`} /></span>
              </div>
            )}

            {isEmpty ? (
              <Empty kind={emptyKind} onReset={onReset} />
            ) : (
              <>
                <div className="vlist">
                  {shown.map(r => (
                    <Row key={r.id} r={r} open={openId === r.id} onToggle={() => setOpenId(id => id === r.id ? null : r.id)} />
                  ))}
                </div>
                {filtered.length > shown.length && (
                  <div className="load-more"><button onClick={() => setLimit(n => n + 8)}><L ko={`${filtered.length - shown.length}건 더 보기`} en={`Show ${filtered.length - shown.length} more`} /></button></div>
                )}
                <div className="retention-hint"><I.info /><L ko="보존 기간(최장 7일)이 지난 항목은 자동으로 사라질 수 있어요." en="Entries past the retention window (up to 7 days) may age out automatically." /></div>
              </>
            )}
          </div>
        </div>
      </main>

      {toast && <div className="aud-toast"><I.check /><L ko={<>{toast.n}건을 <span className="mono">{toast.fmt}</span>로 내보냈어요</>} en={<>Exported {toast.n} records as <span className="mono">{toast.fmt}</span></>} /></div>}

      <TweaksPanel>
        <TweakSection label="레이아웃" />
        <TweakRadio label="필터 배치" value={t.filters} options={['top', 'left']} onChange={(v) => setTweak('filters', v)} />
        <TweakRadio label="펼침 상세" value={t.detail} options={['table', 'card']} onChange={(v) => setTweak('detail', v)} />
        <TweakSection label="언어" />
        <TweakRadio label="locale" value={t.locale} options={['ko', 'en']} onChange={(v) => setTweak('locale', v)} />
        <TweakSection label="상태 미리보기" />
        <TweakToggle label="데이터 없음(빈 상태)" value={simulateEmpty} onChange={setSimEmpty} />
      </TweaksPanel>
    </>
  );
}

const AUDIT_TWEAKS = /*EDITMODE-BEGIN*/{
  "filters": "top",
  "detail": "table",
  "locale": "ko"
}/*EDITMODE-END*/;

ReactDOM.createRoot(document.getElementById('app')).render(<AuditApp />);
