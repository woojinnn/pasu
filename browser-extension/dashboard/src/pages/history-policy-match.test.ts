import { describe, expect, it } from "vitest";
import { matchDefForVerdict } from "./history-policy-match";
import type { PolicyDef } from "../../../sdk/policy-store-types";

const def = (id: string, annId?: string): PolicyDef => ({
  id,
  displayName: id,
  holes: [],
  source: "mine",
  updatedAtMs: 1,
  defaults: { enabled: true, params: {} },
  skeleton: { ir: { kind: "policy", annotations: annId ? [{ name: "id", value: annId }] : [] } },
});

describe("matchDefForVerdict", () => {
  it("matches by IR @id annotation first, then by def id", () => {
    const defs = {
      "def::1": def("def::1", "unlimited-approval-deny"),
      "def::2": def("def::2"),
    };
    expect(matchDefForVerdict(defs, "unlimited-approval-deny")?.id).toBe("def::1");
    expect(matchDefForVerdict(defs, "def::2")?.id).toBe("def::2");
    expect(matchDefForVerdict(defs, "nope")).toBeNull();
  });

  it("tolerates IR without annotations array", () => {
    const defs = { "def::x": { ...def("def::x"), skeleton: { ir: {} } } };
    expect(matchDefForVerdict(defs, "def::x")?.id).toBe("def::x");
  });
});
