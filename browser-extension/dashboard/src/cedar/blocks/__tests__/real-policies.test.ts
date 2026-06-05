import { describe, it, expect } from "vitest";
import real from "./fixtures/real-policies-est.json";
import { estToBlocks } from "../estToBlocks";
import { blocksToEst } from "../blocksToEst";

// Every shipped policy from `crates/policy-engine/tests/fixtures/default_policies_v2`
// (parsed to EST by the Rust emitter). If any real-world construct can't be
// structurally mapped, it surfaces as a `raw` node and fails here.
function rawEsts(node: any, acc: any[] = []): any[] {
  if (node && typeof node === "object") {
    if (node.kind === "raw") acc.push(node.est);
    for (const v of Object.values(node)) {
      if (Array.isArray(v)) v.forEach((x) => rawEsts(x, acc));
      else if (v && typeof v === "object") rawEsts(v, acc);
    }
  }
  return acc;
}

describe("real shipped policies (default_policies_v2) round-trip via the engine", () => {
  for (const c of real as { name: string; est: any }[]) {
    it(c.name, () => {
      const ir = estToBlocks(c.est, null);
      const raws = rawEsts(ir);
      expect(raws, `unmapped constructs (raw): ${JSON.stringify(raws)}`).toHaveLength(0);
      expect(blocksToEst(ir)).toEqual(c.est); // byte-exact
    });
  }
});
