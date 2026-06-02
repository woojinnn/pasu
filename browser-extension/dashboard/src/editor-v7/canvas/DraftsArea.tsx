import { useState, type DragEvent } from "react";

import { BlockTree } from "./BlockTree";
import { getDragState, setDragState } from "./Canvas";
import type { EditorAction } from "../reducer";
import type { Doc, NodeId } from "../types";

/**
 * Below the tree, floating blocks live in a single lane. Drag a block
 * from the tree into here to disconnect it; drag from here back into
 * a logic block to re-attach.
 */
export interface DraftsAreaProps {
  drafts: NodeId[];
  doc: Doc;
  selectedId: NodeId | null;
  dispatch: (a: EditorAction) => void;
}

export function DraftsArea({ drafts, doc, selectedId, dispatch }: DraftsAreaProps) {
  const [over, setOver] = useState(false);

  const onDragOver = (e: DragEvent<HTMLDivElement>) => {
    const drag = getDragState();
    if (!drag) return;
    const child = doc.nodes[drag.nodeId];
    if (!child || child.type === "hat") return;
    // Already a draft → no-op.
    if (drafts.includes(drag.nodeId)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    setOver(true);
  };
  const onDragLeave = () => setOver(false);
  const onDrop = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setOver(false);
    const drag = getDragState();
    setDragState(null);
    if (!drag) return;
    if (drafts.includes(drag.nodeId)) return;
    dispatch({ type: "DISCONNECT", nodeId: drag.nodeId });
  };

  if (drafts.length === 0 && !getDragState()) {
    return (
      <section
        className={`v7-drafts empty${over ? " over" : ""}`}
        onDragOver={onDragOver}
        onDragLeave={onDragLeave}
        onDrop={onDrop}
      >
        <span className="drafts-hint">트리에서 떼어낸 블록은 여기에 모입니다</span>
      </section>
    );
  }

  return (
    <section
      className={`v7-drafts${over ? " over" : ""}`}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDrop}
    >
      <header className="drafts-head">미연결 블록 · {drafts.length}</header>
      <div className="drafts-row">
        {drafts.map((id) => (
          <BlockTree key={id} id={id} doc={doc} selectedId={selectedId} dispatch={dispatch} />
        ))}
      </div>
    </section>
  );
}
