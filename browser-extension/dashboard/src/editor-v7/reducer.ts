/**
 * State machine for the v7 block canvas.
 *
 * Wraps `Doc` in an `EditorState` that carries an undo/redo stack
 * (50 steps max, oldest dropped). Actions split into two camps:
 *   - **Doc-mutating**: ADD_*, DELETE, CONNECT, UPDATE_*, MOVE_NODE,
 *     SET_HAT, LOAD_DOC — pushed onto the undo stack.
 *   - **View-only**: SET_PAN, SET_ZOOM, SELECT — bypass history so
 *     panning the canvas doesn't pollute undo.
 *
 * The reducer rebuilds `nodes` immutably per action; helpers below
 * stay non-mutating so React-style equality checks behave.
 */

import { descendants, makeLogic, makePredicate, newId, type LogicConfig, type PredicateConfig } from "./doc";
import type { Doc, LogicNode, Node, NodeId, PredicateNode } from "./types";
import type { Op, PredicateValue } from "./schema";

const HISTORY_LIMIT = 50;

export interface EditorState {
  doc: Doc;
  past: Doc[];
  future: Doc[];
  /** Currently selected block id (for Inspector panel). Not part of
   *  history so toggling selection doesn't pollute undo. */
  selectedId: NodeId | null;
}

export type EditorAction =
  | { type: "ADD_PREDICATE"; param: string; cfg: PredicateConfig; asDraft?: boolean }
  | { type: "ADD_LOGIC"; op: "AND" | "OR" | "NOT"; cfg?: LogicConfig }
  | { type: "CONNECT"; childId: NodeId; parentId: NodeId; index?: number }
  | { type: "MOVE_CHILD"; nodeId: NodeId; toIndex: number }
  | { type: "DISCONNECT"; nodeId: NodeId }
  | { type: "DELETE"; nodeId: NodeId }
  | { type: "DUPLICATE"; nodeId: NodeId }
  | { type: "UPDATE_PREDICATE"; nodeId: NodeId; patch: Partial<Pick<PredicateNode,
        "op" | "value" | "fieldKind" | "absence" | "label" | "guardId" | "enabled" | "userCopy" | "note">> }
  | { type: "UPDATE_LOGIC"; nodeId: NodeId; patch: Partial<Pick<LogicNode,
        "op" | "label" | "guardId" | "enabled" | "userCopy">> }
  | { type: "MOVE_NODE"; nodeId: NodeId; x: number; y: number }
  | { type: "SET_HAT"; effect?: "permit" | "deny"; action?: string }
  | { type: "SET_POLICY_NAME"; name: string }
  | { type: "SET_DENY_MESSAGE"; message: string }
  | { type: "LOAD_DOC"; doc: Doc }
  | { type: "SET_PAN"; pan: { x: number; y: number } }
  | { type: "SET_ZOOM"; zoom: number }
  | { type: "SELECT"; nodeId: NodeId | null }
  | { type: "UNDO" }
  | { type: "REDO" };

export function initialEditorState(doc: Doc): EditorState {
  return { doc, past: [], future: [], selectedId: null };
}

// ── helpers ────────────────────────────────────────────────────────────

function withNodes(doc: Doc, patch: Record<NodeId, Node>): Doc {
  return { ...doc, nodes: { ...doc.nodes, ...patch } };
}

function patchNode<N extends Node>(doc: Doc, id: NodeId, patch: Partial<N>): Doc {
  const cur = doc.nodes[id];
  if (!cur) return doc;
  const next = { ...cur, ...patch } as Node;
  return withNodes(doc, { [id]: next });
}

/** Remove `childId` from whatever parent currently lists it. Walks
 *  `parentId` first (fast path), then falls back to a scan in case
 *  the link is stale. */
