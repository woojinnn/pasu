// editor-shell.jsx
// Topbar + context bar + nav-rail-lock-collapsed primitives.

const { useState, useEffect, useRef } = React;

// ─── tiny icon set ──────────────────────────────────────────────────────────
const I = {
  caretDown: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  search: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.5" />
      <path d="M10.5 10.5L14 14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  ),
  play: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M5 3.5v9l7-4.5-7-4.5z" fill="currentColor" />
    </svg>
  ),
  undo: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M3 7.5h7a3 3 0 0 1 0 6H6.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M5.5 5L3 7.5 5.5 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  redo: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M13 7.5H6a3 3 0 0 0 0 6h3.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M10.5 5L13 7.5 10.5 10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  collapse: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M2.5 8h11M5.5 5l-3 3 3 3M10.5 5l3 3-3 3" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  block: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <rect x="2" y="3" width="5" height="4" rx="1" stroke="currentColor" strokeWidth="1.4" />
      <rect x="9" y="3" width="5" height="4" rx="1" stroke="currentColor" strokeWidth="1.4" />
      <rect x="2" y="9" width="12" height="4" rx="1" stroke="currentColor" strokeWidth="1.4" />
    </svg>
  ),
  builder: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M2.5 4h11M2.5 8h11M2.5 12h7" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  ),
  code: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M5.5 5L2.5 8l3 3M10.5 5l3 3-3 3M9.5 4l-3 8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  edit: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M11 3l2 2-7.5 7.5H3.5V10L11 3z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
    </svg>
  ),
  cog: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <circle cx="8" cy="8" r="2.2" stroke="currentColor" strokeWidth="1.4" />
      <path d="M8 1.5v2M8 12.5v2M14.5 8h-2M3.5 8h-2M12.6 3.4l-1.4 1.4M4.8 11.2l-1.4 1.4M12.6 12.6l-1.4-1.4M4.8 4.8L3.4 3.4" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  ),
  warn: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M8 2.5L14 13H2L8 2.5z" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
      <path d="M8 6.5v3M8 11.2v.6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  ),
  check: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M3.5 8.5l3 3 6-7" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  x: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  ),
  arrowRight: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M3 8h10M9 4l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  plus: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <path d="M8 3v10M3 8h10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  ),
  dots: (props) => (
    <svg viewBox="0 0 16 16" fill="none" {...props}>
      <circle cx="3.5" cy="8" r="1.2" fill="currentColor" />
      <circle cx="8" cy="8" r="1.2" fill="currentColor" />
      <circle cx="12.5" cy="8" r="1.2" fill="currentColor" />
    </svg>
  ),
};

// ─── Topbar ──────────────────────────────────────────────────────────────────
function EditorTopbar({ mode, onModeChange, dirty, undoEnabled, locale, onTriggerSave }) {
  const modes = [
    { id: 'block',   label: 'Block',   icon: I.block,   sub: '시각' },
    { id: 'builder', label: 'Builder', icon: I.builder, sub: '폼' },
    { id: 'code',    label: 'Code',    icon: I.code,    sub: 'Cedar' },
  ];
  return (
    <div className="ed-topbar">
      <div className="crumb">
        <span style={{ color: 'var(--slate-400)' }}>Scopeball</span>
        <span className="sep">·</span>
        <span className="here">Editor</span>
        <span className="sep">·</span>
        <span style={{ color: 'var(--slate-700)', fontSize: 14 }}>Swap baseline</span>
        {dirty && <span className="dirty-dot" title="미저장 변경" />}
      </div>

      <div className="mode-toggle" role="tablist" aria-label="Editor mode">
        {modes.map(m => {
          const Ic = m.icon;
          const active = mode === m.id;
          return (
            <button
              key={m.id}
              role="tab"
              aria-selected={active}
              className={`mode-tab ${active ? 'on' : ''}`}
              onClick={() => onModeChange(m.id)}
            >
              <Ic className="mt-ic" />
              <span className="mt-lbl">{m.label}</span>
              <span className="mt-sub">{m.sub}</span>
            </button>
          );
        })}
      </div>

      <div className="topbar-right">
        <button className="icon-btn-sq" disabled={!undoEnabled} title="Undo (⌘Z)">
          <I.undo />
        </button>
        <button className="icon-btn-sq" disabled title="Redo (⌘⇧Z)">
          <I.redo />
        </button>
        <div className="vbar" />
        <button className="btn-secondary" onClick={() => {}}>
          <I.collapse style={{ width: 14, height: 14 }} />
          <span>Nav lock</span>
        </button>
        <button className={`btn-primary ${dirty ? 'on' : ''}`} onClick={onTriggerSave}>
          <span>정책 저장</span>
          <span className="btn-sub">SDK putRaw</span>
        </button>
      </div>
    </div>
  );
}

// ─── Context bar — action + signal palette summary ───────────────────────────
function ContextBar({ policy, onOpenManifest }) {
  return (
    <div className="ctx-bar">
      <div className="ctx-block">
        <span className="ctx-k">Action</span>
        <button className="ctx-action">
          <span>swap</span>
          <I.caretDown style={{ width: 12, height: 12, color: 'var(--slate-400)' }} />
        </button>
      </div>

      <div className="ctx-sep" />

      <div className="ctx-block">
        <span className="ctx-k">신호</span>
        <span className="ctx-v">
          <span className="sig-pill sig-base">
            <span className="sig-n">{policy.signalCounts.base}</span> 기본
          </span>
          <span className="ctx-plus">+</span>
          <span className="sig-pill sig-custom">
            <span className="sig-n">{policy.signalCounts.custom}</span> enrichment
          </span>
        </span>
      </div>

      <div className="ctx-sep" />

      <div className="ctx-block">
        <span className="ctx-k">Manifest</span>
        <span className="ctx-hash">{policy.manifestHash}</span>
      </div>

      <div className="ctx-right">
        <button className="ctx-edit" onClick={onOpenManifest}>
          <I.edit style={{ width: 13, height: 13 }} />
          <span>신호 편집</span>
          <I.arrowRight style={{ width: 13, height: 13, marginLeft: 2 }} />
        </button>
      </div>
    </div>
  );
}

// ─── Save bar at bottom of editing pane ──────────────────────────────────────
function SaveBar({ dirty, decision, onSave }) {
  return (
    <div className="save-bar">
      <div className="sb-left">
        <span className="sb-deci">
          <span className="sb-deci-kw">Deny</span>
          <span className="sb-deci-reason">"{decision.reason}"</span>
          <span className="sb-deci-sev">severity: {decision.severity}</span>
        </span>
      </div>
      <div className="sb-right">
        {dirty && <span className="sb-dirty"><span className="sb-dot" /> 미저장 변경 3개</span>}
        <button className="btn-secondary">Cedar 내보내기</button>
        <button className="btn-primary on" onClick={onSave}>
          정책 저장 <span className="btn-sub">SDK putRaw</span>
        </button>
      </div>
    </div>
  );
}

Object.assign(window, { I, EditorTopbar, ContextBar, SaveBar });
