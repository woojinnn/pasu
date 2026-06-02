import { describe, it, expect } from "vitest";
import corpus from "./fixtures/est-corpus.json";
import { estToBlocks } from "../estToBlocks";

// Every supported expression IR node kind. The corpus must exercise each one
// as a non-`raw` node, and produce zero `raw` fallbacks — this is the
// load-bearing check that forces a real structural mapping (a degenerate
// raw-everything converter passes byte-exact round-trip but fails here).
const EXPECTED_KINDS = [
  "var", "lit", "litEntity", "set", "record", "attr", "has",
  "binary", "unary", "like", "is", "if", "ext",
];

function walk(node: any, fn: (n: any) => void): void {
  if (node && typeof node === "object") {
    fn(node);
    for (const v of Object.values(node)) {
      if (Array.isArray(v)) v.forEach((x) => walk(x, fn));
      else if (v && typeof v === "object") walk(v, fn);
    }
  }
}

describe("coverage (#4)", () => {
  it("every supported node kind appears non-raw across the corpus", () => {
    const seen = new Set<string>();
    for (const c of corpus as any[]) {
      walk(estToBlocks(c.est, null), (n) => {
        if (typeof n.kind === "string") seen.add(n.kind);
      });
    }
    for (const k of EXPECTED_KINDS) expect(seen, `missing node kind: ${k}`).toContain(k);
  });

  it("no raw fallback nodes across the supported-grammar corpus", () => {
    const raws: unknown[] = [];
    for (const c of corpus as any[]) {
      walk(estToBlocks(c.est, null), (n) => {
        if (n.kind === "raw") raws.push(n.est);
      });
    }
    expect(raws, `unexpected raw fallbacks: ${JSON.stringify(raws)}`).toHaveLength(0);
  });
});
