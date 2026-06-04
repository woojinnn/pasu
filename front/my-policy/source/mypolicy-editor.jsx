// mypolicy-editor.jsx — 에디터 (폼 / 블록 / Cedar) · v4
// 상단 NLG 배너(유일한 읽어주기 위치) + 좌/우 분할(splitter, 최대폭 캡·가운데 정렬은 CSS)
//   폼·블록 → 좌 편집 / 우 Cedar 코드(편집 가능 · 라이브 양방향 동기화)
//   Cedar   → 코드 에디터가 주인공 + manifest 보조(접기)

const { useState: useSE, useMemo: useME, useRef: useRE, useEffect: useEE } = React;

const ROLE_ICON_MP = { numeric: MPI.hash, address: MPI.key, ref: MPI.token, enum: MPI.shield, auth: MPI.clock, misc: MPI.dot };

function cedarOp(op) { return ({ ">": ">", "≥": ">=", "<": "<", "≤": "<=", "≠": "!=", "=": "==", "= 참": "== true", "∌": "∌" })[op] || op; }
function modelOp(op) { return ({ ">": ">", ">=": "≥", "<": "<", "<=": "≤", "!=": "≠", "==": "=" })[op] || op; }
function cedarVal(v) { if (v == null) return ""; if (/^@/.test(v)) return v.slice(1); return v; }
function chipFor(c) { return `${c.canon} ${cedarOp(c.op)} ${cedarVal(c.val)}`; }

function predExpr(c) { return `${c.canon} ${cedarOp(c.op)} ${cedarVal(c.val)}`.trim(); }

// ── 폼 모델 → Cedar 텍스트 (single-level OR 지원) ──
function buildCedarText(detail, model) {
  const ent = detail.actionEnt || "Action";
  const id = detail.meta.id;
  const out = [];
  out.push(`// ${id} · forbid policy`);
  out.push("forbid (");
  out.push(`  principal, action == Action::"${ent}", resource`);
  out.push(") when {");
  (model.conds || []).forEach((c, i) => {
    const lead = i > 0 ? "  && " : "  ";
    if (c.or && c.or.length) {
      const terms = [predExpr(c), ...c.or.map(predExpr)];
      out.push(`${lead}( ${terms.join(" || ")} )`);
    } else {
      out.push(`${lead}${predExpr(c)}`);
    }
  });
  out.push("};");
  return out.join("\n");
}

// 괄호 깊이를 존중하며 최상위 && 로만 분할
function splitTopAnd(body) {
  const parts = []; let depth = 0, cur = "";
  for (let i = 0; i < body.length; i++) {
    const ch = body[i];
    if (ch === "(") depth++;
    else if (ch === ")") depth--;
    if (depth === 0 && ch === "&" && body[i + 1] === "&") { parts.push(cur); cur = ""; i++; continue; }
    cur += ch;
  }
  if (cur.trim()) parts.push(cur);
  return parts.map(s => s.trim()).filter(Boolean);
}
function parsePred(p) {
  p = p.trim().replace(/;$/, "");
  let m = p.match(/^(context\.[A-Za-z0-9_.]+)\s*(>=|<=|==|!=|>|<)\s*(.+)$/);
  if (m) {
    const rawVal = m[3].trim();
    const isRef = /^(principal|resource|context)\./.test(rawVal);
    if (rawVal === "true" || rawVal === "false") return { canon: m[1], op: m[2] === "!=" ? "≠ 참" : "= 참", val: "", role: "auth" };
    const role = isRef && /recipient|delegatee|address|spender|router|pool|wallet/i.test(rawVal) ? "address"
      : /^[0-9]/.test(rawVal) ? "numeric" : (isRef ? "ref" : "auth");
    return { canon: m[1], op: modelOp(m[2]), val: /^(principal|resource)\./.test(rawVal) ? "@" + rawVal : rawVal, role };
  }
  m = p.match(/^(context\.[A-Za-z0-9_.]+)$/);
  if (m) return { canon: m[1], op: "= 참", val: "", role: "auth" };
  return null;
}

// ── Cedar 텍스트 → 폼 (안전망: 범위 초과면 exceeded. forbid + AND + single-level OR 까지) ──
function parseCedar(text) {
  const wm = text.match(/when\s*\{([\s\S]*?)\}/);
  if (!wm) return { exceeded: true, reason: "structure" };
  const body = wm[1];
  if ((text.match(/action\s*==/g) || []).length > 1) return { exceeded: true, reason: "multi-action" };
  if (/\bhas\b|\.contains|\.containsAny|\bif\b/.test(body)) return { exceeded: true, reason: "beyond" };
  const parts = splitTopAnd(body);
  if (!parts.length) return { exceeded: true, reason: "empty" };
  const conds = [];
  for (const p of parts) {
    const orMatch = p.match(/^\(([\s\S]*)\)$/);
    if (orMatch) {
      const terms = orMatch[1].split("||").map(s => s.trim()).filter(Boolean);
      if (terms.length < 2) return { exceeded: true, reason: "paren" };
      const leaves = [];
      for (const term of terms) { const leaf = parsePred(term); if (!leaf) return { exceeded: true, reason: "unparsed", frag: term }; leaves.push(leaf); }
      conds.push({ ...leaves[0], or: leaves.slice(1) });
    } else {
      if (p.includes("||")) return { exceeded: true, reason: "bare-or" };
      const leaf = parsePred(p); if (!leaf) return { exceeded: true, reason: "unparsed", frag: p };
      conds.push(leaf);
    }
  }
  return { exceeded: false, conds };
}

