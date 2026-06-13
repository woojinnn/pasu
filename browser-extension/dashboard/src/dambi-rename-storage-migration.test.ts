import { beforeEach, describe, expect, it } from "vitest";

import { migrateDambiRenameLocalStorage } from "./dambi-rename-storage-migration";

const newerLegacyKey = (suffix: string) => `${"pa" + "su"}_${suffix}`;
const olderLegacyKey = (suffix: string) => `${"scope" + "ball"}_${suffix}`;
const newerLegacyScopedKey = (suffix: string) => `${"pa" + "su"}:${suffix}`;
const activeKey = (suffix: string) => `dambi_${suffix}`;
const activeScopedKey = (suffix: string) => `dambi:${suffix}`;

describe("migrateDambiRenameLocalStorage", () => {
  // Self-mocked localStorage so the test behaves identically under the
  // dashboard's jsdom runner and the extension's happy-dom runner (whose
  // default localStorage shim does not implement `.clear()`). Mirrors the
  // pattern in `server-api/client.test.ts`.
  let storage: Map<string, string>;

  beforeEach(() => {
    storage = new Map();
    Object.defineProperty(window, "localStorage", {
      configurable: true,
      value: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
        removeItem: (key: string) => storage.delete(key),
        clear: () => storage.clear(),
        get length() {
          return storage.size;
        },
      },
    });
  });

  it("no-ops when no old keys are present (fresh install)", () => {
    const result = migrateDambiRenameLocalStorage();
    expect(result).toEqual({ copied: 0, removed: 0 });
    expect(storage.size).toBe(0);
  });

  it("copies an old key to its new key and removes the old one", () => {
    window.localStorage.setItem(newerLegacyKey("jwt"), "access-token");

    const result = migrateDambiRenameLocalStorage();

    expect(result).toEqual({ copied: 1, removed: 1 });
    expect(window.localStorage.getItem(newerLegacyKey("jwt"))).toBeNull();
    expect(window.localStorage.getItem(activeKey("jwt"))).toBe("access-token");
  });

  it("migrates every renamed key in one pass", () => {
    window.localStorage.setItem(newerLegacyKey("jwt"), "a");
    window.localStorage.setItem(newerLegacyKey("jwt_refresh"), "r");
    window.localStorage.setItem(newerLegacyKey("server_url"), "https://example.test");
    window.localStorage.setItem(newerLegacyScopedKey("market-locale"), "en-US");

    const result = migrateDambiRenameLocalStorage();

    expect(result).toEqual({ copied: 4, removed: 4 });
    expect(window.localStorage.getItem(activeKey("jwt"))).toBe("a");
    expect(window.localStorage.getItem(activeKey("jwt_refresh"))).toBe("r");
    expect(window.localStorage.getItem(activeKey("server_url"))).toBe(
      "https://example.test",
    );
    expect(window.localStorage.getItem(activeScopedKey("market-locale"))).toBe("en-US");
    expect(window.localStorage.getItem(newerLegacyKey("jwt"))).toBeNull();
    expect(window.localStorage.getItem(newerLegacyKey("jwt_refresh"))).toBeNull();
    expect(window.localStorage.getItem(newerLegacyKey("server_url"))).toBeNull();
    expect(window.localStorage.getItem(newerLegacyScopedKey("market-locale"))).toBeNull();
  });

  it("rewrites the legacy production server URL to the current dambi host", () => {
    window.localStorage.setItem(
      newerLegacyKey("server_url"),
      "https://pasu-policy.duckdns.org",
    );

    const result = migrateDambiRenameLocalStorage();

    expect(result).toEqual({ copied: 1, removed: 1 });
    expect(window.localStorage.getItem(activeKey("server_url"))).toBe(
      "https://dambi-policy.duckdns.org",
    );
  });

  it("repairs an already-migrated legacy production server URL", () => {
    window.localStorage.setItem(activeKey("server_url"), "https://pasu-policy.duckdns.org");

    const result = migrateDambiRenameLocalStorage();

    expect(result).toEqual({ copied: 0, removed: 0 });
    expect(window.localStorage.getItem(activeKey("server_url"))).toBe(
      "https://dambi-policy.duckdns.org",
    );
  });

  it("new key wins when both old and new exist; old is dropped", () => {
    window.localStorage.setItem(newerLegacyKey("jwt"), "old");
    window.localStorage.setItem(activeKey("jwt"), "new");

    const result = migrateDambiRenameLocalStorage();

    expect(result).toEqual({ copied: 0, removed: 1 });
    expect(window.localStorage.getItem(newerLegacyKey("jwt"))).toBeNull();
    expect(window.localStorage.getItem(activeKey("jwt"))).toBe("new");
  });

  it("newer legacy keys win over older legacy keys", () => {
    window.localStorage.setItem(olderLegacyKey("jwt"), "older");
    window.localStorage.setItem(newerLegacyKey("jwt"), "newer");

    const result = migrateDambiRenameLocalStorage();

    expect(result).toEqual({ copied: 1, removed: 2 });
    expect(window.localStorage.getItem(activeKey("jwt"))).toBe("newer");
    expect(window.localStorage.getItem(olderLegacyKey("jwt"))).toBeNull();
    expect(window.localStorage.getItem(newerLegacyKey("jwt"))).toBeNull();
  });

  it("is idempotent — second run is a no-op", () => {
    window.localStorage.setItem(newerLegacyScopedKey("market-locale"), "ko-KR");

    const first = migrateDambiRenameLocalStorage();
    expect(first).toEqual({ copied: 1, removed: 1 });

    const second = migrateDambiRenameLocalStorage();
    expect(second).toEqual({ copied: 0, removed: 0 });
    expect(window.localStorage.getItem(activeScopedKey("market-locale"))).toBe("ko-KR");
  });
});
