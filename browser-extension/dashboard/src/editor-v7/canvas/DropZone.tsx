import { useState, type DragEvent, type ReactNode } from "react";

import { getDragState } from "./Canvas";

/**
 * A horizontal slot that highlights when a draggable block is hovering
 * over it. Owners (logic blocks, hat, drafts area) attach a single
 * `onDrop` callback; the zone takes care of preventDefault + dragLeave
 * dead-reckoning so the highlight doesn't flicker.
 */
export interface DropZoneProps {
  onDrop: () => void;
  /** When true, the zone is the "empty container" hint and renders a label. */
  empty?: boolean;
  children?: ReactNode;
}

export function DropZone({ onDrop, empty, children }: DropZoneProps) {
  const [over, setOver] = useState(false);

  const onDragOver = (e: DragEvent<HTMLDivElement>) => {
    if (!getDragState()) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    if (!over) setOver(true);
  };
  const onDragLeave = () => setOver(false);
  const onDropEvt = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setOver(false);
    onDrop();
  };

  return (
    <div
      className={`v7-drop-zone${empty ? " empty" : ""}${over ? " over" : ""}`}
      onDragOver={onDragOver}
      onDragLeave={onDragLeave}
      onDrop={onDropEvt}
    >
      {empty && children}
    </div>
  );
}
