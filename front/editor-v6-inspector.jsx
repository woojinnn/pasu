// editor-v6-inspector.jsx — block style inspector (color / shape / tag).
// Color = 3 policy-meaning signal tones (차단/경고/정상) + custom fill/border picker.
// Selecting a canvas block opens this right-docked panel; edits serialize to JSON.

const V6_TONE_HEX = {
  fail: { fill: '#F4D9D5', border: '#C77268' },
  warn: { fill: '#FBF2DF', border: '#D69238' },
  pass: { fill: '#E4EFE1', border: '#7CB069' },
};
const V6_TONE_ORDER = ['fail', 'warn', 'pass'];
const V6_TONE_DESC = {
  fail: '위반 시 거래를 막는 가드',
  warn: '검토가 필요한 조건',
  pass: '허용·통과 경로',
};

function V6BlockInspector({ node, dispatch, onClose }) {
  if (!node) return null;
  const style = node.style || v6DefaultStyle();
  const isGroup = node.kind === 'group';
  const domain = v6DomainOf(node);
  const r = v6ResolveStyle(style, domain);
  const blockName = isGroup ? `${node.combinator} 묶음` : (node.label || node.sigId || 'block');

  const set = (patch) => dispatch({ type: 'SET_BLOCK_STYLE', id: node.id, patch });

  return (
    <div className="insp-dock" style={{ '--bk-fill': r.fill, '--bk-border': r.border }}>
      <div className="insp-h">
        <span className="insp-sw" />
        <div className="insp-meta">
          <div className="insp-eye">Block style · {node.id}</div>
          <div className="insp-name">{blockName}</div>
        </div>
        <button className="insp-x" onClick={onClose} title="닫기"><V5I.x style={{ width: 12, height: 12 }} /></button>
      </div>

      <div className="insp-body">
        {/* color = domain identity (auto, fixed) · severity = status accent */}
        <div>
          <div className="insp-sec-t">색상 <span className="sub">자동 · 고정</span></div>
          <div className="insp-domain">
            <span className="insp-dom-sw" />
            <div className="insp-dom-meta">
              <div className="insp-dom-name">{r.domainLabel} <span className="insp-dom-key">{domain}</span></div>
              <div className="insp-dom-note">경로의 도메인이 색을 정합니다 — 블록 색은 status가 아니라 정체성입니다.</div>
            </div>
          </div>

          <div className="insp-sec-t" style={{ marginTop: 14 }}>심각도 <span className="sub">엣지 + pill</span></div>
          <div className="insp-tones four">
            <button className={`insp-tone ${!style.tone ? 'on' : ''}`} onClick={() => set({ tone: null })}>
              <span className="tsw none" />
              <span className="tlab">없음</span>
            </button>
            {V6_TONE_ORDER.map(t => (
              <button key={t} className={`insp-tone ${style.tone === t ? 'on' : ''}`} onClick={() => set({ tone: t })}>
                <span className={`tsw sev ${t}`} />
                <span className="tlab">{V6_TONES[t].label}</span>
              </button>
            ))}
          </div>
          <div className="insp-legend">
            {V6_TONE_ORDER.map(t => (
              <div key={t} className="insp-leg-row"><span className={`insp-leg-sw ${t}`} /><span><b>{V6_TONES[t].label}</b> · {V6_TONE_DESC[t]}</span></div>
            ))}
          </div>
        </div>

        {/* shape */}
        <div>
          <div className="insp-sec-t">모양 <span className="sub">silhouette</span></div>
          <div className="insp-shapes">
            {V6_SHAPES.map(sh => (
              <button key={sh} className={`insp-shape ${r.shape === sh ? 'on' : ''}`} onClick={() => set({ shape: sh })}>
                <span className={`ssw ${sh}`} />
                <span className="slab">{V6_SHAPE_LABEL[sh]}</span>
                <span className="shint">{sh === 'diamond' ? '조건' : sh === 'hex' ? '논리' : sh === 'rect' ? '기본' : '·'}</span>
              </button>
            ))}
          </div>
        </div>

        {/* tag */}
        <div>
          <div className="insp-sec-t">라벨 · 태그 <span className="sub">블록 상단 표시</span></div>
          <div className="insp-tag-input">
            <input value={style.tag || ''} maxLength={18}
              placeholder="예: 차단, hot-path, v2…"
              onChange={(e) => set({ tag: e.target.value || null })} />
            {style.tag && <button className="reset insp-color-row" onClick={() => set({ tag: null })} style={{ padding: '6px 9px' }}>지움</button>}
          </div>
        </div>

        {/* JSON metadata */}
        <div>
          <div className="insp-sec-t">JSON 메타데이터 <span className="sub">직렬화</span></div>
          <div className="insp-json">
            <div><span className="jk">"style"</span>: {'{'}</div>
            <div>&nbsp;&nbsp;<span className="jk">"domain"</span>: <span className="js">"{domain}"</span>,</div>
            <div>&nbsp;&nbsp;<span className="jk">"severity"</span>: {style.tone ? <span className="js">"{style.tone}"</span> : <span className="jv">null</span>},</div>
            <div>&nbsp;&nbsp;<span className="jk">"shape"</span>: <span className="js">"{r.shape}"</span>,</div>
            <div>&nbsp;&nbsp;<span className="jk">"tag"</span>: {style.tag ? <span className="js">"{style.tag}"</span> : <span className="jv">null</span>}</div>
            <div>{'}'}</div>
          </div>
        </div>
      </div>

      <div className="insp-foot">
        <button className="reset-all" onClick={() => set({ ...(isGroup ? v6GroupStyle() : v6DefaultStyle()) })}>기본값</button>
        <span className="spc" />
        <button className="done" onClick={onClose}>완료</button>
      </div>
    </div>
  );
}

Object.assign(window, { V6BlockInspector, V6_TONE_HEX });
