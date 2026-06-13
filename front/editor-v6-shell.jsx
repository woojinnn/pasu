// editor-v6-shell.jsx — v6 icons (tree/inspector/modal) + Topbar with breadcrumb.
// Reuses V5I, V5NavRail, V5SaveBar from editor-v5-shell.jsx (loaded first).

const V6I = {
  folder:     (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M1.8 4.2c0-.66.54-1.2 1.2-1.2h2.5l1.3 1.4h5.4c.66 0 1.2.54 1.2 1.2v6c0 .66-.54 1.2-1.2 1.2H3c-.66 0-1.2-.54-1.2-1.2V4.2z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/></svg>,
  folderOpen: (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M1.8 4.2c0-.66.54-1.2 1.2-1.2h2.5l1.3 1.4h5.4c.66 0 1.2.54 1.2 1.2v1.2H1.8V4.2z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/><path d="M1.8 6.8h12.4l-1.3 5c-.1.5-.5.8-1 .8H3.1c-.5 0-.9-.3-1-.8l-1.3-5z" fill="currentColor" fillOpacity="0.12" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/></svg>,
  file:       (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M4 1.8h5l3 3v9.4c0 .33-.27.6-.6.6H4c-.33 0-.6-.27-.6-.6V2.4c0-.33.27-.6.6-.6z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/><path d="M9 1.8v3h3" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/></svg>,
  lock:       (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><rect x="3.2" y="7" width="9.6" height="6.5" rx="1.3" stroke="currentColor" strokeWidth="1.3"/><path d="M5.2 7V5.3a2.8 2.8 0 0 1 5.6 0V7" stroke="currentColor" strokeWidth="1.3"/></svg>,
  edit2:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M8 13.5h6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/><path d="M10.5 2.8l2.7 2.7L6 12.7l-3.2.5.5-3.2 7.2-7.2z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/></svg>,
  filePlus:   (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3.6 2h4.4l2.8 2.8V9" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/><path d="M8 2v3h3" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/><path d="M3.6 2.6v10.8c0 .3.27.6.6.6h3.2" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/><path d="M11.5 11v4M9.5 13h4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>,
  folderPlus: (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M1.8 4.2c0-.66.54-1.2 1.2-1.2h2.5l1.3 1.4h5.4c.66 0 1.2.54 1.2 1.2V8" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/><path d="M1.8 6.4v4.6c0 .66.54 1.2 1.2 1.2h4.3" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/><path d="M11.5 10.5v4M9.5 12.5h4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>,
  sync:       (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M13 8a5 5 0 1 1-1.4-3.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/><path d="M13 2.2V5h-2.8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  collapseAll:(p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M5 6l3-2.5L11 6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/><path d="M5 10l3 2.5L11 10" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  expandAll:  (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M5 4.5l3 2.5 3-2.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/><path d="M5 11.5l3-2.5 3 2.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  grab:       (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><circle cx="6" cy="4" r="1" fill="currentColor"/><circle cx="10" cy="4" r="1" fill="currentColor"/><circle cx="6" cy="8" r="1" fill="currentColor"/><circle cx="10" cy="8" r="1" fill="currentColor"/><circle cx="6" cy="12" r="1" fill="currentColor"/><circle cx="10" cy="12" r="1" fill="currentColor"/></svg>,
  palette:    (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M8 1.6c3.5 0 6.4 2.6 6.4 5.8 0 2-1.7 3.2-3.3 3.2H9.7c-.6 0-1 .5-1 1 0 .3.16.5.3.7.16.23.3.46.3.8 0 .6-.5 1-1.3 1C4.9 15 1.6 11.8 1.6 7.8 1.6 4.3 4.5 1.6 8 1.6z" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round"/><circle cx="5" cy="7" r="0.9" fill="currentColor"/><circle cx="7.6" cy="4.6" r="0.9" fill="currentColor"/><circle cx="10.6" cy="5.4" r="0.9" fill="currentColor"/></svg>,
  trash:      (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M3 4.2h10M6.4 4.2V3c0-.4.3-.7.7-.7h1.8c.4 0 .7.3.7.7v1.2M4.4 4.2l.5 8.4c0 .4.4.8.8.8h4.6c.4 0 .8-.4.8-.8l.5-8.4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  wrap:       (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><rect x="2" y="3" width="12" height="10" rx="2" stroke="currentColor" strokeWidth="1.3" strokeDasharray="2.4 2"/><rect x="5" y="6" width="6" height="4" rx="1" fill="currentColor" fillOpacity="0.18" stroke="currentColor" strokeWidth="1.2"/></svg>,
  focusIn:    (p) => <svg viewBox="0 0 16 16" fill="none" {...p}><path d="M2 5.5V3.4c0-.5.4-.9.9-.9H5M11 2.5h2.1c.5 0 .9.4.9.9V5.5M14 10.5v2.1c0 .5-.4.9-.9.9H11M5 13.5H2.9c-.5 0-.9-.4-.9-.9V10.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/><circle cx="8" cy="8" r="1.7" fill="currentColor"/></svg>,
};

// ─── TOPBAR (breadcrumb only on the lower line) ──────────────────────────────
function V6Topbar({
  mode, onModeChange, dirty, dirtyCount,
  undoEnabled, redoEnabled, onUndo, onRedo, onSave, saveBtnRef, onAimSave,
  breadcrumb, onCrumbClick,
  onAddLogic, showPolicy, showCedar, onTogglePolicy, onToggleCedar,
}) {
  const crumb = breadcrumb || ['actions', 'amm', 'swap.cedarschema'];
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
          {dirty && (
            <button className="tb-unsaved" onClick={onAimSave} title="미저장 변경 — 클릭 시 저장 버튼 강조">
              <span className="tb-unsaved-dot" />
              <span>미저장 {dirtyCount}</span>
            </button>
          )}
          <button ref={saveBtnRef} className={`btn-primary ${dirty ? 'on' : ''}`} onClick={onSave}>
            <span>정책 저장</span>
            <span className="btn-sub">SDK putRaw</span>
          </button>
        </div>
      </div>

      <div className="tb-row bot">
        <nav className="bc" aria-label="schema path">
          {crumb.map((seg, i) => {
            const isLeaf = i === crumb.length - 1;
            const isFile = /\.cedarschema$/.test(seg);
            return (
              <React.Fragment key={i}>
                {i > 0 && <span className="bc-sep">/</span>}
                <button
                  className={`bc-seg ${isLeaf ? 'leaf' : ''}`}
                  onClick={() => onCrumbClick && onCrumbClick(seg, i)}
                  title={isLeaf ? '트리에서 이 파일로 점프' : `${seg} 폴더로 점프`}
                >
                  <span className="bc-ic">
                    {isFile ? <V6I.file /> : (isLeaf ? <V6I.file /> : <V6I.folder />)}
                  </span>
                  <span>{seg}</span>
                </button>
              </React.Fragment>
            );
          })}
        </nav>

        <span className="tb-bot-spc" />

        {mode === 'editor' && (
          <div className="tb-logic">
            <span className="tb-logic-lab">논리 묶음</span>
            <button className="tb-lt" onClick={() => onAddLogic && onAddLogic('AND')} title="AND 그룹 추가">AND</button>
            <button className="tb-lt or" onClick={() => onAddLogic && onAddLogic('OR')} title="OR 그룹 추가">OR</button>
            <button className="tb-lt not" onClick={() => onAddLogic && onAddLogic('NOT')} title="NOT 그룹 추가">NOT</button>
          </div>
        )}

        {mode === 'editor' && <div className="tb-bot-div" />}

        {mode === 'editor' && (
          <div className="tb-panels" role="group" aria-label="우측/하단 패널 토글">
            <button className={`tb-ptoggle ${showPolicy ? 'on' : ''}`} aria-pressed={!!showPolicy} onClick={onTogglePolicy}>Policy test</button>
            <button className={`tb-ptoggle ${showCedar ? 'on' : ''}`} aria-pressed={!!showCedar} onClick={onToggleCedar}>Live Cedar</button>
          </div>
        )}

        <div className="tb-bot-div" />

        <div className="tb-help">
          <button className="tb-help-btn" aria-label="단축키 도움말">?</button>
          <div className="tb-help-pop" role="tooltip">
            <div className="thp-t">단축키</div>
            <ul className="thp-list">
              <li><kbd>⌘S</kbd><span>정책 저장</span></li>
              <li><kbd>⌘Z</kbd><span>되돌리기</span></li>
              <li><kbd>⌘⇧Z</kbd><span>다시 실행</span></li>
              <li><kbd>⌘B</kbd><span>Nav 펼침 / 접기</span></li>
              <li><kbd>Esc</kbd><span>선택 해제 · 모달 닫기</span></li>
              <li><kbd>휠</kbd><span>줌 인 / 아웃</span></li>
              <li><kbd>Space+드래그</kbd><span>캔버스 팬</span></li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { V6I, V6Topbar });
