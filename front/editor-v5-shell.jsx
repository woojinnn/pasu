// editor-v5-shell.jsx — Icons, NavRail (Nav.html style), Topbar (Builder/Code), SaveBar

const { useState: useSSh, useEffect: useESh, useRef: useRSh } = React;

const V5I = {
  caretDown: (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  caretRight:(p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  caretLeft: (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M10 4L6 8l4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  search:    (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.5"/><path d="M10.5 10.5L14 14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>,
  undo:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3 7.5h7a3 3 0 0 1 0 6H6.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/><path d="M5.5 5L3 7.5 5.5 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  redo:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M13 7.5H6a3 3 0 0 0 0 6h3.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/><path d="M10.5 5L13 7.5 10.5 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  edit:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M11 3l2 2-7.5 7.5H3.5V10L11 3z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round"/></svg>,
  cog:       (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><circle cx="8" cy="8" r="2.2" stroke="currentColor" strokeWidth="1.4"/><path d="M8 1.5v2M8 12.5v2M14.5 8h-2M3.5 8h-2M12.6 3.4l-1.4 1.4M4.8 11.2l-1.4 1.4M12.6 12.6l-1.4-1.4M4.8 4.8L3.4 3.4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/></svg>,
  warn:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M8 2.5L14 13H2L8 2.5z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round"/><path d="M8 6.5v3M8 11.2v.6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/></svg>,
  check:     (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3.5 8.5l3 3 6-7" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  x:         (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>,
  plus:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M8 3v10M3 8h10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>,
  arrowRight:(p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3 8h10M9 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  play:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M5 3.5v9l7-4.5-7-4.5z" fill="currentColor"/></svg>,
  home:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M2 7l6-5 6 5M3.5 6.5v7h9v-7" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  blocks:    (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><rect x="2" y="2" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.4"/><rect x="9" y="2" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.4"/><rect x="2" y="9" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.4"/><rect x="9" y="9" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.4"/></svg>,
  library:   (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3 3v10M6 3v10M10 4l3 9M11 4l3 9" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/></svg>,
  audit:     (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3 2v12h11" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/><path d="M5 10l3-3 2 2 3-4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  code:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M5.5 5L2.5 8l3 3M10.5 5l3 3-3 3M9.5 4l-3 8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  shapes:    (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><rect x="2" y="2" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.4"/><circle cx="11.5" cy="11.5" r="2.5" stroke="currentColor" strokeWidth="1.4"/></svg>,
  zoomFit:   (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3 6V3h3M13 6V3h-3M3 10v3h3M13 10v3h-3" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/></svg>,
};

// ─── NAV RAIL — Home.html parity (hover-expand; locked in Editor; ⌘B opens) ─
function V5NavRail({ locked, forceExpanded }) {
  const cls = ['nav-rail', locked ? 'locked' : '', forceExpanded ? 'expanded' : ''].filter(Boolean).join(' ');
  return (
    <nav className={cls} tabIndex="0" aria-label="Cloudy Pond global nav">
      <div className="nav-logo">
        <div className="mark">logo</div>
        <div className="word">scopeball</div>
      </div>

      <div className="nav-cta">
        <a className="main" href="#">
          <svg className="plus" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M12 5v14M5 12h14"/></svg>
          <span className="label">새 정책</span>
        </a>
        <div className="caret">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg>
        </div>
      </div>

      <div className="nav-ws">
        <span className="ws-av">A</span>
        <div className="ws-label">Acme<span className="sub">4 wallets · 14 policies</span></div>
        <svg className="ws-caret" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg>
      </div>

      <div className="nav-divider"></div>

      <div className="nav-group">
        <a className="nav-item" href="Home.html">
          <span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 11.5 12 4l9 7.5"/><path d="M5 10v10h14V10"/></svg></span>
          <span className="label">Home</span>
        </a>
        <a className="nav-item active" href="#" aria-current="page">
          <span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></svg></span>
          <span className="label">Editor</span>
        </a>
        <a className="nav-item" href="#">
          <span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="9"/><path d="m10 8.5 5 3.5-5 3.5z"/></svg></span>
          <span className="label">Simulation</span>
        </a>
        <a className="nav-item" href="#">
          <span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 12h4l3 8 4-16 3 8h4"/></svg></span>
          <span className="label">Monitoring</span>
        </a>
      </div>

      <div className="nav-divider"></div>

      <div className="nav-group">
        <a className="nav-item" href="#">
          <span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 3v18h18"/><path d="m7 14 4-4 4 3 5-7"/></svg></span>
          <span className="label">Audit</span>
          <span className="badge">12</span>
          <span className="dot-badge"></span>
        </a>
        <a className="nav-item" href="#">
          <span className="icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15c.1.4.3.8.6 1L19 17l-1-1c-.3-.3-.6-.5-1-.6"/></svg></span>
          <span className="label">Settings</span>
        </a>
      </div>

      <div className="nav-bottom">
        <div className="nav-user">
          <span className="av">TY</span>
          <div className="meta">
            <div className="nm">Taeyoon Kim</div>
            <div className="em">ty@scopeball.co</div>
          </div>
        </div>
      </div>

      <div className="nav-kbd-hint">⌘B</div>
    </nav>
  );
}

// ─── TOPBAR (2-row, Builder/Code toggle) ─────────────────────────────────────
function V5Topbar({
  mode, onModeChange, dirty,
  undoEnabled, redoEnabled, onUndo, onRedo, onSave, saveBtnRef,
  category, action, manifestHash, signalCounts, onOpenManifest,
}) {
  return (
    <div className="tb">
      <div className="tb-row top">
        <div className="crumb">
          <span>Scopeball</span>
          <span className="sep">·</span>
          <span className="here">Editor</span>
          <span className="sep">·</span>
          <span className="base">Swap baseline</span>
          {dirty && <span className="dirty" title="미저장 변경" />}
        </div>

        <div className="mode-toggle" role="tablist" aria-label="Editor mode">
          <button role="tab" aria-selected={mode === 'editor'}
            className={`mode-tab ${mode === 'editor' ? 'on' : ''}`}
            onClick={() => onModeChange('editor')}>
            <V5I.shapes />
            <span>Builder</span>
          </button>
          <button role="tab" aria-selected={mode === 'code'}
            className={`mode-tab ${mode === 'code' ? 'on' : ''}`}
            onClick={() => onModeChange('code')}>
            <V5I.code />
            <span>Code</span>
          </button>
        </div>

        <div className="tb-right">
          <button className="tb-icon" disabled={!undoEnabled} onClick={onUndo} title="Undo (⌘Z)"><V5I.undo /></button>
          <button className="tb-icon" disabled={!redoEnabled} onClick={onRedo} title="Redo (⌘⇧Z)"><V5I.redo /></button>
          <div className="tb-vbar" />
          <button ref={saveBtnRef} className={`btn-primary ${dirty ? 'on' : ''}`} onClick={onSave}>
            <span>정책 저장</span>
            <span className="btn-sub">SDK putRaw</span>
          </button>
        </div>
      </div>

      <div className="tb-row bot">
        <div className="tb-pill">
          <span className="tb-k">Category</span>
          <button className="tb-toggle"><span>{category}</span><V5I.caretDown style={{ width: 11, height: 11 }} /></button>
        </div>
        <div className="tb-pill">
          <span className="tb-k">Action</span>
          <button className="tb-toggle"><span>{action}</span><V5I.caretDown style={{ width: 11, height: 11 }} /></button>
        </div>

        <div className="tb-bot-meta" style={{ marginLeft: 'auto' }}>
          <span className="tb-k">Manifest</span>
          <span className="tb-hash">{manifestHash}</span>
          <span className="tb-sigs">
            <span className="tb-sigs-n">{signalCounts.base}</span> 기본
            <span style={{ color: 'var(--cyan-400)' }}>+</span>
            <span className="tb-sigs-n">{signalCounts.custom}</span> enrichment
          </span>
        </div>
        <button className="tb-setting" onClick={onOpenManifest}>
          <V5I.cog style={{ width: 12, height: 12 }} /><span>setting</span><V5I.arrowRight style={{ width: 11, height: 11 }} />
        </button>
      </div>
    </div>
  );
}

// ─── SAVE BAR ────────────────────────────────────────────────────────────────
function V5SaveBar({ dirty, dirtyCount, onAimSave }) {
  return (
    <div className="save-bar">
      <div className="sb-l">
        {dirty ? (
          <span className="sb-dirty"><span className="sb-d" /><span>미저장 변경 {dirtyCount}개 — 우상단 ‘정책 저장’으로 커밋합니다</span></span>
        ) : (
          <span className="sb-clean"><V5I.check style={{ width: 14, height: 14 }} /><span>현재 정책이 마지막 저장본과 일치합니다</span></span>
        )}
      </div>
      <div style={{ display: 'inline-flex', alignItems: 'center', gap: 10 }}>
        <span className="sb-keys">⌘S 저장 · ⌘Z 되돌리기 · ⌘B nav</span>
        {dirty && (<button className="sb-aim" onClick={onAimSave}>저장 버튼 강조</button>)}
      </div>
    </div>
  );
}

Object.assign(window, { V5I, V5NavRail, V5Topbar, V5SaveBar });