function detachFromParent(doc: Doc, childId: NodeId): Doc {
  const child = doc.nodes[childId];
  if (!child || child.type === "hat") return doc;
  const parentId = child.parentId;
  let next = doc;
  if (parentId) {
    const parent = doc.nodes[parentId];
    if (parent?.type === "logic") {
      next = patchNode<LogicNode>(next, parent.id, {
        childIds: parent.childIds.filter((c) => c !== childId),
      });
    } else if (parent?.type === "hat" && parent.childId === childId) {
      next = patchNode(next, parent.id, { childId: null });
    }
  }
  next = patchNode(next, childId, { parentId: null } as Partial<Node>);
  return next;
}

function pushHistory(state: EditorState, nextDoc: Doc): EditorState {
  const past = [...state.past, state.doc];
  if (past.length > HISTORY_LIMIT) past.shift();
  return { ...state, doc: nextDoc, past, future: [] };
}

// ── action handlers ───────────────────────────────────────────────────

function handleAddPredicate(
  state: EditorState,
  a: Extract<EditorAction, { type: "ADD_PREDICATE" }>,
): EditorState {
  const node = makePredicate(a.param, a.cfg);
  const asDraft = a.asDraft ?? !a.cfg.parentId;
  if (asDraft) node.float = true;

  let next: Doc = withNodes(state.doc, { [node.id]: node });
  if (asDraft) {
    next = { ...next, drafts: [...next.drafts, node.id] };
  } else if (a.cfg.parentId) {
    const parent = next.nodes[a.cfg.parentId];
    if (parent?.type === "logic") {
      next = patchNode<LogicNode>(next, parent.id, {
        childIds: [...parent.childIds, node.id],
      });
    } else if (parent?.type === "hat") {
      next = patchNode(next, parent.id, { childId: node.id });
    }
  }
  return pushHistory(state, next);
}

function handleAddLogic(
  state: EditorState,
  a: Extract<EditorAction, { type: "ADD_LOGIC" }>,
): EditorState {
  const node = makeLogic(a.op, a.cfg);
  let next: Doc = withNodes(state.doc, { [node.id]: node });
  if (a.cfg?.parentId) {
    const parent = next.nodes[a.cfg.parentId];
    if (parent?.type === "logic") {
      next = patchNode<LogicNode>(next, parent.id, {
        childIds: [...parent.childIds, node.id],
      });
    } else if (parent?.type === "hat") {
      next = patchNode(next, parent.id, { childId: node.id });
    }
  } else {
    // No parent → land in drafts so the user can see and drag it.
    next = { ...next, drafts: [...next.drafts, node.id] };
  }
  return pushHistory(state, next);
}

function handleConnect(
  state: EditorState,
  a: Extract<EditorAction, { type: "CONNECT" }>,
): EditorState {
  // Reject cycles — `parentId` can't already be a descendant of `childId`.
  if (descendants(state.doc, a.childId).includes(a.parentId)) return state;

  let next = detachFromParent(state.doc, a.childId);
  next = { ...next, drafts: next.drafts.filter((d) => d !== a.childId) };

  const parent = next.nodes[a.parentId];
  if (parent?.type === "logic") {
    const insertAt = a.index ?? parent.childIds.length;
    const newKids = [...parent.childIds];
    newKids.splice(insertAt, 0, a.childId);
    next = patchNode<LogicNode>(next, parent.id, { childIds: newKids });
  } else if (parent?.type === "hat") {
    if (parent.childId && parent.childId !== a.childId) return state;
    next = patchNode(next, parent.id, { childId: a.childId });
  } else {
    return state;
  }

  next = patchNode(next, a.childId, { parentId: a.parentId, float: false } as Partial<Node>);
  return pushHistory(state, next);
}

function handleDisconnect(
  state: EditorState,
  a: Extract<EditorAction, { type: "DISCONNECT" }>,
): EditorState {
  const node = state.doc.nodes[a.nodeId];
  if (!node || node.type === "hat") return state;
  let next = detachFromParent(state.doc, a.nodeId);
  if (node.type === "predicate") {
    next = patchNode<PredicateNode>(next, a.nodeId, { float: true });
    if (!next.drafts.includes(a.nodeId)) {
      next = { ...next, drafts: [...next.drafts, a.nodeId] };
    }
  }
  return pushHistory(state, next);
}

