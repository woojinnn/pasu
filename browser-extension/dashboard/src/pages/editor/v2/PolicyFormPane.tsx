/**
 * PolicyFormPane — the "폼으로 만들기" editor surface.
 *
 * Edits a {@link FormModel} (the constrained subset) and keeps the parent's
 * `cedarText` + `ir` in sync: every change rebuilds the IR via `formToIr` and
 * (debounced) renders Cedar via `blocksToText`, then calls `onChange`. Mirrors
 * the Block tab's (WorkspaceV9) contract so the editor wiring is identical.
 *
 * Sections: 검사 대상 (trigger) / 조건 (when, AND of OR, per-group NOT) / 예외
 * (unless) / 알림. Right side = a live read-only policy.cedar preview. Beyond the
 * subset (deep nesting, if/then/else, …) the editor hands off to the Block tab.
 */
import { useEffect, useMemo, useRef, useState } from "react";

import { blocksToText } from "../../../cedar";
import type { PolicyIR } from "../../../cedar/blocks/ir";
import { exprToText } from "../../../cedar/diagram/PolicyDiagram";
import {
  emptyFormModel,
  fieldsForTrigger,
  formToIr,
  irToForm,
  KNOWN_ACTIONS,
  leafToExpr,
  operatorsFor,
  valueKindForField,
  type FieldOption,
  type FormCondition,
  type FormModel,
  type FormOp,
  type FormValue,
  type GroupOp,
} from "../../../cedar/form";
import { generateManifest } from "../../../editor-v9/manifest-gen";

import { FieldCombobox } from "./FieldCombobox";

import "./policy-form.css";

export interface PolicyFormPaneProps {
  initialModel?: FormModel | null;
  onChange: (next: { cedarText: string; ir: PolicyIR; model: FormModel }) => void;
}

const OP_LABEL: Record<FormOp, string> = {
  "==": "=",
  "!=": "≠",
  "<": "<",
  "<=": "≤",
  ">": ">",
  ">=": "≥",
  contains: "포함",
  in: "다음 중 하나",
};

/** Ops that compare two scalars — these can take a field-vs-field RHS. */
const SCALAR_OPS = new Set<FormOp>(["==", "!=", "<", "<=", ">", ">="]);

/** Well-known comparison target offered alongside the catalog fields. */
const PRINCIPAL_ADDRESS: FieldOption = {
  path: "principal.address",
  label: "내 지갑 주소",
  role: "address",
  fieldKind: "primitive.String",
  source: "base",
};

function defaultValueOfKind(kind: FormValue["kind"]): FormValue {
  switch (kind) {
    case "bool":
      return { kind: "bool", value: true };
    case "long":
      return { kind: "long", value: 0 };
    case "decimal":
      return { kind: "decimal", value: "0" };
    case "set":
      return { kind: "set", values: [] };
    case "field":
      return { kind: "field", path: PRINCIPAL_ADDRESS.path };
    default:
      return { kind: "string", value: "" };
  }
}

/** Value kind for a (field, op): `in` is always a set, else the field's kind. */
function valueKindFor(field: FieldOption | undefined, op: FormOp): FormValue["kind"] {
  if (op === "in") return "set";
  return field ? valueKindForField(field.fieldKind) : "string";
}

/** EVM address shape — used to validate/hint address-typed value inputs. */
const ADDR_RE = /^0x[0-9a-fA-F]{40}$/;
const isAddr = (s: string) => s === "" || ADDR_RE.test(s.trim());

/** Known enum value suggestions (no machine-readable enum list exists; these are
 *  the ones we're confident about — shown as datalist hints, not enforced). */
const ENUM_SUGGESTIONS: Record<string, string[]> = {
  "context.direction.kind": ["exact_input", "exact_output"],
  "context.side": ["long", "short"],
};

