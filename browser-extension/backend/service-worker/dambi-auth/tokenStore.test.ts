import { beforeEach, describe, expect, it, vi } from "vitest";

// Shared in-memory chrome.storage.local stand-in for BOTH the token store
// and the dambi-rename migration (they both import webextension-polyfill).
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

import { migrateDambiRenameStorageKeys } from "../manifests/dambi-rename-storage-migration";
import { _resetCacheForTests, getAccessToken, setTokens } from "./tokenStore";

const legacyKey = (suffix: string) => `${"pa" + "su"}_${suffix}`;

describe("tokenStore rename race", () => {
  beforeEach(() => {
    mocks.localStore.clear();
    vi.clearAllMocks();
    _resetCacheForTests();
  });

  it("reads the token after the rename migration runs (cold cache, only OLD key set)", async () => {
    // Pre-rename user: token sits under the legacy access key only.
    mocks.localStore.set(legacyKey("jwt"), "legacy-access-token");

    // The migration runs at SW boot (here we await it explicitly, the way
    // the gated auth handlers do via `bootReady`).
    await migrateDambiRenameStorageKeys();

    // After the migration lands, a token read returns the migrated token —
    // it must NOT report logged-out.
    expect(await getAccessToken()).toBe("legacy-access-token");
  });

  it("does not permanently cache null when a read momentarily finds nothing", async () => {
    // Cold cache, nothing in storage yet (mirrors a token read that races
    // ahead of the migration's `set`).
    expect(await getAccessToken()).toBeNull();

    // The token lands later (migration copy / dashboard sync / login).
    await setTokens("late-access-token", null);

    // A subsequent read must succeed — the earlier empty read must not have
    // poisoned the in-memory cache with `null` for the SW lifetime.
    expect(await getAccessToken()).toBe("late-access-token");
  });

  it("caches a real token on the fast path (no second storage read)", async () => {
    mocks.localStore.set("dambi_jwt", "cached-access-token");

    expect(await getAccessToken()).toBe("cached-access-token");
    expect(mocks.browser.storage.local.get).toHaveBeenCalledTimes(1);

    // Second read is served from the in-memory cache: no extra storage hit.
    expect(await getAccessToken()).toBe("cached-access-token");
    expect(mocks.browser.storage.local.get).toHaveBeenCalledTimes(1);
  });
});
