// monitoring-shared.jsx — shared icons, helpers, NavRail, and atoms (badges, VaR cell, chain pill).
// Exported to window for the other monitoring-*.jsx files.

const { useState: useSM, useMemo: useMM, useEffect: useEM, useRef: useRM } = React;

/* ── icons ── */
const MI = {
  back: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M15 18l-6-6 6-6"/></svg>,
  caretR: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="m9 6 6 6-6 6"/></svg>,
  caretD: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="m6 9 6 6 6-6"/></svg>,
  lock: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/></svg>,
  eye: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/></svg>,
  alert: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M10.3 3.8 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.8a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4M12 17h.01"/></svg>,
  ban: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="12" cy="12" r="9"/><path d="M5.6 5.6 18.4 18.4"/></svg>,
  infinity: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M6 16c2.2 0 3.3-1.8 4.5-3.5C12 10 13 8 15.5 8a3.5 3.5 0 0 1 0 7C13 15 12 13 10.5 11 9.3 9.3 8.2 8 6 8a3.5 3.5 0 0 0 0 8Z"/></svg>,
  check: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M20 6 9 17l-5-5"/></svg>,
  shield: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M12 3 5 6v6c0 4 3 6.5 7 9 4-2.5 7-5 7-9V6l-7-3Z"/></svg>,
  x: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M6 6l12 12M18 6 6 18"/></svg>,
  wallet: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="3" y="6" width="18" height="13" rx="2.5"/><path d="M16 12h.01M3 9h18"/></svg>,
  layers: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="m12 3 9 5-9 5-9-5 9-5Z"/><path d="m3 13 9 5 9-5"/></svg>,
  scan: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M3 7V5a2 2 0 0 1 2-2h2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2M3 12h18"/></svg>,
  revoke: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/></svg>,
  edit: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/></svg>,
  rule: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="4" y="3" width="16" height="18" rx="2"/><path d="M8 8h8M8 12h8M8 16h5"/></svg>,
  block: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="12" cy="12" r="9"/><path d="M5.6 5.6 18.4 18.4"/></svg>,
  wc: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M7 10c2.8-2.7 7.2-2.7 10 0M5 8c3.9-3.8 10.1-3.8 14 0M9.5 12.5c1.4-1.3 3.6-1.3 5 0"/><circle cx="12" cy="16" r="1.2" fill="currentColor" stroke="none"/></svg>,
  ext: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M7 17 17 7M9 7h8v8"/></svg>,
  spin: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" {...p}><path d="M12 3a9 9 0 1 0 9 9" opacity="0.9"/></svg>,
};

/* ── helpers ── */
function L({ ko, en }) { return (<>{ko != null && <span lang="ko">{ko}</span>}{en != null && <span lang="en">{en}</span>}</>); }
function fmtUsd(n) { return '$' + Number(n).toLocaleString('en-US', { maximumFractionDigits: 0 }); }
function chainOf(id) { return CHAINS[id] || { name: id, short: id, color: '#999' }; }
function spenderOf(id) { return SPENDERS[id] || { label: id, short: id, addr: '—', rep: 'unknown' }; }
function ds() { return document.body.dataset.state || 'full'; }

/* ── read-only badge ── */
function ReadOnly() {
  return <span className="ro-badge"><MI.eye /><L ko="읽기 전용" en="Read-only" /></span>;
}

/* ── chain pill ── */
function ChainPill({ id }) {
  const c = chainOf(id);
  return <span className="chain-pill"><span className="cp-dot" style={{ background: c.color }}></span>{c.name}</span>;
}

/* ── spender reputation cell ── */
function Spender({ id, compact }) {
  const s = spenderOf(id);
  return (
    <span className={`spender ${s.rep}`}>
      <span className="sp-dot"></span>
      <span className="sp-label">{compact ? s.short : s.label}</span>
      <span className="sp-addr mono">{s.addr}</span>
    </span>
  );
}

/* ── risk overlay badge ── */
function RiskBadge({ r }) {
  const Ic = r.kind === 'fail' ? MI.ban : r.kind === 'unlimited' ? MI.infinity : r.kind === 'warn' ? MI.alert : MI.check;
  return <span className={`r-badge ${r.kind}`}><Ic /><L {...r.label} /></span>;
}

/* ── VaR cell (3-state, honesty rule §4) ── */
function VarCell({ value, risk }) {
  const state = ds();
  if (state === 'loading') return <span className="hv-loading"><MI.spin style={{ width: 13, height: 13 }} /><span className="sk"></span></span>;
  if (state === 'p0') return <span className="hv-none"><L ko="연결 전" en="not connected" /></span>;
  if (value == null) return <span className="hv-none">—</span>;
  return <span className={`hv-amt ${risk ? 'risk' : ''}`}>{fmtUsd(value)}</span>;
}

/* ── nav rail (Monitoring active) ── */
function NavRail() {
  return (
    <nav className="nav-rail" aria-label="Global">
      <div className="nav-logo"><span className="mark">LOGO</span><span className="word">Scopeball</span></div>
      <div className="nav-cta"><div className="main"><svg className="plus" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M12 5v14M5 12h14"/></svg><span className="label"><L ko="새 정책" en="New policy" /></span></div><div className="caret"><MI.caretD /></div></div>
      <div className="nav-ws"><span className="ws-av">A</span><div className="ws-label">Acme<span className="sub">4 지갑 · 14 정책</span></div><MI.caretD className="ws-caret" /></div>
      <div className="nav-divider"></div>
      <div className="nav-group">
        <a className="nav-item" href="Home.html"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 11.5 12 4l9 7.5"/><path d="M5 10v10h14V10"/></svg></span><span className="label">Home</span></a>
        <a className="nav-item" href="Editor v6.html"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></svg></span><span className="label">Editor</span></a>
        <a className="nav-item" href="#"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="9"/><path d="m10 8.5 5 3.5-5 3.5z"/></svg></span><span className="label">Simulation</span></a>
        <a className="nav-item active" href="#" aria-current="page"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 12h4l3 8 4-16 3 8h4"/></svg></span><span className="label">Monitoring</span></a>
      </div>
      <div className="nav-divider"></div>
      <div className="nav-group">
        <a className="nav-item" href="History.html"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 3v18h18"/><path d="m7 14 4-4 4 3 5-7"/></svg></span><span className="label">History</span></a>
        <a className="nav-item" href="#"><span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9c0 .7.4 1.31 1 1.51H21a2 2 0 0 1 0 4h-.09c-.7 0-1.31.4-1.51 1z"/></svg></span><span className="label">Settings</span></a>
      </div>
      <div className="nav-bottom"><div className="nav-user"><span className="av">TY</span><div className="meta"><div className="nm">Taeyoon Kim</div><div className="em">ty@acme.xyz</div></div></div></div>
    </nav>
  );
}

Object.assign(window, { MI, L, fmtUsd, chainOf, spenderOf, ds, ReadOnly, ChainPill, Spender, RiskBadge, VarCell, NavRail });