// 정책이 폼으로 round-trip 되는지 (폼 탭 게이팅). 저장 진실=Cedar 하나, 폼은 표현 가능할 때만.
function isFormCapable(detail) {
  if (!detail || !Array.isArray(detail.conds) || !detail.conds.length) return false;
  const cedarText = (detail.cedar || []).map(l => l.t).join("\n");
  if (/\bhas\b|\.contains|\.containsAny/.test(cedarText)) return false;
  if ((cedarText.match(/action\s*==/g) || []).length > 1) return false;
  return true;
}
function applyParsed(model, parsed) {
  const byCanon = {}; (model.conds || []).forEach(c => { byCanon[c.canon] = c; });
  const conds = parsed.conds.map((pc, i) => {
    const ex = byCanon[pc.canon];
    if (ex) return { ...ex, op: pc.op, val: pc.val, role: pc.role, chip: chipFor(pc) };
    const leaf = pc.canon.split(".").pop();
    return { id: "p" + i + "_" + (Date.now() % 9999), field: { ko: leaf, en: leaf }, canon: pc.canon, op: pc.op, val: pc.val, unit: pc.role === "numeric" ? "" : "", role: pc.role, or: null, chip: chipFor(pc) };
  });
  return { ...model, conds };
}

// ════════════ 상단 NLG 배너 (모든 뷰 공통 · 정책에서 자동 생성 · 편집 불가) ════════════
function NlgBanner({ parts, loc }) {
  const lead = MP.t(parts.lead, loc);
  const base = (parts.base || []).map(b => MP.t(b, loc));
  const verb = MP.t(parts.verb, loc);
  const tail = loc === "en" ? `, ${verb}.` : ` ${verb}합니다.`;
  return (
    <div className="mpe-nlg">
      <span className="mpe-nlg-ic">{MPI.speech()}</span>
      <div className="mpe-nlg-main">
        <div className="mpe-nlg-eyebrow">{loc === "en" ? "What this policy means" : "이 정책은 이런 뜻"}
          <span className="mpe-nlg-auto" title={loc === "en" ? "Generated from the policy — always matches the code" : "정책에서 자동 생성 — 코드와 항상 일치"}>{MPI.check()}{loc === "en" ? "auto · always matches code" : "자동 · 코드와 일치"}</span>
        </div>
        <div className="mpe-nlg-text">
          <b>{lead}</b>{base.length > 0 && <> {base.join(loc === "en" ? " and " : " 그리고 ")}</>}
          {!parts.or && tail}
        </div>
        {parts.or && (
          <div className="mpe-nlg-or">
            <div className="oh">{MP.t(parts.or.head, loc)}</div>
            <ul>{parts.or.items.map((it, i) => <li key={i}>{MP.t(it, loc)}</li>)}</ul>
          </div>
        )}
        {parts.or && <div className="mpe-nlg-text" style={{ marginTop: 6 }}>{loc === "en" ? `→ ${verb}.` : `→ ${verb}합니다.`}</div>}
      </div>
    </div>
  );
}

// ════════════ "내 메모" 칸 (자유 편집 · 정책 동작과 무관 · 자동 문구를 덮지 않음) ════════════
function MemoField({ value, onChange, loc }) {
  const [open, setOpen] = useSE(!!value);
  if (!open) {
    return (
      <button className="mpe-memo-add" onClick={() => setOpen(true)}>
        {MPI.pencil()}{loc === "en" ? "Add a personal note" : "내 메모 추가"}
      </button>
    );
  }
  return (
    <div className="mpe-memo">
      <div className="mpe-memo-h">{MPI.pencil()}{loc === "en" ? "My note" : "내 메모"}<span className="mut">{loc === "en" ? "just for you · doesn't affect the policy" : "나만 보는 메모 · 정책 동작과 무관"}</span></div>
      <textarea className="mpe-memo-ta" value={value} onChange={e => onChange(e.target.value)}
        placeholder={loc === "en" ? "Why you keep this, what to revisit…" : "이 정책을 둔 이유, 나중에 확인할 점…"} rows={2} />
    </div>
  );
}

// ════════════ 뷰 전환 탭 (Cedar / 폼 / 블록) ════════════
function ViewTabs({ view, setView, formOk, loc }) {
  const tabs = [
    { key: "cedar", label: "Cedar", icon: MPI.shield, enabled: true },
    { key: "form", label: loc === "en" ? "Form" : "폼", icon: MPI.form, enabled: formOk },
    { key: "block", label: loc === "en" ? "Block" : "블록", icon: MPI.blocks, enabled: true }
  ];
  return (
    <div className="mpe-vtabs" role="tablist">
      {tabs.map(t => {
        const Ic = t.icon;
        const disabled = !t.enabled;
        return (
          <button key={t.key} role="tab" aria-selected={view === t.key}
            className={`mpe-vtab ${view === t.key ? "on" : ""} ${disabled ? "disabled" : ""}`}
            disabled={disabled}
            title={disabled ? (loc === "en" ? "This policy can't be shown as a form — use Block or Cedar" : "이 정책은 폼으로 표현 불가 — 블록/Cedar로 보세요") : ""}
            onClick={() => !disabled && setView(t.key)}>
            <Ic />{t.label}{disabled && <span className="lk">{MPI.lock()}</span>}
          </button>
        );
      })}
    </div>
  );
}

