/**
 * CustomFieldModal — "+ 새 보강 필드 만들기".
 *
 * policy-server 메서드를 골라 파라미터를 매핑하면 `context.custom.<이름>`
 * 필드가 생긴다: 즉시 필드 선택기에 나타나고, 저장 시 manifest의
 * `policy_rpc` + `custom_context`로 직렬화된다.
 *
 * 파라미터 값은 셀렉터 문법(`$.root.chain_id`)을 직접 쓰게 하지 않는다 —
 * 폼의 한국어 필드 카탈로그를 재사용한 드롭다운("이 거래에서 가져오기")으로
 * 고르고, 셀렉터 원문/결과 위치는 "고급" 안에서만 보인다. `context.<X>` Cedar
 * 경로는 plan-time 셀렉터 `$.action.<X>`와 1:1 대응한다.
 */
import { useMemo, useState } from "react";

import type { EnrichmentField } from "../../../editor-v9/manifest-gen";
import type { FieldOption } from "../../../cedar/form";

import { METHOD_CATALOG, type MethodSpec } from "./custom-field-methods";

export interface CustomFieldDraft {
  name: string;
  field: EnrichmentField;
}

const NAME_RE = /^[a-zA-Z_][a-zA-Z0-9_]*$/;

/** 거래(tx) 레벨 셀렉터 — 액션 컨텍스트 밖의 몇 안 되는 값들. */
const ROOT_OPTIONS: { sel: string; label: string }[] = [
  { sel: "$.root.chain_id", label: "체인 (어느 네트워크인지)" },
  { sel: "$.root.from", label: "보내는 주소 (내 지갑)" },
  { sel: "$.root.to", label: "대상 컨트랙트 주소" },
];

/** 메서드 파라미터 키 → 한국어 라벨 (모르는 키는 키 그대로). */
const PARAM_KEY_KO: Record<string, string> = {
  chain_id: "체인",
  asset: "토큰",
  amount: "수량",
  address: "주소",
  wallet: "지갑",
  decimals: "소수 자릿수",
};

interface SelectorOption {
  sel: string;
  label: string;
}

