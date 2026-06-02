import { type DragEvent } from "react";

import { DropZone } from "./DropZone";
import { LogicBlock } from "./LogicBlock";
import { PredicateBlock } from "./PredicateBlock";
import { getDragState, setDragState } from "./Canvas";
import { descendants } from "../doc";
import type { EditorAction } from "../reducer";
import type { Doc, NodeId } from "../types";

/**
 * Recursive subtree renderer. Logic blocks render a C-shape:
 *
 *   ┌─[ AND ]─────────┐
 *   │  <DropZone 0>   │
 *   │  <child 0>      │
 *   │  <DropZone 1>   │
 *   │  <child 1>      │
 *   │  <DropZone n>   │  ← append
 *   └─────────────────┘
 *
 * Predicate blocks render flat; they're never containers.
 *
 * Drag-and-drop:
 *   - Each block has `draggable=true`; on drag start we stash node id.
 *   - Drop zones intercept dragover/drop and dispatch `CONNECT` or
 *     `MOVE_CHILD` depending on whether the source is the same parent
 *     (re-order) or a different one (move).
 *   - Cycle prevention: a logic block won't accept a drop that would
 *     make it a descendant of itself.
 */
export interface BlockTreeProps {
  id: NodeId;
  doc: Doc;
  selectedId: NodeId | null;
  dispatch: (a: EditorAction) => void;
}

export function BlockTree({ id, doc, selectedId, dispatch }: BlockTreeProps) {
  const node = doc.nodes[id];
  if (!node) return null;

  const onDragStart = (e: DragEvent<HTMLDivElement>) => {
    e.stopPropagation();
    setDragState({ nodeId: id });
    e.dataTransfer.effectAllowed = "move";
    // Required for Firefox to start the drag.
    try { e.dataTransfer.setData("text/plain", id); } catch { /* noop */ }
  };

  if (node.type === "predicate") {
    return (
      <PredicateBlock
        node={node}
        locale={doc.locale}
        selected={selectedId === id}
        onSelect={() => dispatch({ type: "SELECT", nodeId: id })}
        onDragStart={onDragStart}
        dispatch={dispatch}
      />
    );
  }

  if (node.type === "logic") {
    const wouldCycle = (childId: NodeId): boolean => {
      if (childId === id) return true;
      return descendants(doc, childId).includes(id);
    };

    const acceptDrop = (toIndex: number) => {
      const drag = getDragState();
      if (!drag) return;
      setDragState(null);
      if (drag.fromPalette) {
        // Palette item — synthesize an ADD_PREDICATE + CONNECT pair via
        // a single ADD_PREDICATE with parentId pre-set so it inserts at
        // the end, then MOVE_CHILD to the requested index.
        dispatch({
          type: "ADD_PREDICATE",
          param: drag.fromPalette.param,
          cfg: { fk: drag.fromPalette.fk as never, op: "eq", parentId: id },
        });
        // Use a microtask trick: the new node id isn't available
        // synchronously; we accept appending only for palette drops.
        return;
      }
      if (wouldCycle(drag.nodeId)) return;
      const child = doc.nodes[drag.nodeId];
      if (!child || child.type === "hat") return;
      if (child.parentId === id) {
        // Re-order within the same parent.
        dispatch({ type: "MOVE_CHILD", nodeId: drag.nodeId, toIndex });
      } else {
        dispatch({ type: "CONNECT", childId: drag.nodeId, parentId: id, index: toIndex });
      }
    };

    return (
      <LogicBlock
        node={node}
        selected={selectedId === id}
        onSelect={() => dispatch({ type: "SELECT", nodeId: id })}
        onDragStart={onDragStart}
        dispatch={dispatch}
      >
        {node.childIds.length === 0 ? (
          <DropZone onDrop={() => acceptDrop(0)} empty>
            여기에 블록을 놓아 추가
          </DropZone>
        ) : (
          <>
            <DropZone onDrop={() => acceptDrop(0)} />
            {node.childIds.map((cid, i) => (
              <div key={cid} className="v7-child-slot">
                <BlockTree id={cid} doc={doc} selectedId={selectedId} dispatch={dispatch} />
                <DropZone onDrop={() => acceptDrop(i + 1)} />
              </div>
            ))}
          </>
        )}
      </LogicBlock>
    );
  }

  return null;
}