// ════════════ 정책 제목 인라인 편집 ════════════
function TitleEdit({ title, slug, sev, onCommit, loc }) {
  const [editing, setEditing] = useSE(false);
  const [draft, setDraft] = useSE(title);
  const inputRef = useRE(null);
  useEE(() => { setDraft(title); }, [title]);
  useEE(() => { if (editing && inputRef.current) { inputRef.current.focus(); inputRef.current.select(); } }, [editing]);
  const commit = () => { const nv = (draft || "").trim() || title; setEditing(false); if (nv !== title) onCommit(nv); };
  const sevTxt = sev === "fail" ? (loc === "en" ? "Block" : "차단") : (loc === "en" ? "Warn" : "경고");
  return (
    <div className="mpe-titlewrap">
      <div className="mpe-titlerow">
        {editing
          ? <input ref={inputRef} className="mpe-title-input" value={draft} onChange={e => setDraft(e.target.value)}
              onKeyDown={e => { if (e.key === "Enter") commit(); if (e.key === "Escape") { setDraft(title); setEditing(false); } }} onBlur={commit} />
          : <button className="mpe-title-btn" onClick={() => setEditing(true)} title={loc === "en" ? "Rename" : "이름 수정"}>
              <span className="t">{title}</span><span className="pen">{MPI.pencil()}</span>
            </button>}
        <span className={`mpe-title-sev ${sev}`}><span className="d" />{sevTxt}</span>
      </div>
      <div className="mpe-title-slug">{slug}</div>
    </div>
  );
}

