import { describe, expect, it } from "vitest";
import { deriveWalletRows, defUsageCount, packageDisplayOn } from "./wallet-policies-derive";
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
  packages: {},
  bindings: Object.fromEntries(Object.entries(bindings).map(([id, b]) => [id, { id, updatedAtMs: 1, ...b }])),
  packageEnabled: pkgEnabled,
});

describe("deriveWalletRows", () => {
  it("rows = ps2 wallets ∪ server wallets (lowercased, labeled)", () => {
    const rows = deriveWalletRows(
      snap({ "0xa1": W({ b1: { defId: "def::a", packageId: "pkg::x", enabled: true } }) }),
      [{ address: "0xB2", label: "콜드" }],
    );
    expect(rows.map((r) => r.address)).toEqual(["0xa1", "0xb2"]);
    expect(rows[1].label).toBe("콜드");
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

describe("packageDisplayOn (하이브리드 토글)", () => {
  it("gate on이라도 활성 멤버 0이면 off로 보인다", () => {
    expect(packageDisplayOn(true, 0)).toBe(false);
    expect(packageDisplayOn(true, 2)).toBe(true);
    expect(packageDisplayOn(false, 2)).toBe(false);
  });
});
