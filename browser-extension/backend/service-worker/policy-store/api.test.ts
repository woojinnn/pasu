import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    browser: {
      storage: {
        local: {
          get: vi.fn(async (key?: string | string[] | null) => {
            if (key == null) return Object.fromEntries(localStore);
            const keys = Array.isArray(key) ? key : [key];
            return Object.fromEntries(keys.filter((k) => localStore.has(k)).map((k) => [k, localStore.get(k)]));
          }),
          set: vi.fn(async (obj: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(obj)) localStore.set(k, v);
          }),
          remove: vi.fn(async (key: string | string[]) => {
            for (const k of Array.isArray(key) ? key : [key]) localStore.delete(k);
          }),
        },
      },
    },
  };
});
vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));
vi.mock("./seed", () => ({ ensureSeeded: vi.fn(async () => undefined) }));
vi.mock("../dashboard/current-user", () => ({ getCurrentUserId: vi.fn(async () => "u") }));

import { handlePs2Request, isPs2Request, provisionFromWalletSync } from "./api";
import { provisionWallets, putDef } from "./ops";
import { readStore } from "./store";
import { UNCATEGORIZED_PKG, type Binding, type PolicyDef } from "./types";

const def = (id: string, holes: string[] = []): PolicyDef => ({
  id,
  displayName: id,
  skeleton: { ir: { kind: "policy" } },
  holes: holes.map((name) => ({ name, type: "long", label: name })),
  defaults: { enabled: true, params: {} },
  source: "market",
  updatedAtMs: 1,
});

beforeEach(() => mocks.localStore.clear());

describe("ps2 message API", () => {
  it("isPs2Request gates on the ps2: prefix", () => {
    expect(isPs2Request({ type: "ps2:get-library" })).toBe(true);
    expect(isPs2Request({ type: "pasu-list-wallets" })).toBe(false);
    expect(isPs2Request({})).toBe(false);
  });

  it("get-wallet-state returns an empty state for unknown wallets", async () => {
    const out = await handlePs2Request({ type: "ps2:get-wallet-state", address: "0xNOPE" });
    expect(out).toEqual({ bindings: {}, packages: {}, packageEnabled: {} });
  });

  it("put-def → bind → get-overview round-trip", async () => {
    await handlePs2Request({ type: "ps2:put-def", def: def("def::a") });
    await handlePs2Request({
      type: "ps2:bind",
      defId: "def::a",
      packageId: UNCATEGORIZED_PKG,
      addresses: ["0xA1"],
    });
    const overview = (await handlePs2Request({ type: "ps2:get-overview" })) as Awaited<ReturnType<typeof readStore>>;
    expect(Object.keys(overview.wallets.byAddress)).toEqual(["0xa1"]);
    expect(overview.rev).toBe(2);
  });

  it("install-market scope:all binds to every known wallet", async () => {
    await putDef("u", def("def::seedlike"));
    await provisionWallets("u", ["0xa1", "0xa2"]);
    await handlePs2Request({
      type: "ps2:install-market",
      defs: [def("def::m", ["cap"])],
      pkg: { id: "pkg::m", displayName: "마켓팩", source: "market", updatedAtMs: 1 },
      scope: { kind: "all" },
      params: { "def::m": { cap: 7 } },
    });
    const s = await readStore("u");
    for (const addr of ["0xa1", "0xa2"]) {
      const b = Object.values(s.wallets.byAddress[addr].bindings).find((x) => x.defId === "def::m");
      expect(b, addr).toBeTruthy();
      expect(b!.packageId).toBe("pkg::m");
      expect(b!.params).toEqual({ cap: 7 });
    }
  });

  it("install-market scope:library-only registers defs without bindings", async () => {
    await handlePs2Request({
      type: "ps2:install-market",
      defs: [def("def::m")],
      scope: { kind: "library-only" },
    });
    const s = await readStore("u");
    expect(s.library.defs["def::m"].source).toBe("market");
    expect(Object.keys(s.wallets.byAddress)).toEqual([]);
  });

  it("market update keeps binding params, drops vanished holes", async () => {
    await handlePs2Request({
      type: "ps2:install-market",
      defs: [def("def::m", ["a", "b"])],
      scope: { kind: "wallets", addresses: ["0xa1"] },
      params: { "def::m": { a: 1, b: 2 } },
    });
    await handlePs2Request({
      type: "ps2:install-market",
      defs: [def("def::m", ["a"])], // v2: hole b 삭제
      scope: { kind: "library-only" },
    });
    const s = await readStore("u");
    const b = Object.values(s.wallets.byAddress["0xa1"].bindings).find((x) => x.defId === "def::m") as Binding;
    expect(b.params).toEqual({ a: 1 });
    expect(s.library.defs["def::m"].holes.map((h) => h.name)).toEqual(["a"]);
  });

  it("provisionFromWalletSync is a no-op when signed out", async () => {
    const { getCurrentUserId } = await import("../dashboard/current-user");
    vi.mocked(getCurrentUserId).mockResolvedValueOnce(null);
    await provisionFromWalletSync(["0xa1"]);
    expect(mocks.localStore.size).toBe(0);
  });

  it("provisionFromWalletSync provisions for the signed-in account", async () => {
    await putDef("u", def("def::a"));
    await provisionFromWalletSync(["0xA1"]);
    const s = await readStore("u");
    expect(Object.values(s.wallets.byAddress["0xa1"].bindings).map((b) => b.defId)).toEqual(["def::a"]);
  });
});
