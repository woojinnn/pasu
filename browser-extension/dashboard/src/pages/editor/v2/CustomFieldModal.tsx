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

/** 내부 필드 이름 자동 생성: 메서드 꼬리를 camelCase로, 충돌 시 숫자 suffix.
 *  (`address.risk_score` → `riskScore`, 이미 있으면 `riskScore2`, …) */
function autoName(methodName: string, existing: readonly string[]): string {
  const tail = methodName.split(".").pop() ?? "value";
  const base = tail.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
  if (!existing.includes(base)) return base;
  for (let i = 2; ; i++) {
    const cand = `${base}${i}`;
    if (!existing.includes(cand)) return cand;
  }
}

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
  const [method, setMethod] = useState<MethodSpec>(METHOD_CATALOG[0]);
  // 표시 이름은 메서드 라벨이 기본값 — 사용자가 고치기 전까지 메서드를 따라간다.
  const [label, setLabel] = useState(METHOD_CATALOG[0].label.replace(/\s*\(예시\)\s*$/, ""));
  const [labelTouched, setLabelTouched] = useState(false);
  // 파라미터 값은 메서드 템플릿이 기본값 — 메서드를 바꾸면 그 템플릿으로 리셋.
  const [params, setParams] = useState<Record<string, string>>(() => paramStrings(METHOD_CATALOG[0]));
  const [projection, setProjection] = useState(METHOD_CATALOG[0].projection);

  // 내부 필드 이름(manifest의 id)은 메서드에서 자동 생성 — 사용자는 안 만진다.
  const name = useMemo(() => autoName(method.method, existingNames), [method.method, existingNames]);

  // 드롭다운 항목: 거래 레벨 값 + (액션의 기본 필드들 → `$.action.*` 셀렉터).
  const selectorOptions = useMemo<SelectorOption[]>(() => {
    const action = fields
      .filter((f) => f.source === "base" && f.path.startsWith("context.") && f.fieldKind.startsWith("primitive."))
      .map((f) => ({ sel: `$.action.${f.path.slice("context.".length)}`, label: f.label }));
    return [...ROOT_OPTIONS, ...action];
  }, [fields]);

  const canCreate = label.trim().length > 0;

  const pickMethod = (m: MethodSpec) => {
    setMethod(m);
    setParams(paramStrings(m));
    setProjection(m.projection);
    if (!labelTouched) setLabel(m.label.replace(/\s*\(예시\)\s*$/, ""));
  };

  const create = () => {
    if (!canCreate) return;
    onCreate({
      name,
      field: {
        type: method.type,
        label: { ko: label.trim() || name, en: name },
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
          서버에서 조회한 값을 조건에 쓸 수 있는 필드로 만들어요 — 무엇을 조회할지 고르고,
          이 거래의 어떤 값을 넘길지 정하고, 받은 값에 이름을 붙이면 끝.
        </p>

        <div className="cfm-step">① 무엇을 조회할까요?</div>
        <label className="cfm-row">
          <span className="cfm-label">조회</span>
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
                {m.label}
              </option>
            ))}
          </select>
        </label>
        <div className="cfm-desc">
          {method.desc}
          {method.mock && <span className="cfm-mock"> · 서버 미구현 예시 — 저장은 되지만 아직 값이 채워지지 않아요</span>}
        </div>

        <div className="cfm-params">
          <div className="cfm-step">② 서버에 무엇을 넘길까요?</div>
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

        <div className="cfm-step">③ 받은 값을 뭐라고 부를까요?</div>
        <label className="cfm-row">
          <span className="cfm-label">이름</span>
          <input
            className="pf-val wide"
            value={label}
            autoFocus
            onChange={(e) => {
              setLabel(e.target.value);
              setLabelTouched(true);
            }}
            placeholder="예: 상대 주소 위험 점수"
          />
        </label>
        <div className="cfm-autoname">
          필드 선택기와 다이어그램에 이 이름으로 나와요 · 저장 이름은 자동:{" "}
          <code>context.custom.{name}</code>
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
