/**
 * Reducer behavior:
 *   - ADD_PREDICATE w/o parent → goes to drafts.
 *   - CONNECT: detaches from old parent + drafts, inserts at index.
 *   - DELETE: removes node + descendants, scrubs from drafts.
 *   - UNDO/REDO: 50-step stack, view-only actions don't touch history.
 *   - Cycle prevention on CONNECT.
 */

import { describe, expect, test } from "vitest";

import { goldenSwapDoc, initialDoc } from "./doc";
import { editorReducer, initialEditorState } from "./reducer";

describe("editorReducer · structural mutations", () => {
  test("ADD_PREDICATE without parent goes to drafts", () => {
    const s0 = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    const s1 = editorReducer(s0, {
      type: "ADD_PREDICATE",
      param: "context.slippageBp",
      cfg: { fk: "primitive.Long", op: "lt", value: 100 },
    });
    expect(s1.doc.drafts).toHaveLength(1);
    const draft = s1.doc.nodes[s1.doc.drafts[0]];
    expect(draft?.type).toBe("predicate");
    if (draft?.type === "predicate") {
      expect(draft.float).toBe(true);
      expect(draft.param).toBe("context.slippageBp");
    }
  });

  test("ADD_PREDICATE with parentId attaches under that logic", () => {
    const s0 = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    const root = s0.doc.rootId;
    const s1 = editorReducer(s0, {
      type: "ADD_PREDICATE",
      param: "context.slippageBp",
      cfg: { fk: "primitive.Long", op: "lt", value: 100, parentId: root },
    });
    const newRoot = s1.doc.nodes[root];
    expect(newRoot?.type).toBe("logic");
    if (newRoot?.type === "logic") expect(newRoot.childIds).toHaveLength(1);
    expect(s1.doc.drafts).toEqual([]);
  });

  test("CONNECT moves a draft into the tree", () => {
    const s0 = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    const s1 = editorReducer(s0, {
      type: "ADD_PREDICATE",
      param: "context.slippageBp",
      cfg: { fk: "primitive.Long", op: "lt", value: 100 },
    });
    const draftId = s1.doc.drafts[0];
    const s2 = editorReducer(s1, { type: "CONNECT", childId: draftId, parentId: s1.doc.rootId });
    expect(s2.doc.drafts).toEqual([]);
    const root = s2.doc.nodes[s2.doc.rootId];
    if (root?.type === "logic") expect(root.childIds).toContain(draftId);
    const pred = s2.doc.nodes[draftId];
    if (pred?.type === "predicate") {
      expect(pred.float).toBe(false);
      expect(pred.parentId).toBe(s2.doc.rootId);
    }
  });

  test("CONNECT rejects cycles", () => {
    const s0 = initialEditorState(goldenSwapDoc());
    // s1 is a NOT under root; try connecting root underneath s1 — illegal.
    const root = s0.doc.nodes[s0.doc.rootId];
    if (root?.type !== "logic") throw new Error("expected logic root");
    const s1Id = root.childIds[0];
    const next = editorReducer(s0, { type: "CONNECT", childId: s0.doc.rootId, parentId: s1Id });
    // No state change → past stack unchanged.
    expect(next).toBe(s0);
  });

  test("DELETE removes node + descendants + cleans drafts", () => {
    const s0 = initialEditorState(goldenSwapDoc());
    const root = s0.doc.nodes[s0.doc.rootId];
    if (root?.type !== "logic") throw new Error("expected logic root");
    const s1NotId = root.childIds[0];

    const s1 = editorReducer(s0, { type: "DELETE", nodeId: s1NotId });
    // The NOT and its AND + 2 predicates all gone.
    expect(s1.doc.nodes[s1NotId]).toBeUndefined();
    const rootAfter = s1.doc.nodes[s1.doc.rootId];
    if (rootAfter?.type === "logic") expect(rootAfter.childIds).toHaveLength(2);
    // Drafts untouched (they weren't descendants of s1).
    expect(s1.doc.drafts).toHaveLength(2);
  });

  test("DELETE on draft prunes from drafts list", () => {
    const s0 = initialEditorState(goldenSwapDoc());
    const draftId = s0.doc.drafts[0];
    const s1 = editorReducer(s0, { type: "DELETE", nodeId: draftId });
    expect(s1.doc.nodes[draftId]).toBeUndefined();
    expect(s1.doc.drafts).toHaveLength(1);
  });

  test("DELETE on hat or root is a no-op", () => {
    const s0 = initialEditorState(goldenSwapDoc());
    const s1 = editorReducer(s0, { type: "DELETE", nodeId: s0.doc.hatId });
    const s2 = editorReducer(s0, { type: "DELETE", nodeId: s0.doc.rootId });
    expect(s1).toBe(s0);
    expect(s2).toBe(s0);
  });
});