function handleDelete(
  state: EditorState,
  a: Extract<EditorAction, { type: "DELETE" }>,
): EditorState {
  if (a.nodeId === state.doc.hatId || a.nodeId === state.doc.rootId) return state;
  let next = detachFromParent(state.doc, a.nodeId);
  const orphans = descendants(next, a.nodeId);
  const nodes = { ...next.nodes };
  for (const id of orphans) delete nodes[id];
  next = {
    ...next,
    nodes,
    drafts: next.drafts.filter((d) => !orphans.includes(d)),
  };
  const selectedId = orphans.includes(state.selectedId ?? "") ? null : state.selectedId;
  return { ...pushHistory(state, next), selectedId };
}

function handleMoveChild(
  state: EditorState,
  a: Extract<EditorAction, { type: "MOVE_CHILD" }>,
): EditorState {
  const child = state.doc.nodes[a.nodeId];
  if (!child || child.type === "hat" || !child.parentId) return state;
  const parent = state.doc.nodes[child.parentId];
  if (!parent || parent.type !== "logic") return state;
  const cur = parent.childIds.indexOf(a.nodeId);
  if (cur < 0) return state;
  const filtered = parent.childIds.filter((c) => c !== a.nodeId);
  const clamped = Math.max(0, Math.min(filtered.length, a.toIndex));
  filtered.splice(clamped, 0, a.nodeId);
  if (filtered.every((id, i) => parent.childIds[i] === id)) return state;
  const next = patchNode<LogicNode>(state.doc, parent.id, { childIds: filtered });
  return pushHistory(state, next);
}

/**
 * Deep-clone a subtree with fresh ids. Used by DUPLICATE — the new
 * subtree starts as a floating draft (top-level predicate becomes a
 * draft; logic blocks land in `drafts` array via the top node).
 */
function cloneSubtree(doc: Doc, rootId: NodeId): { nodes: Record<NodeId, Node>; newRoot: NodeId } {
  const remap: Record<NodeId, NodeId> = {};
  const ids = descendants(doc, rootId);
  for (const id of ids) {
    const src = doc.nodes[id];
    if (!src) continue;
    const prefix = src.type === "logic" ? "L" : src.type === "predicate" ? "p" : "n";
    remap[id] = newId(prefix as "p" | "L");
  }
  const cloned: Record<NodeId, Node> = {};
  for (const id of ids) {
    const src = doc.nodes[id];
    if (!src) continue;
    const newSelf = remap[id];
    const newParent = src.type !== "hat" && src.parentId ? remap[src.parentId] ?? null : null;
    if (src.type === "logic") {
      cloned[newSelf] = {
        ...src,
        id: newSelf,
        parentId: newParent,
        childIds: src.childIds.map((c) => remap[c]).filter(Boolean),
      };
    } else if (src.type === "predicate") {
      cloned[newSelf] = { ...src, id: newSelf, parentId: newParent };
    }
  }
  return { nodes: cloned, newRoot: remap[rootId] };
}

function handleDuplicate(
  state: EditorState,
  a: Extract<EditorAction, { type: "DUPLICATE" }>,
): EditorState {
  const src = state.doc.nodes[a.nodeId];
  if (!src || src.type === "hat") return state;
  const { nodes, newRoot } = cloneSubtree(state.doc, a.nodeId);
  // The duplicated subtree lands as a floating draft (or logic) at a
  // slight offset so the user can see it.
  const top = nodes[newRoot];
  if (!top) return state;
  if (top.type === "predicate") {
    top.float = true;
    top.x = (src.x ?? 0) + 24;
    top.y = (src.y ?? 0) + 24;
    top.parentId = null;
  } else if (top.type === "logic") {
    top.parentId = null;
    top.x = (src.x ?? 0) + 24;
    top.y = (src.y ?? 0) + 24;
  }
  const next: Doc = {
    ...state.doc,
    nodes: { ...state.doc.nodes, ...nodes },
    drafts: [...state.doc.drafts, newRoot],
  };
  return pushHistory(state, next);
}

