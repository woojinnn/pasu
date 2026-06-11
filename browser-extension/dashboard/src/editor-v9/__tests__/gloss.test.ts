/**
 * Gloss + field-block round-trip.
 *
 * Locks the bidirectional contract that makes the UX layer non-destructive:
 *   gloss path P ──dottedPathToChain──▶ attr chain C ──chainToDottedPath──▶ P
 *
 * If any gloss entry breaks this, the matching field block can't read its
 * own output back — silently corrupting saved policies. Failing this test
 * is a hard stop before merge.
 */

import { describe, expect, it } from "vitest";
import {
  allGloss,
  blockTypeForPath,
  pathForBlockType,
  getGloss,
  glossByRole,
} from "../gloss";
import {
  chainToDottedPath,
  chainToSegments,
  dottedPathToChain,
} from "../mapping/attr-path";
import { ENRICHMENT_FIELDS } from "../manifest-gen/registry";

const ENRICHMENT_COUNT = Object.keys(ENRICHMENT_FIELDS).length;

describe("gloss table integrity", () => {
  it("has 64 base entries plus the registry enrichment fields", () => {
    expect(allGloss().length).toBe(64 + ENRICHMENT_COUNT);
  });

  it("surfaces every enrichment field as a context.custom.* palette path", () => {
    for (const field of Object.keys(ENRICHMENT_FIELDS)) {
      const path = `context.custom.${field}`;
      const entry = allGloss().find((e) => e.path === path);
      expect(entry, `missing palette entry for ${path}`).toBeDefined();
      expect(entry?.role).toBe("derived");
      // The path the palette inserts must be the one the generator detects.
      const back = chainToDottedPath(dottedPathToChain(path)!);
      expect(back).toBe(path);
    }
  });

  it("every path starts with a Cedar request variable", () => {
    const validRoots = new Set(["principal", "action", "resource", "context"]);
    for (const e of allGloss()) {
      const root = e.path.split(".")[0];
      expect(validRoots.has(root), `bad root in "${e.path}"`).toBe(true);
    }
  });

  it("paths are unique", () => {
    const paths = allGloss().map((e) => e.path);
    expect(new Set(paths).size).toBe(paths.length);
  });

  it("block-type ↔ path is reversible for every entry", () => {
    for (const e of allGloss()) {
      const bt = blockTypeForPath(e.path);
      expect(pathForBlockType(bt)).toBe(e.path);
    }
  });

  it("ko labels are non-empty (this is the UX point)", () => {
    for (const e of allGloss()) {
      expect(e.ko.length, `empty ko on ${e.path}`).toBeGreaterThan(0);
    }
  });

  it("glossByRole partitions every entry exactly once", () => {
    const total = Object.values(glossByRole()).reduce((n, arr) => n + arr.length, 0);
    expect(total).toBe(allGloss().length);
  });
});

describe("attr-chain ↔ dotted-path", () => {
  it("round-trips every gloss path through the chain helpers", () => {
    for (const e of allGloss()) {
      const chain = dottedPathToChain(e.path);
      expect(chain, `dottedPathToChain returned null for "${e.path}"`).not.toBeNull();
      const back = chainToDottedPath(chain!);
      expect(back).toBe(e.path);
    }
  });

  it("chainToSegments preserves order", () => {
    const chain = dottedPathToChain("context.tokenIn.key.address")!;
    expect(chainToSegments(chain)).toEqual(["context", "tokenIn", "key", "address"]);
  });

  it("rejects bare-root paths (no attr chain to build)", () => {
    expect(dottedPathToChain("context")).toBeNull();
    expect(dottedPathToChain("")).toBeNull();
  });

  it("rejects non-var roots", () => {
    expect(dottedPathToChain("meta.from")).toBeNull();
    expect(dottedPathToChain("enrichment.x")).toBeNull();
  });

  it("chainToDottedPath returns null when chain doesn't terminate at a var", () => {
    // attr(attr(lit(...), "x"), "y")  — invalid chain root
    const bad = {
      kind: "attr" as const,
      attr: "y",
      of: {
        kind: "attr" as const,
        attr: "x",
        of: { kind: "lit" as const, litType: "long" as const, value: 0 },
      },
    };
    expect(chainToDottedPath(bad)).toBeNull();
  });
});

describe("preset field block coverage", () => {
  it("blockTypeForPath produces snake_case block ids that survive round-trip", () => {
    for (const e of allGloss()) {
      const bt = blockTypeForPath(e.path);
      expect(bt).toMatch(/^field_[a-zA-Z][a-zA-Z0-9_]*$/);
    }
  });

  it("getGloss returns undefined for unknown paths", () => {
    expect(getGloss("context.nope.notReal")).toBeUndefined();
  });
});
