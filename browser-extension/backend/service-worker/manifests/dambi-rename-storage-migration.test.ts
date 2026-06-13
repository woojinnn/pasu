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

import { migrateDambiRenameStorageKeys } from "./dambi-rename-storage-migration";

const newerLegacyKey = (suffix: string) => `${"pa" + "su"}_${suffix}`;
const olderLegacyKey = (suffix: string) => `${"scope" + "ball"}_${suffix}`;
const activeKey = (suffix: string) => `dambi_${suffix}`;

describe("migrateDambiRenameStorageKeys", () => {
  beforeEach(() => {
    mocks.localStore.clear();
    vi.clearAllMocks();
  });

  it("no-ops when no old keys are present (fresh install)", async () => {
    const result = await migrateDambiRenameStorageKeys();
    expect(result).toEqual({ copied: 0, removed: 0 });
    expect(mocks.browser.storage.local.set).not.toHaveBeenCalled();
    expect(mocks.browser.storage.local.remove).not.toHaveBeenCalled();
  });

  it("copies an old key to its new key and removes the old one", async () => {
    mocks.localStore.set(newerLegacyKey("jwt"), "access-token");

    const result = await migrateDambiRenameStorageKeys();

    expect(result).toEqual({ copied: 1, removed: 1 });
    expect(mocks.localStore.has(newerLegacyKey("jwt"))).toBe(false);
    expect(mocks.localStore.get(activeKey("jwt"))).toBe("access-token");
  });

  it("migrates every renamed key in one pass", async () => {
    mocks.localStore.set(newerLegacyKey("jwt"), "a");
    mocks.localStore.set(newerLegacyKey("jwt_refresh"), "r");
    mocks.localStore.set(newerLegacyKey("server_url"), "https://example.test");
    mocks.localStore.set(newerLegacyKey("diag_timeouts"), [{ at: 1 }]);

    const result = await migrateDambiRenameStorageKeys();

    expect(result).toEqual({ copied: 4, removed: 4 });
    expect(mocks.localStore.get(activeKey("jwt"))).toBe("a");
    expect(mocks.localStore.get(activeKey("jwt_refresh"))).toBe("r");
    expect(mocks.localStore.get(activeKey("server_url"))).toBe("https://example.test");
    expect(mocks.localStore.get(activeKey("diag_timeouts"))).toEqual([{ at: 1 }]);
    // every old key cleaned up
    expect(mocks.localStore.has(newerLegacyKey("jwt"))).toBe(false);
    expect(mocks.localStore.has(newerLegacyKey("jwt_refresh"))).toBe(false);
    expect(mocks.localStore.has(newerLegacyKey("server_url"))).toBe(false);
    expect(mocks.localStore.has(newerLegacyKey("diag_timeouts"))).toBe(false);
  });

  it("rewrites the legacy production server URL to the current dambi host", async () => {
    mocks.localStore.set(newerLegacyKey("server_url"), "https://pasu-policy.duckdns.org");

    const result = await migrateDambiRenameStorageKeys();

    expect(result).toEqual({ copied: 1, removed: 1 });
    expect(mocks.localStore.get(activeKey("server_url"))).toBe(
      "https://dambi-policy.duckdns.org",
    );
  });

  it("repairs an already-migrated legacy production server URL", async () => {
    mocks.localStore.set(activeKey("server_url"), "https://pasu-policy.duckdns.org");

    const result = await migrateDambiRenameStorageKeys();

    expect(result).toEqual({ copied: 0, removed: 0 });
    expect(mocks.localStore.get(activeKey("server_url"))).toBe(
      "https://dambi-policy.duckdns.org",
    );
  });

  it("new key wins when both old and new exist; old is dropped", async () => {
    mocks.localStore.set(newerLegacyKey("jwt"), "old");
    mocks.localStore.set(activeKey("jwt"), "new");

    const result = await migrateDambiRenameStorageKeys();

    // dropped-but-not-copied → removed counts, copied does not
    expect(result).toEqual({ copied: 0, removed: 1 });
    expect(mocks.localStore.has(newerLegacyKey("jwt"))).toBe(false);
    expect(mocks.localStore.get(activeKey("jwt"))).toBe("new");
  });

  it("newer legacy keys win over older legacy keys", async () => {
    mocks.localStore.set(olderLegacyKey("jwt"), "older");
    mocks.localStore.set(newerLegacyKey("jwt"), "newer");

    const result = await migrateDambiRenameStorageKeys();

    expect(result).toEqual({ copied: 1, removed: 2 });
    expect(mocks.localStore.get(activeKey("jwt"))).toBe("newer");
    expect(mocks.localStore.has(olderLegacyKey("jwt"))).toBe(false);
    expect(mocks.localStore.has(newerLegacyKey("jwt"))).toBe(false);
  });

  it("is idempotent — second run is a no-op", async () => {
    mocks.localStore.set(newerLegacyKey("server_url"), "https://example.test");

    const first = await migrateDambiRenameStorageKeys();
    expect(first).toEqual({ copied: 1, removed: 1 });

    const second = await migrateDambiRenameStorageKeys();
    expect(second).toEqual({ copied: 0, removed: 0 });
    expect(mocks.localStore.get(activeKey("server_url"))).toBe("https://example.test");
  });
});