// ════════════ Cedar 라이브 코드 에디터 (편집 가능 · 양방향) ════════════
function highlightCedar(t) {
  let s = t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  s = s.replace(/(\?[A-Za-z][A-Za-z0-9]*)/g, '<span class="hole">$1</span>');
  s = s.replace(/("[^"]*")/g, '<span class="str">$1</span>');
  s = s.replace(/(\/\/[^\n]*)/g, '<span class="cmt">$1</span>');
  s = s.replace(/\bMAX_UINT\b/g, '<span class="str">MAX_UINT</span>');
  s = s.replace(/\b(forbid|permit|when|principal|action|resource|context|has|true|false)\b/g, '<span class="kw">$1</span>');
  return s;
}
function SyncBadge({ state, loc }) {
  if (state === "exceeded") return <span className="mpe-sync warn">{MPI.warn()}{loc === "en" ? "beyond form" : "폼 범위 초과"}</span>;
  if (state === "ast") return <span className="mpe-sync neutral">{MPI.blocks()}{loc === "en" ? "full Cedar AST" : "전체 Cedar AST"}</span>;
  if (state === "outside") return <span className="mpe-sync warn">{MPI.lock()}{loc === "en" ? "outside form guard" : "폼 안전망 밖"}</span>;
  return <span className="mpe-sync ok">{MPI.check()}{loc === "en" ? "synced ↔ form" : "폼과 동기화됨"}</span>;
}
function CedarCodeField({ code, onChange, editable, syncState, loc, label }) {
  const taRef = useRE(null), gutRef = useRE(null), hlRef = useRE(null);
  const lines = code.split("\n");
  const sync = () => { if (taRef.current) { if (gutRef.current) gutRef.current.scrollTop = taRef.current.scrollTop; if (hlRef.current) { hlRef.current.scrollTop = taRef.current.scrollTop; hlRef.current.scrollLeft = taRef.current.scrollLeft; } } };
  return (
    <div className="mpe-codepane">
      <div className="mpe-codepane-h">
        <span className="lang">{label || "CEDAR"}</span>
        <SyncBadge state={syncState} loc={loc} />
      </div>
      <div className="mpe-codepane-body">
        <div className="mpe-gutter" ref={gutRef}>{lines.map((_, i) => <div key={i}>{i + 1}</div>)}</div>
        <div className="mpe-codestack">
          <pre className="mpe-hl" ref={hlRef} aria-hidden="true"><code dangerouslySetInnerHTML={{ __html: lines.map(l => highlightCedar(l) || "&nbsp;").join("\n") }} /></pre>
          <textarea ref={taRef} className="mpe-ta" value={code} spellCheck={false} wrap="off"
            onScroll={sync} onChange={e => onChange(e.target.value)} readOnly={!editable}
            placeholder={editable ? "" : undefined} />
        </div>
      </div>
    </div>
  );
}

// ════════════ 인라인 값 편집 (네이티브 팝업 금지) ════════════
function InlineVal({ cond, loc, onCommit, onFlash, disabled }) {
  const isRef = /^@|context\.|principal\.|resource\./.test(cond.val);
  const [editing, setEditing] = useSE(false);
  const [draft, setDraft] = useSE(cond.val);
  const inputRef = useRE(null);
  useEE(() => { if (editing && inputRef.current) { inputRef.current.focus(); inputRef.current.select(); } }, [editing]);

  if (isRef) {
    return <span className="mpe-pred-val ref" title={loc === "en" ? "Reference — fills from the wallet" : "참조값 — 지갑에서 채워짐"}>{cond.val.replace(/^@/, "")}{MPI.key({ style: { width: 11, height: 11, marginLeft: 4, opacity: 0.7 } })}</span>;
  }
  const commit = (v) => { const raw = v != null ? v : (inputRef.current ? inputRef.current.value : draft); const nv = (raw || "").trim() || cond.val; setEditing(false); if (nv !== cond.val) { onCommit(cond.id, nv); onFlash(cond.id); } };
  if (editing && !disabled) {
    return (
      <span className="mpe-pred-val editing">
        <input ref={inputRef} className="mpe-val-input" defaultValue={draft} inputMode="numeric"
          onChange={e => setDraft(e.target.value)}
          onKeyDown={e => { if (e.key === "Enter") commit(); if (e.key === "Escape") { setEditing(false); } }}
          onBlur={() => commit()} />
        {cond.unit ? <span className="u">{cond.unit}</span> : null}
      </span>
    );
  }
  return (
    <span className="mpe-pred-val" tabIndex={0} title={loc === "en" ? "Click to edit" : "클릭해서 수정"}
      onClick={() => { if (!disabled) { setDraft(cond.val); setEditing(true); } }}
      onKeyDown={e => { if (e.key === "Enter" && !disabled) { setDraft(cond.val); setEditing(true); } }}>
      {cond.val}{cond.unit ? <span className="u">{cond.unit}</span> : null}
      <span className="mpe-val-pen">{MPI.pencil()}</span>
    </span>
  );
}

// ════════════ 폼 조건 행 ════════════
function FormCond({ cond, loc, onCommit, onFlash, onAddOr, onAddOrItem, onDel, disabled }) {
  const Pred = ({ c, editable }) => {
    const R = ROLE_ICON_MP[c.role || "numeric"] || MPI.dot;
    return (
      <div className={`mpe-pred role-${c.role || "numeric"}`}>
        <span className="mpe-pred-cap"><R /></span>
        <span className="mpe-pred-field"><span className="nm">{MP.t(c.field, loc)}</span><span className="canon">{c.canon}</span></span>
        <span className="mpe-pred-op">{c.op}</span>
        {editable
          ? <InlineVal cond={c} loc={loc} onCommit={onCommit} onFlash={onFlash} disabled={disabled} />
          : <span className={`mpe-pred-val ${/^@|context\./.test(c.val) ? "ref" : ""}`}>{c.val.replace(/^@/, "")}{c.unit ? <span className="u">{c.unit}</span> : null}</span>}
      </div>
    );
  };
  return (
    <div className={["mpe-cond", cond.err ? "err" : ""].filter(Boolean).join(" ")}>
      <div className="mpe-cond-row">
        <Pred c={cond} editable />
        <button className="mpe-cond-del" onClick={() => onDel && onDel(cond)} disabled={disabled} title={loc === "en" ? "Remove" : "삭제"}>{MPI.x()}</button>
      </div>
      {cond.chip && (
        <div className="mpe-cond-chip"><span className="lbl">cedar</span><code>{cond.chip}</code></div>
      )}
      {Array.isArray(cond.recommended) && cond.recommended.length > 0 && (
        <div className="mpe-rec">
          <span className="mpe-rec-lbl">{loc === "en" ? "Suggested" : "추천값"}</span>
          {cond.recommended.map(v => (
            <button key={v} className={`mpe-rec-chip ${cond.val === v ? "on" : ""}`} disabled={disabled} onClick={() => { if (v !== cond.val) { onCommit(cond.id, v); onFlash(cond.id); } }}>{v}{cond.unit ? <span className="u">{cond.unit}</span> : null}</button>
          ))}
        </div>
      )}
      {!cond.or && (
        <button className="mpe-or-btn" onClick={() => onAddOr(cond)} disabled={disabled}>{MPI.plus()}{loc === "en" ? "+ or (OR)" : "+ 또는(OR)"}</button>
      )}
      {cond.or && (
        <div className="mpe-orgrp">
          <div className="mpe-orgrp-h">
            <span className="mpe-orgrp-kw">OR</span>
            <span className="mpe-orgrp-note">{loc === "en" ? "Matches if A or B — single level" : "A 또는 B 중 하나라도 — single level"}</span>
          </div>
          <div className="mpe-orgrp-list">
            <div className="mpe-orgrp-item"><Pred c={cond} /></div>
            {cond.or.map((o, i) => (
              <React.Fragment key={i}>
                <div className="mpe-orgrp-or">{loc === "en" ? "OR" : "또는"}</div>
                <div className="mpe-orgrp-item"><Pred c={o} /><button className="mpe-cond-del" onClick={() => onDel && onDel(cond, i)}>{MPI.x()}</button></div>
              </React.Fragment>
            ))}
          </div>
          <button className="mpe-add-or" onClick={() => onAddOrItem(cond)} style={{ marginTop: 8 }}>{MPI.plus()}{loc === "en" ? "Add OR option" : "OR 항목 추가"}</button>
          <div className="mpe-nest-lock">{MPI.lock()}{loc === "en" ? "Nesting locked — OR can't contain another AND group" : "중첩 잠김 — OR 안에 AND 그룹을 넣을 수 없습니다"}</div>
        </div>
      )}
    </div>
  );
}

// ════════════ 폼 에디터 ════════════
function FormEditor({ detail, loc, model, setModel, onFlash, onFormEdit, disabled }) {
  const onCommit = (id, val) => onFormEdit(m => ({ ...m, conds: m.conds.map(c => c.id === id ? { ...c, val, chip: chipFor({ ...c, val }) } : c) }));
  const addOr = (cond) => onFormEdit(m => ({ ...m, conds: m.conds.map(c => c.id === cond.id ? { ...c, or: [{ field: { ko: "또 다른 조건", en: "another condition" }, canon: "context.other", op: "= 참", val: "", role: "auth" }] } : c) }));
  const addOrItem = (cond) => onFormEdit(m => ({ ...m, conds: m.conds.map(c => c.id === cond.id ? { ...c, or: [...(c.or || []), { field: { ko: "추가 조건", en: "extra condition" }, canon: "context.extra", op: "= 참", val: "", role: "auth" }] } : c) }));
  const del = (cond, orIdx) => onFormEdit(m => {
    if (orIdx != null) return { ...m, conds: m.conds.map(c => c.id === cond.id ? { ...c, or: c.or.filter((_, i) => i !== orIdx) } : c) };
    return { ...m, conds: m.conds.filter(c => c.id !== cond.id) };
  });
  return (
    <div className={`mpe-form ${disabled ? "is-frozen" : ""}`}>
      <div className="mpe-fstep">
        <div className="mpe-fstep-h"><span className="mpe-fstep-no">1</span><span className="mpe-fstep-t">{loc === "en" ? "What to check" : "무엇을 검사하나요?"}<span className="mut">{loc === "en" ? "which transactions this applies to" : "어떤 거래에 적용할지 골라요"}</span></span><span className="mpe-fstep-line" /></div>
        <div className="mpe-field">
          <span className="lab">{loc === "en" ? "Action" : "검사 대상"}</span>
          <div className="mpe-select">{MP.t(detail.action, loc)}<span className="canon">tag · {detail.actionTag || "—"}</span>{MPI.caretD()}</div>
        </div>
      </div>

      <div className="mpe-fstep">
        <div className="mpe-fstep-h"><span className="mpe-fstep-no">2</span><span className="mpe-fstep-t">{loc === "en" ? "When it's risky" : "언제 위험한가요?"}<span className="mut">{loc === "en" ? "add conditions — several means AND" : "조건 추가, 여러 개면 모두 참(AND)"}</span></span><span className="mpe-fstep-line" /></div>
        <div className="mpe-conds">
          {model.conds.length > 1 && <span className="mpe-and-tag">AND</span>}
          {model.conds.map(c => (
            <FormCond key={c.id} cond={c} loc={loc} onCommit={onCommit} onFlash={onFlash} onAddOr={addOr} onAddOrItem={addOrItem} onDel={del} disabled={disabled} />
          ))}
          <button className="mpe-add-cond" disabled={disabled} onClick={() => onFormEdit(m => ({ ...m, conds: [...m.conds, { id: "c" + Date.now(), field: { ko: "새 조건", en: "new condition" }, canon: "context.field", op: ">", val: "0", unit: "", role: "numeric", or: null, chip: "context.field > 0" }] }))}>{MPI.plus()}{loc === "en" ? "Add condition" : "조건 추가"}</button>
        </div>
      </div>

      <div className="mpe-fstep">
        <div className="mpe-fstep-h"><span className="mpe-fstep-no">3</span><span className="mpe-fstep-t">{loc === "en" ? "How to alert" : "어떻게 알릴까요?"}<span className="mut">{loc === "en" ? "name · severity · reason" : "이름·심각도·사유"}</span></span><span className="mpe-fstep-line" /></div>
        <div className="mpe-meta">
          <div className="mpe-meta-row"><span className="lab">{loc === "en" ? "Rule id" : "규칙 id"}</span><span className="mpe-meta-val">{detail.meta.id}</span></div>
          <div className="mpe-meta-row"><span className="lab">{loc === "en" ? "Severity" : "심각도"}</span>
            <div className="mpe-sev-pick">
              <span className={`mpe-sev-opt warn ${detail.meta.sev === "warn" ? "on" : ""}`}><span className="d" />{loc === "en" ? "Warn" : "경고"}</span>
              <span className={`mpe-sev-opt fail ${detail.meta.sev === "fail" ? "on" : ""}`}><span className="d" />{loc === "en" ? "Block" : "차단"}</span>
            </div>
          </div>
          <div className="mpe-meta-row" style={{ alignItems: "flex-start" }}><span className="lab">{loc === "en" ? "Reason" : "사유"}</span><span className="mpe-meta-val" style={{ fontFamily: "var(--ff-sans)" }}>{MP.t(detail.meta.reason, loc)}</span></div>
        </div>
      </div>

      <div className="mpe-valid">
        <span className="mpe-valid-badge">{MPI.check()}{loc === "en" ? "Valid policy · .cedar and manifest in sync" : "유효한 정책 · .cedar와 manifest 짝 맞음"}</span>
        <span className="mpe-valid-meta">trigger: action.tag == <code>{detail.actionTag || "—"}</code> · enrichment: <code>none</code></span>
      </div>
    </div>
  );
}

// ════════════ 블록 에디터 (detail.conds / nlgParts 기반 · 전체 Cedar 표현) ════════════
function blockRole(canon, val) {
  const s = (canon || "") + " " + (val || "");
  if (/recipient|delegatee|address|spender|router|pool|wallet/i.test(s)) return "address";
  if (/^@?[0-9]/.test(val || "")) return "numeric";
  if (/allowlist|allowed|contains|pool|router/i.test(canon || "")) return "ref";
  return "auth";
}
function BPred({ field, canon, op, val }) {
  const role = blockRole(canon, val);
  const Ic = ROLE_ICON_MP[role] || MPI.dot;
  const isRef = /^@|principal\.|resource\.|context\./.test(val || "");
  const showVal = val !== "" && val != null;
  return (
    <div className={`mpe-bpred role-${role}`}>
      <span className="mpe-bpred-cap"><Ic /></span>
      <span className="mpe-bpred-field"><span className="nm">{field}</span>{canon && <span className="canon">{canon}</span>}</span>
      {op && <span className="mpe-bpred-op">{op}</span>}
      {showVal && <span className={`mpe-bpred-val ${isRef ? "ref" : ""}`}>{String(val).replace(/^@/, "")}</span>}
    </div>
  );
}
function BlockFromParts({ parts, loc }) {
  const base = parts.base || [];
  const or = parts.or;
  return (
    <>
      {base.map((b, i) => (
        <React.Fragment key={"b" + i}>
          {i > 0 && <span className="mpe-and-conn">AND</span>}
          <BPred field={MP.t(b, loc)} />
        </React.Fragment>
      ))}
      {or && (
        <>
          {base.length > 0 && <span className="mpe-and-conn">AND</span>}
          <div className="mpe-logic op-OR">
            <span className="mpe-logic-spine" />
            <div className="mpe-logic-head"><span className="mpe-logic-kw">OR</span><span className="mpe-logic-note">{MP.t(or.head, loc)}</span></div>
            <div className="mpe-logic-slot">{or.items.map((it, i) => <BPred key={i} field={MP.t(it, loc)} />)}</div>
          </div>
        </>
      )}
    </>
  );
}
function BlockEditor({ detail, loc }) {
  const actLabel = MP.t(detail.action, loc);
  const conds = detail.conds && detail.conds.length ? detail.conds : null;
  const verb = detail.meta && detail.meta.sev === "warn" ? (loc === "en" ? "warn" : "경고") : (loc === "en" ? "block" : "차단");
  return (
    <div className="mpe-block-canvas">
      <div className="mpe-block-stage">
        <div className="mpe-hat">
          <div className="mpe-hat-head">
            <span className="mpe-hat-eff">{MPI.shield()}{loc === "en" ? `Forbid · ${verb}` : `금지 · ${verb}`}</span>
            <span className="mpe-hat-sentence">
              {loc === "en" ? <><b>Wallet</b> on <span className="mpe-hat-act">{actLabel}</span> — {verb} when <b>all</b> below hold:</>
                : <><b>지갑</b>이 <span className="mpe-hat-act">{actLabel}</span> 할 때, 아래가 <b>모두 참</b>이면 {verb}:</>}
            </span>
          </div>
          <div className="mpe-hat-body">
            <div className="mpe-and-stack">
              {conds ? conds.map((c, i) => (
                <React.Fragment key={c.id || i}>
                  {i > 0 && <span className="mpe-and-conn">AND</span>}
                  {c.or && c.or.length ? (
                    <div className="mpe-logic op-OR">
                      <span className="mpe-logic-spine" />
                      <div className="mpe-logic-head"><span className="mpe-logic-kw">OR</span><span className="mpe-logic-note">{loc === "en" ? "any one matches" : "하나라도 해당"}</span></div>
                      <div className="mpe-logic-slot">
                        <BPred field={MP.t(c.field, loc)} canon={c.canon} op={c.op} val={c.val} />
                        {c.or.map((o, j) => <BPred key={j} field={MP.t(o.field, loc)} canon={o.canon} op={o.op} val={o.val} />)}
                      </div>
                    </div>
                  ) : (
                    <BPred field={MP.t(c.field, loc)} canon={c.canon} op={c.op} val={c.val} />
                  )}
                </React.Fragment>
              )) : <BlockFromParts parts={detail.nlgParts || { base: [], or: null }} loc={loc} />}
            </div>
          </div>
        </div>
      </div>
      <div className="mpe-block-hint">{MPI.blocks()}{loc === "en" ? "Full Cedar AST — OR · has · set" : "전체 Cedar AST — OR · has · set"}</div>
    </div>
  );
}

// ── manifest 보조 패널 (Cedar 모드 · 접기) ──
function ManifestAux({ detail, loc }) {
  const [open, setOpen] = useSE(false);
  const text = detail.manifest || '{\n  "id": "' + detail.meta.id + '",\n  "effect": "forbid"\n}';
  return (
    <div className={`mpe-maux ${open ? "open" : ""}`}>
      <button className="mpe-maux-h" onClick={() => setOpen(o => !o)}>
        <span className="car">{MPI.caret()}</span>
        <span className="lang">manifest.json</span>
        <span className="tag">{loc === "en" ? "derived · auxiliary" : "파생 · 보조"}</span>
      </button>
      {open && (
        <div className="mpe-maux-body">
          {text.split("\n").map((ln, i) => (
            <div key={i} className="mpe-maux-ln"><span className="g">{i + 1}</span>
              <span className="t" dangerouslySetInnerHTML={{ __html: ln.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/("[^"]*")(\s*:)/g, '<span class="key">$1</span>$2').replace(/:\s*("[^"]*")/g, ': <span class="str">$1</span>') }} /></div>
          ))}
        </div>
      )}
    </div>
  );
}

