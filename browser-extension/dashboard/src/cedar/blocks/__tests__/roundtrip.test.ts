import { describe, it, expect } from "vitest";
import corpus from "./fixtures/est-corpus.json";
import { estToBlocks } from "../estToBlocks";
import { blocksToEst } from "../blocksToEst";

// Constructs allowed to fall back to `raw`. Empty for the supported grammar —
// coverage.test.ts enforces zero raw nodes across the corpus.
export const RAW_ALLOWLIST: string[] = [];

export function rawNodes(node: any, acc: any[] = []): any[] {
  if (node && typeof node === "object") {
    if (node.kind === "raw") acc.push(node);
    for (const v of Object.values(node)) {
      if (Array.isArray(v)) v.forEach((x) => rawNodes(x, acc));
      else if (v && typeof v === "object") rawNodes(v, acc);
    }
  }
  return acc;
}

describe("EST → IR → EST is byte-exact (invariant #2)", () => {
  for (const c of corpus as any[]) {
    it(`${c.category}/${c.name}`, () => {
      const ir = estToBlocks(c.est, null);
      const est2 = blocksToEst(ir);
      expect(est2).toEqual(c.est); // deep structural equality
    });
  }
});
