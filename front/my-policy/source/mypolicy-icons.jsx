// mypolicy-icons.jsx — 공유 아이콘 세트 + Cloudy Pond nav rail(계승) + 카테고리 비주얼 헬퍼

const MPI = {
  shield: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" {...p}><path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/><path d="M9 12l2 2 4-4" strokeLinecap="round"/></svg>,
  search: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" {...p}><circle cx="11" cy="11" r="7"/><path d="M16 16l5 5"/></svg>,
  x: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" {...p}><path d="M6 6l12 12M18 6L6 18"/></svg>,
  plus: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.3" strokeLinecap="round" {...p}><path d="M12 5v14M5 12h14"/></svg>,
  caret: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M9 6l6 6-6 6"/></svg>,
  caretD: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M6 9l6 6 6-6"/></svg>,
  back: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M15 6l-6 6 6 6"/></svg>,
  grip: (p) => <svg viewBox="0 0 24 24" fill="currentColor" {...p}><circle cx="9" cy="6" r="1.6"/><circle cx="15" cy="6" r="1.6"/><circle cx="9" cy="12" r="1.6"/><circle cx="15" cy="12" r="1.6"/><circle cx="9" cy="18" r="1.6"/><circle cx="15" cy="18" r="1.6"/></svg>,
  folder: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinejoin="round" {...p}><path d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z"/></svg>,
  folderOpen: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinejoin="round" {...p}><path d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v1H3z"/><path d="M3 9h18l-2 8a1 1 0 01-1 1H4a1 1 0 01-1-.8z"/></svg>,
  lock: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 018 0v3"/></svg>,
  extract: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M12 3v12"/><path d="M8 7l4-4 4 4"/><path d="M5 14v5a2 2 0 002 2h10a2 2 0 002-2v-5"/></svg>,
  copy: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.1" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M12 6v12M6 12h12"/></svg>,
  pencil: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M4 20l4-1 10-10-3-3L5 16z"/><path d="M14 6l3 3"/></svg>,
  draft: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M4 20l4-1 10-10-3-3L5 16z"/></svg>,
  warn: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M12 3l9 16H3z"/><path d="M12 10v4M12 17h.01"/></svg>,
  check: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M5 13l4 4L19 7"/></svg>,
  wallet: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="3" y="6" width="18" height="13" rx="2.5"/><path d="M16 12h2"/><path d="M3 9h13a2 2 0 012 2"/></svg>,
  save: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M5 3h11l3 3v15H5z"/><path d="M8 3v5h7V3M8 21v-7h8v7"/></svg>,
  link: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M10 14a4 4 0 005.7 0l3-3a4 4 0 00-5.7-5.7l-1.5 1.5"/><path d="M14 10a4 4 0 00-5.7 0l-3 3a4 4 0 005.7 5.7l1.5-1.5"/></svg>,
  blocks: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinejoin="round" {...p}><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></svg>,
  form: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="4" y="4" width="16" height="16" rx="2.5"/><path d="M8 9h8M8 13h8M8 17h5"/></svg>,
  speech: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M4 5h16v11H9l-4 4v-4H4z"/></svg>,
  hash: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" {...p}><path d="M4 9h16M4 15h16M10 3L8 21M16 3l-2 18"/></svg>,
  key: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="8" cy="8" r="5"/><path d="M11.5 11.5L21 21M17 17l2-2M14 14l2-2"/></svg>,
  clock: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" {...p}><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>,
  token: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" {...p}><circle cx="12" cy="12" r="8"/><path d="M9.5 12l1.8 1.8 3.5-3.6" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  dot: (p) => <svg viewBox="0 0 24 24" fill="currentColor" {...p}><circle cx="12" cy="12" r="4"/></svg>,
  merge: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M7 4v6a5 5 0 005 5h5"/><path d="M14 12l3 3-3 3"/><path d="M17 4v0"/></svg>,
  layers: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M12 3l9 5-9 5-9-5z"/><path d="M3 13l9 5 9-5"/></svg>,
};

