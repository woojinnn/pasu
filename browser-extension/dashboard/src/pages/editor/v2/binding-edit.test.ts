import { describe, expect, it } from "vitest";
import { applyBindingEdit, diffBindingEdit } from "./binding-edit";
import { concretizeIr, extractParams } from "../../../cedar/blocks";
import type { Expr, PolicyIR } from "../../../cedar/blocks";
import type { PolicyDef } from "../../../server-api/policy-store";

const lit = (v: number): Expr => ({ kind: "lit", litType: "long", value: v });
const strSet = (vals: string[]): Expr => ({
  kind: "set",
  elements: vals.map((v) => ({ kind: "lit", litType: "string", value: v })),
});
const attr = (name: string): Expr => ({
  kind: "attr",
  of: { kind: "var", name: "context" },
  attr: name,
});
const policy = (body: Expr): PolicyIR => ({
  kind: "policy",
  effect: "forbid",
  annotations: [{ name: "id", value: "p" }],
  scope: {
    principal: { kind: "scopeAll" },
    action: { kind: "scopeAll" },
    resource: { kind: "scopeAll" },
  },
  conditions: [{ kind: "when", body }],
});

const defOf = (ir: PolicyIR): PolicyDef => ({
  id: "def::x",
  displayName: "X",
  skeleton: { ir },
  holes: [],
  defaults: { enabled: true, params: {} },
  source: "mine",
  updatedAtMs: 1,
});

describe("diffBindingEdit", () => {
  const base = () =>
    policy({
      kind: "binary",
      op: "&&",
      left: { kind: "binary", op: ">", left: attr("amt"), right: lit(100) },
      right: {
        kind: "unary",
        op: "!",
        operand: { kind: "binary", op: "contains", left: strSet(["0xaa"]), right: attr("spender") },
      },
    });

  it("동일 IR → unchanged", () => {
    expect(diffBindingEdit(base(), base()).kind).toBe("unchanged");
  });

  it("lit/set 값 변경 → params (각 변경이 기록됨)", () => {
    const edited = policy({
      kind: "binary",
      op: "&&",
      left: { kind: "binary", op: ">", left: attr("amt"), right: lit(42) },
      right: {
        kind: "unary",
        op: "!",
        operand: {
          kind: "binary",
          op: "contains",
          left: strSet(["0xaa", "0xbb"]),
          right: attr("spender"),
        },
      },
    });
    const d = diffBindingEdit(base(), edited);
    expect(d.kind).toBe("params");
    if (d.kind !== "params") return;
    expect(d.updates).toHaveLength(2);
    expect(d.updates[0].labelHint).toBe("amt");
    expect(d.updates[1].labelHint).toBe("spender");
  });

  it("연산자/구조 변경 → structural", () => {
    const edited = policy({
      kind: "binary",
      op: "&&",
      left: { kind: "binary", op: "<", left: attr("amt"), right: lit(100) }, // > → <
      right: base().conditions[0].body,
    });
    expect(diffBindingEdit(base(), policy(edited.conditions[0].body)).kind).toBe("structural");
  });

  it("심각도(annotation) 변경 → structural", () => {
    const edited = base();
    edited.annotations = [{ name: "id", value: "p" }, { name: "severity", value: "warn" }];
    expect(diffBindingEdit(base(), edited).kind).toBe("structural");
  });

  it("def에 이미 홀이 있으면 그 자리는 params 갱신으로(승격 없음)", () => {
    const notContains = (): Expr => ({
      kind: "unary",
      op: "!",
      operand: { kind: "binary", op: "contains", left: strSet(["0xaa"]), right: attr("spender") },
    });
    const both = (rhs: Expr): PolicyIR =>
      policy({
        kind: "binary",
        op: "&&",
        left: { kind: "binary", op: ">", left: attr("amt"), right: rhs },
        right: notContains(),
      });
    // 1차 편집: 100 → 7 (diff와 apply는 같은 def IR 인스턴스를 공유해야 한다)
    const defIr = both(lit(100));
    const d1 = diffBindingEdit(defIr, both(lit(7)));
    expect(d1.kind).toBe("params");
    if (d1.kind !== "params") return;
    const first = applyBindingEdit(defOf(defIr), d1.updates);
    // 2차 편집: 이제 그 자리는 홀 → existingHole 경로(추가 승격 없음)
    const d2 = diffBindingEdit(first.def.skeleton.ir as PolicyIR, both(lit(9)));
    expect(d2.kind).toBe("params");
    if (d2.kind !== "params") return;
    expect(d2.updates[0].existingHole).not.toBeNull();
    const second = applyBindingEdit(first.def, d2.updates);
    expect(second.def).toBe(first.def); // 승격 없음 — def 불변
    expect(Object.values(second.params)).toEqual([9]);
  });
});

describe("applyBindingEdit", () => {
  it("승격: def에 숨은 optional 홀(기본=원래 값) + 바인딩 params", () => {
    const ir = policy({ kind: "binary", op: ">", left: attr("amt"), right: lit(100) });
    const edited = policy({ kind: "binary", op: ">", left: attr("amt"), right: lit(42) });
    const d = diffBindingEdit(ir, edited);
    expect(d.kind).toBe("params");
    if (d.kind !== "params") return;
    const out = applyBindingEdit(defOf(ir), d.updates);

    const specs = extractParams(out.def.skeleton.ir as PolicyIR);
    expect(specs).toHaveLength(1);
    expect(specs[0].optional).toBe(true);
    expect(specs[0].default).toEqual(lit(100)); // 기본값 = 원래 값 (다른 지갑 불변)
    expect(out.def.holes[0].type).toBe("long");
    expect(out.def.defaults.params[specs[0].name]).toBe(100);
    expect(Object.values(out.params)).toEqual([42]); // 이 바인딩만 42

    // concretize: 기본값이면 원래 정책과 동일, params 넣으면 42
    const back = concretizeIr(out.def.skeleton.ir as PolicyIR);
    expect(JSON.stringify(back)).toContain('"value":100');
  });
});
