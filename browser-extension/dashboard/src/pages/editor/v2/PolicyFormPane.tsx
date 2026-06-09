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
import { naturalCondition } from "../../../cedar/nl";
import { useAddressBook, shortAddress, type AddressEntry } from "../../../hooks/useAddressBook";
import { PolicyDiagram } from "../../../cedar/diagram/PolicyDiagram";
import { AddressInput, AddressSetInput } from "./AddressPicker";
import {
  emptyFormModel,
  fieldsForTrigger,
  formToIr,
  irToForm,
  isGroupNode,
  KNOWN_ACTIONS,
  ACTION_GROUPS,
  operatorsFor,
  valueKindForField,
  type FieldOption,
  type FormCondition,
  type FormGroupNode,
  type FormModel,
  type FormNode,
  type FormOp,
  type FormTrigger,
  type FormValue,
  type GroupOp,
} from "../../../cedar/form";
import { generateManifest } from "../../../editor-v9/manifest-gen";

import { FieldCombobox } from "./FieldCombobox";

import "./policy-form.css";

export interface PolicyFormPaneProps {
  initialModel?: FormModel | null;
  /** `manifest` is the effective manifest to persist — the user's hand-edited
   *  override when `manifestOverridden`, otherwise the auto-generated one
   *  (`undefined` = none). */
  onChange: (next: {
    cedarText: string;
    ir: PolicyIR;
    model: FormModel;
    manifest: unknown;
    manifestOverridden: boolean;
  }) => void;
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

/**
 * Move a condition (matched by object identity) to `targetBox` (append) or to the
 * top level (`null`). Removes it from wherever it was; a box left empty is
 * dropped. Pure — returns the new node list. Self-drops are no-ops.
 */
function moveCond(
  nodes: FormNode[],
  cond: FormCondition,
  targetBox: FormGroupNode | null,
): FormNode[] {
  if (targetBox ? targetBox.conds.includes(cond) : nodes.includes(cond)) return nodes; // already there
  const removed: FormNode[] = [];
  for (const n of nodes) {
    if (n === cond) continue; // a top-level leaf being moved
    if (isGroupNode(n)) {
      const conds = n.conds.filter((c) => c !== cond);
      if (conds.length === 0) continue; // box emptied by the move → drop it
      removed.push(conds.length === n.conds.length ? n : { ...n, conds });
    } else removed.push(n);
  }
  const leaf: FormCondition = { ...cond, joiner: "and" };
  if (!targetBox) return [...removed, leaf];
  // `targetBox` never contained `cond` (guarded above), so its identity survived
  // the removal pass and we can match it by reference.
  return removed.map((n) => (n === targetBox ? { ...n, conds: [...n.conds, leaf] } : n));
}

export function PolicyFormPane({ initialModel, onChange }: PolicyFormPaneProps) {
  const [model, setModel] = useState<FormModel>(() => initialModel ?? emptyFormModel());
  // We no longer display the Cedar text (the right pane shows the diagram), but
  // we still build it to push up via onChange and to surface conversion errors.
  const [cedarError, setCedarError] = useState<string | null>(null);

  const fields = useMemo(() => fieldsForTrigger(model.trigger), [model.trigger]);
  const rhsFields = useMemo(() => [PRINCIPAL_ADDRESS, ...fields], [fields]);
  const fieldByPath = useMemo(() => {
    const m = new Map<string, FieldOption>();
    for (const f of fields) m.set(f.path, f);
    return m;
  }, [fields]);
  const addrBook = useAddressBook();
  const ctx = useMemo<EditorCtx>(
    () => ({ fields, rhsFields, fieldByPath, lookupAddr: addrBook.lookup }),
    [fields, rhsFields, fieldByPath, addrBook.lookup],
  );
  // Resolve 0x addresses in the structure diagram to friendly names.
  const humanizeAddrs = (text: string): string =>
    text.replace(/0x[0-9a-fA-F]{40}/g, (m) => {
      const e = addrBook.lookup(m);
      return e ? `${e.name}(${shortAddress(m)})` : m;
    });

  const ir = useMemo(() => formToIr(model), [model]);

  // Validity badge + manifest preview: cedar rendered + manifest generates
  // cleanly + the IR is still form-representable (round-trips). Manifest gen is
  // pure/sync — the same call the save path makes, so the preview matches what
  // gets persisted.
  const gen = useMemo(
    () => generateManifest(ir, undefined, { id: model.id, severity: model.severity }),
    [ir, model.id, model.severity],
  );
  const manifestErrors = gen.errors;
  const roundTrips = useMemo(() => irToForm(ir) !== null, [ir]);
  const valid = !cedarError && manifestErrors.length === 0;
  const [manifestOpen, setManifestOpen] = useState(false);
  // Manual manifest override. `null` = use the auto-generated manifest.
  const [manifestText, setManifestText] = useState<string | null>(null);
  // The effective manifest to persist + its parse error (if the override is
  // invalid JSON we fall back to the auto manifest so save never breaks).
  const { manifest: effectiveManifest, parseErr: manifestParseErr } = useMemo(() => {
    if (manifestText === null) return { manifest: gen.manifest, parseErr: null as string | null };
    try {
      return { manifest: JSON.parse(manifestText) as unknown, parseErr: null as string | null };
    } catch (e) {
      return { manifest: gen.manifest, parseErr: e instanceof Error ? e.message : "JSON 형식 오류" };
    }
  }, [manifestText, gen.manifest]);
  const enrichCount = useMemo(
    () =>
      new Set(
        [...model.when, ...model.unless]
          .flatMap((n) => (isGroupNode(n) ? n.conds : [n]))
          .map((c) => c.fieldPath)
          .filter((p) => p.startsWith("context.custom.")),
      ).size,
    [model.when, model.unless],
  );
  const trig = model.trigger;
  // Cascading trigger picker: 분류(group) → 동작(action). `currentGroup` is
  // `"*"` for the any-action case (no second dropdown).
  const currentAction =
    trig.kind === "actionEq"
      ? KNOWN_ACTIONS.find((k) => k.entityType === trig.entityType && k.id === trig.id)
      : undefined;
  const currentGroup = trig.kind === "actionEq" ? currentAction?.group ?? ACTION_GROUPS[0]?.group ?? "*" : "*";
  const groupActions = ACTION_GROUPS.find((g) => g.group === currentGroup)?.actions ?? [];
  const triggerText = trig.kind === "actionEq" ? currentAction?.label ?? trig.id : "모든 동작";

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
          setCedarError(null);
          onChangeRef.current({
            cedarText: text,
            ir,
            model,
            manifest: effectiveManifest,
            manifestOverridden: manifestText !== null,
          });
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
  }, [ir, model, effectiveManifest, manifestText]);

  const patch = (next: Partial<FormModel>) => setModel((m) => ({ ...m, ...next }));

  /** Change the trigger. Because the available fields are action-specific, any
   *  real change clears the conditions (언제 위험한가요?) and 예외 — they would
   *  reference fields the new action doesn't have. */
  const setTrigger = (next: FormTrigger) =>
    setModel((m) => {
      const same =
        (m.trigger.kind === "any" && next.kind === "any") ||
        (m.trigger.kind === "actionEq" &&
          next.kind === "actionEq" &&
          m.trigger.entityType === next.entityType &&
          m.trigger.id === next.id);
      if (same) return m;
      return { ...m, trigger: next, when: [], unless: [] };
    });

  return (
    <div className="pf-pane">
      <div className="pf-form">
        {/* ① 검사 대상 */}
        <section className="pf-section">
          <h3 className="pf-h">
            <span className="pf-num">1</span> 무엇을 검사하나요? <span className="pf-sub">어떤 거래에 적용할지 골라요</span>
          </h3>
          <div className="pf-row">
            <label className="pf-label">분류</label>
            <select
              className="pf-select"
              value={currentGroup}
              onChange={(e) => {
                const v = e.target.value;
                if (v === "*") return setTrigger({ kind: "any" });
                // Pick a category → default to its first action so the trigger
                // stays valid; the user refines it in the second dropdown.
                const first = ACTION_GROUPS.find((g) => g.group === v)?.actions[0];
                if (first)
                  setTrigger({ kind: "actionEq", entityType: first.entityType, id: first.id });
              }}
            >
              <option value="*">모든 동작</option>
              {ACTION_GROUPS.map((g) => (
                <option key={g.group} value={g.group}>
                  {g.group}
                </option>
              ))}
            </select>
          </div>
          <div className="pf-row">
            <label className="pf-label">동작</label>
            <select
              className="pf-select"
              disabled={currentGroup === "*"}
              value={currentAction ? `${currentAction.entityType}::${currentAction.id}` : ""}
              onChange={(e) => {
                const a = groupActions.find((k) => `${k.entityType}::${k.id}` === e.target.value);
                if (a) setTrigger({ kind: "actionEq", entityType: a.entityType, id: a.id });
              }}
            >
              {currentGroup === "*" ? (
                <option value="">먼저 분류를 골라요</option>
              ) : (
                groupActions.map((a) => (
                  <option key={`${a.entityType}::${a.id}`} value={`${a.entityType}::${a.id}`}>
                    {a.label}
                  </option>
                ))
              )}
            </select>
          </div>
        </section>

        {/* ② 조건 (when) */}
        <section className="pf-section">
          <h3 className="pf-h">
            <span className="pf-num">2</span> 언제 위험한가요? <span className="pf-sub">조건 추가, 여러 개면 모두 참(AND)</span>
          </h3>
          <ConditionEditor
            nodes={model.when}
            ctx={ctx}
            emptyHint="조건이 없으면 이 동작은 항상 막힙니다."
            onChange={(when) => patch({ when })}
          />
        </section>

        {/* ③ 알림 */}
        <section className="pf-section">
          <h3 className="pf-h">
            <span className="pf-num">3</span> 어떻게 알릴까요? <span className="pf-sub">심각도·사유</span>
          </h3>
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

      {/* 우측 라이브 구조 다이어그램 + manifest 미리보기 */}
      <aside className="pf-cedar">
        <div className="pf-cedar-head">
          구조 미리보기
          <span className={`pf-sync${cedarError ? " err" : ""}`}>{cedarError ? "변환 오류" : "폼과 동기화됨"}</span>
        </div>
        <div className="pf-diagram-body">
          <PolicyDiagram ir={ir} humanizeLabel={humanizeAddrs} />
        </div>
        <ManifestPreview
          open={manifestOpen}
          onToggle={() => setManifestOpen((v) => !v)}
          autoManifest={gen.manifest}
          errors={manifestErrors}
          overrideText={manifestText}
          parseErr={manifestParseErr}
          onEdit={() =>
            setManifestText(JSON.stringify(gen.manifest ?? {}, null, 2))
          }
          onChangeText={setManifestText}
          onReset={() => setManifestText(null)}
        />
      </aside>
    </div>
  );
}

