/**
 * PolicyFormPane — the "폼으로 만들기" editor surface.
 *
 * Edits a {@link FormModel} (the constrained subset) and keeps the parent's
 * `cedarText` + `ir` in sync: every change rebuilds the IR via `formToIr` and
 * (debounced) renders Cedar via `blocksToText`, then calls `onChange`. Mirrors
 * the Block tab's (WorkspaceV9) contract so the editor wiring is identical.
 *
 * Sections: 검사 대상 (trigger) / 언제 위험한가요 (when — "위험 상황" 카드:
 * 카드 안은 모두-AND, 카드끼리 OR; 괄호 묶음은 "다음 중 하나라도" OR-전용) /
 * 알림. Right side = a live structure diagram (pan/zoom + click-sync). Beyond
 * the subset (deep nesting, if/then/else, …) the editor hands off to the Block
 * tab.
 */
import { useEffect, useMemo, useRef, useState } from "react";

import { blocksToText } from "../../../cedar";
import type { PolicyIR } from "../../../cedar/blocks/ir";
import { pathByNode } from "../../../cedar/diagnosis/path";
import { naturalCondition } from "../../../cedar/nl";
import { useAddressBook, shortAddress, type AddressEntry } from "../../../hooks/useAddressBook";
import { PolicyDiagram } from "../../../cedar/diagram/PolicyDiagram";
import { AddressInput, AddressSetInput } from "./AddressPicker";
import {
  conditionsDeep,
  emptyFormModel,
  fieldsForTrigger,
  flattenSituations,
  formToIrWithMap,
  irToForm,
  isGroupNode,
  KNOWN_ACTIONS,
  ACTION_GROUPS,
  moveCondTo,
  operatorsFor,
  situationsOf,
  valueKindForField,
  type DropTarget,
  type FieldOption,
  type FormCondition,
  type FormGroupNode,
  type FormModel,
  type FormNode,
  type FormOp,
  type FormTrigger,
  type FormValue,
} from "../../../cedar/form";
import {
  ENRICHMENT_FIELDS,
  generateManifest,
  type CustomType,
  type EnrichmentRegistry,
} from "../../../editor-v9/manifest-gen";

import { CustomFieldModal } from "./CustomFieldModal";
import { FieldCombobox } from "./FieldCombobox";

import "./policy-form.css";

