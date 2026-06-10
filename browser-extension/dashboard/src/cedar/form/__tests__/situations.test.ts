import { describe, expect, it } from "vitest";

import type { FormCondition, FormNode } from "../model";
import { flattenSituations, moveCondTo, situationsOf } from "../situations";

const c = (name: string, joiner: "and" | "or" = "and"): FormCondition => ({
  fieldPath: `context.${name}`,
  op: "==",
  value: { kind: "long", value: 1 },
  joiner,
});

describe("situations", () => {
  it("situationsOf splits at or-joiners; flattenSituations restores joiners", () => {
    const nodes: FormNode[] = [c("a"), c("b"), c("x", "or"), c("y")];
    const runs = situationsOf(nodes);
    expect(runs.map((r) => r.length)).toEqual([2, 2]);
    // 왕복: 구조 보존 (joiner는 정규화: run 머리 or, 나머지 and)
    expect(situationsOf(flattenSituations(runs)).map((r) => r.length)).toEqual([2, 2]);
  });

  it("flattenSituations drops empty runs", () => {
    expect(flattenSituations([[], [c("a")], []])).toHaveLength(1);
  });

  it("moveCondTo moves a row into another situation (joiner → and)", () => {
    const a = c("a");
    const x = c("x", "or");
    const next = moveCondTo([a, x], x, { kind: "situation", index: 0 });
    const runs = situationsOf(next);
    expect(runs).toHaveLength(1);
    expect(runs[0].map((n) => (n as FormCondition).fieldPath)).toEqual([
      "context.a",
      "context.x",
    ]);
  });

  it("moveCondTo moves a row out of a group; an emptied group is dropped", () => {
    const inner = c("g1");
    const group: FormNode = { kind: "group", joiner: "and", conds: [inner] };
    const a = c("a");
    const next = moveCondTo([a, group], inner, { kind: "new-situation" });
    expect(next.some((n) => "kind" in n && n.kind === "group")).toBe(false);
    expect(situationsOf(next)).toHaveLength(2); // [a], [g1]
  });

  it("moveCondTo into a group appends as an or-alternative", () => {
    const g0 = c("g0");
    const group: FormNode = { kind: "group", joiner: "and", conds: [g0] };
    const a = c("a");
    const next = moveCondTo([group, a], a, { kind: "group", group });
    const g = next.find((n) => "kind" in n && n.kind === "group");
    expect(g && "conds" in g ? g.conds.map((x) => x.joiner) : null).toEqual(["and", "or"]);
  });
});