// 도메인/카테고리 라인 아이콘 (단순 패스)
const CAT_ICON = {
  swap: "M7 7h11l-3-3M17 17H6l3 3",
  amm: "M12 3c3 4 6 7 6 10a6 6 0 01-12 0c0-3 3-6 6-10z",
  perp: "M3 17l5-6 4 3 5-7 4 4",
  bridge: "M3 16c0-4 3-7 9-7s9 3 9 7M8 13v6M16 13v6",
  security: "M12 3l7 3v5c0 4-3 7-7 9-4-2-7-5-7-9V6z",
  airdrop: "M12 3a6 6 0 016 6c0 3-6 9-6 9S6 12 6 9a6 6 0 016-6M12 21v-3",
  lending: "M3 10h18M5 10v8h14v-8M9 14h6",
  nft: "M4 4h16v16H4zM8 10a1.5 1.5 0 100-3 1.5 1.5 0 000 3M4 16l5-4 4 3 3-2 4 3",
  core: "M12 3l8 4v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V7z",
  token: "M12 4a8 8 0 100 16 8 8 0 000-16M9.5 12l1.8 1.8 3.5-3.6"
};
function CatIcon({ cat, ...p }) {
  const d = CAT_ICON[cat] || CAT_ICON.core;
  return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" {...p}><path d={d} /></svg>;
}

// 카테고리 색 스타일 헬퍼 (배경 soft / 글자 hex)
function catStyle(cat) {
  const c = MP.CAT[cat] || MP.CAT.core;
  return { iconWrap: { background: c.soft, color: c.hex }, tag: { background: c.soft, color: c.ink }, hex: c.hex, soft: c.soft, ink: c.ink };
}

const NAV_SVG = {
  home: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 11.5 12 4l9 7.5"/><path d="M5 10v10h14V10"/></svg>,
  editor: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></svg>,
  policy: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/><path d="M9 12l2 2 4-4"/></svg>,
  sim: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="9"/><path d="m10 8.5 5 3.5-5 3.5z"/></svg>,
  mon: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 12h4l3 8 4-16 3 8h4"/></svg>,
  hist: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 3v18h18"/><path d="m7 14 4-4 4 3 5-7"/></svg>,
  set: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15c.1.4.3.8.6 1L19 17l-1-1c-.3-.3-.6-.5-1-.6"/><path d="M4.6 9c.1-.4.3-.8.6-1L4 7l1-1c.3.3.6.5 1 .6"/></svg>,
};
function NavRail({ onNewPolicy, loc }) {
  const T = loc === "en"
    ? { newp: "New policy", ws: "Main wallet", home: "Home", editor: "Editor", policy: "My Policy", sim: "Simulation", mon: "Monitoring", hist: "History", set: "Settings" }
    : { newp: "새 정책", ws: "메인 지갑", home: "Home", editor: "Editor", policy: "My Policy", sim: "Simulation", mon: "Monitoring", hist: "History", set: "Settings" };
  return (
    <nav className="nav-rail">
      <div className="nav-logo"><div className="mark">{MPI.shield()}</div><div className="word">Dambi</div></div>
      <div className="nav-cta">
        <div className="main" onClick={onNewPolicy}>{MPI.plus({ className: "plus" })}<span className="label">{T.newp}</span></div>
      </div>
      <div className="nav-ws"><span className="ws-av">A</span><div className="ws-label">Acme<span className="sub">{T.ws} · 0xA1c4···7e29</span></div></div>
      <div className="nav-divider" />
      <div className="nav-group">
        <a className="nav-item"><span className="icon">{NAV_SVG.home}</span><span className="label">{T.home}</span></a>
        <a className="nav-item active"><span className="icon">{NAV_SVG.policy}</span><span className="label">{T.policy}</span></a>
        <a className="nav-item"><span className="icon">{NAV_SVG.sim}</span><span className="label">{T.sim}</span></a>
        <a className="nav-item"><span className="icon">{NAV_SVG.mon}</span><span className="label">{T.mon}</span></a>
      </div>
      <div className="nav-divider" />
      <div className="nav-group">
        <a className="nav-item"><span className="icon">{NAV_SVG.hist}</span><span className="label">{T.hist}</span><span className="badge">12</span></a>
        <a className="nav-item"><span className="icon">{NAV_SVG.set}</span><span className="label">{T.set}</span></a>
      </div>
      <div className="nav-bottom"><div className="nav-user"><span className="av">TY</span><div className="meta"><div className="nm">Taeyoon Kim</div><div className="em">ty@dambi.co</div></div></div></div>
    </nav>
  );
}

Object.assign(window, { MPI, CatIcon, CAT_ICON, catStyle, NavRail });
