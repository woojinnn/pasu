/**
 * Evaluator + serializer regression checks against V7_SAMPLE_TX
 * (3 hand-curated fixtures from `front/scopeball-v3/editor-v7-data.js`).
 *
 *   tx-calm              → ALLOW (all 3 guards true)
 *   tx-market-expiry     → DENY  (s2 + s3 fail)
 *   tx-send-to-contract  → DENY  (s1 fails: NOT(recipient≠from ∧ recipientIsContract))
 *
 * The serializer test only pins structural invariants (permit header,
 * action, guard count, NOT/AND/OR composition). Exact whitespace is a
 * snapshot detail we leave for the wasm round-trip check.
 */

import { describe, expect, test } from "vitest";

import { goldenSwapDoc } from "./doc";
import { evalDoc, type TxLike } from "./evaluate";
import { serializeDoc } from "./serialize";

const FIXTURES: Record<
  "calm" | "marketExpiry" | "sendToContract",
  { tx: TxLike; verdict: "ALLOW" | "DENY"; failed: string[] }
> = {
  calm: {
    tx: {
      meta: { from: "0xA1c4000000000000000000000000000000007e29" },
      enrichment: { validityDeltaSec: 300, recipientIsContract: false, totalInputUsd: 4800 },
      context: {
        recipient: "0xA1c4000000000000000000000000000000007e29",
        slippageBp: 50,
        priceImpactBp: 12,
      },
    },
    verdict: "ALLOW",
    failed: [],
  },
  marketExpiry: {
    tx: {
      meta: { from: "0xA1c4000000000000000000000000000000007e29" },
      enrichment: { validityDeltaSec: 18, recipientIsContract: false, totalInputUsd: 4800 },
      context: {
        recipient: "0xA1c4000000000000000000000000000000007e29",
        slippageBp: 150,
        priceImpactBp: 60,
      },
    },
    verdict: "DENY",
    failed: ["s2", "s3"],
  },
  sendToContract: {
    tx: {
      meta: { from: "0xA1c4000000000000000000000000000000007e29" },
      enrichment: { validityDeltaSec: 200, recipientIsContract: true, totalInputUsd: 4800 },
      context: {
        recipient: "0xBEEF000000000000000000000000000000001234",
        slippageBp: 40,
        priceImpactBp: 10,
      },
    },
    verdict: "DENY",
    failed: ["s1"],
  },
};

describe("evalDoc · V7_SAMPLE_TX golden fixtures", () => {
  test("Calm swap → ALLOW (no failed guards)", () => {
    const doc = goldenSwapDoc();
    const r = evalDoc(doc, FIXTURES.calm.tx);
    expect(r.verdict).toBe("ALLOW");
    expect(r.permitMatch).toBe(true);
    expect(r.failed).toEqual([]);
  });

  test("Market swap · expiry-near → DENY (s2 + s3 fail)", () => {
    const doc = goldenSwapDoc();
    const r = evalDoc(doc, FIXTURES.marketExpiry.tx);
    expect(r.verdict).toBe("DENY");
    expect(r.failed.map((f) => f.guardId).sort()).toEqual(["s2", "s3"]);
  });

  test("Send-to-contract → DENY (s1 fails)", () => {
    const doc = goldenSwapDoc();
    const r = evalDoc(doc, FIXTURES.sendToContract.tx);
    expect(r.verdict).toBe("DENY");
    expect(r.failed.map((f) => f.guardId)).toEqual(["s1"]);
  });

  test("missing enrichment → absence: treatAsFalse stays DENY-safe", () => {
    const doc = goldenSwapDoc();
    // Drop enrichment entirely — validityDeltaSec / recipientIsContract
    // both vanish. s1's recipientIsContract should resolve `false`
    // (treatAsFalse) → NOT(false && …) = NOT(false) = true → s1 passes.
    // s3's validityDeltaSec → false → NOT(false && …) = true. So only s2
    // (slippageBp < 100) decides verdict.
    const r = evalDoc(doc, {
      meta: { from: "0xabc" },
      context: { recipient: "0xabc", slippageBp: 50, priceImpactBp: 0 },
    });
    expect(r.verdict).toBe("ALLOW");
  });
});

describe("serializeDoc · root op (regression for sticky-AND bug)", () => {
  test("root AND joins guards with &&", () => {
    const doc = goldenSwapDoc();
    const cedar = serializeDoc(doc);
    expect(cedar).toMatch(/&& /);
    expect(cedar).not.toMatch(/\|\| /);
  });

  test("root OR joins guards with ||", () => {
    const doc = goldenSwapDoc();
    const root = doc.nodes[doc.rootId];
    if (root?.type === "logic") root.op = "OR";
    const cedar = serializeDoc(doc);
    expect(cedar).toMatch(/\|\| /);
    expect(cedar).not.toMatch(/^\s*&& /m);
  });

  test("root NOT wraps the conjunction of children", () => {
    const doc = goldenSwapDoc();
    const root = doc.nodes[doc.rootId];
    if (root?.type === "logic") root.op = "NOT";
    const cedar = serializeDoc(doc);
    expect(cedar).toMatch(/!\(.+&&.+\)/);
  });
});

describe("serializeDoc · structural invariants", () => {
  test("emits permit header + Amm::Swap action + 3 guards", () => {
    const doc = goldenSwapDoc();
    const text = serializeDoc(doc);

    expect(text).toMatch(/^@id\("Swap_baseline"\)/m);
    expect(text).toMatch(/permit \(/);
    expect(text).toContain('action == Amm::Action::"Swap"');
    expect(text).toMatch(/when \{/);
    expect(text).toMatch(/\};/);

    // Three guards under `when` — one per s1/s2/s3.
    const guardLines = text
      .split("\n")
      .filter((l) => /^\s*(&& )?[!a-zA-Z(]/.test(l) && /\/\/ /.test(l));
    expect(guardLines.length).toBeGreaterThanOrEqual(3);

    // s1: NOT-tree wrapping AND of recipient/recipientIsContract.
    expect(text).toMatch(/!\(.*context\.recipient != meta\.from/);
    expect(text).toMatch(/enrichment has recipientIsContract && enrichment\.recipientIsContract == true/);

    // s2: slippageBp comparison.
    expect(text).toMatch(/context\.slippageBp < 100/);

    // s3: NOT(validityDeltaSec<30 && priceImpactBp>50).
    expect(text).toMatch(/enrichment has validityDeltaSec && enrichment\.validityDeltaSec < 30/);
    expect(text).toMatch(/context\.priceImpactBp > 50/);

    // Drafts mentioned in trailing comment.
    expect(text).toMatch(/미연결 2개/);
  });

  test("empty doc emits placeholder true", () => {
    const empty = goldenSwapDoc();
    const root = empty.nodes[empty.rootId];
    if (root?.type === "logic") root.childIds = [];
    const text = serializeDoc(empty);
    expect(text).toMatch(/true {2}\/\/ \(no safety conditions\)/);
  });
});
