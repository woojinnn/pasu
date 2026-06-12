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
import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from "react";

import { blocksToText } from "../../../cedar";
import type { PolicyIR } from "../../../cedar/blocks/ir";
import { pathByNode } from "../../../cedar/diagnosis/path";
import { naturalCondition, withJosa } from "../../../cedar/nl";
import { useAddressBook, shortAddress, type AddressEntry } from "../../../hooks/useAddressBook";
import { PolicyDiagram } from "../../../cedar/diagram/PolicyDiagram";
import { AddressInput, AddressSetInput } from "./AddressPicker";
import {
  emptyFormModel,
  fieldsForTrigger,
  findInvalidModelDecimals,
  flattenSituations,
  formToIrWithMap,
  isGroupNode,
  KNOWN_ACTIONS,
  ACTION_GROUPS,
  moveCondTo,
  normalizeDecimal,
  normalizeSituations,
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
import "./policy-value-sheet.css";

export interface PolicyFormPaneProps {
  initialModel?: FormModel | null;
  /** The policy's saved manifest — user-defined enrichment fields (policy_rpc
   *  entries outside the built-in registry) are restored from it. */
  initialManifest?: unknown;
  /** 지갑 인스턴스 편집 모드 — 구조 컨트롤(조건/상황 추가·삭제, 또는/그리고,
   *  필드·연산자·트리거 변경)을 숨기고 값(RHS 입력·Set 멤버·필드 비교 대상)만
   *  편집할 수 있다. 어차피 저장이 구조 변경을 거부하므로 버튼도 안 보여준다. */
  valuesOnly?: boolean;
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
  /** 유효성 변화 보고 — Cedar 변환 실패나 형식 오류(잘못된 decimal 등)를
   *  부모(저장 버튼)가 알 수 있게 한다. 값 시트에서 "빨간불 + 되돌리기"에 쓰임. */
  onValidity?: (v: { valid: boolean; error: string | null }) => void;
  /** 이 토큰이 바뀌면 시트를 연 시점의 값으로 되돌린다(저장 시 형식오류 → 복원). */
  resetToken?: number;
}

const OP_LABEL: Record<FormOp, string> = {
  "==": "=",
  "!=": "≠",
  "<": "<",
  "<=": "≤",
  ">": ">",
  ">=": "≥",
  contains: "포함",
  notContains: "포함 안 함",
  in: "다음 중 하나",
  notIn: "다음 중 아님",
};

/** Ops that compare two scalars — these can take a field-vs-field RHS. */
const SCALAR_OPS = new Set<FormOp>(["==", "!=", "<", "<=", ">", ">="]);

/** 값 시트(문장형)에서 한 조건을 "주어 [값] 서술어"로 읽을 때 값 뒤에 붙는 말.
 *  "~면"으로 끝나 그리고/또는 연결어와 마지막 "→ 차단"으로 자연스럽게 이어진다. */
const SUFFIX_KO: Record<FormOp, string> = {
  "==": "와 같으면",
  "!=": "와 다르면",
  "<": "보다 작으면",
  "<=": "이하이면",
  ">": "보다 크면",
  ">=": "이상이면",
  contains: "을 포함하면",
  notContains: "을 포함하지 않으면",
  in: "중 하나이면",
  notIn: "중 어느 것도 아니면",
};

/** 트리 안에서 특정 leaf(동일 참조)의 값만 교체 — 시트는 값만 편집하므로
 *  구조(트리거·필드·연산자·중첩)는 그대로 두고 RHS 값만 갈아끼운다. */
function replaceLeafValue(nodes: FormNode[], target: FormCondition, value: FormValue): FormNode[] {
  return nodes.map((n) =>
    isGroupNode(n)
      ? { ...n, conds: replaceLeafValue(n.conds, target, value) }
      : n === target
        ? { ...n, value }
        : n,
  );
}

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

/** Value kind for a (field, op): membership ops take a set, else the field's kind. */
function valueKindFor(field: FieldOption | undefined, op: FormOp): FormValue["kind"] {
  if (op === "in" || op === "notIn") return "set";
  return field ? valueKindForField(field.fieldKind) : "string";
}

/** RHS options for a field-vs-field comparison: only PRIMITIVE fields whose
 *  value kind matches the LHS (String↔String, Long↔Long, …), addresses only
 *  against addresses (another String rarely compares meaningfully against
 *  one), and never the LHS field itself. */
function compatibleRhsFields(all: FieldOption[], lhs: FieldOption | undefined): FieldOption[] {
  if (!lhs) return all;
  const kind = valueKindForField(lhs.fieldKind);
  return all.filter(
    (f) =>
      f.path !== lhs.path &&
      f.fieldKind.startsWith("primitive.") &&
      valueKindForField(f.fieldKind) === kind &&
      (f.role === "address") === (lhs.role === "address"),
  );
}


/** Known enum value suggestions (no machine-readable enum list exists; these are
 *  the ones we're confident about — shown as datalist hints, not enforced). */
const ENUM_SUGGESTIONS: Record<string, string[]> = {
  "context.direction.kind": ["exact_input", "exact_output"],
  "context.side": ["long", "short"],
  // Perp::PlaceOrder order shape (HL/perp consolidation) — value hints.
  "context.orderType.kind": ["limit", "stop", "twap"],
  "context.orderType.timeInForce.kind": ["gtc", "ioc", "fok", "post_only", "gtd"],
  "context.orderType.orderKind": ["stop_market", "stop_limit", "take_profit", "take_profit_limit"],
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

export function PolicyFormPane({ initialModel, initialManifest, valuesOnly = false, onChange, onValidity, resetToken }: PolicyFormPaneProps) {
  const [model, setModel] = useState<FormModel>(() =>
    initialModel
      ? {
          ...initialModel,
          when: normalizeSituations(initialModel.when),
          unless: normalizeSituations(initialModel.unless),
        }
      : emptyFormModel(),
  );
  // 값 시트(valuesOnly)의 "되돌리기" 기준 — 이 화면을 연 시점의 모델.
  const openModelRef = useRef<FormModel | null>(null);
  if (openModelRef.current === null) openModelRef.current = model;

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
      onCreateCustom: () => setFieldModalOpen(true),
      customFieldEnabled: model.trigger.kind === "actionEq",
      valuesOnly,
    }),
    [fields, rhsFields, fieldByPath, addrBook.lookup, model.trigger.kind, valuesOnly],
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
  const valid = !cedarError && manifestErrors.length === 0;

  // 값 시트 유효성: Cedar 변환 실패 + 잘못된 decimal 형식. 부모(저장 버튼)에
  // 보고해 "빨간불 + 되돌리기"에 쓴다.
  const badDecimals = useMemo(() => findInvalidModelDecimals(model), [model]);
  const badDecimalSet = useMemo(() => new Set(badDecimals), [badDecimals]);
  const sheetError =
    cedarError ??
    (badDecimals.length
      ? `소수 형식이 잘못됐어요: ${badDecimals.map((v) => `"${v}"`).join(", ")} — 숫자로, 소수점 아래 최대 4자리 (예: 3 → 3.0)`
      : null);
  const onValidityRef = useRef(onValidity);
  onValidityRef.current = onValidity;
  useEffect(() => {
    onValidityRef.current?.({ valid: !sheetError, error: sheetError });
  }, [sheetError]);

  // 부모가 resetToken을 올리면 시트를 연 시점의 값으로 되돌린다.
  const resetRef = useRef(resetToken);
  useEffect(() => {
    if (resetToken === undefined || resetToken === resetRef.current) return;
    resetRef.current = resetToken;
    if (openModelRef.current) setModel(openModelRef.current);
  }, [resetToken]);

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

  // 지갑 인스턴스(값만) 편집 — 빌더 대신 문장형 "빈칸 채우기" 시트를 띄운다.
  // 구조는 읽기전용으로 보여주고, 파라미터(RHS 값)만 인라인으로 편집한다.
  if (valuesOnly) {
    const open = openModelRef.current;
    const dirty = !!open && (open.when !== model.when || open.unless !== model.unless);
    return (
      <ValueSheet
        model={model}
        ir={ir}
        ctx={ctx}
        triggerLabel={triggerText}
        triggerAny={trig.kind === "any"}
        severity={model.severity}
        reason={model.reason}
        dirty={dirty}
        error={sheetError}
        badDecimals={badDecimalSet}
        humanizeLabel={humanizeAddrs}
        onValue={(target, value) =>
          setModel((m) => ({
            ...m,
            when: replaceLeafValue(m.when, target, value),
            unless: replaceLeafValue(m.unless, target, value),
          }))
        }
        onRevert={() => {
          if (open) setModel(open);
        }}
      />
    );
  }

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
              disabled={valuesOnly}
              title={valuesOnly ? "검사 대상 변경은 라이브러리 정책에서 해주세요" : undefined}
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
              disabled={valuesOnly || currentGroup === "*"}
              title={valuesOnly ? "검사 대상 변경은 라이브러리 정책에서 해주세요" : undefined}
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
            <div
              className="pf-sev"
              title={valuesOnly ? "심각도·사유는 라이브러리 정책에서 관리돼요" : undefined}
            >
              <button
                type="button"
                className={`pf-sev-btn warn${model.severity === "warn" ? " on" : ""}`}
                disabled={valuesOnly}
                onClick={() => patch({ severity: "warn" })}
              >
                ● 경고
              </button>
              <button
                type="button"
                className={`pf-sev-btn deny${model.severity === "deny" ? " on" : ""}`}
                disabled={valuesOnly}
                onClick={() => patch({ severity: "deny" })}
              >
                ● 차단
              </button>
            </div>
          </div>
          <div className="pf-row">
            <label className="pf-label">사유</label>
            <input
              className="pf-input"
              value={model.reason}
              readOnly={valuesOnly}
              title={valuesOnly ? "심각도·사유는 라이브러리 정책에서 관리돼요" : undefined}
              onChange={(e) => {
                if (!valuesOnly) patch({ reason: e.target.value });
              }}
              placeholder="예: 고위험 동작 차단"
            />
          </div>
        </section>

        {/* 문제가 있을 때만 보이는 경고 줄 — 정상일 땐 미리보기 헤더의
            "폼과 동기화됨" 배지가 그 역할을 한다. */}
        {!valid && (
          <div className="pf-status bad">
            <span className="pf-status-main">
              ⚠ {cedarError ?? manifestErrors[0]?.message ?? "유효하지 않음"}
            </span>
          </div>
        )}
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
          {model.reason ? `'${model.reason}' (이)라는 메시지와 함께 ` : ""}
          {model.severity === "deny" ? "차단" : "경고"}
        </div>
        <ManifestPreview
          open={manifestOpen}
          onToggle={() => setManifestOpen((v) => !v)}
          autoManifest={gen.manifest}
          errors={manifestErrors}
          overrideText={manifestText}
          parseErr={manifestParseErr}
          canEdit={!valuesOnly}
          onEdit={() =>
            setManifestText(JSON.stringify(gen.manifest ?? {}, null, 2))
          }
          onChangeText={setManifestText}
          onReset={() => setManifestText(null)}
        />
      </aside>

      {fieldModalOpen && (
        <CustomFieldModal
          existing={registry}
          actionTag={actionTagOf(model.trigger)}
          fields={fields}
          onCreate={({ name, field }) => setUserFields((prev) => ({ ...prev, [name]: field }))}
          onClose={() => setFieldModalOpen(false)}
        />
      )}
    </div>
  );
}

