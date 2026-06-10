import { describe, expect, it } from "vitest";
import { deriveMatrix, defUsageCount } from "./apply-matrix-derive";
import type { StoreSnapshot, WalletPolicyState } from "../../../server-api/policy-store";

const snap = (byAddress: Record<string, WalletPolicyState> = {}): StoreSnapshot => ({
  library: {
    schemaVersion: 1,
    defs: {
      "def::a": {
        id: "def::a",
        displayName: "A",
        skeleton: { ir: {} },
        holes: [],
        defaults: { enabled: true, params: {} },
        source: "builtin",
        updatedAtMs: 1,
      },
    },
    packages: {
      "pkg::uncategorized": { id: "pkg::uncategorized", displayName: "미분류", source: "builtin", updatedAtMs: 0 },
      "pkg::x": { id: "pkg::x", displayName: "X", source: "mine", updatedAtMs: 1 },
    },
  },
  wallets: { schemaVersion: 1, byAddress },
  rev: 1,
});

const W = (
  bindings: Record<string, { defId: string; packageId: string; enabled: boolean }>,
  pkgEnabled: Record<string, boolean> = {},
): WalletPolicyState => ({
  bindings: Object.fromEntries(Object.entries(bindings).map(([id, b]) => [id, { id, updatedAtMs: 1, ...b }])),
  packageEnabled: pkgEnabled,
});

describe("deriveMatrix", () => {
  it("rows = ps2 wallets ∪ server wallets (lowercased, labeled), cols = all packages", () => {
    const m = deriveMatrix(snap({ "0xa1": W({ b1: { defId: "def::a", packageId: "pkg::x", enabled: true } }) }), [
      { address: "0xB2", label: "콜드" },
    ]);
    expect(m.rows.map((r) => r.address)).toEqual(["0xa1", "0xb2"]);
    expect(m.rows[1].label).toBe("콜드");
    expect(m.cols.map((c) => c.id)).toEqual(["pkg::uncategorized", "pkg::x"]);
  });

  it("cell carries active/total counts and package-on state", () => {
    const m = deriveMatrix(
      snap({
        "0xa1": W(
          {
            b1: { defId: "def::a", packageId: "pkg::x", enabled: true },
            b2: { defId: "def::a", packageId: "pkg::x", enabled: false },
          },
          { "pkg::x": false },
        ),
      }),
      [],
    );
    const cell = m.cellOf("0xa1", "pkg::x");
    expect(cell).toEqual({ total: 2, activeBindings: 1, packageOn: false, bindingIds: ["b1", "b2"] });
    expect(m.cellOf("0xa1", "pkg::uncategorized")).toEqual({
      total: 0,
      activeBindings: 0,
      packageOn: true,
      bindingIds: [],
    });
  });
});

describe("defUsageCount", () => {
  it("counts distinct wallets a def is bound to", () => {
    const s = snap({
      "0xa1": W({ b1: { defId: "def::a", packageId: "pkg::x", enabled: true } }),
      "0xa2": W({ b2: { defId: "def::a", packageId: "pkg::uncategorized", enabled: false } }),
    });
    expect(defUsageCount(s, "def::a")).toBe(2);
    expect(defUsageCount(s, "def::nope")).toBe(0);
  });
});