describe("editorReducer · undo/redo", () => {
  test("UNDO restores previous doc, REDO replays", () => {
    const s0 = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    const s1 = editorReducer(s0, {
      type: "ADD_PREDICATE",
      param: "context.slippageBp",
      cfg: { fk: "primitive.Long", op: "lt", value: 100 },
    });
    expect(s1.doc.drafts).toHaveLength(1);

    const s2 = editorReducer(s1, { type: "UNDO" });
    expect(s2.doc.drafts).toHaveLength(0);
    expect(s2.future).toHaveLength(1);

    const s3 = editorReducer(s2, { type: "REDO" });
    expect(s3.doc.drafts).toHaveLength(1);
    expect(s3.future).toHaveLength(0);
  });

  test("history is capped at 50 entries", () => {
    let s = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    for (let i = 0; i < 60; i += 1) {
      s = editorReducer(s, {
        type: "ADD_PREDICATE",
        param: "context.slippageBp",
        cfg: { fk: "primitive.Long", op: "lt", value: i },
      });
    }
    expect(s.past.length).toBe(50);
  });

  test("SET_PAN / SET_ZOOM / SELECT bypass history", () => {
    const s0 = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    const s1 = editorReducer(s0, { type: "SET_PAN", pan: { x: 100, y: 50 } });
    const s2 = editorReducer(s1, { type: "SET_ZOOM", zoom: 1.5 });
    const s3 = editorReducer(s2, { type: "SELECT", nodeId: s0.doc.hatId });
    expect(s1.past).toEqual([]);
    expect(s2.past).toEqual([]);
    expect(s3.past).toEqual([]);
    expect(s3.doc.pan).toEqual({ x: 100, y: 50 });
    expect(s3.doc.zoom).toBe(1.5);
    expect(s3.selectedId).toBe(s0.doc.hatId);
  });

  test("a new action after UNDO clears the redo future", () => {
    const s0 = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    const s1 = editorReducer(s0, {
      type: "ADD_PREDICATE",
      param: "context.slippageBp",
      cfg: { fk: "primitive.Long", op: "lt", value: 100 },
    });
    const s2 = editorReducer(s1, { type: "UNDO" });
    expect(s2.future).toHaveLength(1);
    const s3 = editorReducer(s2, {
      type: "ADD_PREDICATE",
      param: "context.priceImpactBp",
      cfg: { fk: "primitive.Long", op: "gt", value: 50 },
    });
    expect(s3.future).toHaveLength(0);
  });
});

