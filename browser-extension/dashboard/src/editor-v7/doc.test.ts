/**
 * Phase 1 regression checks for the doc builder + schema audit.
 * No serializer or evaluator yet — just structural invariants:
 *
 *   - `initialDoc` makes a hat + empty AND root, nothing else.
 *   - `goldenSwapDoc` reproduces V7_SAMPLE_POLICY's structure
 *     (3 guards, 4 predicates, 2 floating drafts).
 *   - V7_GLOSS audit: every key the doc references is in V7_GLOSS,
 *     and our `isSupportedParam` correctly tags the 8 keys our
 *     server-side `policy-schema.json` covers.
 */

import { describe, expect, test } from "vitest";

import { descendants, goldenSwapDoc, initialDoc, isConnected } from "./doc";
import {
  allGlossKeys,
  getGlossEntry,
  isSupportedParam,
} from "./schema";

describe("initialDoc", () => {
  test("starts with hat + empty AND root", () => {
    const doc = initialDoc({ action: "Amm::Swap" });
    const hat = doc.nodes[doc.hatId];
    expect(hat?.type).toBe("hat");
    if (hat?.type === "hat") {
      expect(hat.effect).toBe("permit");
      expect(hat.action).toBe("Amm::Swap");
      expect(hat.childId).toBe(doc.rootId);
    }
    const root = doc.nodes[doc.rootId];
    expect(root?.type).toBe("logic");
    if (root?.type === "logic") {
      expect(root.op).toBe("AND");
      expect(root.childIds).toEqual([]);
    }
    expect(Object.keys(doc.nodes)).toHaveLength(2);
    expect(doc.drafts).toEqual([]);
  });
});

describe("goldenSwapDoc", () => {
  test("matches V7_SAMPLE_POLICY structure (3 guards + 2 drafts)", () => {
    const doc = goldenSwapDoc();
    const root = doc.nodes[doc.rootId];
    expect(root?.type).toBe("logic");
    if (root?.type !== "logic") return;

    // Root AND has three guards: s1 (NOT), s2 (predicate), s3 (NOT)
    expect(root.childIds).toHaveLength(3);
    const [s1, s2, s3] = root.childIds.map((id) => doc.nodes[id]!);
    expect(s1.type).toBe("logic");
    expect(s2.type).toBe("predicate");
    expect(s3.type).toBe("logic");
    if (s1.type === "logic") {
      expect(s1.op).toBe("NOT");
      expect(s1.guardId).toBe("s1");
    }
    if (s2.type === "predicate") {
      expect(s2.guardId).toBe("s2");
      expect(s2.param).toBe("context.slippageBp");
    }
    if (s3.type === "logic") expect(s3.op).toBe("NOT");

    // s1's NOT wraps an AND with 2 predicates.
    if (s1.type === "logic") {
      const s1and = doc.nodes[s1.childIds[0]]!;
      expect(s1and.type).toBe("logic");
      if (s1and.type === "logic") expect(s1and.childIds).toHaveLength(2);
    }

    // Two floating drafts excluded from compile.
    expect(doc.drafts).toHaveLength(2);
    for (const draftId of doc.drafts) {
      expect(isConnected(doc, draftId)).toBe(false);
    }
  });

  test("every body node has a parentId chain back to the hat", () => {
    const doc = goldenSwapDoc();
    const body = descendants(doc, doc.rootId);
    for (const id of body) {
      const n = doc.nodes[id]!;
      if (id === doc.rootId) {
        expect(n.type === "logic" && n.parentId).toBe(doc.hatId);
      } else if (n.type !== "hat") {
        expect(n.parentId).not.toBeNull();
        expect(doc.nodes[n.parentId!]).toBeDefined();
      }
    }
  });

  test("derived params default to absence: treatAsFalse", () => {
    const doc = goldenSwapDoc();
    const validityDelta = Object.values(doc.nodes).find(
      (n) => n.type === "predicate" && n.param === "enrichment.validityDeltaSec",
    );
    expect(validityDelta?.type).toBe("predicate");
    if (validityDelta?.type === "predicate") {
      expect(validityDelta.absence).toBe("treatAsFalse");
    }
  });
});

describe("V7_GLOSS audit", () => {
  test("all golden-doc params are in V7_GLOSS", () => {
    const doc = goldenSwapDoc();
    for (const n of Object.values(doc.nodes)) {
      if (n.type !== "predicate") continue;
      expect(getGlossEntry(n.param), `param ${n.param} should be in V7_GLOSS`).toBeDefined();
    }
  });

  test("isSupportedParam matches policy-schema.json overlap", () => {
    const supported = allGlossKeys().filter(isSupportedParam);
    expect(supported.sort()).toEqual(
      [
        "context.tokenIn",
        "context.tokenOut",
        "context.recipient",
        "context.slippageBp",
        "context.priceImpactBp",
        "context.amount",
        "meta.from",
        "enrichment.totalInputUsd",
        "enrichment.recipientIsContract",
        "enrichment.effectiveRateVsOracleBps",
      ].sort(),
    );
  });
});
