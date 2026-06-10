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

import { mutate, readStore } from "./store";
import { UNCATEGORIZED_PKG, type PolicyDef } from "./types";

const def = (id: string): PolicyDef => ({
  id,
  displayName: id,
  skeleton: { ir: { kind: "policy" } },
  holes: [],
  defaults: { enabled: true, params: {} },
  source: "mine",
  updatedAtMs: 1,
});

beforeEach(() => {
  mocks.localStore.clear();
  mocks.browser.storage.local.set.mockClear();
});

describe("policy-store core", () => {
  it("readStore on empty storage returns seedable empty docs with 미분류 package", async () => {
    const s = await readStore("u1");
    expect(s.rev).toBe(0);
    expect(s.library.packages[UNCATEGORIZED_PKG]).toBeTruthy();
    expect(Object.keys(s.library.defs)).toEqual([]);
  });

  it("mutate persists atomically and bumps rev", async () => {
    await mutate("u1", (d) => {
      d.library.defs["def::a"] = def("def::a");
    });
    const s = await readStore("u1");
    expect(s.rev).toBe(1);
    expect(s.library.defs["def::a"].id).toBe("def::a");
    const lastSet = mocks.browser.storage.local.set.mock.calls.at(-1)![0] as Record<string, unknown>;
    expect(Object.keys(lastSet).sort()).toEqual(["ps2:u1:library", "ps2:u1:rev", "ps2:u1:wallets"]);
  });

  it("mutate is serialized — 50 concurrent writes lose nothing", async () => {
    await Promise.all(
      Array.from({ length: 50 }, (_, i) =>
        mutate("u1", (d) => {
          d.library.defs[`def::p${i}`] = def(`def::p${i}`);
        }),
      ),
    );
    const s = await readStore("u1");
    expect(Object.keys(s.library.defs)).toHaveLength(50);
    expect(s.rev).toBe(50);
  });

  it("invariant violation rejects and writes nothing", async () => {
    await expect(
      mutate("u1", (d) => {
        d.wallets.byAddress["0xabc"] = {
          bindings: {
            "bind::x": { id: "bind::x", defId: "def::ghost", packageId: UNCATEGORIZED_PKG, enabled: true, updatedAtMs: 1 },
          },
          packageEnabled: {},
        };
      }),
    ).rejects.toThrow(/defId/);
    expect((await readStore("u1")).rev).toBe(0);
  });

  it("a failed mutate does not break the queue for later mutations", async () => {
    await mutate("u1", (d) => {
      d.library.defs["def::a"] = def("def::a");
    });
    await expect(
      mutate("u1", () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");
    await mutate("u1", (d) => {
      d.library.defs["def::b"] = def("def::b");
    });
    const s = await readStore("u1");
    expect(Object.keys(s.library.defs).sort()).toEqual(["def::a", "def::b"]);
    expect(s.rev).toBe(2);
  });

  it("accounts are isolated by key prefix", async () => {
    await mutate("u1", (d) => {
      d.library.defs["def::a"] = def("def::a");
    });
    expect(Object.keys((await readStore("u2")).library.defs)).toEqual([]);
  });
});
