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

import { migratePasuRenameStorageKeys } from "./pasu-rename-storage-migration";

describe("migratePasuRenameStorageKeys", () => {
  beforeEach(() => {
    mocks.localStore.clear();
    vi.clearAllMocks();
  });

  it("no-ops when no old keys are present (fresh install)", async () => {
    const result = await migratePasuRenameStorageKeys();
    expect(result).toEqual({ copied: 0, removed: 0 });
    expect(mocks.browser.storage.local.set).not.toHaveBeenCalled();
    expect(mocks.browser.storage.local.remove).not.toHaveBeenCalled();
  });

  it("copies an old key to its new key and removes the old one", async () => {
    mocks.localStore.set("scopeball_jwt", "access-token");

    const result = await migratePasuRenameStorageKeys();

    expect(result).toEqual({ copied: 1, removed: 1 });
    expect(mocks.localStore.has("scopeball_jwt")).toBe(false);
    expect(mocks.localStore.get("pasu_jwt")).toBe("access-token");
  });

  it("migrates every renamed key in one pass", async () => {
    mocks.localStore.set("scopeball_jwt", "a");
    mocks.localStore.set("scopeball_jwt_refresh", "r");
    mocks.localStore.set("scopeball_server_url", "https://example.test");
    mocks.localStore.set("scopeball_diag_timeouts", [{ at: 1 }]);

    const result = await migratePasuRenameStorageKeys();

    expect(result).toEqual({ copied: 4, removed: 4 });
    expect(mocks.localStore.get("pasu_jwt")).toBe("a");
    expect(mocks.localStore.get("pasu_jwt_refresh")).toBe("r");
    expect(mocks.localStore.get("pasu_server_url")).toBe("https://example.test");
    expect(mocks.localStore.get("pasu_diag_timeouts")).toEqual([{ at: 1 }]);
    // every old key cleaned up
    expect(mocks.localStore.has("scopeball_jwt")).toBe(false);
    expect(mocks.localStore.has("scopeball_jwt_refresh")).toBe(false);
    expect(mocks.localStore.has("scopeball_server_url")).toBe(false);
    expect(mocks.localStore.has("scopeball_diag_timeouts")).toBe(false);
  });

  it("new key wins when both old and new exist; old is dropped", async () => {
    mocks.localStore.set("scopeball_jwt", "old");
    mocks.localStore.set("pasu_jwt", "new");

    const result = await migratePasuRenameStorageKeys();

    // dropped-but-not-copied → removed counts, copied does not
    expect(result).toEqual({ copied: 0, removed: 1 });
    expect(mocks.localStore.has("scopeball_jwt")).toBe(false);
    expect(mocks.localStore.get("pasu_jwt")).toBe("new");
  });

  it("is idempotent — second run is a no-op", async () => {
    mocks.localStore.set("scopeball_server_url", "https://example.test");

    const first = await migratePasuRenameStorageKeys();
    expect(first).toEqual({ copied: 1, removed: 1 });

    const second = await migratePasuRenameStorageKeys();
    expect(second).toEqual({ copied: 0, removed: 0 });
    expect(mocks.localStore.get("pasu_server_url")).toBe("https://example.test");
  });
});