// ── 값 시트 (문장형 "빈칸 채우기") — 지갑 인스턴스의 파라미터만 편집 ─────────
//
// 정책을 한 문장으로 읽어주고(이 거래에서 … 면 → 차단), 편집 가능한 곳은
// 강조된 빈칸(RHS 값)뿐. 트리거·필드·연산자·심각도·사유는 읽기전용 텍스트로
// 보여 빌더(PolicyFormPane)와 확실히 구분된다.

function ValueSheet({
  model,
  ir,
  ctx,
  triggerLabel,
  triggerAny,
  severity,
  reason,
  dirty,
  error,
  badDecimals,
  humanizeLabel,
  onValue,
  onRevert,
}: {
  model: FormModel;
  ir: PolicyIR;
  ctx: EditorCtx;
  triggerLabel: string;
  triggerAny: boolean;
  severity: FormModel["severity"];
  reason: string;
  dirty: boolean;
  /** 전체 유효성 메시지(없으면 정상). 상단 배너로 보여준다. */
  error: string | null;
  /** 형식이 잘못된 decimal 값들 — 해당 칸을 빨갛게 표시. */
  badDecimals: Set<string>;
  humanizeLabel: (text: string) => string;
  onValue: (target: FormCondition, value: FormValue) => void;
  onRevert: () => void;
}) {
  const renderLeaf = (cond: FormCondition): ReactNode => {
    const field = ctx.fieldByPath.get(cond.fieldPath);
    const subject = field?.label ?? cond.fieldPath ?? "값";
    // RHS는 값이든 "다른 필드(예: 내 지갑 주소)"든 전부 파라미터다 — 빌더처럼
    // 값/필드 토글 + 편집을 그대로 제공한다(저장은 이미 field 참조도 지원).
    const rhsOptions = compatibleRhsFields(ctx.rhsFields, field);
    const canField = SCALAR_OPS.has(cond.op) && rhsOptions.length > 0;
    const fieldMode = cond.value.kind === "field";
    const invalid = cond.value.kind === "decimal" && badDecimals.has(cond.value.value);
    return (
      <span className="pv-line">
        <span className="pv-subj">{withJosa(subject, "이", "가")}</span>
        {fieldMode ? (
          <span className="pv-blank field">
            <FieldCombobox
              value={cond.value.kind === "field" ? cond.value.path : ""}
              fields={rhsOptions}
              onChange={(p) => onValue(cond, { kind: "field", path: p })}
            />
          </span>
        ) : (
          <span className={`pv-blank${invalid ? " invalid" : ""}`}>
            <ValueInput value={cond.value} field={field} invalid={invalid} onChange={(v) => onValue(cond, v)} />
          </span>
        )}
        {canField && (
          <button
            type="button"
            className="pv-mode"
            onClick={() =>
              onValue(
                cond,
                fieldMode
                  ? defaultValueOfKind(valueKindFor(field, cond.op))
                  : { kind: "field", path: rhsOptions[0]?.path ?? "principal.address" },
              )
            }
            title={fieldMode ? "고정 값(주소·숫자)으로 바꾸기" : "다른 필드와 비교"}
          >
            {fieldMode ? "값으로" : "필드로"}
          </button>
        )}
        <span className="pv-word">{SUFFIX_KO[cond.op]}</span>
      </span>
    );
  };

  // nodes를 "그리고"(or=false)/"또는"(or=true)로 잇고, 묶음을 만나면 패리티를
  // 뒤집어 재귀 — 빌더의 카드/묶음(AND/OR by nesting)과 같은 규칙.
  const renderNodes = (nodes: FormNode[], or: boolean): ReactNode =>
    nodes.map((n, i) => (
      <Fragment key={i}>
        {i > 0 && <span className="pv-conn">{or ? "또는" : "그리고"}</span>}
        {isGroupNode(n) ? (
          <span className="pv-group">{renderNodes(n.conds, !or)}</span>
        ) : (
          renderLeaf(n)
        )}
      </Fragment>
    ));

  const renderSituations = (nodes: FormNode[], sm = false): ReactNode => {
    const runs = situationsOf(nodes);
    return runs.map((run, si) => (
      <Fragment key={si}>
        {si > 0 && (
          <div className={`pv-or-div${sm ? " sm" : ""}`}>
            <span>또는</span>
          </div>
        )}
        <div className={`pv-flow${sm ? " sm" : ""}`}>{renderNodes(run, false)}</div>
      </Fragment>
    ));
  };

  const whenRuns = situationsOf(model.when);
  const hasUnless = situationsOf(model.unless).length > 0;

  return (
    <div className="pv-sheet">
      {error && (
        <div className="pv-error" role="alert">
          <span className="pv-error-ic">⚠</span>
          <span>{error}</span>
        </div>
      )}
      <div className="pv-main">
      <div className="pv-card">
        <div className="pv-top">
          <span className="pv-top-lk">이 지갑에서</span>
          <b className={`pv-trigchip${triggerAny ? " any" : ""}`}>
            {triggerAny ? "모든 거래" : triggerLabel}
          </b>
          <span className="pv-top-lk">{triggerAny ? "마다" : "거래에서"}</span>
          <span className="pv-spacer" />
          <span className="pv-ro-pill">뼈대 · 읽기전용</span>
        </div>

        <div className="pv-when">
          {whenRuns.length === 0 ? (
            <div className="pv-empty">조건이 없어 이 거래는 항상 적용돼요.</div>
          ) : (
            renderSituations(model.when)
          )}
        </div>

        <div className={`pv-verb ${severity}`}>
          <span className="pv-arrow">→</span>
          <span className="pv-verb-act">
            {severity === "deny" ? "🚫 차단" : severity === "warn" ? "⚠ 경고" : "ℹ 정보"}
          </span>
          {reason && <span className="pv-reason">'{reason}'</span>}
        </div>

        {hasUnless && (
          <div className="pv-unless">
            <span className="pv-unless-lk">단, 다음이면 적용하지 않아요</span>
            {renderSituations(model.unless, true)}
          </div>
        )}
      </div>

      <div className="pv-diagram-card">
        <div className="pv-diagram-head">
          정책 흐름도
          <span className="pv-ro-pill">읽기전용</span>
        </div>
        <div className="pv-diagram-body">
          <PolicyDiagram ir={ir} interactive humanizeLabel={humanizeLabel} />
        </div>
      </div>
      </div>

      <div className="pv-foot">
        <span className="pv-foot-note">
          값만 바꿀 수 있어요 · 구조·트리거·심각도는 라이브러리 정책에서 수정해요.
        </span>
        <span className="pv-spacer" />
        <button type="button" className="pv-revert" onClick={onRevert} disabled={!dirty}>
          되돌리기
        </button>
      </div>
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
  canEdit,
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
  /** 인스턴스 편집에서는 manifest가 저장되지 않으므로 직접 편집을 숨긴다. */
  canEdit: boolean;
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
                {canEdit && (
                  <button type="button" className="pf-manifest-btn" onClick={onEdit}>
                    ✎ 직접 편집
                  </button>
                )}
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
  /** Open the "+ 새 보강 필드 만들기" modal. */
  onCreateCustom: () => void;
  /** False while the trigger is "모든 동작" — enrichment params are
   *  action-shaped, so a concrete action must be chosen first. The button
   *  stays visible but disabled with the reason, instead of vanishing. */
  customFieldEnabled: boolean;
  /** 지갑 인스턴스 편집 — 구조 컨트롤 숨김, 값만 편집. */
  valuesOnly: boolean;
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
  // 모든 편집은 정규화를 거친다 — 투명 박스(자식 0/1)가 즉시 녹아서, 행의
  // "+또는"이 한 줄짜리 모두-박스 안에서 눌려도 바깥 하나라도-박스로 합쳐진다.
  const commit = (next: FormNode[][]) => onChange(normalizeSituations(flattenSituations(next)));
  // 상황 si의 노드 ni 교체/삭제 — runs를 수술하고 flatten으로 joiner 정규화.
  const updateNode = (si: number, ni: number, n: FormNode) =>
    commit(runs.map((r, i) => (i === si ? r.map((x, j) => (j === ni ? n : x)) : r)));
  const removeNode = (si: number, ni: number) =>
    commit(runs.map((r, i) => (i === si ? r.filter((_, j) => j !== ni) : r)));
  const addCond = (si: number) =>
    commit(runs.map((r, i) => (i === si ? [...r, newCond(ctx.fields)] : r)));
  const addSituation = () => commit([...runs, [newCond(ctx.fields)]]);
  const removeSituation = (si: number) => commit(runs.filter((_, i) => i !== si));
  // 행에 "또는" 선택지 붙이기 — 행을 "다음 중 하나라도" 묶음으로 감싸고 빈
  // 선택지를 바로 하나 추가 (한 클릭으로 "B 또는 C" 의도 완성).
  const addOr = (si: number, ni: number) => {
    const n = runs[si][ni];
    if (isGroupNode(n)) return;
    updateNode(si, ni, {
      kind: "group",
      joiner: n.joiner,
      conds: [{ ...n, joiner: "and" }, { ...newCond(ctx.fields), joiner: "or" }],
    });
  };
  // Drag-and-drop: drag a row onto a situation card (AND로 합류), a choice
  // group (선택지로), or the bottom strip (새 상황). Payload = the condition
  // object itself (matched by identity in `moveCondTo`).
  const [drag, setDrag] = useState<FormCondition | null>(null);
  const dropTo = (target: DropTarget) => {
    if (drag) onChange(normalizeSituations(moveCondTo(nodes, drag, target)));
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
              {!ctx.valuesOnly && (
                <button
                  type="button"
                  className="pf-iconbtn danger"
                  onClick={() => removeSituation(si)}
                  aria-label="상황 삭제"
                  title="상황 삭제"
                >
                  ✕
                </button>
              )}
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
                  onConds={(conds) => updateNode(si, ni, { ...n, conds })}
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
                  onField={(p) => updateNode(si, ni, pickFieldCond(n, p, ctx.fieldByPath))}
                  onOp={(op) => updateNode(si, ni, pickOpCond(n, op, ctx.fieldByPath))}
                  onValue={(value) => updateNode(si, ni, { ...n, value })}
                  onGroup={() => addOr(si, ni)}
                  onRemove={() => removeNode(si, ni)}
                />
              ),
            )}
            {!ctx.valuesOnly && (
              <button type="button" className="pf-add-cond sm" onClick={() => addCond(si)}>
                + 그리고
              </button>
            )}
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
      {!ctx.valuesOnly && (
        <div className="pf-add-row">
          <button type="button" className="pf-add-cond" onClick={addSituation}>
            {runs.length === 0 ? "+ 위험 상황 추가" : "+ 또는"}
          </button>
          <button
            type="button"
            className="pf-add-cond accent"
            onClick={ctx.onCreateCustom}
            disabled={!ctx.customFieldEnabled}
            title={
              ctx.customFieldEnabled
                ? "서버에서 조회한 값(위험 점수, USD 가치 등)으로 조건을 걸 수 있는 필드를 만들어요"
                : "먼저 ①에서 동작을 골라주세요 — 어떤 값을 넘길 수 있는지가 동작마다 달라요"
            }
          >
            ＋ 새 보강 필드 만들기
          </button>
        </div>
      )}
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
  onConds,
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
  onConds: (conds: FormNode[]) => void;
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
  // 자식 행을 반대 패리티 묶음으로 감싸며 빈 항목을 바로 추가 (원클릭
  // "또는/그리고") / 단일 leaf 묶음 풀기.
  const wrapChild = (i: number) => {
    const n = conds[i];
    if (isGroupNode(n)) return;
    update(i, {
      kind: "group",
      joiner: n.joiner,
      conds: [{ ...n, joiner: "and" }, { ...newCond(ctx.fields), joiner: "or" }],
    });
  };
  return (
    <div
      className={`pf-box ${orCtx ? "or" : "and"}${
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
        <span className="pf-spc" />
        {!ctx.valuesOnly && (
          <button type="button" className="pf-iconbtn danger" onClick={onRemove} aria-label="삭제" title="이 묶음 전체 삭제">
            ✕
          </button>
        )}
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
            onConds={(next) => update(i, { ...c, conds: next })}
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
            onField={(p) => update(i, pickFieldCond(c, p, ctx.fieldByPath))}
            onOp={(op) => update(i, pickOpCond(c, op, ctx.fieldByPath))}
            onValue={(value) => update(i, { ...c, value })}
            onGroup={() => wrapChild(i)}
            onRemove={() => removeAt(i)}
          />
        ),
      )}
      {!ctx.valuesOnly && (
        <button type="button" className="pf-or-btn" onClick={() => onConds(norm([...conds, newCond(ctx.fields)]))}>
          {orCtx ? "+ 또는" : "+ 그리고"}
        </button>
      )}
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
  onField: (path: string) => void;
  onOp: (op: FormOp) => void;
  onValue: (v: FormValue) => void;
  onGroup?: () => void;
  onRemove: () => void;
}) {
  const field = ctx.fieldByPath.get(cond.fieldPath);
  const ops = field ? operatorsFor(field.fieldKind) : (["=="] as FormOp[]);
  const chip = cond.fieldPath ? condChip(cond, ctx) : "…";
  // 필드-비교 RHS는 LHS와 값 타입이 맞는 필드만 — 하나도 없으면 토글 자체를 숨김.
  const rhsOptions = compatibleRhsFields(ctx.rhsFields, field);
  const canField = SCALAR_OPS.has(cond.op) && rhsOptions.length > 0;
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
        {onDragStart && !ctx.valuesOnly && (
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
        {ctx.valuesOnly ? (
          // 인스턴스 편집: 비교 필드(LHS)·연산자는 구조 — 읽기 전용 칩으로.
          <>
            <span className="pf-ctl pf-ro" title="조건 구성 변경은 라이브러리 정책에서 해주세요">
              {field?.label ?? cond.fieldPath ?? "…"}
            </span>
            <span className="pf-ctl pf-leaf-op pf-ro">{OP_LABEL[cond.op]}</span>
          </>
        ) : (
          <>
            <FieldCombobox value={cond.fieldPath} fields={ctx.fields} onChange={onField} />
            <select className="pf-ctl pf-leaf-op" value={cond.op} onChange={(e) => onOp(e.target.value as FormOp)}>
              {ops.map((op) => (
                <option key={op} value={op}>
                  {OP_LABEL[op]}
                </option>
              ))}
            </select>
          </>
        )}
        {canField && (
          <button
            type="button"
            className="pf-ctl pf-mode"
            onClick={() =>
              onValue(
                fieldMode
                  ? defaultValueOfKind(valueKindFor(field, cond.op))
                  : { kind: "field", path: rhsOptions[0]?.path ?? "principal.address" },
              )
            }
            title={fieldMode ? "고정 값으로" : "다른 필드와 비교"}
          >
            {fieldMode ? "필드" : "값"}
          </button>
        )}
        {fieldMode ? (
          <FieldCombobox
            value={cond.value.kind === "field" ? cond.value.path : ""}
            fields={rhsOptions}
            onChange={(p) => onValue({ kind: "field", path: p })}
          />
        ) : (
          <ValueInput value={cond.value} field={field} onChange={onValue} />
        )}
        <span className="pf-grow" />
        {onGroup && !ctx.valuesOnly && (
          <button
            type="button"
            className="pf-iconbtn"
            onClick={onGroup}
            title={
              alt
                ? "이 선택지에 '그리고' 조건을 붙여요 — 다음에 모두 해당 묶음이 생겨요"
                : "이 조건에 '또는' 선택지를 붙여요 — 다음 중 하나라도 묶음이 생겨요"
            }
          >
            {alt ? "+그리고" : "+또는"}
          </button>
        )}
        {!ctx.valuesOnly && (
          <button type="button" className="pf-iconbtn danger" onClick={onRemove} aria-label="조건 삭제" title="삭제">
            ✕
          </button>
        )}
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

/** Plain-Korean chip for a condition, falling back to "…" if anything is off.
 *  The negative memberships map onto nl's positive op + `neg`. */
function condChip(cond: FormCondition, ctx: EditorCtx): string {
  const neg = cond.op === "notIn" || cond.op === "notContains";
  try {
    return naturalCondition({
      subject: labelOf(cond.fieldPath, ctx),
      op: cond.op === "notIn" ? "in" : cond.op === "notContains" ? "contains" : cond.op,
      value: valueText(cond.value, ctx, ctx.fieldByPath.get(cond.fieldPath)),
      emptyStr: cond.value.kind === "string" && cond.value.value === "",
      neg,
    });
  } catch {
    return "…";
  }
}

// ── value widget by kind + field type (literal kinds only) ──────────────────

function ValueInput({
  value,
  field,
  invalid,
  onChange,
}: {
  value: FormValue;
  field: FieldOption | undefined;
  /** 형식 오류 표시(빨간 테두리) — 값 시트에서 잘못된 decimal 등에 쓰임. */
  invalid?: boolean;
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
          <input
            className={`pf-val num${invalid ? " invalid" : ""}`}
            value={value.value}
            onChange={(e) => onChange({ kind: "decimal", value: e.target.value })}
            onBlur={(e) => {
              // Cedar decimal은 소수점이 필수 — "3"은 "3.0"으로 정규화.
              const n = normalizeDecimal(e.target.value);
              if (n !== null && n !== e.target.value) onChange({ kind: "decimal", value: n });
            }}
            placeholder="0.05"
          />
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
