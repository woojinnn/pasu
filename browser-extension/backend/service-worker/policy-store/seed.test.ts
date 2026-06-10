import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    browser: {
      runtime: { getURL: vi.fn((p: string) => `chrome-extension://x/${p}`) },
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

const EST = {
  effect: "forbid",
  principal: { op: "All" },
  action: { op: "All" },
  resource: { op: "All" },
  conditions: [],
};
vi.mock("../wasm-bridge", () => ({
  policyTextToEst: vi.fn(async () => JSON.stringify({ ok: true, policies: [{ id: "p", est: EST }] })),
}));

import { cleanupLegacyKeys, clearSeedCache, ensureSeeded, BUILTIN_PKG } from "./seed";
import { readStore } from "./store";

const BAKED = [
  { id: "no-unlimited-approval", policy: "forbid(...);", manifest: { id: "no-unlimited-approval", schema_version: 2 } },
  { id: "swap-cap", policy: "forbid(...);", manifest: { id: "swap-cap", schema_version: 2 } },
];

beforeEach(() => {
  mocks.localStore.clear();
  clearSeedCache();
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({ ok: true, text: async () => JSON.stringify(BAKED) })),
  );
});
afterEach(() => vi.unstubAllGlobals());

describe("builtin seeding", () => {
  it("ensureSeeded creates builtin defs + 기본 안전팩 package once", async () => {
    await ensureSeeded("u");
    await ensureSeeded("u"); // 멱등 (캐시 경유)
    const s = await readStore("u");
    const builtin = Object.values(s.library.defs).filter((d) => d.source === "builtin");
    expect(builtin).toHaveLength(2);
    expect(s.library.packages[BUILTIN_PKG]).toBeTruthy();
    expect(builtin.every((d) => d.defaults.enabled && d.defaults.packageId === BUILTIN_PKG)).toBe(true);
    expect(s.rev).toBe(1); // 두 번째 호출은 무변경
  });

  it("seeded skeletons carry the baked manifest and a BlockIR ir", async () => {
    await ensureSeeded("u");
    const s = await readStore("u");
    const d = s.library.defs["def::builtin.swap-cap"];
    expect(d).toBeTruthy();
    expect(d.skeleton.manifest).toEqual({ id: "swap-cap", schema_version: 2 });
    expect((d.skeleton.ir as { kind: string }).kind).toBe("policy");
  });

  it("already-seeded library (builtin def present) skips re-fetch", async () => {
    await ensureSeeded("u");
    const fetchMock = vi.mocked(globalThis.fetch);
    fetchMock.mockClear();
    await ensureSeeded("u");
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("cleanupLegacyKeys", () => {
  it("removes old-namespace keys only", async () => {
    mocks.localStore.set("dashboard:policies:u", []);
    mocks.localStore.set("dashboard:sets:u", []);
    mocks.localStore.set("policy-selection:enabled-ids:u", []);
    mocks.localStore.set("migration:done", true);
    mocks.localStore.set("rpc:manifests", {});
    mocks.localStore.set("ps2:u:rev", 3);
    await cleanupLegacyKeys();
    expect([...mocks.localStore.keys()].sort()).toEqual(["ps2:u:rev", "rpc:manifests"]);
  });
});