/** How a string-typed value should be entered, refined from the field's role. */
type StringFlavor = "address" | "enum" | "plain";
function stringFlavor(field: FieldOption | undefined): StringFlavor {
  if (!field) return "plain";
  if (field.role === "address") return "address";
  if (field.role === "enum") return "enum";
  return "plain";
}

function newCond(fields: FieldOption[]): FormCondition {
  return { fieldPath: fields[0]?.path ?? "", op: "==", value: defaultValueOfKind("string"), joiner: "and" };
}

export function PolicyFormPane({ initialModel, onChange }: PolicyFormPaneProps) {
  const [model, setModel] = useState<FormModel>(() => initialModel ?? emptyFormModel());
  const [cedar, setCedar] = useState<string>("");
  const [cedarError, setCedarError] = useState<string | null>(null);

  const fields = useMemo(() => fieldsForTrigger(model.trigger), [model.trigger]);
  const rhsFields = useMemo(() => [PRINCIPAL_ADDRESS, ...fields], [fields]);
  const fieldByPath = useMemo(() => {
    const m = new Map<string, FieldOption>();
    for (const f of fields) m.set(f.path, f);
    return m;
  }, [fields]);

  const ir = useMemo(() => formToIr(model), [model]);

  // Validity badge: cedar rendered + manifest generates cleanly + the IR is
  // still form-representable (round-trips). Manifest gen is pure/sync.
  const manifestErrors = useMemo(
    () => generateManifest(ir, undefined, { id: model.id, severity: model.severity }).errors,
    [ir, model.id, model.severity],
  );
  const roundTrips = useMemo(() => irToForm(ir) !== null, [ir]);
  const valid = !cedarError && manifestErrors.length === 0;
  const enrichCount = useMemo(
    () =>
      new Set(
        [...model.when, ...model.unless]
          .map((c) => c.fieldPath)
          .filter((p) => p.startsWith("context.custom.")),
      ).size,
    [model.when, model.unless],
  );
  const triggerText = model.trigger.kind === "actionEq" ? model.trigger.id : "모든 동작";

  // Keep onChange in a ref so the sync effect depends only on `ir`.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  // Rebuild Cedar (debounced) and push {cedarText, ir, model} up.
  useEffect(() => {
    let cancelled = false;
    const t = window.setTimeout(() => {
      void blocksToText(ir)
        .then((text) => {
          if (cancelled) return;
          setCedar(text);
          setCedarError(null);
          onChangeRef.current({ cedarText: text, ir, model });
        })
        .catch((err: unknown) => {
          if (cancelled) return;
          setCedarError(err instanceof Error ? err.message : "Cedar 변환 실패");
        });
    }, 200);
    return () => {
      cancelled = true;
      window.clearTimeout(t);
    };
  }, [ir, model]);

  const patch = (next: Partial<FormModel>) => setModel((m) => ({ ...m, ...next }));

  return (
    <div className="pf-pane">
      <div className="pf-form">
        {/* ① 검사 대상 */}
        <section className="pf-section">
          <h3 className="pf-h">
            <span className="pf-num">1</span> 무엇을 검사하나요? <span className="pf-sub">어떤 거래에 적용할지 골라요</span>
          </h3>
          <div className="pf-row">
            <label className="pf-label">검사 대상</label>
            <select
              className="pf-select"
              value={model.trigger.kind === "actionEq" ? `${model.trigger.entityType}::${model.trigger.id}` : "*"}
              onChange={(e) => {
                if (e.target.value === "*") return patch({ trigger: { kind: "any" } });
                const a = KNOWN_ACTIONS.find((k) => `${k.entityType}::${k.id}` === e.target.value);
                if (a) patch({ trigger: { kind: "actionEq", entityType: a.entityType, id: a.id } });
              }}
            >
              <option value="*">모든 동작</option>
              {KNOWN_ACTIONS.map((a) => (
                <option key={`${a.entityType}::${a.id}`} value={`${a.entityType}::${a.id}`}>
                  {a.label}
                </option>
              ))}
            </select>
          </div>
        </section>

        {/* ② 조건 (when) */}
        <section className="pf-section">
          <h3 className="pf-h">
            <span className="pf-num">2</span> 언제 위험한가요? <span className="pf-sub">조건 추가, 여러 개면 모두 참(AND)</span>
          </h3>
          <ConditionEditor
            conds={model.when}
            fields={fields}
            rhsFields={rhsFields}
            fieldByPath={fieldByPath}
            emptyHint="조건이 없으면 이 동작은 항상 막힙니다."
            onChange={(when) => patch({ when })}
          />
        </section>

        {/* ③ 예외 (unless) */}
        <section className="pf-section">
          <h3 className="pf-h">
            <span className="pf-num">3</span> 예외가 있나요? <span className="pf-sub">단, 다음이면 제외(unless) · 선택</span>
          </h3>
          <ConditionEditor
            conds={model.unless}
            fields={fields}
            rhsFields={rhsFields}
            fieldByPath={fieldByPath}
            emptyHint="예외 없음 — 위 조건이 맞으면 항상 적용됩니다."
            onChange={(unless) => patch({ unless })}
          />
        </section>

        {/* ④ 알림 */}
        <section className="pf-section">
          <h3 className="pf-h">
            <span className="pf-num">4</span> 어떻게 알릴까요? <span className="pf-sub">이름·심각도·사유</span>
          </h3>
          <div className="pf-row">
            <label className="pf-label">규칙 id</label>
            <input className="pf-input" value={model.id} onChange={(e) => patch({ id: e.target.value })} />
          </div>
          <div className="pf-row">
            <label className="pf-label">심각도</label>
            <div className="pf-sev">
              <button
                type="button"
                className={`pf-sev-btn warn${model.severity === "warn" ? " on" : ""}`}
                onClick={() => patch({ severity: "warn" })}
              >
                ● 경고
              </button>
              <button
                type="button"
                className={`pf-sev-btn deny${model.severity === "deny" ? " on" : ""}`}
                onClick={() => patch({ severity: "deny" })}
              >
                ● 차단
              </button>
            </div>
          </div>
          <div className="pf-row">
            <label className="pf-label">사유</label>
            <input className="pf-input" value={model.reason} onChange={(e) => patch({ reason: e.target.value })} placeholder="예: 고위험 동작 차단" />
          </div>
        </section>

        <div className={`pf-status${valid ? " ok" : " bad"}`}>
          <span className="pf-status-main">
            {valid ? "✓ 유효한 정책 · .cedar와 manifest 짝 맞음" : "⚠ " + (cedarError ?? manifestErrors[0]?.message ?? "유효하지 않음")}
          </span>
          <span className="pf-status-meta">
            trigger: {triggerText} · 보강필드: {enrichCount > 0 ? `${enrichCount}개` : "없음"} ·
            round-trip: {roundTrips ? "통과" : "불가"}
          </span>
        </div>
      </div>

      {/* 우측 라이브 Cedar */}
      <aside className="pf-cedar">
        <div className="pf-cedar-head">
          policy.cedar
          <span className={`pf-sync${cedarError ? " err" : ""}`}>{cedarError ? "변환 오류" : "폼과 동기화됨"}</span>
        </div>
        <pre className="pf-cedar-body">{cedarError ?? cedar}</pre>
      </aside>
    </div>
  );
}

