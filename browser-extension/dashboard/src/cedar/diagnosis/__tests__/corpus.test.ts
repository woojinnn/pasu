import { describe, it, expect } from "vitest";
import estCorpus from "../../blocks/__tests__/fixtures/real-policies-est.json";
import { estToBlocks } from "../../blocks/estToBlocks";
import { buildProbes } from "../index";

// real-policies-est.json is an array of `{ name, est }` entries (118 of them).
describe("corpus: every shipped policy is probe-able", () => {
  it("builds probes for all policies without throwing", () => {
    const policies = (estCorpus as { est: unknown }[]) ?? [];
    expect(policies.length).toBeGreaterThan(0);
    for (const entry of policies) {
      const ir = estToBlocks(entry.est as any, null);
      expect(() => buildProbes(ir)).not.toThrow();
    }
  });
});
