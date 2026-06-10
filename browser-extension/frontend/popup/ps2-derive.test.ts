import { describe, expect, it } from "vitest";
import { deriveBaseline, derivePopupPackages } from "./ps2-derive";

const lib = {
  defs: {
    "def::a": {
      id: "def::a",
      displayName: "A",
      source: "builtin",
      skeleton: { ir: { annotations: [{ name: "severity", value: "deny" }] } },
    },
    "def::b": { id: "def::b", displayName: "B", source: "mine", skeleton: { ir: {} } },
  },
  packages: {
    "pkg::uncategorized": { id: "pkg::uncategorized", displayName: "미분류" },
    "pkg::builtin.day1-safety": { id: "pkg::builtin.day1-safety", displayName: "기본 안전팩" },
    "pkg::empty": { id: "pkg::empty", displayName: "빈팩" },
  },
};

const wallet = {
  bindings: {
    b1: { id: "b1", defId: "def::a", packageId: "pkg::builtin.day1-safety", enabled: true },
    b2: { id: "b2", defId: "def::b", packageId: "pkg::uncategorized", enabled: false },
  },
  packageEnabled: { "pkg::uncategorized": false },
};

describe("derivePopupPackages", () => {
  it("groups by package (builtin first, empty hidden), carries pkg on/off + member state", () => {
    const pkgs = derivePopupPackages(lib, wallet);
    expect(pkgs.map((p) => p.id)).toEqual(["pkg::builtin.day1-safety", "pkg::uncategorized"]);
    expect(pkgs[0]).toMatchObject({ name: "기본 안전팩", on: true });
    expect(pkgs[0].members).toEqual([
      { bindingId: "b1", defId: "def::a", name: "A", sev: "deny", enabled: true },
    ]);
    expect(pkgs[1]).toMatchObject({ on: false });
    expect(pkgs[1].members[0]).toMatchObject({ sev: "warn", enabled: false }); // severity 기본값
  });

  it("null wallet → no packages", () => {
    expect(derivePopupPackages(lib, null)).toEqual([]);
  });
});

describe("deriveBaseline", () => {
  it("lists builtin defs with severity", () => {
    expect(deriveBaseline(lib)).toEqual([{ id: "def::a", title: "A", sev: "deny" }]);
  });
});