// ── condition editor — a flat list with per-row AND/OR + NOT ─────────────────

function ConditionEditor({
  conds,
  fields,
  rhsFields,
  fieldByPath,
  emptyHint,
  onChange,
}: {
  conds: FormCondition[];
  fields: FieldOption[];
  rhsFields: FieldOption[];
  fieldByPath: Map<string, FieldOption>;
  emptyHint: string;
  onChange: (conds: FormCondition[]) => void;
}) {
  const update = (i: number, c: FormCondition) => onChange(conds.map((x, j) => (j === i ? c : x)));

  const onPickField = (i: number, path: string) => {
    const field = fieldByPath.get(path);
    const op = (field ? operatorsFor(field.fieldKind)[0] : "==") as FormOp;
    update(i, { ...conds[i], fieldPath: path, op, value: defaultValueOfKind(valueKindFor(field, op)) });
  };

  const onPickOp = (i: number, op: FormOp) => {
    const c = conds[i];
    // Keep a field-vs-field RHS when the new op still compares scalars.
    if (c.value.kind === "field" && SCALAR_OPS.has(op)) return update(i, { ...c, op });
    const field = fieldByPath.get(c.fieldPath);
    const wantKind = valueKindFor(field, op);
    const value = c.value.kind === wantKind ? c.value : defaultValueOfKind(wantKind);
    update(i, { ...c, op, value });
  };

  return (
    <>
      {conds.length === 0 && <div className="pf-empty-cond">{emptyHint}</div>}
      {conds.map((c, i) => (
        <ConditionRow
          key={i}
          cond={c}
          first={i === 0}
          field={fieldByPath.get(c.fieldPath)}
          fields={fields}
          rhsFields={rhsFields}
          onJoiner={(joiner) => update(i, { ...c, joiner })}
          onToggleNot={() => update(i, { ...c, not: !c.not })}
          onField={(p) => onPickField(i, p)}
          onOp={(op) => onPickOp(i, op)}
          onValue={(value) => update(i, { ...c, value })}
          onRemove={() => onChange(conds.filter((_, j) => j !== i))}
        />
      ))}
      {conds.length > 1 && (
        <div className="pf-precedence">AND가 OR보다 먼저 묶여요 — 예: A 그리고 B 또는 C = (A 그리고 B) 또는 C</div>
      )}
      <button type="button" className="pf-add-cond" onClick={() => onChange([...conds, newCond(fields)])}>
        + 조건 추가
      </button>
    </>
  );
}