describe("editorReducer · ADD_LOGIC", () => {
  test("ADD_LOGIC without parent lands in drafts", () => {
    const s0 = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    const s1 = editorReducer(s0, { type: "ADD_LOGIC", op: "OR", cfg: {} });
    expect(s1.doc.drafts).toHaveLength(1);
    const node = s1.doc.nodes[s1.doc.drafts[0]];
    expect(node?.type).toBe("logic");
    if (node?.type === "logic") expect(node.op).toBe("OR");
  });

  test("ADD_LOGIC with parentId attaches to that logic", () => {
    const s0 = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    const s1 = editorReducer(s0, {
      type: "ADD_LOGIC",
      op: "NOT",
      cfg: { parentId: s0.doc.rootId },
    });
    const root = s1.doc.nodes[s1.doc.rootId];
    if (root?.type === "logic") expect(root.childIds).toHaveLength(1);
    expect(s1.doc.drafts).toEqual([]);
  });
});

describe("editorReducer · MOVE_CHILD + DUPLICATE", () => {
  test("MOVE_CHILD reorders siblings under the same parent", () => {
    const s0 = initialEditorState(goldenSwapDoc());
    const root = s0.doc.nodes[s0.doc.rootId];
    if (root?.type !== "logic") throw new Error("expected logic root");
    const original = [...root.childIds];
    expect(original).toHaveLength(3);
    // Move s3 (index 2) to the front.
    const s1 = editorReducer(s0, { type: "MOVE_CHILD", nodeId: original[2], toIndex: 0 });
    const rootAfter = s1.doc.nodes[s1.doc.rootId];
    if (rootAfter?.type !== "logic") throw new Error("expected logic root");
    expect(rootAfter.childIds).toEqual([original[2], original[0], original[1]]);
  });

  test("MOVE_CHILD is a no-op when the index doesn't change", () => {
    const s0 = initialEditorState(goldenSwapDoc());
    const root = s0.doc.nodes[s0.doc.rootId];
    if (root?.type !== "logic") throw new Error("expected logic root");
    const s1 = editorReducer(s0, { type: "MOVE_CHILD", nodeId: root.childIds[0], toIndex: 0 });
    expect(s1).toBe(s0);
  });

  test("DUPLICATE clones a subtree into drafts with fresh ids", () => {
    const s0 = initialEditorState(goldenSwapDoc());
    const root = s0.doc.nodes[s0.doc.rootId];
    if (root?.type !== "logic") throw new Error("expected logic root");
    const s1NotId = root.childIds[0]; // s1 = NOT(AND(...))
    const before = Object.keys(s0.doc.nodes).length;
    const s1 = editorReducer(s0, { type: "DUPLICATE", nodeId: s1NotId });
    // s1 NOT-tree has 4 nodes (NOT + AND + 2 predicates). Clone adds 4.
    expect(Object.keys(s1.doc.nodes).length).toBe(before + 4);
    // The duplicated top is a draft.
    expect(s1.doc.drafts.length).toBeGreaterThan(s0.doc.drafts.length);
    // Original tree untouched.
    expect(s1.doc.nodes[s1NotId]).toEqual(s0.doc.nodes[s1NotId]);
  });
});

describe("editorReducer · updates", () => {
  test("UPDATE_PREDICATE patches op/value", () => {
    const s0 = initialEditorState(goldenSwapDoc());
    const slippage = Object.values(s0.doc.nodes).find(
      (n) => n.type === "predicate" && n.param === "context.slippageBp",
    );
    if (!slippage) throw new Error("expected slippage predicate");
    const s1 = editorReducer(s0, {
      type: "UPDATE_PREDICATE",
      nodeId: slippage.id,
      patch: { op: "lte" },
    });
    const updated = s1.doc.nodes[slippage.id];
    if (updated?.type === "predicate") expect(updated.op).toBe("lte");
  });

  test("SET_HAT updates effect + action on doc + hat node", () => {
    const s0 = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    const s1 = editorReducer(s0, { type: "SET_HAT", effect: "deny", action: "Erc20::Approve" });
    const hat = s1.doc.nodes[s1.doc.hatId];
    if (hat?.type === "hat") {
      expect(hat.effect).toBe("deny");
      expect(hat.action).toBe("Erc20::Approve");
    }
    expect(s1.doc.action).toBe("Erc20::Approve");
  });
});
