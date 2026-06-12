import { describe, expect, it, vi } from "vitest";
import { holeInputToValue, listingToDefs, requiredHolesOf } from "./market-install-convert";
import type { PolicyIR } from "../cedar/blocks";
import { formToIr } from "../cedar/form";
import type { FormModel } from "../cedar/form";
import { MANIFEST_HOLES_KEY } from "./editor/publish-holes";
import { ZERO_ADDR } from "./editor/publish-redact";

const ir = { kind: "policy" } as unknown as PolicyIR;
const textToBlocks = vi.fn(async (t: string) => (t.includes("bad") ? [] : [ir]));

const policyVersion = { cedar_text: "permit(...);", manifest: { id: "m" }, members: [] };
const setVersion = {
  cedar_text: "",
  manifest: undefined,
  members: [
    { slug: "a", cedar_text: "permit(a);", manifest: { id: "a" } },
    { slug: "b", cedar_text: "permit(b);", manifest: { id: "b" } },
  ],
};

describe("listingToDefs", () => {
  it("policy listing → 1 def with market id/source/listing refs", async () => {
    const defs = await listingToDefs(
      { id: "L1", kind: "policy", displayName: "한도", version: "1.2.0", cat: "스왑" },
      policyVersion as never,
      textToBlocks,
    );
    expect(defs).toHaveLength(1);
    expect(defs[0].id).toBe("def::market.L1");
    expect(defs[0]).toMatchObject({
      source: "market",
      sourceListingId: "L1",
      sourceVersion: "1.2.0",
      displayName: "한도",
      cat: "스왑",
      holes: [],
    });
    expect(defs[0].skeleton).toEqual({ ir, manifest: { id: "m" } });
  });

  it("set listing → member defs with per-member ids", async () => {
    const defs = await listingToDefs(
      { id: "L2", kind: "set", displayName: "팩", version: "1.0.0", cat: undefined },
      setVersion as never,
      textToBlocks,
    );
    expect(defs.map((d) => d.id)).toEqual(["def::market.L2.a", "def::market.L2.b"]);
    expect(defs.every((d) => d.sourceListingId === "L2")).toBe(true);
  });

  it("unconvertible cedar aborts the whole install with the member name", async () => {
    await expect(
      listingToDefs(
        { id: "L3", kind: "set", displayName: "팩", version: "1", cat: undefined },
        { cedar_text: "", members: [{ slug: "x", cedar_text: "bad", manifest: {} }] } as never,
        textToBlocks,
      ),
    ).rejects.toThrow(/x/);
  });

  it("form-compatible listing → holed skeleton + required holes from x_dambi_holes, placeholder excluded from defaults", async () => {
    // redact 직후 모양의 리스팅: 주소는 제로주소 플레이스홀더, 숫자는 추천값 유지.
    const model: FormModel = {
      trigger: { kind: "actionEq", entityType: "Dambi::Action", id: "swap" },
      when: [
        {
          joiner: "and",
          fieldPath: "context.recipient",
          op: "==",
          value: { kind: "string", value: ZERO_ADDR },
        },
        {
          joiner: "and",
          fieldPath: "context.slippageBp",
          op: ">=",
          value: { kind: "long", value: 150 },
        },
      ],
      unless: [],
      id: "p",
      severity: "warn",
      reason: "",
    };
    const formIr = formToIr(model);
    const defs = await listingToDefs(
      { id: "L4", kind: "policy", displayName: "수신자 제한", version: "1.0.0", cat: undefined },
      {
        cedar_text: "forbid(...)",
        manifest: {
          id: "p",
          schema_version: 2,
          [MANIFEST_HOLES_KEY]: [
            { name: "v1", type: "address", label: "받는 주소", required: true },
          ],
        },
      } as never,
      async () => [formIr],
    );
    const def = defs[0];
    // 1) hole 스펙이 def.holes로 복원되고 required가 표시된다.
    expect(requiredHolesOf(def).map((h) => ({ name: h.name, type: h.type, label: h.label }))).toEqual([
      { name: "v1", type: "address", label: "받는 주소" },
    ]);
    // 2) required 칸의 플레이스홀더는 defaults.params에 없다(미충전 표현);
    //    추천값으로 남긴 숫자는 기본값으로 들어간다.
    expect(def.defaults.params.v1).toBeUndefined();
    expect(def.defaults.params.v2).toBe(150);
    // 3) skeleton은 holed IR(파라미터 홀 포함)이고, manifest에서 운송용
    //    키는 제거된다.
    expect(JSON.stringify(def.skeleton.ir)).toContain('"hole"');
    expect(def.skeleton.manifest).toEqual({ id: "p", schema_version: 2 });
  });
});

describe("holeInputToValue", () => {
  it("address: 40자리 hex만, 소문자 정규화", () => {
    expect(holeInputToValue("address", " 0xA1c4000000000000000000000000000000007E29 ")).toBe(
      "0xa1c4000000000000000000000000000000007e29",
    );
    expect(holeInputToValue("address", "0x123")).toBeNull();
    expect(holeInputToValue("address", "")).toBeNull();
  });

  it("addressSet: 쉼표/줄바꿈 구분, 하나라도 틀리면 null", () => {
    expect(
      holeInputToValue(
        "addressSet",
        "0xA1c4000000000000000000000000000000007e29,\n0x91d2000000000000000000000000000000000001",
      ),
    ).toEqual([
      "0xa1c4000000000000000000000000000000007e29",
      "0x91d2000000000000000000000000000000000001",
    ]);
    expect(holeInputToValue("addressSet", "0xA1c4..., nope")).toBeNull();
  });

  it("long/decimal 검증", () => {
    expect(holeInputToValue("long", "150")).toBe(150);
    expect(holeInputToValue("long", "1.5")).toBeNull();
    expect(holeInputToValue("decimal", "3")).toBe("3.0"); // Cedar 형식으로 정규화
    expect(holeInputToValue("decimal", "abc")).toBeNull();
  });
});