// ── one condition row ──────────────────────────────────────────────────────

function ConditionRow({
  cond,
  first,
  field,
  fields,
  rhsFields,
  onJoiner,
  onToggleNot,
  onField,
  onOp,
  onValue,
  onRemove,
}: {
  cond: FormCondition;
  first: boolean;
  field: FieldOption | undefined;
  fields: FieldOption[];
  rhsFields: FieldOption[];
  onJoiner: (op: GroupOp) => void;
  onToggleNot: () => void;
  onField: (path: string) => void;
  onOp: (op: FormOp) => void;
  onValue: (v: FormValue) => void;
  onRemove: () => void;
}) {
  const ops = field ? operatorsFor(field.fieldKind) : (["=="] as FormOp[]);
  const chip = cond.fieldPath ? rowChip(cond) : "…";
  const canField = SCALAR_OPS.has(cond.op);
  const fieldMode = cond.value.kind === "field";
  return (
    <div className="pf-leaf">
      {first ? (
        <span className="pf-join-tag">조건</span>
      ) : (
        <select className={`pf-join ${cond.joiner}`} value={cond.joiner} onChange={(e) => onJoiner(e.target.value as GroupOp)}>
          <option value="and">그리고(AND)</option>
          <option value="or">또는(OR)</option>
        </select>
      )}
      <button
        type="button"
        className={`pf-not${cond.not ? " on" : ""}`}
        onClick={onToggleNot}
        title="이 조건을 부정 — NOT"
      >
        아니다
      </button>
      <FieldCombobox value={cond.fieldPath} fields={fields} onChange={onField} />
      <select className="pf-leaf-op" value={cond.op} onChange={(e) => onOp(e.target.value as FormOp)}>
        {ops.map((op) => (
          <option key={op} value={op}>
            {OP_LABEL[op]}
          </option>
        ))}
      </select>
      {canField && (
        <button
          type="button"
          className="pf-mode"
          onClick={() =>
            onValue(fieldMode ? defaultValueOfKind(valueKindFor(field, cond.op)) : { kind: "field", path: rhsFields[0]?.path ?? "principal.address" })
          }
          title={fieldMode ? "고정 값으로" : "다른 필드와 비교"}
        >
          {fieldMode ? "필드" : "값"}
        </button>
      )}
      {fieldMode ? (
        <FieldCombobox
          value={cond.value.kind === "field" ? cond.value.path : ""}
          fields={rhsFields}
          onChange={(p) => onValue({ kind: "field", path: p })}
        />
      ) : (
        <ValueInput value={cond.value} field={field} onChange={onValue} />
      )}
      <span className="pf-leaf-chip">{chip}</span>
      <button type="button" className="pf-x" onClick={onRemove} aria-label="조건 삭제">
        ×
      </button>
    </div>
  );
}