export function CustomFieldModal({
  existingNames,
  actionTag,
  fields,
  onCreate,
  onClose,
}: {
  /** 이미 쓰는 custom 필드 이름들 (중복 방지). */
  existingNames: readonly string[];
  /** 현재 trigger의 action tag (appliesTo로 기록). null = 모든 동작. */
  actionTag: string | null;
  /** 폼의 필드 카탈로그 — 파라미터 드롭다운의 "이 거래에서 가져오기" 항목. */
  fields: readonly FieldOption[];
  onCreate: (draft: CustomFieldDraft) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState("");
  const [label, setLabel] = useState("");
  const [method, setMethod] = useState<MethodSpec>(METHOD_CATALOG[0]);
  // 파라미터 값은 메서드 템플릿이 기본값 — 메서드를 바꾸면 그 템플릿으로 리셋.
  const [params, setParams] = useState<Record<string, string>>(() => paramStrings(METHOD_CATALOG[0]));
  const [projection, setProjection] = useState(METHOD_CATALOG[0].projection);

  // 드롭다운 항목: 거래 레벨 값 + (액션의 기본 필드들 → `$.action.*` 셀렉터).
  const selectorOptions = useMemo<SelectorOption[]>(() => {
    const action = fields
      .filter((f) => f.source === "base" && f.path.startsWith("context.") && f.fieldKind.startsWith("primitive."))
      .map((f) => ({ sel: `$.action.${f.path.slice("context.".length)}`, label: f.label }));
    return [...ROOT_OPTIONS, ...action];
  }, [fields]);

  const nameErr = !name
    ? null
    : !NAME_RE.test(name)
      ? "영문/숫자/_ 만, 숫자로 시작 불가"
      : existingNames.includes(name)
        ? "이미 있는 이름이에요"
        : null;
  const canCreate = name.length > 0 && !nameErr;

  const pickMethod = (m: MethodSpec) => {
    setMethod(m);
    setParams(paramStrings(m));
    setProjection(m.projection);
  };

  const create = () => {
    if (!canCreate) return;
    onCreate({
      name,
      field: {
        type: method.type,
        label: { ko: label || name, en: name },
        appliesTo: actionTag ? [actionTag] : [],
        method: method.method,
        projection,
        params: Object.fromEntries(
          Object.entries(params).map(([k, v]) => [k, parseParam(v)]),
        ),
      },
    });
    onClose();
  };

  return (
    <div className="cfm-bd" role="dialog" aria-modal onClick={onClose}>
      <div className="cfm" onClick={(e) => e.stopPropagation()}>
        <div className="cfm-h">
          새 보강 필드 만들기
          <button type="button" className="pf-iconbtn" onClick={onClose} aria-label="닫기">
            ✕
          </button>
        </div>
        <p className="cfm-sub">
          policy-server 메서드를 호출해 채워지는 <code>context.custom.*</code> 필드를 정의해요.
          저장 시 manifest에 자동 반영됩니다.
        </p>

        <label className="cfm-row">
          <span className="cfm-label">필드 이름</span>
          <span className="cfm-name">
            <code>context.custom.</code>
            <input
              className={`pf-val${nameErr ? " invalid" : ""}`}
              value={name}
              autoFocus
              onChange={(e) => setName(e.target.value.trim())}
              placeholder="myRiskScore"
            />
          </span>
        </label>
        {nameErr && <div className="cfm-err">{nameErr}</div>}

        <label className="cfm-row">
          <span className="cfm-label">표시 이름</span>
          <input
            className="pf-val wide"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="예: 상대 주소 위험 점수 (비우면 필드 이름)"
          />
        </label>

        <label className="cfm-row">
          <span className="cfm-label">메서드</span>
          <select
            className="pf-select"
            value={method.method}
            onChange={(e) => {
              const m = METHOD_CATALOG.find((x) => x.method === e.target.value);
              if (m) pickMethod(m);
            }}
          >
            {METHOD_CATALOG.map((m) => (
              <option key={m.method} value={m.method}>
                {m.label} — {m.method}
              </option>
            ))}
          </select>
        </label>
        <div className="cfm-desc">
          {method.desc}
          {method.mock && <span className="cfm-mock"> · 서버 미구현 예시 — 저장은 되지만 아직 값이 채워지지 않아요</span>}
        </div>

        <div className="cfm-params">
          <div className="cfm-params-h">
            서버에 넘길 값 <span className="cfm-hint">각 항목을 이 거래의 어떤 값으로 채울지 골라요</span>
          </div>
          {Object.entries(params).map(([key, v]) => (
            <div key={key} className="cfm-row">
              <span className="cfm-label">{PARAM_KEY_KO[key] ?? key}</span>
              <ParamPicker
                value={v}
                options={selectorOptions}
                onChange={(next) => setParams((p) => ({ ...p, [key]: next }))}
              />
            </div>
          ))}
          <div className="cfm-row">
            <span className="cfm-label">결과 타입</span>
            <span className="cfm-type">
              {method.type === "decimal" ? "소수" : method.type === "Long" ? "숫자" : method.type === "Bool" ? "참/거짓" : "문자"}
            </span>
          </div>
          <details className="cfm-adv">
            <summary>고급 (셀렉터 원문·결과 위치)</summary>
            {Object.entries(params).map(([key, v]) => (
              <label key={key} className="cfm-row">
                <span className="cfm-label mono">{key}</span>
                <input
                  className="pf-val wide mono"
                  value={v}
                  onChange={(e) => setParams((p) => ({ ...p, [key]: e.target.value }))}
                />
              </label>
            ))}
            <label className="cfm-row">
              <span className="cfm-label mono">결과 위치</span>
              <input
                className="pf-val wide mono"
                value={projection}
                onChange={(e) => setProjection(e.target.value)}
              />
            </label>
          </details>
        </div>

        <div className="cfm-actions">
          <button type="button" className="pf-add-cond" onClick={onClose}>
            취소
          </button>
          <button type="button" className="cfm-create" disabled={!canCreate} onClick={create}>
            필드 만들기
          </button>
        </div>
      </div>
    </div>
  );
}

/** One param's value picker: a labeled dropdown of transaction values, with
 *  "고정값"/"셀렉터 직접 입력" as escape hatches (raw `$.…` never required). */
function ParamPicker({
  value,
  options,
  onChange,
}: {
  value: string;
  options: SelectorOption[];
  onChange: (v: string) => void;
}) {
  const known = options.find((o) => o.sel === value);
  const mode = known ? "known" : value.startsWith("$.") ? "raw" : "lit";
  return (
    <span className="cfm-pick">
      <select
        className="pf-select"
        value={known ? value : mode === "raw" ? "__raw" : "__lit"}
        onChange={(e) => {
          const v = e.target.value;
          if (v === "__lit") onChange("");
          else if (v === "__raw") onChange(value.startsWith("$.") ? value : "$.action.");
          else onChange(v);
        }}
      >
        <optgroup label="이 거래에서 가져오기">
          {options.map((o) => (
            <option key={o.sel} value={o.sel}>
              {o.label}
            </option>
          ))}
        </optgroup>
        <option value="__lit">고정값 직접 입력…</option>
        <option value="__raw">셀렉터 직접 입력 (고급)…</option>
      </select>
      {mode !== "known" && (
        <input
          className="pf-val mono"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={mode === "raw" ? "$.action.…" : "고정값 (예: 6)"}
        />
      )}
    </span>
  );
}

/** Param template → editable string form (`{literal: 6}` → `6`). */
function paramStrings(m: MethodSpec): Record<string, string> {
  return Object.fromEntries(
    Object.entries(m.params).map(([k, v]) => [
      k,
      typeof v === "object" && v !== null && "literal" in v ? String(v.literal) : String(v),
    ]),
  );
}

/** Editable string → ParamSpec: `$.`-prefixed stays a selector; otherwise a
 *  literal (number when numeric, true/false when boolean, else string). */
function parseParam(v: string): string | { literal: number | string | boolean } {
  const t = v.trim();
  if (t.startsWith("$.")) return t;
  if (t === "true" || t === "false") return { literal: t === "true" };
  if (t !== "" && !Number.isNaN(Number(t))) return { literal: Number(t) };
  return { literal: t };
}