export interface PolicyFormPaneProps {
  initialModel?: FormModel | null;
  /** The policy's saved manifest — user-defined enrichment fields (policy_rpc
   *  entries outside the built-in registry) are restored from it. */
  initialManifest?: unknown;
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
  // 프로토콜(venue) 제한 — manifest trigger의 `action.venue`와 같은 어휘.
  "context.venue.name": [
    "uniswap_v2",
    "uniswap_v3",
    "uniswap_v4",
    "aave_v3",
    "curve",
    "balancer_v2",
    "cowswap",
    "1inch",
    "hyperliquid",
  ],
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

/** `custom_context` 타입 → 폼 FieldOption의 fieldKind. */
const KIND_BY_TYPE: Record<CustomType, FieldOption["fieldKind"]> = {
  decimal: "primitive.decimal",
  Long: "primitive.Long",
  Bool: "primitive.Bool",
  String: "primitive.String",
};

/** 현재 trigger의 enrichment action tag (`Erc20Approve` → `erc20_approve`). */
function actionTagOf(trigger: FormTrigger): string | null {
  if (trigger.kind !== "actionEq") return null;
  return trigger.id.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
}

/** 저장된 manifest에서 사용자 정의 보강 필드(기본 레지스트리에 없는
 *  policy_rpc 항목)를 복원한다. 형태가 어긋나는 항목은 조용히 건너뜀. */
function userFieldsFromManifest(manifest: unknown, actionTag: string | null): EnrichmentRegistry {
  const out: EnrichmentRegistry = {};
  const m = manifest as
    | {
        policy_rpc?: unknown;
        custom_context?: { fields?: Record<string, string> };
      }
    | null
    | undefined;
  if (!m || !Array.isArray(m.policy_rpc)) return out;
  const types = m.custom_context?.fields ?? {};
  for (const raw of m.policy_rpc) {
    const rpc = raw as {
      id?: unknown;
      method?: unknown;
      params?: unknown;
      outputs?: { from?: unknown }[];
    };
    if (typeof rpc?.id !== "string" || typeof rpc?.method !== "string") continue;
    if (rpc.id in ENRICHMENT_FIELDS) continue; // 내장 필드는 레지스트리가 원본
    const type = types[rpc.id];
    if (type !== "decimal" && type !== "Long" && type !== "Bool" && type !== "String") continue;
    const from = rpc.outputs?.[0]?.from;
    out[rpc.id] = {
      type,
      label: { ko: rpc.id, en: rpc.id },
      appliesTo: actionTag ? [actionTag] : [],
      method: rpc.method,
      projection: typeof from === "string" ? from : "$.result.value",
      params: (rpc.params ?? {}) as EnrichmentRegistry[string]["params"],
    };
  }
  return out;
}

/** 폼↔다이어그램 동기화의 선택 단위: 행/묶음 노드, 또는 상황 카드(머리 노드). */
type Selection2 = { kind: "node"; node: FormNode } | { kind: "situation"; head: FormNode };

/** ConditionEditor에 내려보내는 선택 배선. */
interface EditorSelection {
  isNodeSelected: (n: FormNode) => boolean;
  isSituationSelected: (head: FormNode) => boolean;
  onClickNode: (n: FormNode) => void;
  onClickSituation: (head: FormNode) => void;
  registerRow: (n: FormNode, el: HTMLElement | null) => void;
}

export function PolicyFormPane({ initialModel, initialManifest, onChange }: PolicyFormPaneProps) {
  const [model, setModel] = useState<FormModel>(() => initialModel ?? emptyFormModel());
  // We no longer display the Cedar text (the right pane shows the diagram), but
  // we still build it to push up via onChange and to surface conversion errors.
  const [cedarError, setCedarError] = useState<string | null>(null);

  // 사용자 정의 보강 필드 — 저장된 manifest에서 복원, 모달로 추가.
  const [userFields, setUserFields] = useState<EnrichmentRegistry>(() =>
    userFieldsFromManifest(initialManifest, actionTagOf((initialModel ?? emptyFormModel()).trigger)),
  );
  const [fieldModalOpen, setFieldModalOpen] = useState(false);
  const registry = useMemo<EnrichmentRegistry>(
    () => ({ ...ENRICHMENT_FIELDS, ...userFields }),
    [userFields],
  );

  const fields = useMemo(() => {
    const extra: FieldOption[] = Object.entries(userFields).map(([name, def]) => ({
      path: `context.custom.${name}`,
      label: def.label.ko,
      fieldKind: KIND_BY_TYPE[def.type],
      role: "derived" as const,
      source: "custom" as const,
      optional: true,
      desc: `${def.method} 호출 값`,
    }));
    return [...fieldsForTrigger(model.trigger), ...extra];
  }, [model.trigger, userFields]);
  const rhsFields = useMemo(() => [PRINCIPAL_ADDRESS, ...fields], [fields]);
  const fieldByPath = useMemo(() => {
    const m = new Map<string, FieldOption>();
    for (const f of fields) m.set(f.path, f);
    return m;
  }, [fields]);
  const addrBook = useAddressBook();
  const ctx = useMemo<EditorCtx>(
    () => ({
      fields,
      rhsFields,
      fieldByPath,
      lookupAddr: addrBook.lookup,
      onCreateCustom:
        model.trigger.kind === "actionEq" ? () => setFieldModalOpen(true) : undefined,
    }),
    [fields, rhsFields, fieldByPath, addrBook.lookup, model.trigger.kind],
  );
  // Resolve 0x addresses in the structure diagram to friendly names.
  const humanizeAddrs = (text: string): string =>
    text.replace(/0x[0-9a-fA-F]{40}/g, (m) => {
      const e = addrBook.lookup(m);
      return e ? `${e.name}(${shortAddress(m)})` : m;
    });

  const { ir, exprsByNode, runRootByHead } = useMemo(() => formToIrWithMap(model), [model]);
  const pathOf = useMemo(() => pathByNode(ir), [ir]);

  // ── 폼 ↔ 다이어그램 선택 동기화 ─────────────────────────────────────────
  // 선택 단위: 폼 노드(행/묶음) 또는 상황 카드(머리 노드로 식별).
  const [selected, setSelected] = useState<Selection2 | null>(null);
  // 모델 편집으로 선택 대상이 사라지면 해제된 것으로 취급.
  const sel =
    selected &&
    (selected.kind === "node" ? exprsByNode.has(selected.node) : runRootByHead.has(selected.head))
      ? selected
      : null;

  const selectedPaths = useMemo(() => {
    if (!sel) return [];
    const exprs =
      sel.kind === "node"
        ? (exprsByNode.get(sel.node) ?? [])
        : [runRootByHead.get(sel.head)].filter((e): e is NonNullable<typeof e> => !!e);
    return exprs.map((e) => pathOf.get(e)).filter((p): p is string => !!p);
  }, [sel, exprsByNode, runRootByHead, pathOf]);

  // canonical path → 선택 (다이어그램 클릭의 역방향). run 루트를 먼저 깔고,
  // 노드가 같은 path를 덮어쓴다(단일 조건 상황 = 행 선택 우선).
  const selectionByPath = useMemo(() => {
    const m = new Map<string, Selection2>();
    for (const [head, e] of runRootByHead) {
      const p = pathOf.get(e);
      if (p) m.set(p, { kind: "situation", head });
    }
    for (const [node, exprs] of exprsByNode) {
      for (const e of exprs) {
        const p = pathOf.get(e);
        if (p) m.set(p, { kind: "node", node });
      }
    }
    return m;
  }, [exprsByNode, runRootByHead, pathOf]);

  const sameSelection = (a: Selection2, b: Selection2) =>
    a.kind === b.kind &&
    (a.kind === "node" ? a.node === (b as { node: FormNode }).node : a.head === (b as { head: FormNode }).head);

  // 다이어그램 클릭 → 폼 선택 + 해당 행으로 스크롤.
  const rowElByNode = useRef(new Map<FormNode, HTMLElement>());
  const onDiagramNodeClick = (path: string) => {
    const next = selectionByPath.get(path);
    if (!next) return;
    if (sel && sameSelection(sel, next)) {
      setSelected(null);
      return;
    }
    setSelected(next);
    const el = rowElByNode.current.get(next.kind === "node" ? next.node : next.head);
    el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  };
  const editorSelection: EditorSelection = {
    isNodeSelected: (n) => !!sel && sel.kind === "node" && sel.node === n,
    isSituationSelected: (h) => !!sel && sel.kind === "situation" && sel.head === h,
    onClickNode: (n) =>
      setSelected((s) => (s?.kind === "node" && s.node === n ? null : { kind: "node", node: n })),
    onClickSituation: (h) =>
      setSelected((s) =>
        s?.kind === "situation" && s.head === h ? null : { kind: "situation", head: h },
      ),
    registerRow: (n, el) => {
      if (el) rowElByNode.current.set(n, el);
      else rowElByNode.current.delete(n);
    },
  };

  // Validity badge + manifest preview: cedar rendered + manifest generates
  // cleanly + the IR is still form-representable (round-trips). Manifest gen is
  // pure/sync — the same call the save path makes, so the preview matches what
  // gets persisted.
  const gen = useMemo(
    () => generateManifest(ir, registry, { id: model.id, severity: model.severity }),
    [ir, registry, model.id, model.severity],
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
        conditionsDeep([...model.when, ...model.unless])
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
      // 보강 필드 정의는 action-shaped(파라미터 셀렉터) — 함께 비운다.
      setUserFields({});
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
            <span className="pf-num">2</span> 언제 위험한가요?{" "}
            <span className="pf-sub">아래 상황 중 하나라도 해당되면 발동해요</span>
          </h3>
          <ConditionEditor
            nodes={model.when}
            ctx={ctx}
            emptyHint="조건이 없으면 이 동작은 항상 막힙니다."
            onChange={(when) => patch({ when })}
            selection={editorSelection}
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
        {/* 문장형 프레임: 위(무엇에서) → 트리(언제) → 아래(어떻게) 한 문장으로 읽힘 */}
        <div className="pf-sentence top">
          {trig.kind === "actionEq" ? <>「{triggerText}」 거래에서</> : <>모든 거래에서</>}
        </div>
        <div className="pf-diagram-body">
          <PolicyDiagram
            ir={ir}
            interactive
            selectedPaths={selectedPaths}
            onNodeClick={onDiagramNodeClick}
            humanizeLabel={humanizeAddrs}
          />
        </div>
        <div className={`pf-sentence bottom ${model.severity}`}>
          {model.severity === "deny" ? "🚫 " : "⚠ "}
          {model.reason ? `'${model.reason}' 이유로 ` : ""}
          {model.severity === "deny" ? "차단해요" : "경고해요"}
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

      {fieldModalOpen && (
        <CustomFieldModal
          existingNames={Object.keys(registry)}
          actionTag={actionTagOf(model.trigger)}
          onCreate={({ name, field }) => setUserFields((prev) => ({ ...prev, [name]: field }))}
          onClose={() => setFieldModalOpen(false)}
        />
      )}
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

// ── condition editor — "위험 상황" 카드 (카드 안 AND, 카드끼리 OR) ────────────

/** Shared field/operator context threaded to rows and boxes. */
interface EditorCtx {
  fields: FieldOption[];
  rhsFields: FieldOption[];
  fieldByPath: Map<string, FieldOption>;
  /** Resolve an address to a friendly name (my wallet / token), or undefined. */
  lookupAddr: (address: string) => AddressEntry | undefined;
  /** Open the "+ 새 보강 필드 만들기" modal (LHS field picker entry).
   *  Undefined while the trigger is "모든 동작" — enrichment params are
   *  action-shaped, so a concrete action must be chosen first. */
  onCreateCustom?: () => void;
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
  selection,
}: {
  nodes: FormNode[];
  ctx: EditorCtx;
  emptyHint: string;
  onChange: (nodes: FormNode[]) => void;
  selection: EditorSelection;
}) {
  const runs = situationsOf(nodes);
  const commit = (next: FormNode[][]) => onChange(flattenSituations(next));
  // 상황 si의 노드 ni 교체/삭제 — runs를 수술하고 flatten으로 joiner 정규화.
  const updateNode = (si: number, ni: number, n: FormNode) =>
    commit(runs.map((r, i) => (i === si ? r.map((x, j) => (j === ni ? n : x)) : r)));
  const removeNode = (si: number, ni: number) =>
    commit(runs.map((r, i) => (i === si ? r.filter((_, j) => j !== ni) : r)));
  const addCond = (si: number) =>
    commit(runs.map((r, i) => (i === si ? [...r, newCond(ctx.fields)] : r)));
  const addSituation = () => commit([...runs, [newCond(ctx.fields)]]);
  const removeSituation = (si: number) => commit(runs.filter((_, i) => i !== si));
  // 행 → "다음 중 하나라도" 선택지 묶음으로
  const wrap = (si: number, ni: number) => {
    const n = runs[si][ni];
    if (isGroupNode(n)) return;
    updateNode(si, ni, { kind: "group", joiner: n.joiner, conds: [{ ...n, joiner: "and" }] });
  };
  // 단일 leaf 묶음 → 행으로 (항목이 여럿이면 의미가 바뀌므로 버튼이 비활성)
  const unwrap = (si: number, ni: number) => {
    const n = runs[si][ni];
    if (!isGroupNode(n) || n.conds.length !== 1 || isGroupNode(n.conds[0])) return;
    updateNode(si, ni, {
      ...(n.conds[0] as FormCondition),
      joiner: n.joiner,
      ...(n.not ? { not: true } : {}),
    });
  };

  // Drag-and-drop: drag a row onto a situation card (AND로 합류), a choice
  // group (선택지로), or the bottom strip (새 상황). Payload = the condition
  // object itself (matched by identity in `moveCondTo`).
  const [drag, setDrag] = useState<FormCondition | null>(null);
  const dropTo = (target: DropTarget) => {
    if (drag) onChange(moveCondTo(nodes, drag, target));
    setDrag(null);
  };

  return (
    <>
      {runs.length === 0 && <div className="pf-empty-cond">{emptyHint}</div>}
      {runs.map((run, si) => (
        <div key={si}>
          {si > 0 && (
            <div className="pf-or-div">
              <span>또는</span>
            </div>
          )}
          <div
            className={`pf-sit${drag ? " droppable" : ""}${
              selection.isSituationSelected(run[0]) ? " is-selected" : ""
            }`}
            onDragOver={drag ? (e) => e.preventDefault() : undefined}
            onDrop={
              drag
                ? (e) => {
                    e.preventDefault();
                    dropTo({ kind: "situation", index: si });
                  }
                : undefined
            }
          >
            <div
              className="pf-sit-head"
              onClick={(ev) => {
                if ((ev.target as HTMLElement).closest("button")) return;
                selection.onClickSituation(run[0]);
              }}
            >
              <span className="pf-sit-title">상황 {si + 1}</span>
              {run.length > 1 && <span className="pf-sit-mode">다음에 모두 해당</span>}
              <span className="pf-spc" />
              <button
                type="button"
                className="pf-iconbtn danger"
                onClick={() => removeSituation(si)}
                aria-label="상황 삭제"
                title="상황 삭제"
              >
                ✕
              </button>
            </div>
            {run.map((n, ni) =>
              isGroupNode(n) ? (
                <GroupBox
                  key={ni}
                  group={n}
                  orCtx
                  ctx={ctx}
                  dragging={drag !== null}
                  selection={selection}
                  onDragStartCond={(c) => setDrag(c)}
                  onDropIntoGroup={(g) => dropTo({ kind: "group", group: g })}
                  onToggleNot={() => updateNode(si, ni, { ...n, not: !n.not })}
                  onConds={(conds) => updateNode(si, ni, { ...n, conds })}
                  onUngroup={() => unwrap(si, ni)}
                  onRemove={() => removeNode(si, ni)}
                />
              ) : (
                <ConditionRow
                  key={ni}
                  cond={n}
                  ctx={ctx}
                  selected={selection.isNodeSelected(n)}
                  onSelect={() => selection.onClickNode(n)}
                  rowRef={(el) => selection.registerRow(n, el)}
                  onDragStart={() => setDrag(n)}
                  onToggleNot={() => updateNode(si, ni, { ...n, not: !n.not })}
                  onField={(p) => updateNode(si, ni, pickFieldCond(n, p, ctx.fieldByPath))}
                  onOp={(op) => updateNode(si, ni, pickOpCond(n, op, ctx.fieldByPath))}
                  onValue={(value) => updateNode(si, ni, { ...n, value })}
                  onGroup={() => wrap(si, ni)}
                  onRemove={() => removeNode(si, ni)}
                />
              ),
            )}
            <button type="button" className="pf-add-cond sm" onClick={() => addCond(si)}>
              + 조건 추가
            </button>
          </div>
        </div>
      ))}
      {drag !== null && (
        <div
          className="pf-dropstrip"
          onDragOver={(e) => e.preventDefault()}
          onDrop={(e) => {
            e.preventDefault();
            dropTo({ kind: "new-situation" });
          }}
        >
          여기에 놓아 새 상황으로 만들기
        </div>
      )}
      <div className="pf-add-row">
        <button type="button" className="pf-add-cond" onClick={addSituation}>
          {runs.length === 0 ? "+ 위험 상황 추가" : "+ 다른 위험 상황 추가"}
        </button>
      </div>
    </>
  );
}

// ── a nested group box — OR("다음 중 하나라도") / AND("다음에 모두 해당") by
//    nesting parity, recursive ─────────────────────────────────────────────

function GroupBox({
  group,
  orCtx,
  ctx,
  dragging,
  selection,
  onDragStartCond,
  onDropIntoGroup,
  onToggleNot,
  onConds,
  onUngroup,
  onRemove,
}: {
  group: FormGroupNode;
  /** true = this group ORs its children; false = an AND-subgroup. */
  orCtx: boolean;
  ctx: EditorCtx;
  dragging: boolean;
  selection: EditorSelection;
  onDragStartCond: (c: FormCondition) => void;
  /** Drop the dragged row into `g` (this group or a nested descendant). */
  onDropIntoGroup: (g: FormGroupNode) => void;
  onToggleNot: () => void;
  onConds: (conds: FormNode[]) => void;
  onUngroup: () => void;
  onRemove: () => void;
}) {
  const { conds } = group;
  // joiner는 그룹 안에서 의미가 없지만 관례(머리 and, 나머지 or)로 정규화.
  const norm = (xs: FormNode[]): FormNode[] =>
    xs.map((c, i) => {
      const want = i === 0 ? "and" : "or";
      return c.joiner === want ? c : { ...c, joiner: want };
    });
  const update = (i: number, n: FormNode) => onConds(norm(conds.map((x, j) => (j === i ? n : x))));
  const removeAt = (i: number) => {
    const next = conds.filter((_, j) => j !== i);
    if (next.length === 0) onRemove(); // emptying the box removes it
    else onConds(norm(next));
  };
  // 자식 행을 반대 패리티 묶음으로 감싸기 / 단일 leaf 묶음 풀기.
  const wrapChild = (i: number) => {
    const n = conds[i];
    if (isGroupNode(n)) return;
    update(i, { kind: "group", joiner: n.joiner, conds: [{ ...n, joiner: "and" }] });
  };
  const unwrapChild = (i: number) => {
    const n = conds[i];
    if (!isGroupNode(n) || n.conds.length !== 1 || isGroupNode(n.conds[0])) return;
    update(i, { ...(n.conds[0] as FormCondition), joiner: n.joiner, ...(n.not ? { not: true } : {}) });
  };
  const singleLeaf = conds.length === 1 && !isGroupNode(conds[0]);
  return (
    <div
      className={`pf-box ${orCtx ? "or" : "and"}${group.not ? " neg" : ""}${
        dragging ? " droppable" : ""
      }${selection.isNodeSelected(group) ? " is-selected" : ""}`}
      ref={(el) => selection.registerRow(group, el)}
      onDragOver={dragging ? (e) => e.preventDefault() : undefined}
      onDrop={
        dragging
          ? (e) => {
              e.stopPropagation(); // 바깥 카드/묶음의 drop이 같이 받지 않게
              e.preventDefault();
              onDropIntoGroup(group);
            }
          : undefined
      }
    >
      <div
        className="pf-box-head"
        onClick={(ev) => {
          if ((ev.target as HTMLElement).closest("button")) return;
          selection.onClickNode(group);
        }}
      >
        <span className="pf-box-label">{orCtx ? "다음 중 하나라도" : "다음에 모두 해당"}</span>
        <button
          type="button"
          className={`pf-ctl pf-not${group.not ? " on" : ""}`}
          onClick={onToggleNot}
          title={orCtx ? "뒤집기 — 다음 중 어느 것도 아닐 때" : "뒤집기 — 다음에 모두 해당하지는 않을 때"}
        >
          아니다
        </button>
        <span className="pf-spc" />
        <button
          type="button"
          className="pf-box-act"
          onClick={onUngroup}
          disabled={!singleLeaf}
          title={singleLeaf ? "묶음 풀기" : "항목이 여러 개면 풀 수 없어요"}
        >
          해제
        </button>
        <button type="button" className="pf-iconbtn danger" onClick={onRemove} aria-label="묶음 삭제" title="묶음 삭제">
          ✕
        </button>
      </div>
      {conds.map((c, i) =>
        isGroupNode(c) ? (
          <GroupBox
            key={i}
            group={c}
            orCtx={!orCtx}
            ctx={ctx}
            dragging={dragging}
            selection={selection}
            onDragStartCond={onDragStartCond}
            onDropIntoGroup={onDropIntoGroup}
            onToggleNot={() => update(i, { ...c, not: !c.not })}
            onConds={(next) => update(i, { ...c, conds: next })}
            onUngroup={() => unwrapChild(i)}
            onRemove={() => removeAt(i)}
          />
        ) : (
          <ConditionRow
            key={i}
            cond={c}
            alt={orCtx}
            ctx={ctx}
            selected={selection.isNodeSelected(c)}
            onSelect={() => selection.onClickNode(c)}
            rowRef={(el) => selection.registerRow(c, el)}
            onDragStart={() => onDragStartCond(c)}
            onToggleNot={() => update(i, { ...c, not: !c.not })}
            onField={(p) => update(i, pickFieldCond(c, p, ctx.fieldByPath))}
            onOp={(op) => update(i, pickOpCond(c, op, ctx.fieldByPath))}
            onValue={(value) => update(i, { ...c, value })}
            onGroup={() => wrapChild(i)}
            onRemove={() => removeAt(i)}
          />
        ),
      )}
      <button type="button" className="pf-or-btn" onClick={() => onConds(norm([...conds, newCond(ctx.fields)]))}>
        {orCtx ? "+ 선택지 추가" : "+ 조건 추가"}
      </button>
    </div>
  );
}

// ── one condition row ──────────────────────────────────────────────────────

function ConditionRow({
  cond,
  alt,
  ctx,
  selected,
  onSelect,
  rowRef,
  onDragStart,
  onToggleNot,
  onField,
  onOp,
  onValue,
  onGroup,
  onRemove,
}: {
  cond: FormCondition;
  /** 묶음 안의 선택지 행(◦ 불릿) — 상황 카드의 행은 • 불릿. */
  alt?: boolean;
  ctx: EditorCtx;
  selected?: boolean;
  onSelect?: () => void;
  rowRef?: (el: HTMLElement | null) => void;
  onDragStart?: () => void;
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
    <div
      className={`pf-cond${selected ? " is-selected" : ""}`}
      ref={rowRef}
      onClick={
        onSelect
          ? (ev) => {
              // 행의 컨트롤(필드/연산/값/버튼) 조작은 선택 토글이 아니다.
              if ((ev.target as HTMLElement).closest("button, select, input, [draggable], .fc")) return;
              onSelect();
            }
          : undefined
      }
    >
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
            title="드래그해서 다른 상황·묶음으로 이동"
          >
            ⠿
          </span>
        )}
        <span className={`pf-bullet${alt ? " alt" : ""}`}>{alt ? "◦" : "•"}</span>
        <button
          type="button"
          className={`pf-ctl pf-not${cond.not ? " on" : ""}`}
          onClick={onToggleNot}
          title="이 조건을 부정 — NOT"
        >
          아니다
        </button>
        <FieldCombobox
          value={cond.fieldPath}
          fields={ctx.fields}
          onChange={onField}
          onCreateCustom={ctx.onCreateCustom}
        />
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
          <button
            type="button"
            className="pf-iconbtn"
            onClick={onGroup}
            title={alt ? "이 선택지를 '다음에 모두 해당' 묶음으로" : "이 조건을 '다음 중 하나라도' 묶음으로"}
          >
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
      // A suggestion list applies whenever we know one for the path (e.g.
      // venue names), not only for role-"enum" fields.
      const sugg = field ? ENUM_SUGGESTIONS[field.path] : undefined;
      if (flavor === "enum" || sugg) {
        const listId = `enum-${field?.path ?? ""}`;
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