/** Inline Cedar chip for a condition (reflects NOT). */
function rowChip(cond: FormCondition): string {
  try {
    const t = exprToText(leafToExpr(cond));
    return cond.not ? `!(${t})` : t;
  } catch {
    return "…";
  }
}

// ── value widget by kind + field type (literal kinds only) ──────────────────

function ValueInput({
  value,
  field,
  onChange,
}: {
  value: FormValue;
  field: FieldOption | undefined;
  onChange: (v: FormValue) => void;
}) {
  const unit = field?.unit;
  switch (value.kind) {
    case "bool":
      return (
        <select className="pf-val" value={String(value.value)} onChange={(e) => onChange({ kind: "bool", value: e.target.value === "true" })}>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      );
    case "long":
      return (
        <span className="pf-val-wrap">
          <input
            className="pf-val num"
            type="number"
            value={value.value}
            onChange={(e) => onChange({ kind: "long", value: Number(e.target.value) })}
          />
          {unit && <span className="pf-unit">{unit}</span>}
        </span>
      );
    case "decimal":
      return (
        <span className="pf-val-wrap">
          <input className="pf-val num" value={value.value} onChange={(e) => onChange({ kind: "decimal", value: e.target.value })} placeholder="0.05" />
          {unit && <span className="pf-unit">{unit}</span>}
        </span>
      );
    case "set": {
      // `in` over an address field → validate each entry as an EVM address.
      const addr = field?.role === "address";
      const bad = addr && value.values.some((v) => !isAddr(v));
      return (
        <input
          className={`pf-val wide${addr ? " mono" : ""}${bad ? " invalid" : ""}`}
          value={value.values.join(", ")}
          onChange={(e) =>
            onChange({ kind: "set", values: e.target.value.split(",").map((s) => s.trim()).filter(Boolean) })
          }
          placeholder={addr ? "0x…, 0x…" : "값1, 값2, …"}
        />
      );
    }
    case "field":
      return null; // handled by LeafRow's field combobox
    default: {
      // string — refine by the field's semantic type (address / enum / plain).
      const flavor = stringFlavor(field);
      if (flavor === "address") {
        const bad = !isAddr(value.value);
        return (
          <input
            className={`pf-val mono${bad ? " invalid" : ""}`}
            value={value.value}
            onChange={(e) => onChange({ kind: "string", value: e.target.value })}
            placeholder="0x…"
            spellCheck={false}
          />
        );
      }
      if (flavor === "enum") {
        const listId = `enum-${field?.path ?? ""}`;
        const sugg = field ? ENUM_SUGGESTIONS[field.path] : undefined;
        return (
          <>
            <input
              className="pf-val"
              list={sugg ? listId : undefined}
              value={value.value}
              onChange={(e) => onChange({ kind: "string", value: e.target.value })}
              placeholder={field?.desc ? field.desc.slice(0, 24) : "값"}
            />
            {sugg && (
              <datalist id={listId}>
                {sugg.map((s) => (
                  <option key={s} value={s} />
                ))}
              </datalist>
            )}
          </>
        );
      }
      return <input className="pf-val" value={value.value} onChange={(e) => onChange({ kind: "string", value: e.target.value })} />;
    }
  }
}