// ════════════ 에디터 루트 (저장 진실 = Cedar 하나 · 뷰 전환: Cedar / 폼 / 블록) ════════════
function PolicyEditor({ policy, loc, onRename, toast }) {
  const detail = MP.detailFor(policy) || fallbackDetail(policy, loc);
  const pid = policy.uid || policy.id || detail.meta.id;
  const capable = useME(() => isFormCapable(detail), [detail]);

  const [view, setView] = useSE(policy._startView || "cedar");           // 열 때 기본 = Cedar (새 정책은 추셔에서 고른 시작 방식)
  const [title, setTitle] = useSE(() => MP.t(policy.name, loc));
  const [memo, setMemo] = useSE("");
  const [model, setModel] = useSE(() => ({ conds: (detail.conds || []).map(c => ({ ...c, chip: c.chip || chipFor(c) })) }));
  const [code, setCode] = useSE(() => detail.cedar.map(l => l.t).join("\n"));
  const [exceeded, setExceeded] = useSE(false);
  const [flash, setFlash] = useSE(null);
  const [mirrorW, setMirrorW] = useSE(460);
  const panesRef = useRE(null);

  // 정책이 바뀌면 전체 리셋 (다른 행 열기)
  useEE(() => {
    setView(policy._startView || "cedar");
    setTitle(MP.t(policy.name, loc));
    setModel({ conds: (detail.conds || []).map(c => ({ ...c, chip: c.chip || chipFor(c) })) });
    setCode(detail.cedar.map(l => l.t).join("\n"));
    setExceeded(false);
    try { setMemo(localStorage.getItem("mp-memo:" + pid) || ""); } catch (e) { setMemo(""); }
  }, [pid]);

  const formOk = capable && !exceeded;
  const onFlash = (id) => { setFlash(id); setTimeout(() => setFlash(null), 700); };

  const commitMemo = (v) => { setMemo(v); try { localStorage.setItem("mp-memo:" + pid, v); } catch (e) {} };
  const commitTitle = (v) => { setTitle(v); if (onRename) onRename(policy, v); };

  // 폼 → 코드 (라이브 양방향)
  const onFormEdit = (updater) => {
    setModel(m => { const nm = updater(m); setCode(buildCedarText(detail, nm)); return nm; });
    setExceeded(false);
  };
  // 코드 → 폼 (round-trip 안전망: 폼 범위 넘으면 폼 탭 비활성, 코드는 진실 유지)
  const onCodeChange = (text) => {
    setCode(text);
    const parsed = parseCedar(text);
    if (parsed.exceeded) setExceeded(true);
    else { setModel(m => applyParsed(m, parsed)); setExceeded(false); }
  };

  // 드래그 splitter
  const startSplit = (e) => {
    e.preventDefault();
    const startX = e.clientX, startW = mirrorW;
    const move = (ev) => {
      const dx = ev.clientX - startX;
      const total = panesRef.current ? panesRef.current.getBoundingClientRect().width : 1000;
      setMirrorW(Math.max(340, Math.min(total - 380, startW - dx)));
    };
    const up = () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up); document.body.style.cursor = ""; document.body.style.userSelect = ""; };
    document.body.style.cursor = "col-resize"; document.body.style.userSelect = "none";
    window.addEventListener("pointermove", move); window.addEventListener("pointerup", up);
  };

  const liveParts = detail.nlgParts || { verb: { ko: "경고", en: "warn" }, lead: { ko: "", en: "" }, base: [], or: null };

  return (
    <div className="mpe-body">
      {/* 헤더: 제목 인라인 편집 + 저장 + 뷰 전환 탭 (형식 출처 표시 없음) */}
      <div className="mpe-head">
        <TitleEdit title={title} slug={policy.slug || detail.meta.id} sev={detail.meta.sev} onCommit={commitTitle} loc={loc} />
        <span className="mpe-head-spc" />
        <button className="mpe-head-save" onClick={() => toast(loc === "en" ? "Saved · draft" : "저장됨 · draft", policy.slug || detail.meta.id)}>{MPI.save()}{loc === "en" ? "Save" : "저장"}</button>
        <ViewTabs view={view} setView={setView} formOk={formOk} loc={loc} />
      </div>

      {/* 상단 자동 NLG (편집 불가) + 그 아래 자유 "내 메모" 칸 */}
      <NlgBanner parts={liveParts} loc={loc} />
      <MemoField value={memo} onChange={commitMemo} loc={loc} />

      {view === "cedar" ? (
        // ── Cedar 뷰: 코드 주인공 + manifest 보조 ──
        <div className="mpe-cedar-solo">
          <div className="mpe-cedar-note">{MPI.warn()}<span>{loc === "en"
            ? <>Cedar is <b>outside the form safety net (round-trip)</b> — for experienced users. The manifest below is derived/managed.</>
            : <>Cedar는 <b>폼 안전망(round-trip) 밖</b> — 숙련자용입니다. 아래 manifest는 파생·관리됩니다.</>}</span></div>
          <CedarCodeField code={code} onChange={onCodeChange} editable={true} syncState={exceeded ? "exceeded" : (capable ? "synced" : "outside")} loc={loc} label={"policy.cedar"} />
          <ManifestAux detail={detail} loc={loc} />
        </div>
      ) : (
        // ── 폼 / 블록 뷰: 좌 편집 / splitter / 우 편집 가능 Cedar (라이브 양방향) ──
        <div className="mpe-panes" ref={panesRef}>
          <div className="mpe-left">
            <div className="mpe-lh">
              {view === "block"
                ? <><span className="mpe-lh-mode block">{MPI.blocks()}{loc === "en" ? "Block view" : "블록 뷰"}</span><span className="mpe-rt ok">{MPI.check()}{loc === "en" ? "full Cedar AST" : "전체 Cedar AST"}</span></>
                : <><span className="mpe-lh-mode form">{MPI.form()}{loc === "en" ? "Form view" : "폼 뷰"}</span>{exceeded
                    ? <span className="mpe-rt bad">{MPI.warn()}{loc === "en" ? "form can't show this" : "폼 표현 범위 초과"}</span>
                    : <span className="mpe-rt ok">{MPI.check()}{loc === "en" ? "round-trip ✓" : "round-trip 통과"}</span>}</>}
              <span className="mpe-lh-spc" />
              <span className="mpe-lh-note">{view === "block" ? (loc === "en" ? "Cedar ↔ blocks live" : "Cedar ↔ 블록 실시간") : (loc === "en" ? "Cedar ↔ form live" : "Cedar ↔ 폼 실시간")}</span>
            </div>

            {view === "form" && exceeded && (
              <div className="mpe-exceeded">
                <div className="mpe-exceeded-h">{MPI.warn()}{loc === "en" ? "This edit can't be fully drawn as a form" : "이 편집은 폼으로 다 못 그려요"}</div>
                <div className="mpe-exceeded-b">{loc === "en"
                  ? <>Your code uses OR / <span className="mono">has</span> / set ops or multiple actions — beyond the form's single-level AND. The <b>code stays the source of truth</b>; switch views to keep editing.</>
                  : <>코드가 <span className="mono">has</span> · 집합 연산이나 다중 action을 써서 폼의 single-level AND 범위를 넘었습니다. <b>코드를 진실로 유지</b>하니, 뷰를 바꿔 계속 편집하세요.</>}</div>
                <div className="mpe-exceeded-acts">
                  <button className="mpe-exceeded-go" onClick={() => setView("block")}>{MPI.blocks()}{loc === "en" ? "View in blocks" : "블록으로 보기"}</button>
                  <button className="mpe-exceeded-go alt" onClick={() => setView("cedar")}>{MPI.shield()}{loc === "en" ? "Open in Cedar" : "Cedar로 보기"}</button>
                </div>
              </div>
            )}

            <div className="mpe-left-scroll">
              {view === "form" && <FormEditor detail={detail} loc={loc} model={model} setModel={setModel} onFlash={onFlash} onFormEdit={onFormEdit} disabled={exceeded} />}
              {view === "block" && <BlockEditor detail={detail} loc={loc} />}
            </div>
          </div>

          <div className="mpe-splitter" onPointerDown={startSplit} title={loc === "en" ? "Drag to resize" : "드래그로 크기 조절"}><span className="grip" /></div>

          <div className="mpe-mirror" style={{ width: mirrorW }}>
            <CedarCodeField code={code} onChange={onCodeChange} editable={true}
              syncState={view === "block" ? "ast" : (exceeded ? "exceeded" : "synced")} loc={loc} label={"policy.cedar"} />
          </div>
        </div>
      )}
    </div>
  );
}