/** Collapsible preview/editor of the enrichment manifest. By default it shows
 *  the auto-generated manifest (the exact value `save` persists). "직접 편집"
 *  switches to an editable JSON textarea whose value overrides the auto one;
 *  "자동으로" reverts. Invalid JSON falls back to auto on save (warned inline). */
function ManifestPreview({
  open,
  onToggle,
  autoManifest,
  errors,
  overrideText,
  parseErr,
  onEdit,
  onChangeText,
  onReset,
}: {
  open: boolean;
  onToggle: () => void;
  autoManifest: unknown;
  errors: { message: string }[];
  overrideText: string | null;
  parseErr: string | null;
  onEdit: () => void;
  onChangeText: (v: string) => void;
  onReset: () => void;
}) {
  const editing = overrideText !== null;
  const hasManifest = autoManifest !== undefined;
  const tag = editing
    ? "직접 편집됨"
    : errors.length > 0
      ? `오류 ${errors.length}`
      : hasManifest
        ? "보강 필드 있음"
        : "필요 없음";
  return (
    <div className="pf-manifest">
      <button type="button" className="pf-manifest-head" onClick={onToggle} aria-expanded={open}>
        <span className={`pf-manifest-caret${open ? " open" : ""}`}>▶</span>
        manifest
        <span className={`pf-manifest-tag${editing ? " edited" : ""}`}>{tag}</span>
      </button>
      {open && (
        <div className="pf-manifest-body">
          <div className="pf-manifest-bar">
            {editing ? (
              <>
                <span className={`pf-manifest-status${parseErr ? " err" : " ok"}`}>
                  {parseErr ? `JSON 오류 · 저장 시 자동값 사용` : "직접 편집 중 · 저장 시 이 값 사용"}
                </span>
                <button type="button" className="pf-manifest-btn" onClick={onReset}>
                  ↺ 자동으로
                </button>
              </>
            ) : (
              <>
                <span className="pf-manifest-status">
                  정책에서 자동 생성됨 · 저장 시 이 값 사용
                </span>
                <button type="button" className="pf-manifest-btn" onClick={onEdit}>
                  ✎ 직접 편집
                </button>
              </>
            )}
          </div>

          {editing ? (
            <textarea
              className={`pf-manifest-edit${parseErr ? " invalid" : ""}`}
              value={overrideText}
              onChange={(e) => onChangeText(e.target.value)}
              spellCheck={false}
              rows={12}
            />
          ) : errors.length > 0 ? (
            <div className="pf-manifest-err">
              {errors.map((e, i) => (
                <div key={i}>⚠ {e.message}</div>
              ))}
            </div>
          ) : hasManifest ? (
            <pre className="pf-manifest-json">{JSON.stringify(autoManifest, null, 2)}</pre>
          ) : (
            <div className="pf-manifest-empty">
              이 정책은 <code>context.custom.*</code> 보강 필드를 쓰지 않아 manifest가 필요 없어요.
              직접 편집으로 수동 manifest를 추가할 수도 있어요.
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── condition editor — a flat list with per-row AND/OR + NOT ─────────────────

/** Shared field/operator context threaded to rows and boxes. */
interface EditorCtx {
  fields: FieldOption[];
  rhsFields: FieldOption[];
  fieldByPath: Map<string, FieldOption>;
  /** Resolve an address to a friendly name (my wallet / token), or undefined. */
  lookupAddr: (address: string) => AddressEntry | undefined;
}

/** Re-derive a condition after the user picks a new field. */
function pickFieldCond(c: FormCondition, path: string, fieldByPath: Map<string, FieldOption>): FormCondition {
  const field = fieldByPath.get(path);
  const op = (field ? operatorsFor(field.fieldKind)[0] : "==") as FormOp;
  return { ...c, fieldPath: path, op, value: defaultValueOfKind(valueKindFor(field, op)) };
}

/** Re-derive a condition after the user picks a new operator. */
function pickOpCond(c: FormCondition, op: FormOp, fieldByPath: Map<string, FieldOption>): FormCondition {
  // Keep a field-vs-field RHS when the new op still compares scalars.
  if (c.value.kind === "field" && SCALAR_OPS.has(op)) return { ...c, op };
  const field = fieldByPath.get(c.fieldPath);
  const wantKind = valueKindFor(field, op);
  const value = c.value.kind === wantKind ? c.value : defaultValueOfKind(wantKind);
  return { ...c, op, value };
}

function ConditionEditor({
  nodes,
  ctx,
  emptyHint,
  onChange,
}: {
  nodes: FormNode[];
  ctx: EditorCtx;
  emptyHint: string;
  onChange: (nodes: FormNode[]) => void;
}) {
  const update = (i: number, n: FormNode) => onChange(nodes.map((x, j) => (j === i ? n : x)));
  const removeAt = (i: number) => onChange(nodes.filter((_, j) => j !== i));
  // Wrap a leaf into a 1-condition `(…)` box (the user then adds OR/AND inside).
  const wrap = (i: number) => {
    const n = nodes[i];
    if (isGroupNode(n)) return;
    update(i, { kind: "group", joiner: n.joiner, conds: [{ ...n, joiner: "and" }] });
  };
  // Ungroup: splice a box's conditions back as leaf nodes (first inherits joiner).
  const unwrap = (i: number) => {
    const n = nodes[i];
    if (!isGroupNode(n)) return;
    const inner: FormNode[] = n.conds.map((c, ci) => (ci === 0 ? { ...c, joiner: n.joiner } : c));
    onChange([...nodes.slice(0, i), ...inner, ...nodes.slice(i + 1)]);
  };

  // Drag-and-drop: drag a condition onto a box to move it in, or onto the
  // top-level strip to move it out. The payload is the condition object itself
  // (matched by identity in `moveCond`).
  const [drag, setDrag] = useState<FormCondition | null>(null);
  const dropTo = (box: FormGroupNode | null) => {
    if (drag) onChange(moveCond(nodes, drag, box));
    setDrag(null);
  };

  return (
    <>
      {nodes.length === 0 && <div className="pf-empty-cond">{emptyHint}</div>}
      {nodes.map((n, i) =>
        isGroupNode(n) ? (
          <GroupBox
            key={i}
            group={n}
            first={i === 0}
            ctx={ctx}
            dragging={drag !== null}
            onDragStartCond={(c) => setDrag(c)}
            onDropInto={() => dropTo(n)}
            onJoiner={(joiner) => update(i, { ...n, joiner })}
            onToggleNot={() => update(i, { ...n, not: !n.not })}
            onConds={(conds) => update(i, { ...n, conds })}
            onUngroup={() => unwrap(i)}
            onRemove={() => removeAt(i)}
          />
        ) : (
          <ConditionRow
            key={i}
            cond={n}
            first={i === 0}
            ctx={ctx}
            onDragStart={() => setDrag(n)}
            onJoiner={(joiner) => update(i, { ...n, joiner })}
            onToggleNot={() => update(i, { ...n, not: !n.not })}
            onField={(p) => update(i, pickFieldCond(n, p, ctx.fieldByPath))}
            onOp={(op) => update(i, pickOpCond(n, op, ctx.fieldByPath))}
            onValue={(value) => update(i, { ...n, value })}
            onGroup={() => wrap(i)}
            onRemove={() => removeAt(i)}
          />
        ),
      )}
      {drag !== null && (
        <div className="pf-dropstrip" onDragOver={(e) => e.preventDefault()} onDrop={() => dropTo(null)}>
          여기에 놓아 묶음에서 빼기 (최상위로)
        </div>
      )}
      {nodes.length > 1 && (
        <div className="pf-precedence">AND가 OR보다 먼저 묶여요 · 괄호로 묶으려면 행의 “묶기”</div>
      )}
      <div className="pf-add-row">
        <button type="button" className="pf-add-cond" onClick={() => onChange([...nodes, newCond(ctx.fields)])}>
          + 조건 추가
        </button>
        <button
          type="button"
          className="pf-add-cond"
          onClick={() => onChange([...nodes, { kind: "group", joiner: "and", conds: [newCond(ctx.fields)] }])}
        >
          + 묶음 ( ) 추가
        </button>
      </div>
    </>
  );
}

// ── a (…) group box ─────────────────────────────────────────────────────────

function GroupBox({
  group,
  first,
  ctx,
  dragging,
  onDragStartCond,
  onDropInto,
  onJoiner,
  onToggleNot,
  onConds,
  onUngroup,
  onRemove,
}: {
  group: FormGroupNode;
  first: boolean;
  ctx: EditorCtx;
  dragging: boolean;
  onDragStartCond: (c: FormCondition) => void;
  onDropInto: () => void;
  onJoiner: (op: GroupOp) => void;
  onToggleNot: () => void;
  onConds: (conds: FormCondition[]) => void;
  onUngroup: () => void;
  onRemove: () => void;
}) {
  const { conds } = group;
  const updateCond = (i: number, c: FormCondition) => onConds(conds.map((x, j) => (j === i ? c : x)));
  return (
    <div
      className={`pf-box${group.not ? " neg" : ""}${dragging ? " droppable" : ""}`}
      onDragOver={dragging ? (e) => e.preventDefault() : undefined}
      onDrop={dragging ? (e) => { e.preventDefault(); onDropInto(); } : undefined}
    >
      <div className="pf-box-head">
        {!first && (
          <select className={`pf-ctl pf-join ${group.joiner}`} value={group.joiner} onChange={(e) => onJoiner(e.target.value as GroupOp)}>
            <option value="and">그리고(AND)</option>
            <option value="or">또는(OR)</option>
          </select>
        )}
        <span className="pf-box-label">( 묶음 )</span>
        <button type="button" className={`pf-ctl pf-not${group.not ? " on" : ""}`} onClick={onToggleNot} title="이 묶음을 부정 — NOT">
          아니다
        </button>
        <span className="pf-spc" />
        <button type="button" className="pf-box-act" onClick={onUngroup}>
          해제
        </button>
        <button type="button" className="pf-iconbtn danger" onClick={onRemove} aria-label="묶음 삭제" title="묶음 삭제">
          ✕
        </button>
      </div>
      {conds.map((c, i) => (
        <ConditionRow
          key={i}
          cond={c}
          first={i === 0}
          ctx={ctx}
          onDragStart={() => onDragStartCond(c)}
          onJoiner={(joiner) => updateCond(i, { ...c, joiner })}
          onToggleNot={() => updateCond(i, { ...c, not: !c.not })}
          onField={(p) => updateCond(i, pickFieldCond(c, p, ctx.fieldByPath))}
          onOp={(op) => updateCond(i, pickOpCond(c, op, ctx.fieldByPath))}
          onValue={(value) => updateCond(i, { ...c, value })}
          onRemove={() => {
            const next = conds.filter((_, j) => j !== i);
            if (next.length === 0) onRemove(); // emptying the box removes it
            else onConds(next);
          }}
        />
      ))}
      <button type="button" className="pf-or-btn" onClick={() => onConds([...conds, newCond(ctx.fields)])}>
        + 조건
      </button>
    </div>
  );
}

// ── one condition row ──────────────────────────────────────────────────────

function ConditionRow({
  cond,
  first,
  ctx,
  onDragStart,
  onJoiner,
  onToggleNot,
  onField,
  onOp,
  onValue,
  onGroup,
  onRemove,
}: {
  cond: FormCondition;
  first: boolean;
  ctx: EditorCtx;
  onDragStart?: () => void;
  onJoiner: (op: GroupOp) => void;
  onToggleNot: () => void;
  onField: (path: string) => void;
  onOp: (op: FormOp) => void;
  onValue: (v: FormValue) => void;
  onGroup?: () => void;
  onRemove: () => void;
}) {
  const field = ctx.fieldByPath.get(cond.fieldPath);
  const ops = field ? operatorsFor(field.fieldKind) : (["=="] as FormOp[]);
  const chip = cond.fieldPath ? condChip(cond, ctx) : "…";
  const canField = SCALAR_OPS.has(cond.op);
  const fieldMode = cond.value.kind === "field";
  return (
    <div className="pf-cond">
      <div className="pf-cond-main">
        {onDragStart && (
          <span
            className="pf-drag"
            draggable
            onDragStart={(e) => {
              e.dataTransfer.effectAllowed = "move";
              e.dataTransfer.setData("text/plain", "cond"); // Firefox needs data
              onDragStart();
            }}
            title="드래그해서 묶음으로 이동 / 빼기"
          >
            ⠿
          </span>
        )}
        {first ? (
          <span className="pf-join-tag">조건</span>
        ) : (
          <select className={`pf-ctl pf-join ${cond.joiner}`} value={cond.joiner} onChange={(e) => onJoiner(e.target.value as GroupOp)}>
            <option value="and">그리고(AND)</option>
            <option value="or">또는(OR)</option>
          </select>
        )}
        <button
          type="button"
          className={`pf-ctl pf-not${cond.not ? " on" : ""}`}
          onClick={onToggleNot}
          title="이 조건을 부정 — NOT"
        >
          아니다
        </button>
        <FieldCombobox value={cond.fieldPath} fields={ctx.fields} onChange={onField} />
        <select className="pf-ctl pf-leaf-op" value={cond.op} onChange={(e) => onOp(e.target.value as FormOp)}>
          {ops.map((op) => (
            <option key={op} value={op}>
              {OP_LABEL[op]}
            </option>
          ))}
        </select>
        {canField && (
          <button
            type="button"
            className="pf-ctl pf-mode"
            onClick={() =>
              onValue(fieldMode ? defaultValueOfKind(valueKindFor(field, cond.op)) : { kind: "field", path: ctx.rhsFields[0]?.path ?? "principal.address" })
            }
            title={fieldMode ? "고정 값으로" : "다른 필드와 비교"}
          >
            {fieldMode ? "필드" : "값"}
          </button>
        )}
        {fieldMode ? (
          <FieldCombobox
            value={cond.value.kind === "field" ? cond.value.path : ""}
            fields={ctx.rhsFields}
            onChange={(p) => onValue({ kind: "field", path: p })}
          />
        ) : (
          <ValueInput value={cond.value} field={field} onChange={onValue} />
        )}
        <span className="pf-grow" />
        {onGroup && (
          <button type="button" className="pf-iconbtn" onClick={onGroup} title="이 조건을 괄호로 묶기">
            묶기
          </button>
        )}
        <button type="button" className="pf-iconbtn danger" onClick={onRemove} aria-label="조건 삭제" title="삭제">
          ✕
        </button>
      </div>
      {cond.fieldPath && <div className="pf-cond-chip">{chip}</div>}
    </div>
  );
}

// ── natural-language chip (the raw Cedar is shown in the right pane) ─────────

function labelOf(path: string, ctx: EditorCtx): string {
  return ctx.fieldByPath.get(path)?.label ?? ctx.rhsFields.find((f) => f.path === path)?.label ?? path;
}

/** Render one address for a chip: friendly name + short 0x when known. */
function addrText(raw: string, ctx: EditorCtx): string {
  const e = ctx.lookupAddr(raw);
  return e ? `${e.name}(${shortAddress(raw)})` : raw;
}

function valueText(v: FormValue, ctx: EditorCtx, field?: FieldOption): string {
  const unit = field?.unit ? ` ${field.unit}` : "";
  switch (v.kind) {
    case "bool":
      return v.value ? "참" : "거짓";
    case "long":
      // nano fields are stored ×10⁹; show plain token units to match the widget.
      return (field?.scale === "nano" ? v.value / 1e9 : v.value) + unit;
    case "decimal":
      return v.value + unit;
    case "string":
      if (v.value === "") return "";
      // Resolve a known address to its name; otherwise quote the literal.
      return ctx.lookupAddr(v.value) ? addrText(v.value, ctx) : `"${v.value}"`;
    case "set":
      return v.values.length ? `[${v.values.map((x) => addrText(x, ctx)).join(", ")}]` : "[비어 있음]";
    case "field":
      return labelOf(v.path, ctx);
  }
}

/** Plain-Korean chip for a condition, falling back to "…" if anything is off. */
function condChip(cond: FormCondition, ctx: EditorCtx): string {
  try {
    return naturalCondition({
      subject: labelOf(cond.fieldPath, ctx),
      op: cond.op,
      value: valueText(cond.value, ctx, ctx.fieldByPath.get(cond.fieldPath)),
      emptyStr: cond.value.kind === "string" && cond.value.value === "",
      neg: cond.not,
    });
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
    case "long": {
      // nano fields store token × 10⁹ but the user enters/sees plain token
      // units — convert at the widget so "nano" never surfaces (Rule: unit
      // auto-convert). decimals are accepted (0.05 토큰 → 50000000 nano).
      const nano = field?.scale === "nano";
      const shown = nano ? value.value / 1e9 : value.value;
      return (
        <span className="pf-val-wrap">
          <input
            className="pf-val num"
            type="number"
            step={nano ? "any" : undefined}
            value={shown}
            onChange={(e) => {
              const n = Number(e.target.value);
              onChange({ kind: "long", value: nano ? Math.round(n * 1e9) : n });
            }}
          />
          {unit && <span className="pf-unit">{unit}</span>}
        </span>
      );
    }
    case "decimal":
      return (
        <span className="pf-val-wrap">
          <input className="pf-val num" value={value.value} onChange={(e) => onChange({ kind: "decimal", value: e.target.value })} placeholder="0.05" />
          {unit && <span className="pf-unit">{unit}</span>}
        </span>
      );
    case "set": {
      // `in` over an address field → name-resolving multi-address picker.
      if (field?.role === "address") {
        return (
          <AddressSetInput
            values={value.values}
            onChange={(values) => onChange({ kind: "set", values })}
          />
        );
      }
      return (
        <input
          className="pf-val wide"
          value={value.values.join(", ")}
          onChange={(e) =>
            onChange({ kind: "set", values: e.target.value.split(",").map((s) => s.trim()).filter(Boolean) })
          }
          placeholder="값1, 값2, …"
        />
      );
    }
    case "field":
      return null; // handled by LeafRow's field combobox
    default: {
      // string — refine by the field's semantic type (address / enum / plain).
      const flavor = stringFlavor(field);
      if (flavor === "address") {
        return (
          <AddressInput
            value={value.value}
            onChange={(v) => onChange({ kind: "string", value: v })}
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
