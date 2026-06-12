import { describe, expect, it } from "vitest";

import type { FormCondition, FormModel } from "../../../cedar/form";
import { holeAssignments } from "../PublishPreviewTree";
import type { PublishHole } from "../publish-redact";

const hole = (key: string, path: string, kind: PublishHole["kind"]): PublishHole => ({
  key,
  ruleId: "p",
  kind,
  path,
  label: path,
  paramName: `?${path.split(".").pop()}`,
  display: "",
  raw: "",
});

const leaf = (fieldPath: string, value: FormCondition["value"]): FormCondition => ({
  joiner: "and",
  fieldPath,
  op: "==",
  value,
});

const model = (when: FormCondition[]): FormModel => ({
  trigger: { kind: "any" },
  when,
  unless: [],
  id: "p",
  severity: "warn",
  reason: "",
});

describe("holeAssignments", () => {
  it("같은 path의 hole 두 개는 순서대로 서로 다른 leaf에 배정된다", () => {
    const l1 = leaf("context.recipient", {
      kind: "string",
      value: "0xA1c4000000000000000000000000000000007e29",
    });
    const l2 = leaf("context.recipient", {
      kind: "string",
      value: "0x91d2000000000000000000000000000000000001",
    });
    const h1 = hole("p#0", "context.recipient", "address");
    const h2 = hole("p#1", "context.recipient", "address");

    const m = holeAssignments(model([l1, l2]), [h1, h2]);

    expect(m.get(l1)).toBe(h1);
    expect(m.get(l2)).toBe(h2);
  });

  it("kind가 안 맞는 leaf는 건너뛴다 (주소 hole ↛ long leaf)", () => {
    const num = leaf("context.slippageBp", { kind: "long", value: 150 });
    const addr = leaf("context.slippageBp", {
      kind: "string",
      value: "0xA1c4000000000000000000000000000000007e29",
    });
    const h = hole("p#0", "context.slippageBp", "address");

    const m = holeAssignments(model([num, addr]), [h]);

    expect(m.get(num)).toBeUndefined();
    expect(m.get(addr)).toBe(h);
  });

  it("그룹 안의 leaf에도 배정된다", () => {
    const inner = leaf("context.amount", { kind: "decimal", value: "3.0" });
    const h = hole("p#0", "context.amount", "number");

    const m = holeAssignments(
      model([{ kind: "group", joiner: "and", conds: [inner] } as never]),
      [h],
    );

    expect(m.get(inner)).toBe(h);
  });
});
