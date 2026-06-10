import { describe, expect, it } from "vitest";
import { buildDefPayload, type SaveScope } from "./save-def";
import type { PolicyDef } from "../../../server-api/policy-store";

const ir = { kind: "policy" } as never;

describe("buildDefPayload", () => {
  it("new def: id is generated, scope/package recorded into defaults", () => {
    const { def, bindPlan } = buildDefPayload({
      existing: null,
      displayName: "내 정책",
      cat: "스왑",
      ir,
      manifest: { id: "m" },
      scope: { kind: "wallets", addresses: ["0xA1"] } satisfies SaveScope,
      packageId: "pkg::x",
      applyToNewWallets: true,
    });
    expect(def.id).toMatch(/^def::/);
    expect(def.source).toBe("mine");
    expect(def.holes).toEqual([]);
    expect(def.defaults).toEqual({ enabled: true, params: {}, packageId: "pkg::x" });
    expect(bindPlan).toEqual({ defId: def.id, packageId: "pkg::x", addresses: ["0xA1"] });
  });

  it("library-only scope yields no bindPlan; applyToNewWallets=false → defaults.enabled false", () => {
    const { def, bindPlan } = buildDefPayload({
      existing: null,
      displayName: "x",
      cat: undefined,
      ir,
      manifest: undefined,
      scope: { kind: "library-only" },
      packageId: "pkg::x",
      applyToNewWallets: false,
    });
    expect(bindPlan).toBeNull();
    expect(def.defaults.enabled).toBe(false);
  });

  it("existing def: id/defaults/source preserved, skeleton+name refreshed, no bindPlan", () => {
    const existing = {
      id: "def::keep",
      displayName: "old",
      skeleton: { ir: { old: true } },
      holes: [],
      defaults: { enabled: true, params: { x: 1 }, packageId: "pkg::y" },
      source: "market",
      updatedAtMs: 1,
    } as unknown as PolicyDef;
    const { def, bindPlan } = buildDefPayload({
      existing,
      displayName: "new",
      cat: "c",
      ir,
      manifest: { id: "m2" },
      scope: null,
      packageId: null,
      applyToNewWallets: null,
    });
    expect(def.id).toBe("def::keep");
    expect(def.source).toBe("market");
    expect(def.defaults).toEqual({ enabled: true, params: { x: 1 }, packageId: "pkg::y" });
    expect(def.skeleton).toEqual({ ir, manifest: { id: "m2" } });
    expect(def.displayName).toBe("new");
    expect(bindPlan).toBeNull();
  });
});
