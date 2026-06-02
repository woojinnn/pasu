import { useEffect, useRef, useState, type DragEvent } from "react";

import {
  displayParam,
  getGlossEntry,
  isSupportedParam,
  OPS_BY_FIELDKIND,
  OP_SYMBOL,
  type FieldKind,
  type Op,
  type PredicateValue,
} from "../schema";
import type { EditorAction } from "../reducer";
import type { PredicateNode } from "../types";

/**
 * `param op value` chip with inline editing for op + value. Color tone
 * follows the field's role (numeric/address/ref/enum/auth/derived/misc).
 *
 * Editing flow:
 *   - Click the op badge → small <select> appears with the allowed
 *     operators for this fieldKind (from `OPS_BY_FIELDKIND`).
 *   - Click the value pill → text input appears; commit on blur / Enter.
 *
 * Inspector is still authoritative for the deeper fields (absence,
 * guardId, userCopy, note) — this block surface is the fast path.
 */
export interface PredicateBlockProps {
  node: PredicateNode;
  locale: "ko" | "en";
  selected: boolean;
  onSelect: () => void;
  onDragStart: (e: DragEvent<HTMLDivElement>) => void;
  dispatch: (a: EditorAction) => void;
}

export function PredicateBlock({
  node,
  locale,
  selected,
  onSelect,
  onDragStart,
  dispatch,
}: PredicateBlockProps) {
  const entry = getGlossEntry(node.param);
  const tone = entry?.group ?? "misc";
  const disabled = node.enabled === false;
  const supported = isSupportedParam(node.param);
  const valueless = node.op === "isTrue" || node.op === "isFalse" || node.op === "isEmpty";
  const ops: Op[] = OPS_BY_FIELDKIND[node.fieldKind] ?? [];

  const [editingOp, setEditingOp] = useState(false);
  const [editingValue, setEditingValue] = useState(false);
  const valueInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (editingValue) valueInputRef.current?.focus();
  }, [editingValue]);

  const valueText = node.value?.text ?? "";
  const rhsDisplay = node.value
    ? node.value.kind === "ref"
      ? node.value.text
      : node.value.kind === "num"
        ? `${node.value.text}${entry?.unit?.[locale] ? ` ${entry.unit[locale]}` : ""}`
        : node.value.text
    : "—";

  const commitValue = (raw: string) => {
    setEditingValue(false);
    if (raw === valueText) return;
    if (raw === "") {
      dispatch({ type: "UPDATE_PREDICATE", nodeId: node.id, patch: { value: null } });
      return;
    }
    dispatch({
      type: "UPDATE_PREDICATE",
      nodeId: node.id,
      patch: { value: coerceValue(raw, node.fieldKind) },
    });
  };

  return (
    <div
      className={`v7-block v7-pred v7-role-${tone}${selected ? " selected" : ""}${disabled ? " disabled" : ""}${node.float ? " float" : ""}`}
      onClick={(e) => {
        e.stopPropagation();
        onSelect();
      }}
      draggable
      onDragStart={onDragStart}
    >
      <span className="drag-handle" title="드래그해 이동">⋮⋮</span>
      <span className="param-label" title={node.param}>
        {displayParam(node.param, locale)}
      </span>

      {editingOp ? (
        <select
          autoFocus
          className="op-inline-select"
          value={node.op}
          onChange={(e) => {
            dispatch({
              type: "UPDATE_PREDICATE",
              nodeId: node.id,
              patch: { op: e.target.value as Op },
            });
            setEditingOp(false);
          }}
          onBlur={() => setEditingOp(false)}
          onClick={(e) => e.stopPropagation()}
        >
          {ops.map((o) => (
            <option key={o} value={o}>
              {o} ({OP_SYMBOL[o]})
            </option>
          ))}
        </select>
      ) : (
        <button
          className="op-symbol"
          title="클릭해 연산자 변경"
          onClick={(e) => {
            e.stopPropagation();
            setEditingOp(true);
          }}
        >
          {OP_SYMBOL[node.op]}
        </button>
      )}

      {!valueless && (
        editingValue ? (
          <input
            ref={valueInputRef}
            className="value-inline-input"
            defaultValue={valueText}
            onClick={(e) => e.stopPropagation()}
            onBlur={(e) => commitValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitValue((e.target as HTMLInputElement).value);
              if (e.key === "Escape") setEditingValue(false);
            }}
          />
        ) : (
          <button
            className="value-label"
            title="클릭해 비교값 편집"
            onClick={(e) => {
              e.stopPropagation();
              setEditingValue(true);
            }}
          >
            {rhsDisplay}
          </button>
        )
      )}

      {node.guardId && <span className="guard-id-inline">{node.guardId}</span>}
      {!supported && (
        <span className="badge-warn" title="우리 policy-schema.json에 미등록된 필드">
          schema 미등록
        </span>
      )}

      {node.parentId && (
        <button
          className="x-btn"
          title="삭제"
          onClick={(e) => {
            e.stopPropagation();
            dispatch({ type: "DELETE", nodeId: node.id });
          }}
        >
          ×
        </button>
      )}
    </div>
  );
}

/**
 * Coerce a free-text inline edit into the discriminated `PredicateValue`
 * based on the predicate's pinned `fieldKind`. Without this, every
 * inline-edited value was stored as `kind: "str"` which Cedar then
 * serialized with quotes (e.g. `slippageBp > "50"`) and the wasm
 * compiler rejected.
 */
function coerceValue(raw: string, fk: FieldKind): PredicateValue {
  if (raw.startsWith("@")) return { kind: "ref", text: raw };
  if (fk === "primitive.Long" || fk === "primitive.decimal") {
    return { kind: "num", text: raw };
  }
  if (fk === "primitive.Bool") {
    return { kind: "bool", text: raw.toLowerCase() === "true" ? "true" : "false" };
  }
  return { kind: "str", text: raw };
}
