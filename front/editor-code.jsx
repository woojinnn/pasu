// editor-code.jsx
// Code mode — Cedar text rendering, read-only by default with opt-in edit.
// Lines tagged with `guardId` highlight in sync with focused guard / matches.

const { useState: useStateCode } = React;

function tokenizeCedarLine(text) {
  // Tiny tokenizer for visual highlighting only.
  // Order matters; we walk the string and pick the first matching pattern.
  const patterns = [
    { re: /^\/\/[^\n]*/, cls: 'cd-comment' },
    { re: /^"[^"]*"/,     cls: 'cd-str' },
    { re: /^\b(forbid|permit|when|unless|principal|action|resource|has|in|like)\b/, cls: 'cd-kw' },
    { re: /^\b(true|false)\b/, cls: 'cd-bool' },
    { re: /^\b\d+(\.\d+)?\b/, cls: 'cd-num' },
    { re: /^(==|!=|<=|>=|<|>|&&|\|\||::)/, cls: 'cd-op' },
    { re: /^\b(context|principal|action|resource)\b/, cls: 'cd-ctx' },
    { re: /^\b[a-zA-Z_][a-zA-Z0-9_]*\b/, cls: 'cd-id' },
    { re: /^[(){}[\],;]/, cls: 'cd-punct' },
    { re: /^\s+/, cls: 'cd-ws' },
    { re: /^./, cls: 'cd-other' },
  ];
  const out = [];
  let i = 0;
  while (i < text.length) {
    const rest = text.slice(i);
    let matched = false;
    for (const p of patterns) {
      const m = rest.match(p.re);
      if (m) {
        out.push({ t: m[0], c: p.cls });
        i += m[0].length;
        matched = true;
        break;
      }
    }
    if (!matched) { out.push({ t: rest[0], c: 'cd-other' }); i++; }
  }
  return out;
}

function CedarLine({ line, focused, matched, dimmed, animateFlash }) {
  const tokens = tokenizeCedarLine(line.text);
  // We split off inline comment so we can color it dim.
  return (
    <div className={`cd-line ${focused ? 'cd-line-focus' : ''} ${matched ? 'cd-line-match' : ''} ${dimmed ? 'cd-line-dim' : ''} ${animateFlash ? 'cd-line-flash' : ''}`}>
      <span className="cd-gutter">{line.n}</span>
      <span className="cd-text">
        {tokens.map((tk, i) => <span key={i} className={tk.c}>{tk.t}</span>)}
      </span>
      {line.kind === 'guard' && line.guardId && (
        <span className={`cd-guard-tag ${line.custom ? 'cd-guard-tag-c' : ''}`}>{line.guardId}</span>
      )}
    </div>
  );
}

function CodeView({ policy, focusedGuard, matchedGuards, skewedGuard, editable, onToggleEditable }) {
  const lines = CEDAR_TEXT.lines;
  return (
    <div className="cd-view" data-screen-label="Code mode (Cedar)">
      <div className="cd-toolbar">
        <span className="cd-toolbar-l">
          <span className="cd-lang">Cedar</span>
          <span className="cd-toolbar-sep">·</span>
          <span className="cd-status">
            {editable ? (
              <span className="cd-status-edit"><span className="cd-status-dot" /> 직접 편집 중</span>
            ) : (
              <span className="cd-status-ro">읽기전용 (Builder/Block과 동기화)</span>
            )}
          </span>
        </span>
        <span className="cd-toolbar-r">
          <button className="cd-btn">복사</button>
          {!editable ? (
            <button className="cd-btn cd-btn-warn" onClick={() => onToggleEditable(true)}>
              <I.edit style={{ width: 12, height: 12 }} /> 직접 편집
            </button>
          ) : (
            <button className="cd-btn" onClick={() => onToggleEditable(false)}>
              읽기전용으로
            </button>
          )}
        </span>
      </div>

      {editable && (
        <div className="cd-warn-bar">
          <I.warn style={{ width: 13, height: 13 }} />
          <span>직접 편집한 내용은 Builder · Block 모드로 복귀하면 폐기됩니다 (단방향).</span>
        </div>
      )}

      <div className="cd-scroll">
        {lines.map(line => {
          const focused = focusedGuard && line.guardId === focusedGuard;
          const matched = matchedGuards && matchedGuards.includes(line.guardId);
          const dimmed = focusedGuard && line.kind === 'guard' && line.guardId !== focusedGuard;
          return (
            <CedarLine
              key={line.n}
              line={line}
              focused={focused}
              matched={matched}
              dimmed={dimmed}
              animateFlash={false}
            />
          );
        })}
      </div>

      <div className="cd-foot">
        <span className="cd-foot-meta">15 lines · 4 guards · manifest #fc20a91</span>
        <span style={{ flex: 1 }} />
        <span className="cd-foot-meta">utf-8 · LF</span>
      </div>
    </div>
  );
}

Object.assign(window, { CodeView });
