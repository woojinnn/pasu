import type { DragEvent, ReactNode } from "react";

import type { EditorAction } from "../reducer";
import type { LogicNode } from "../types";

/**
 * C-shape container for AND / OR / NOT. The header carries the op
 * badge + guard label; the body holds child blocks (rendered by
 * `BlockTree` and passed in via `children`).
 *
 * Click op badge to cycle operators directly without opening Inspector.
 */
export interface LogicBlockProps {
  node: LogicNode;
  selected: boolean;
  onSelect: () => void;
  onDragStart: (e: DragEvent<HTMLDivElement>) => void;
  dispatch: (a: EditorAction) => void;
  children: ReactNode;
}

const NEXT_OP: Record<"AND" | "OR" | "NOT", "AND" | "OR" | "NOT"> = {
  AND: "OR",
  OR: "NOT",
  NOT: "AND",
};

export function LogicBlock({
  node,
  selected,
  onSelect,
  onDragStart,
  dispatch,
  children,
}: LogicBlockProps) {
  const disabled = node.enabled === false;
  return (
    <div
      className={`v7-block v7-logic v7-op-${node.op}${selected ? " selected" : ""}${disabled ? " disabled" : ""}`}
      onClick={(e) => {
        e.stopPropagation();
        onSelect();
      }}
      draggable
      onDragStart={onDragStart}
    >
      <header className="logic-head">
        <span className="drag-handle" title="드래그해 이동">⋮⋮</span>
        <button
          className="op-badge"
          title="클릭해 연산자 변경 (AND→OR→NOT)"
          onClick={(e) => {
            e.stopPropagation();
            dispatch({ type: "UPDATE_LOGIC", nodeId: node.id, patch: { op: NEXT_OP[node.op] } });
          }}
        >
          {node.op}
        </button>
        {node.label && <span className="guard-label">{node.label}</span>}
        {node.guardId && <span className="guard-id">{node.guardId}</span>}
        <span className="grow" />
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
      </header>
      <div className="logic-body">{children}</div>
    </div>
  );
}
