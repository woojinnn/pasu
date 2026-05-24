import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    browser: {
      storage: {
        local: {
          get: vi.fn(async (key: string | string[]) => {
            const keys = Array.isArray(key) ? key : [key];
            const out: Record<string, unknown> = {};
            for (const k of keys) {
              if (localStore.has(k)) out[k] = localStore.get(k);
            }
            return out;
          }),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
          }),
          remove: vi.fn(async (keys: string | string[]) => {
            const arr = Array.isArray(keys) ? keys : [keys];
            for (const k of arr) localStore.delete(k);
          }),
        },
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import { migrateAdapterLoaderStorageKey } from "./adapter-loader-storage-migration";

const OLD_KEY = "marketplace:bundles";
const NEW_KEY = "adapter-loader:bundles";

describe("migrateAdapterLoaderStorageKey", () => {
  beforeEach(() => {
    mocks.localStore.clear();
    vi.clearAllMocks();
  });

  it("returns no_old_data when neither key is set (fresh install)", async () => {
    const result = await migrateAdapterLoaderStorageKey();
    expect(result).toEqual({ migrated: false, reason: "no_old_data" });
    expect(mocks.browser.storage.local.set).not.toHaveBeenCalled();
    expect(mocks.browser.storage.local.remove).not.toHaveBeenCalled();
  });

  it("returns no_old_data when only new key is set (already migrated, cleanup done)", async () => {
    mocks.localStore.set(NEW_KEY, [{ bundle_id: "b1" }]);
    const result = await migrateAdapterLoaderStorageKey();
    expect(result).toEqual({ migrated: false, reason: "no_old_data" });
    // new key untouched
    expect(mocks.localStore.get(NEW_KEY)).toEqual([{ bundle_id: "b1" }]);
    expect(mocks.localStore.has(OLD_KEY)).toBe(false);
  });

  it("copies old → new and deletes old when only old key is set", async () => {
    const bundles = [
      { bundle_id: "b1", version: "1" },
      { bundle_id: "b2", version: "1" },
    ];
    mocks.localStore.set(OLD_KEY, bundles);

    const result = await migrateAdapterLoaderStorageKey();

    expect(result).toEqual({ migrated: true, reason: "copied" });
    expect(mocks.localStore.has(OLD_KEY)).toBe(false);
    expect(mocks.localStore.get(NEW_KEY)).toEqual(bundles);
  });

  it("copies old → new even when old value is empty array []", async () => {
    mocks.localStore.set(OLD_KEY, []);

    const result = await migrateAdapterLoaderStorageKey();

    expect(result).toEqual({ migrated: true, reason: "copied" });
    expect(mocks.localStore.has(OLD_KEY)).toBe(false);
    expect(mocks.localStore.get(NEW_KEY)).toEqual([]);
  });

  it("keeps new key and drops old when both are set (new wins)", async () => {
    const oldBundles = [{ bundle_id: "old", version: "1" }];
    const newBundles = [{ bundle_id: "new", version: "2" }];
    mocks.localStore.set(OLD_KEY, oldBundles);
    mocks.localStore.set(NEW_KEY, newBundles);

    const result = await migrateAdapterLoaderStorageKey();

    expect(result).toEqual({ migrated: false, reason: "new_exists" });
    expect(mocks.localStore.has(OLD_KEY)).toBe(false);
    expect(mocks.localStore.get(NEW_KEY)).toEqual(newBundles);
  });

  it("is idempotent — running twice when only old key existed leaves new key intact", async () => {
    mocks.localStore.set(OLD_KEY, [{ bundle_id: "b1" }]);

    const first = await migrateAdapterLoaderStorageKey();
    expect(first.reason).toBe("copied");

    const second = await migrateAdapterLoaderStorageKey();
    expect(second).toEqual({ migrated: false, reason: "no_old_data" });
    expect(mocks.localStore.get(NEW_KEY)).toEqual([{ bundle_id: "b1" }]);
  });
});
