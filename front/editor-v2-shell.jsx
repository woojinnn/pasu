// editor-v2-shell.jsx
// Topbar (2-row), nav rail (free-rolling), FAB, save bar primitives.

const { useState: useStateShell, useEffect: useEffectShell, useRef: useRefShell } = React;

// ─── icon set (shared globally via window.V2I) ──────────────────────────────
const V2I = {
  caretDown: (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  caretRight:(p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
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
  bolt:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M9 1L3 9h4l-1 6 6-8H8l1-6z" fill="currentColor"/></svg>,
  home:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M2 7l6-5 6 5M3.5 6.5v7h9v-7" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  blocks:    (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><rect x="2" y="2" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.4"/><rect x="9" y="2" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.4"/><rect x="2" y="9" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.4"/><rect x="9" y="9" width="5" height="5" rx="1" stroke="currentColor" strokeWidth="1.4"/></svg>,
  library:   (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3 3v10M6 3v10M10 4l3 9M11 4l3 9" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/></svg>,
  audit:     (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3 2v12h11" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/><path d="M5 10l3-3 2 2 3-4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  code:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M5.5 5L2.5 8l3 3M10.5 5l3 3-3 3M9.5 4l-3 8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  shapes:    (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><rect x="2" y="2" width="6" height="6" rx="1" stroke="currentColor" strokeWidth="1.4"/><circle cx="11.5" cy="11.5" r="2.5" stroke="currentColor" strokeWidth="1.4"/></svg>,
  panel:     (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><rect x="1.5" y="2.5" width="13" height="11" rx="1.5" stroke="currentColor" strokeWidth="1.4"/><path d="M5.5 2.5v11" stroke="currentColor" strokeWidth="1.4"/></svg>,
  q:         (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><circle cx="8" cy="8" r="6.5" stroke="currentColor" strokeWidth="1.4"/><path d="M6 6.2c0-1.1 0.9-2 2-2s2 0.9 2 2c0 0.8-0.5 1.2-1.2 1.6-0.5 0.3-0.8 0.5-0.8 1.2" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/><circle cx="8" cy="11.5" r="0.6" fill="currentColor"/></svg>,
  book:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3 3.5h6a2.5 2.5 0 0 1 2.5 2.5v8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/><path d="M14 3.5H8a2.5 2.5 0 0 0-2.5 2.5v8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  kbd:       (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><rect x="1" y="4" width="14" height="8" rx="1.5" stroke="currentColor" strokeWidth="1.4"/><path d="M4 7h.5M7 7h.5M10 7h.5M4 10h7" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/></svg>,
  bell:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M4 11.5h8L11 9.5V7a3 3 0 1 0-6 0v2.5L4 11.5zM7 13.5h2" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  msg:       (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M2 4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H6l-3 2.5V11H3a1 1 0 0 1-1-1V4z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round"/></svg>,
};

// ─── Nav rail (free-rolling, no lock) ───────────────────────────────────────
function V2NavRail({ collapsed }) {
  return (
    <nav className={`nav-rail-v2 ${collapsed ? 'collapsed' : ''}`} aria-label="Global navigation">
      <div className="nv-logo">
        <div className="nv-mark">SB</div>
        <div className="nv-word">Scopeball</div>
      </div>

      <div className="nv-ws">
        <span className="nv-ws-dot" />
        <span className="nv-ws-l">acme · prod</span>
        <V2I.caretDown style={{ width: 10, height: 10, color: 'var(--slate-400)' }} />
      </div>

      <div className="nv-section">Workspace</div>
      <div className="nv-group">
        <a className="nv-item" href="Home.html">
          <V2I.home className="nv-ic" />
          <span className="nv-lbl">Home</span>
        </a>
        <a className="nv-item active" href="#" aria-current="page">
          <V2I.blocks className="nv-ic" />
          <span className="nv-lbl">Editor</span>
        </a>
        <a className="nv-item" href="#">
          <V2I.library className="nv-ic" />
          <span className="nv-lbl">Library</span>
          <span className="nv-badge">24</span>
        </a>
      </div>

      <div className="nv-section">Operations</div>
      <div className="nv-group">
        <a className="nv-item" href="#">
          <V2I.audit className="nv-ic" />
          <span className="nv-lbl">Audit</span>
          <span className="nv-badge">3</span>
        </a>
        <a className="nv-item" href="#">
          <V2I.cog className="nv-ic" />
          <span className="nv-lbl">Settings</span>
        </a>
      </div>

      <div className="nv-bottom">
        <div className="nv-user">
          <span className="nv-av">TY</span>
          <div className="nv-um">
            <span className="nv-un">Taeyun Park</span>
            <span className="nv-uo">security.acme</span>
          </div>
        </div>
      </div>
    </nav>
  );
}

// ─── 2-row Topbar ───────────────────────────────────────────────────────────
function V2Topbar({
  mode, onModeChange,
  dirty, undoEnabled, redoEnabled, onUndo, onRedo, onSave,
  category, action, manifestHash, signalCounts, onOpenManifest,
  saveButtonRef, dirtyCount,
}) {
  return (
    <div className="tb">
      {/* TOP ROW */}
      <div className="tb-row tb-row-top">
        <div className="tb-crumb">
          <span>Scopeball</span>
          <span className="tb-crumb-sep">·</span>
          <span className="tb-crumb-here">Editor</span>
          <span className="tb-crumb-sep">·</span>
          <span className="tb-crumb-base">Swap baseline</span>
          {dirty && <span className="tb-crumb-dirty" title="미저장 변경" />}
        </div>

        <div className="tb-mode" role="tablist" aria-label="Editor mode">
          <button role="tab" aria-selected={mode === 'editor'}
            className={`tb-mode-tab ${mode === 'editor' ? 'on' : ''}`}
            onClick={() => onModeChange('editor')}>
            <V2I.shapes className="mt-ic" />
            <span>Editor</span>
          </button>
          <button role="tab" aria-selected={mode === 'code'}
            className={`tb-mode-tab ${mode === 'code' ? 'on' : ''}`}
            onClick={() => onModeChange('code')}>
            <V2I.code className="mt-ic" />
            <span>Code</span>
          </button>
        </div>

        <div className="tb-right">
          <button className="tb-icon" disabled={!undoEnabled} onClick={onUndo} title="Undo (⌘Z)"><V2I.undo /></button>
          <button className="tb-icon" disabled={!redoEnabled} onClick={onRedo} title="Redo (⌘⇧Z)"><V2I.redo /></button>
          <div className="tb-vbar" />
          <button
            ref={saveButtonRef}
            className={`btn-primary ${dirty ? 'on' : ''}`}
            onClick={onSave}
            title="정책 저장 (SDK putRaw)"
          >
            <span>정책 저장</span>
            <span className="btn-sub">SDK putRaw</span>
          </button>
        </div>
      </div>

      {/* BOTTOM ROW */}
      <div className="tb-row tb-row-bot">
        <div className="tb-pill">
          <span className="tb-k">Category</span>
          <button className="tb-toggle">
            <span>{category}</span>
            <V2I.caretDown style={{ width: 11, height: 11 }} />
          </button>
        </div>
        <div className="tb-pill">
          <span className="tb-k">Action</span>
          <button className="tb-toggle">
            <span>{action}</span>
            <V2I.caretDown style={{ width: 11, height: 11 }} />
          </button>
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
          <V2I.cog style={{ width: 12, height: 12 }} />
          <span>setting</span>
          <V2I.arrowRight style={{ width: 11, height: 11 }} />
        </button>
      </div>
    </div>
  );
}

// ─── Save bar (state-only) ──────────────────────────────────────────────────
function V2SaveBar({ dirty, dirtyCount, onAimSave }) {
  return (
    <div className="save-bar">
      <div className="sb-l">
        {dirty ? (
          <span className="sb-dirty">
            <span className="sb-dot" />
            <span>미저장 변경 {dirtyCount}개 — 상단의 ‘정책 저장’으로 커밋합니다</span>
          </span>
        ) : (
          <span className="sb-clean">
            <V2I.check className="sb-check" style={{ width: 14, height: 14 }} />
            <span>현재 정책이 마지막 저장본과 일치합니다</span>
          </span>
        )}
      </div>
      <div className="sb-r">
        <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 11, color: 'var(--slate-400)' }}>
          ⌘S 저장 · ⌘Z 되돌리기
        </span>
        {dirty && (
          <button className="sb-aim" onClick={onAimSave} title="우상단 저장 버튼으로 시선 유도">
            저장 버튼 강조
          </button>
        )}
      </div>
    </div>
  );
}

// ─── Global FAB ──────────────────────────────────────────────────────────────
function V2FAB({ theme, onThemeChange }) {
  const [open, setOpen] = useStateShell(false);
  const popRef = useRefShell(null);

  useEffectShell(() => {
    if (!open) return;
    const onDocClick = (e) => {
      if (popRef.current && !popRef.current.contains(e.target)) {
        // also ignore clicks on the FAB button itself
        if (!e.target.closest('.fab-btn')) setOpen(false);
      }
    };
    window.addEventListener('click', onDocClick);
    return () => window.removeEventListener('click', onDocClick);
  }, [open]);

  return (
    <>
      <button className={`fab-btn ${open ? 'open' : ''}`} onClick={() => setOpen(!open)} aria-label="Help & utilities">
        {open ? <V2I.x style={{ width: 18, height: 18 }} /> : <V2I.q style={{ width: 22, height: 22 }} />}
      </button>
      {open && (
        <div className="fab-pop" ref={popRef} onClick={(e) => e.stopPropagation()}>
          <div className="fab-head">Documentation</div>
          <div className="fab-row">
            <V2I.book className="fab-ic" />
            <span>문서 · 가이드</span>
            <span className="fab-r">↗</span>
          </div>
          <div className="fab-row">
            <V2I.kbd className="fab-ic" />
            <span>단축키</span>
            <span className="fab-r">⌘/</span>
          </div>
          <div className="fab-row">
            <V2I.bell className="fab-ic" />
            <span>새 소식</span>
            <span className="fab-r" style={{ background: 'var(--fail-100)', color: 'var(--fail-700)' }}>2</span>
          </div>

          <div className="fab-div" />
          <div className="fab-head">Appearance</div>
          <div className="fab-row theme">
            <V2I.shapes className="fab-ic" />
            <span>테마</span>
            <div className="fab-theme-seg" onClick={(e) => e.stopPropagation()}>
              {['light', 'dark', 'system'].map(t => (
                <button key={t} className={theme === t ? 'on' : ''} onClick={() => onThemeChange(t)}>
                  {t === 'light' ? '밝게' : t === 'dark' ? '어둡게' : '시스템'}
                </button>
              ))}
            </div>
          </div>

          <div className="fab-div" />
          <div className="fab-head">Feedback</div>
          <div className="fab-row">
            <V2I.msg className="fab-ic" />
            <span>의견 보내기</span>
          </div>
        </div>
      )}
    </>
  );
}

Object.assign(window, { V2I, V2NavRail, V2Topbar, V2SaveBar, V2FAB });
