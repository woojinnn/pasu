// editor-builder.jsx
// Builder mode — flat AND form. Same IR as Block mode; renders each guard
// as a row of cascade dropdowns + operator + value. NOTE: Builder cannot
// render OR-between-fields or nesting → we show the read-only fallback
// (§9.8-2) when the policy uses OR/nesting.

const { useState: useStateBuilder } = React;

function CascadeStep({ label, isCustom, onClick }) {
  return (
    <button className={`cs-step ${isCustom ? 'cs-step-custom' : ''}`} onClick={onClick}>
      <span className="cs-step-t">{label}</span>
      <I.caretDown style={{ width: 11, height: 11, color: 'var(--slate-400)' }} />
    </button>
  );
}

function OpSelect({ value, options }) {
  return (
    <div className="op-sel">
      <span>{value}</span>
      <I.caretDown style={{ width: 11, height: 11, color: 'var(--slate-400)' }} />
    </div>
  );
}

function ValueInput({ value, leafType }) {
  const placeholder =
    leafType === 'tokenNative' ? 'e.g. 0.5 (token-native, any token)' :
    leafType === 'bps'         ? 'bps (0–10000)' :
    leafType === 'seconds'     ? 'unix seconds' :
    leafType === 'address'     ? '0x... (40 hex)' :
    leafType === 'symbol'      ? 'USDC, WETH, ...' :
    leafType === 'enum'        ? 'select…' :
    leafType === 'bool'        ? '' : '';

  if (value.kind === 'ref') {
    return (
      <div className="val-in val-in-ref">
        <span className="val-in-pre">ref</span>
        <span className="val-in-t">{value.text}</span>
      </div>
    );
  }
  if (value.kind === 'enum') {
    return (
      <div className="val-in val-in-enum">
        <span className="val-in-t">{value.text}</span>
        <I.caretDown style={{ width: 11, height: 11, color: 'var(--slate-400)' }} />
      </div>
    );
  }
  if (value.kind === 'bool') {
    return (
      <div className="val-in val-in-bool">
        <button className={`bool-btn ${value.text === 'true' ? 'on' : ''}`}>true</button>
        <button className={`bool-btn ${value.text === 'false' ? 'on' : ''}`}>false</button>
      </div>
    );
  }
  return (
    <div className="val-in val-in-num">
      <input className="val-in-input" defaultValue={value.text} placeholder={placeholder} />
      {value.unit && <span className="val-in-unit">{value.unit}</span>}
    </div>
  );
}

function BuilderRow({ guard, idx, colorScheme, focused, onFocus, matched, skewed }) {
  const topSegKey = guard.segments[0].key;
  const color = colorScheme.map[topSegKey] || 'cyan';

  // For a real cascade we'd compute steps. For the baseline guards, segments[0]
  // is already the leaf path; for inputAmount-like cases there'd be more steps.
  // To show the cascade pattern in Builder, we synthesize friendly step labels.
  const steps = guard.segments.map((s, i) => ({
    label: s.label,
    isCustom: guard.custom && i === guard.segments.length - 1,
  }));

  const ops = OPERATORS_BY_TYPE[
    guard.value.kind === 'bool' ? 'boolean' :
    guard.value.kind === 'enum' ? 'enum' :
    guard.value.kind === 'ref'  ? 'address' :
    guard.value.kind === 'num'  ? (guard.value.unit === 'sec' ? 'seconds' : 'bps') : 'number'
  ] || ['==', '!='];

  return (
    <div className={`bd-row ${focused ? 'bd-row-focus' : ''} ${matched ? 'bd-row-match' : ''} ${skewed ? 'bd-row-skew' : ''}`}
         onClick={onFocus}>
      <span className={`bd-bar bd-bar-${color}`} />
      <span className="bd-idx">{idx + 1}</span>
      <div className="bd-cascade">
        {steps.map((s, i) => (
          <React.Fragment key={i}>
            {i > 0 && <span className="bd-cs-sep">›</span>}
            <CascadeStep label={s.label} isCustom={s.isCustom} />
          </React.Fragment>
        ))}
        {guard.custom && <span className="bd-custom-tag" title="manifest enrichment · 점선">enrichment</span>}
      </div>
      <OpSelect value={guard.operator} options={ops} />
      <ValueInput value={guard.value} leafType={
        guard.value.kind === 'bool' ? 'boolean' :
        guard.value.kind === 'enum' ? 'enum' :
        guard.value.kind === 'ref'  ? 'address' :
        guard.value.kind === 'num'  ? (guard.value.unit === 'sec' ? 'seconds' : 'bps') :
        'number'
      } />
      {guard.note && <span className="bd-note">{guard.note}</span>}
      <button className="bd-x" title="삭제"><I.x style={{ width: 12, height: 12 }} /></button>
    </div>
  );
}

function BuilderView({ policy, colorScheme, locale, focusedGuard, onFocusGuard, matchedGuards, skewedGuard }) {
  // The baseline policy uses OR-between-fields → Builder cannot edit, only show as read-only fallback.
  // We render the fallback banner + rows in disabled state for layout/preview.
  const isORPolicy = policy.root.op === 'OR' && policy.root.children.length > 1;

  return (
    <div className="bd-view" data-screen-label="Builder mode">
      {isORPolicy && (
        <div className="bd-fallback">
          <span className="bd-fb-ic"><I.warn style={{ width: 14, height: 14 }} /></span>
          <div className="bd-fb-body">
            <div className="bd-fb-t">이 정책은 <b>필드 간 OR</b> 구조입니다 — Builder에서는 편집 불가.</div>
            <div className="bd-fb-d">Block 또는 Code 모드에서 편집하세요. Builder는 <b>필드 내 OR(in)</b>만 지원합니다.</div>
          </div>
          <button className="bd-fb-cta">Block에서 열기 →</button>
        </div>
      )}

      <div className="bd-section">
        <div className="bd-section-h">
          <span className="bd-trigger">
            <span className="bd-trg-k">trigger</span>
            <span className="bd-trg-eq">action ==</span>
            <span className="bd-trg-v">swap</span>
          </span>
          <span className="bd-sep-v" />
          <span className="bd-op-mode">
            <span className="bd-op-k">조합</span>
            <span className="bd-op-pill bd-op-or">OR · 하나라도 참</span>
            <span className="bd-op-disabled">Builder에서는 AND만 가능</span>
          </span>
        </div>

        <div className={`bd-rows ${isORPolicy ? 'bd-rows-disabled' : ''}`}>
          {policy.root.children.map((g, i) => (
            <BuilderRow key={g.id} guard={g} idx={i} colorScheme={colorScheme} locale={locale}
              focused={focusedGuard === g.id}
              onFocus={() => onFocusGuard(g.id)}
              matched={(matchedGuards || []).includes(g.id)}
              skewed={skewedGuard === g.id} />
          ))}
        </div>

        <button className="bd-add" disabled={isORPolicy}>
          <I.plus style={{ width: 12, height: 12 }} />
          조건 추가
        </button>
      </div>

      <div className="bd-decision">
        <span className="bd-dec-arrow">↓</span>
        <span className="bd-dec-then">Then</span>
        <span className="bd-dec-deny">Deny</span>
        <input className="bd-dec-reason" defaultValue='swap baseline violated' />
        <span className="bd-dec-sev-k">severity</span>
        <span className="bd-dec-sev">FAIL</span>
      </div>
    </div>
  );
}

Object.assign(window, { BuilderView, BuilderRow });