// ── reducer ────────────────────────────────────────────────────────────

export function editorReducer(state: EditorState, action: EditorAction): EditorState {
  switch (action.type) {
    case "ADD_PREDICATE":
      return handleAddPredicate(state, action);
    case "ADD_LOGIC":
      return handleAddLogic(state, action);
    case "CONNECT":
      return handleConnect(state, action);
    case "DISCONNECT":
      return handleDisconnect(state, action);
    case "DELETE":
      return handleDelete(state, action);
    case "MOVE_CHILD":
      return handleMoveChild(state, action);
    case "DUPLICATE":
      return handleDuplicate(state, action);

    case "UPDATE_PREDICATE": {
      const cur = state.doc.nodes[action.nodeId];
      if (!cur || cur.type !== "predicate") return state;
      return pushHistory(state, patchNode<PredicateNode>(state.doc, action.nodeId, action.patch));
    }
    case "UPDATE_LOGIC": {
      const cur = state.doc.nodes[action.nodeId];
      if (!cur || cur.type !== "logic") return state;
      return pushHistory(state, patchNode<LogicNode>(state.doc, action.nodeId, action.patch));
    }

    case "MOVE_NODE": {
      const cur = state.doc.nodes[action.nodeId];
      if (!cur) return state;
      // Don't push to history per pixel — moves are merged with the
      // previous move via overwrite (last drag wins as one undo step).
      const next = patchNode(state.doc, action.nodeId, { x: action.x, y: action.y } as Partial<Node>);
      const prev = state.past[state.past.length - 1];
      const prevNode = prev?.nodes[action.nodeId];
      if (prev && prevNode && (prevNode.x !== cur.x || prevNode.y !== cur.y)) {
        // We already pushed a history entry for this drag; just swap present.
        return { ...state, doc: next };
      }
      return pushHistory(state, next);
    }

    case "SET_HAT": {
      const hat = state.doc.nodes[state.doc.hatId];
      if (!hat || hat.type !== "hat") return state;
      const patch: Partial<typeof hat> = {};
      if (action.effect !== undefined) patch.effect = action.effect;
      if (action.action !== undefined) patch.action = action.action;
      const next: Doc = patchNode(state.doc, hat.id, patch);
      const docPatch = action.action !== undefined ? { action: action.action } : {};
      return pushHistory(state, { ...next, ...docPatch });
    }
    case "SET_POLICY_NAME":
      return pushHistory(state, { ...state.doc, policyName: action.name });
    case "SET_DENY_MESSAGE":
      return pushHistory(state, { ...state.doc, denyMessage: action.message });
    case "LOAD_DOC":
      return pushHistory(state, action.doc);

    case "SET_PAN":
      return { ...state, doc: { ...state.doc, pan: action.pan } };
    case "SET_ZOOM":
      return { ...state, doc: { ...state.doc, zoom: action.zoom } };
    case "SELECT":
      return { ...state, selectedId: action.nodeId };

    case "UNDO": {
      if (state.past.length === 0) return state;
      const prev = state.past[state.past.length - 1];
      return {
        ...state,
        doc: prev,
        past: state.past.slice(0, -1),
        future: [state.doc, ...state.future],
      };
    }
    case "REDO": {
      if (state.future.length === 0) return state;
      const [next, ...rest] = state.future;
      return {
        ...state,
        doc: next,
        past: [...state.past, state.doc],
        future: rest,
      };
    }
    default:
      return state;
  }
}

// Re-export for convenience.
export type { Op, PredicateValue };
