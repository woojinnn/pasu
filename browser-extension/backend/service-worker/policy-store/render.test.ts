import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../wasm-bridge", () => ({
  estToPolicyText: vi.fn(async (estJson: string) =>
    JSON.stringify({ ok: true, text: `CEDAR(${(JSON.parse(estJson) as { effect: string }).effect})` }),
  ),
}));

import { makeHole } from "../../../sdk/block-ir/params";
import type { Expr, PolicyIR } from "../../../sdk/block-ir/ir";
import { estToPolicyText } from "../wasm-bridge";
import { renderDef, substituteHoles, clearRenderCache } from "./render";
import type { PolicyDef } from "./types";

const lit = (value: number): Expr => ({ kind: "lit", litType: "long", value });
const policyWith = (body: Expr): PolicyIR => ({
  kind: "policy",
  effect: "forbid",
  annotations: [],
  scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
  conditions: [{ kind: "when", body }],
});

const defWithHole = (): PolicyDef => ({
  id: "def::cap",
  displayName: "한도",
  skeleton: {
    ir: policyWith({
      kind: "binary",
      op: ">",
      left: { kind: "attr", of: { kind: "var", name: "context" }, attr: "amt" },
      right: makeHole(lit(10000), { name: "limit" }),
    }),
    manifest: { trigger: { where: {} }, policy_rpc: [{ params: { cap: { $hole: "limit" }, fixed: 1 } }] },
  },
  holes: [{ name: "limit", type: "long", label: "한도(USD)" }],
  defaults: { enabled: true, params: { limit: 10000 } },
  source: "mine",
  updatedAtMs: 1,
});

beforeEach(() => {
  clearRenderCache();
  vi.mocked(estToPolicyText).mockClear();
});

describe("render pipeline", () => {
  it("renders skeleton via fillParams→blocksToEst→wasm and caches by (defId, updatedAtMs, params)", async () => {
    const d = defWithHole();
    const r1 = await renderDef(d, { limit: 5 });
    const r2 = await renderDef(d, { limit: 5 });
    expect(r1.text).toBe("CEDAR(forbid)");
    expect(r2).toBe(r1);
    expect(estToPolicyText).toHaveBeenCalledTimes(1); // 캐시 히트
    await renderDef(d, { limit: 6 });
    expect(estToPolicyText).toHaveBeenCalledTimes(2); // params 다르면 미스
    await renderDef({ ...d, updatedAtMs: 2 }, { limit: 6 });
    expect(estToPolicyText).toHaveBeenCalledTimes(3); // 뼈대 수정도 미스
  });

  it("substitutes manifest $hole references with param values", async () => {
    const r = await renderDef(defWithHole(), { limit: 5 });
    expect(r.manifest).toEqual({
      trigger: { where: {} },
      policy_rpc: [{ params: { cap: 5, fixed: 1 } }],
    });
  });

  it("throws a readable error when a required hole is missing", async () => {
    await expect(renderDef(defWithHole(), {})).rejects.toThrow(/limit/);
  });

  it("substituteHoles deep-replaces {$hole:name} only when it is the sole key", () => {
    const m = { a: [{ $hole: "x" }], b: { $hole: "x", extra: 1 } };
    expect(substituteHoles(m, { x: 7 })).toEqual({ a: [7], b: { $hole: "x", extra: 1 } });
  });
});