function fallbackDetail(policy, loc) {
  return {
    action: { ko: "정책 동작", en: "Policy action" }, actionTag: "—", actionEnt: "Action",
    nlgParts: { verb: { ko: policy.sev === "fail" ? "차단" : "경고", en: policy.sev === "fail" ? "block" : "warn" },
      lead: { ko: `이 정책(${MP.t(policy.name, "ko")})은`, en: `This policy (${MP.t(policy.name, "en")})` },
      base: [{ ko: "조건이 충족되면", en: "when its conditions hold" }], or: null },
    cedar: [
      { t: `// ${policy.slug || policy.id}`, k: "cmt" },
      { t: "forbid (", k: "kw" },
      { t: "  principal, action, resource" },
      { t: ") when {" },
      { t: "  context.flagged == true", g: "c1" },
      { t: "};" }
    ],
    manifest: '{\n  "id": "' + (policy.slug || policy.id) + '",\n  "effect": "forbid"\n}',
    meta: { id: policy.slug || policy.id, sev: policy.sev, reason: { ko: "고위험 동작 차단", en: "Block high-risk action" } },
    conds: [{ id: "c1", field: { ko: "위험 플래그", en: "Risk flag" }, canon: "context.flagged", op: "=", val: "true", unit: "", role: "auth", or: null, chip: "context.flagged == true" }]
  };
}

Object.assign(window, { PolicyEditor });
