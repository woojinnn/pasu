/**
 * Phase-8 dogfood — drive the reducer end-to-end through real user
 * scenarios and assert the resulting policy compiles + evaluates as
 * expected.
 *
 * Each scenario walks the same path a hand-user would:
 *   1. initialDoc + ADD_LOGIC / ADD_PREDICATE / CONNECT actions
 *   2. serializeDoc → Cedar text snapshot check
 *   3. evalDoc against pass + fail tx fixtures
 *
 * Gaps surfaced during writing get a `test.skip` + comment so the next
 * developer reading the file sees what's not yet supported.
 */

import { describe, expect, test } from "vitest";

import { initialDoc } from "./doc";
import { evalDoc } from "./evaluate";
import { editorReducer, initialEditorState, type EditorAction } from "./reducer";
import { serializeDoc } from "./serialize";

function run(actions: EditorAction[], action = "Amm::Swap") {
  let s = initialEditorState(initialDoc({ action }));
  for (const a of actions) s = editorReducer(s, a);
  return s;
}

describe("Phase-8 dogfood scenarios", () => {
  test("scenario 1 — slippage guard (slippageBp < 100)", () => {
    const s = run([
      {
        type: "ADD_PREDICATE",
        param: "context.slippageBp",
        cfg: { fk: "primitive.Long", op: "lt", value: 100, parentId: undefined },
      },
    ]);
    // Predicate was added as a draft (no parent). Connect it to root.
    const draftId = s.doc.drafts[0];
    const s2 = editorReducer(s, { type: "CONNECT", childId: draftId, parentId: s.doc.rootId });

    const cedar = serializeDoc(s2.doc);
    expect(cedar).toContain("context.slippageBp < 100");

    const pass = evalDoc(s2.doc, { context: { slippageBp: 50 } });
    expect(pass.verdict).toBe("ALLOW");

    const fail = evalDoc(s2.doc, { context: { slippageBp: 250 } });
    expect(fail.verdict).toBe("DENY");
    expect(fail.failed.map((f) => f.id)).toEqual([draftId]);
  });

  test("scenario 2 — unlimited approve guard (amount != MAX_UINT256)", () => {
    const MAX = "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let s = run([
      {
        type: "ADD_PREDICATE",
        param: "context.amount",
        cfg: { fk: "primitive.String", op: "neq", value: MAX, parentId: undefined },
      },
    ], "Token::Erc20Approve");
    const draftId = s.doc.drafts[0];
    s = editorReducer(s, { type: "CONNECT", childId: draftId, parentId: s.doc.rootId });

    const cedar = serializeDoc(s.doc);
    expect(cedar).toContain('Token::Action::"Erc20Approve"');
    expect(cedar).toContain(`context.amount != "${MAX}"`);

    const bounded = evalDoc(s.doc, { context: { amount: "0x100" } });
    expect(bounded.verdict).toBe("ALLOW");

    const unlimited = evalDoc(s.doc, { context: { amount: MAX } });
    expect(unlimited.verdict).toBe("DENY");
  });

  test("scenario 3 — recipient allowlist", () => {
    // GAP: our PredicateValue discriminant has no `set`/`collection` kind,
    // so we can't express `recipient in {"0xa", "0xb"}` from the reducer
    // path. Single-element allowlist via `eq` works as a stand-in.
    let s = run([
      {
        type: "ADD_PREDICATE",
        param: "context.recipient",
        cfg: { fk: "primitive.String", op: "eq", value: "@meta.from", parentId: undefined },
      },
    ]);
    const draftId = s.doc.drafts[0];
    s = editorReducer(s, { type: "CONNECT", childId: draftId, parentId: s.doc.rootId });

    const cedar = serializeDoc(s.doc);
    expect(cedar).toContain("context.recipient == meta.from");

    const self = evalDoc(s.doc, {
      meta: { from: "0xowner" },
      context: { recipient: "0xowner" },
    });
    expect(self.verdict).toBe("ALLOW");

    const other = evalDoc(s.doc, {
      meta: { from: "0xowner" },
      context: { recipient: "0xstranger" },
    });
    expect(other.verdict).toBe("DENY");
  });

  test.skip("scenario 4 — gas guard (meta.gas < threshold) — GAP: meta.gas not in V7_GLOSS palette", () => {
    // Adding a "meta.gas" predicate via the palette is impossible —
    // V7_GLOSS has `meta.from` only. Users would need to either:
    //   (a) extend V7_GLOSS to include meta.gas / meta.gasUsd, or
    //   (b) edit Cedar directly in Code mode.
    // Punted to a future schema rev.
  });

  test("scenario 5 — NOT-tree (compound risk-pattern exclusion)", () => {
    // Build NOT( AND( recipientIsContract, slippageBp > 500 ) ) — block any
    // transfer where the recipient is a contract AND slippage is suspiciously
    // high.
    let s = initialEditorState(initialDoc({ action: "Amm::Swap" }));
    s = editorReducer(s, { type: "ADD_LOGIC", op: "NOT", cfg: { parentId: s.doc.rootId } });
    const notId = Object.keys(s.doc.nodes).find((id) => {
      const n = s.doc.nodes[id];
      return n.type === "logic" && n.op === "NOT";
    })!;
    s = editorReducer(s, { type: "ADD_LOGIC", op: "AND", cfg: { parentId: notId } });
    const innerAndId = Object.keys(s.doc.nodes).find((id) => {
      const n = s.doc.nodes[id];
      return n.id !== s.doc.rootId && n.type === "logic" && n.op === "AND";
    })!;
    s = editorReducer(s, {
      type: "ADD_PREDICATE",
      param: "enrichment.recipientIsContract",
      cfg: { fk: "primitive.Bool", op: "isTrue", parentId: innerAndId },
    });
    s = editorReducer(s, {
      type: "ADD_PREDICATE",
      param: "context.slippageBp",
      cfg: { fk: "primitive.Long", op: "gt", value: 500, parentId: innerAndId },
    });

    const cedar = serializeDoc(s.doc);
    expect(cedar).toMatch(/!\(.*enrichment has recipientIsContract && enrichment\.recipientIsContract == true/);
    expect(cedar).toMatch(/context\.slippageBp > 500/);

    // Safe tx (low slippage to contract) → ALLOW.
    const safe = evalDoc(s.doc, {
      enrichment: { recipientIsContract: true },
      context: { slippageBp: 50 },
    });
    expect(safe.verdict).toBe("ALLOW");

    // Suspicious (high slippage + contract recipient) → DENY.
    const suspicious = evalDoc(s.doc, {
      enrichment: { recipientIsContract: true },
      context: { slippageBp: 1000 },
    });
    expect(suspicious.verdict).toBe("DENY");

    // High slippage to EOA → still ALLOW (NOT-tree needs both).
    const highSlippageEoa = evalDoc(s.doc, {
      enrichment: { recipientIsContract: false },
      context: { slippageBp: 1000 },
    });
    expect(highSlippageEoa.verdict).toBe("ALLOW");
  });

  test("undo/redo survives a complex build sequence", () => {
    // Simulate a user fumbling through a build: add, undo, redo, edit.
    let s = run([
      {
        type: "ADD_PREDICATE",
        param: "context.slippageBp",
        cfg: { fk: "primitive.Long", op: "lt", value: 100, parentId: undefined },
      },
    ]);
    expect(s.doc.drafts).toHaveLength(1);
    s = editorReducer(s, { type: "UNDO" });
    expect(s.doc.drafts).toHaveLength(0);
    s = editorReducer(s, { type: "REDO" });
    expect(s.doc.drafts).toHaveLength(1);

    // Connect to root.
    const draftId = s.doc.drafts[0];
    s = editorReducer(s, { type: "CONNECT", childId: draftId, parentId: s.doc.rootId });

    // Patch the threshold.
    s = editorReducer(s, {
      type: "UPDATE_PREDICATE",
      nodeId: draftId,
      patch: { value: { kind: "num", text: "50" } },
    });
    expect(serializeDoc(s.doc)).toContain("context.slippageBp < 50");

    // Undo back to threshold=100.
    s = editorReducer(s, { type: "UNDO" });
    expect(serializeDoc(s.doc)).toContain("context.slippageBp < 100");
  });
});
