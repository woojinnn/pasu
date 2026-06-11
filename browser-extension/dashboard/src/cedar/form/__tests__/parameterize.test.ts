import { describe, expect, it } from "vitest";
import {
  collectLeaves,
  diffParamValues,
  parameterizeModel,
  structureKey,
} from "../parameterize";
import { formToIr, irToForm } from "../convert";
import { concretizeIr, extractParams } from "../../blocks";
import type { FormCondition, FormModel } from "../model";

const cond = (
  fieldPath: string,
  op: FormCondition["op"],
  value: FormCondition["value"],
  joiner: FormCondition["joiner"] = "and",
): FormCondition => ({ fieldPath, op, value, joiner });

const model = (overrides: Partial<FormModel> = {}): FormModel => ({
  trigger: { kind: "actionEq", entityType: "Token::Action", id: "Send" },
  when: [
    cond("context.amount", ">", { kind: "long", value: 100 }),
    cond("context.recipient", "in", { kind: "set", values: ["0xaa", "0xbb"] }),
    cond("context.recipient", "!=", { kind: "field", path: "principal.address" }),
  ],
  unless: [],
  id: "p1",
  severity: "deny",
  reason: "사유",
  ...overrides,
});

describe("parameterizeModel", () => {
  it("모든 leaf(필드 참조 포함)가 위치 기반 이름 v1..vN의 파라미터가 된다", () => {
    const p = parameterizeModel(model());
    const leaves = collectLeaves(p);
    expect(leaves.map((l) => l.param?.name)).toEqual(["v1", "v2", "v3"]);

    const ir = formToIr(p);
    const specs = extractParams(ir);
    expect(specs.map((s) => s.name)).toEqual(["v1", "v2", "v3"]);
    expect(specs.every((s) => s.optional)).toBe(true);
    // 필드 참조 홀: 기본값 = attr 체인
    expect(specs[2].default.kind).toBe("attr");
  });

  it("formToIr∘parameterize∘irToForm은 멱등 — 재저장이 def를 바꾸지 않는다", () => {
    const ir1 = formToIr(parameterizeModel(model()));
    const reopened = irToForm(ir1);
    expect(reopened).not.toBeNull();
    const ir2 = formToIr(parameterizeModel(reopened!));
    expect(JSON.stringify(ir2)).toBe(JSON.stringify(ir1));
  });

  it("concretize는 기본값을 복원한다 (필드 참조 포함)", () => {
    const ir = formToIr(parameterizeModel(model()));
    const back = irToForm(concretizeIr(ir));
    expect(back).not.toBeNull();
    const leaves = collectLeaves(back!);
    expect(leaves[0].value).toEqual({ kind: "long", value: 100 });
    expect(leaves[2].value).toEqual({ kind: "field", path: "principal.address" });
  });
});

describe("structureKey", () => {
  it("값·심각도가 달라도 구조는 같다", () => {
    const a = model();
    const b = model({ severity: "warn" });
    b.when = [
      cond("context.amount", ">", { kind: "long", value: 9999 }),
      cond("context.recipient", "in", { kind: "set", values: ["0xcc"] }),
      cond("context.recipient", "!=", { kind: "field", path: "context.custom.inputToken" }),
    ];
    expect(structureKey(a)).toBe(structureKey(b));
  });

  it("필드/연산자/행 추가는 구조 변경", () => {
    const b = model();
    b.when = [...b.when, cond("context.x", "==", { kind: "bool", value: true })];
    expect(structureKey(model())).not.toBe(structureKey(b));
    const c = model();
    c.when = [cond("context.amount", "<", { kind: "long", value: 100 }), ...c.when.slice(1)];
    expect(structureKey(model())).not.toBe(structureKey(c));
  });
});

describe("diffParamValues", () => {
  it("달라진 값만 — 숫자·set 원소 추가·비교 필드 전환 전부 값 변경", () => {
    const edited = model();
    edited.when = [
      cond("context.amount", ">", { kind: "long", value: 42 }),
      cond("context.recipient", "in", { kind: "set", values: ["0xaa", "0xbb", "0xdd"] }),
      cond("context.recipient", "!=", { kind: "field", path: "context.custom.inputToken" }),
    ];
    const params = diffParamValues(model(), edited);
    expect(params).toEqual({
      v1: 42,
      v2: ["0xaa", "0xbb", "0xdd"],
      v3: { field: "context.custom.inputToken" },
    });
  });

  it("기본값과 같으면 비어 있다 (기본값 상속)", () => {
    expect(diffParamValues(model(), model())).toEqual({});
  });
});
