import { describe, it, expect } from "vitest";
import type { Expr } from "../../blocks/ir";
import { nodeAtPath, eachChild } from "../path";

const slippageGt100: Expr = {
  kind: "binary", op: ">",
  left: { kind: "attr", of: { kind: "var", name: "context" }, attr: "slippageBp" },
  right: { kind: "lit", litType: "long", value: 100 },
};

describe("node-path scheme", () => {
  it("resolves a child by path step", () => {
    expect(nodeAtPath(slippageGt100, "left")).toEqual(slippageGt100.left);
    expect(nodeAtPath(slippageGt100, "right")).toEqual(slippageGt100.right);
  });
  it("enumerates labelled children", () => {
    const kids = [...eachChild(slippageGt100)].map((c) => c.step);
    expect(kids).toEqual(["left", "right"]);
  });
});
