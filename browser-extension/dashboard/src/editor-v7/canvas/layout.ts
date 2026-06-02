/**
 * Auto-layout: walks the tree and assigns `x/y` to each connected
 * node so blocks stack neatly under their hat without the user
 * manually positioning them.
 *
 * Algorithm:
 *   - Hat at (hx, hy).
 *   - Root logic at (hx, hy + ROW).
 *   - Children laid out top-down with `COL` indent per nesting level.
 *   - Floating drafts keep their stored x/y (user-positioned).
 *
 * `LayoutResult.positions` is keyed by node id; the caller patches
 * `doc.nodes` or just reads it during render (we render via positions
 * directly so undo doesn't churn x/y).
 */

import type { Doc, NodeId } from "../types";

const ROW = 84;
const COL = 36;
const BLOCK_GAP = 12;

export interface LayoutResult {
  positions: Record<NodeId, { x: number; y: number }>;
  /** Bounding box of the laid-out body (useful for "fit to view"). */
  bounds: { minX: number; minY: number; maxX: number; maxY: number };
}

export function autoLayout(doc: Doc): LayoutResult {
  const positions: Record<NodeId, { x: number; y: number }> = {};
  const hat = doc.nodes[doc.hatId];
  if (!hat || hat.type !== "hat") {
    return { positions, bounds: { minX: 0, minY: 0, maxX: 0, maxY: 0 } };
  }
  positions[hat.id] = { x: hat.x || 80, y: hat.y || 80 };

  let maxX = positions[hat.id].x;
  let maxY = positions[hat.id].y;
  let yCursor = positions[hat.id].y + ROW;

  const walk = (id: NodeId, depth: number): void => {
    const node = doc.nodes[id];
    if (!node) return;
    positions[id] = { x: positions[hat.id].x + depth * COL, y: yCursor };
    maxX = Math.max(maxX, positions[id].x);
    maxY = Math.max(maxY, positions[id].y);
    yCursor += ROW;
    if (node.type === "logic") {
      for (const cid of node.childIds) walk(cid, depth + 1);
    }
  };

  if (hat.childId) walk(hat.childId, 0);

  // Floating drafts retain user-stored coords.
  for (const draftId of doc.drafts) {
    const n = doc.nodes[draftId];
    if (!n) continue;
    positions[draftId] = { x: n.x || 0, y: n.y || 0 };
    maxX = Math.max(maxX, positions[draftId].x);
    maxY = Math.max(maxY, positions[draftId].y);
  }

  return {
    positions,
    bounds: {
      minX: positions[hat.id].x,
      minY: positions[hat.id].y,
      maxX: maxX + 200,
      maxY: maxY + BLOCK_GAP,
    },
  };
}

export const CANVAS_METRICS = { ROW, COL, BLOCK_GAP };
