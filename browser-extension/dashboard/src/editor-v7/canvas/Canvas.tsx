import { useCallback, useRef, useState, type MouseEvent, type WheelEvent } from "react";

import { BlockTree } from "./BlockTree";
import { DraftsArea } from "./DraftsArea";
import { HatBlock } from "./HatBlock";
import type { EditorAction, EditorState } from "../reducer";
import type { NodeId } from "../types";
import "./canvas.css";

/**
 * v7 block canvas — Scratch-style nested containers.
 *
 * Hat sits at the top; the root logic block recursively renders its
 * children *inside* its own C-shape, so the visual hierarchy matches
 * the logical one. Drafts (unconnected predicates / logics) live in a
 * separate "drafts" lane below the tree.
 *
 * Drag-and-drop: every block is draggable; logic blocks expose drop
 * zones between siblings (insert at index) and an "append" zone at the
 * end. The hat exposes one drop zone for the root replacement.
 *
 * Pan / zoom: shift+drag or middle-button pans; ctrl+wheel zooms.
 */
export interface EditorCanvasProps {
  state: EditorState;
  dispatch: (a: EditorAction) => void;
}

/** Shared drag payload — propagates through DataTransfer + a ref so we
 *  don't depend on the (read-only-during-dragover) DataTransfer .types. */
export interface DragState {
  nodeId: NodeId;
  /** When `true`, the source is a palette template — payload encodes
   *  the desired `ADD_PREDICATE` instead of moving an existing node. */
  fromPalette?: { param: string; fk: string };
}

const DRAG_REF = { current: null as DragState | null };

export function getDragState(): DragState | null {
  return DRAG_REF.current;
}

export function setDragState(d: DragState | null): void {
  DRAG_REF.current = d;
}

export function EditorCanvas({ state, dispatch }: EditorCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const panState = useRef<{ x: number; y: number; px: number; py: number } | null>(null);
  const [_, force] = useState(0);
  const { pan, zoom } = state.doc;

  // ── pan handlers ────────────────────────────────────────────────────
  const onMouseDown = useCallback(
    (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      // Don't grab clicks on actual blocks / buttons / inputs.
      if (target.closest(".v7-block, .v7-drop-zone, button, input, select, textarea")) {
        if (e.target === containerRef.current && e.button === 0) {
          dispatch({ type: "SELECT", nodeId: null });
        }
        // Still allow shift+left to pan even when hitting a block.
        if (!(e.button === 1 || (e.button === 0 && e.shiftKey))) return;
      }
      if (e.button === 1 || (e.button === 0 && e.shiftKey)) {
        panState.current = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
        e.preventDefault();
      } else if (e.button === 0 && e.target === containerRef.current) {
        dispatch({ type: "SELECT", nodeId: null });
      }
    },
    [dispatch, pan.x, pan.y],
  );
  const onMouseMove = useCallback(
    (e: MouseEvent) => {
      if (!panState.current) return;
      const dx = e.clientX - panState.current.x;
      const dy = e.clientY - panState.current.y;
      dispatch({ type: "SET_PAN", pan: { x: panState.current.px + dx, y: panState.current.py + dy } });
    },
    [dispatch],
  );
  const onMouseUp = useCallback(() => {
    panState.current = null;
  }, []);

  // ── zoom handler ────────────────────────────────────────────────────
  const onWheel = useCallback(
    (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const next = Math.max(0.5, Math.min(2.0, zoom * (1 - e.deltaY * 0.001)));
      dispatch({ type: "SET_ZOOM", zoom: next });
    },
    [dispatch, zoom],
  );

  // ── drag-over guard on the canvas root: clears drag state on drop
  //    outside a target.
  const onDragEnd = useCallback(() => {
    setDragState(null);
    force((n) => n + 1);
  }, []);

  const hat = state.doc.nodes[state.doc.hatId];

  return (
    <div
      ref={containerRef}
      className="v7-canvas"
      onMouseDown={onMouseDown}
      onMouseMove={onMouseMove}
      onMouseUp={onMouseUp}
      onMouseLeave={onMouseUp}
      onWheel={onWheel}
      onDragEnd={onDragEnd}
    >
      <CanvasFloor pan={pan} zoom={zoom}>
        {hat?.type === "hat" && (
          <div className="v7-tree-column">
            <HatBlock
              node={hat}
              selected={state.selectedId === hat.id}
              onSelect={() => dispatch({ type: "SELECT", nodeId: hat.id })}
            />
            <HatToRootConnector />
            {hat.childId && (
              <BlockTree
                id={hat.childId}
                doc={state.doc}
                selectedId={state.selectedId}
                dispatch={dispatch}
              />
            )}
          </div>
        )}

        <DraftsArea
          drafts={state.doc.drafts}
          doc={state.doc}
          selectedId={state.selectedId}
          dispatch={dispatch}
        />
      </CanvasFloor>

      <CanvasToolbar
        zoom={zoom}
        onZoom={(z) => dispatch({ type: "SET_ZOOM", zoom: z })}
        onReset={() => dispatch({ type: "SET_PAN", pan: { x: 0, y: 0 } })}
        canUndo={state.past.length > 0}
        canRedo={state.future.length > 0}
        onUndo={() => dispatch({ type: "UNDO" })}
        onRedo={() => dispatch({ type: "REDO" })}
      />
    </div>
  );
}

function CanvasFloor({
  pan,
  zoom,
  children,
}: {
  pan: { x: number; y: number };
  zoom: number;
  children: React.ReactNode;
}) {
  return (
    <div
      className="v7-canvas-floor"
      style={{ transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }}
    >
      {children}
    </div>
  );
}

function HatToRootConnector() {
  return <div className="v7-connector" aria-hidden />;
}

function CanvasToolbar({
  zoom,
  onZoom,
  onReset,
  canUndo,
  canRedo,
  onUndo,
  onRedo,
}: {
  zoom: number;
  onZoom: (z: number) => void;
  onReset: () => void;
  canUndo: boolean;
  canRedo: boolean;
  onUndo: () => void;
  onRedo: () => void;
}) {
  return (
    <div className="v7-toolbar">
      <button disabled={!canUndo} onClick={onUndo} title="Undo (⌘Z)">↶</button>
      <button disabled={!canRedo} onClick={onRedo} title="Redo (⇧⌘Z)">↷</button>
      <span className="sep" />
      <button onClick={() => onZoom(Math.max(0.5, zoom - 0.1))} title="Zoom out">−</button>
      <span className="zoom-label">{Math.round(zoom * 100)}%</span>
      <button onClick={() => onZoom(Math.min(2.0, zoom + 0.1))} title="Zoom in">+</button>
      <button onClick={onReset} title="Reset pan">⊕</button>
    </div>
  );
}
