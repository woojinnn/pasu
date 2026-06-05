import { beforeEach, describe, expect, it } from "vitest";

import { migratePasuRenameLocalStorage } from "./pasu-rename-storage-migration";

describe("migratePasuRenameLocalStorage", () => {
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
    const result = migratePasuRenameLocalStorage();
    expect(result).toEqual({ copied: 0, removed: 0 });
    expect(storage.size).toBe(0);
  });

  it("copies an old key to its new key and removes the old one", () => {
    window.localStorage.setItem("scopeball_jwt", "access-token");

    const result = migratePasuRenameLocalStorage();

    expect(result).toEqual({ copied: 1, removed: 1 });
    expect(window.localStorage.getItem("scopeball_jwt")).toBeNull();
    expect(window.localStorage.getItem("pasu_jwt")).toBe("access-token");
  });

  it("migrates every renamed key in one pass", () => {
    window.localStorage.setItem("scopeball_jwt", "a");
    window.localStorage.setItem("scopeball_jwt_refresh", "r");
    window.localStorage.setItem("scopeball_server_url", "https://example.test");
    window.localStorage.setItem("scopeball:market-locale", "en-US");

    const result = migratePasuRenameLocalStorage();

    expect(result).toEqual({ copied: 4, removed: 4 });
    expect(window.localStorage.getItem("pasu_jwt")).toBe("a");
    expect(window.localStorage.getItem("pasu_jwt_refresh")).toBe("r");
    expect(window.localStorage.getItem("pasu_server_url")).toBe(
      "https://example.test",
    );
    expect(window.localStorage.getItem("pasu:market-locale")).toBe("en-US");
    expect(window.localStorage.getItem("scopeball_jwt")).toBeNull();
    expect(window.localStorage.getItem("scopeball_jwt_refresh")).toBeNull();
    expect(window.localStorage.getItem("scopeball_server_url")).toBeNull();
    expect(window.localStorage.getItem("scopeball:market-locale")).toBeNull();
  });

  it("new key wins when both old and new exist; old is dropped", () => {
    window.localStorage.setItem("scopeball_jwt", "old");
    window.localStorage.setItem("pasu_jwt", "new");

    const result = migratePasuRenameLocalStorage();

    expect(result).toEqual({ copied: 0, removed: 1 });
    expect(window.localStorage.getItem("scopeball_jwt")).toBeNull();
    expect(window.localStorage.getItem("pasu_jwt")).toBe("new");
  });

  it("is idempotent — second run is a no-op", () => {
    window.localStorage.setItem("scopeball:market-locale", "ko-KR");

    const first = migratePasuRenameLocalStorage();
    expect(first).toEqual({ copied: 1, removed: 1 });

    const second = migratePasuRenameLocalStorage();
    expect(second).toEqual({ copied: 0, removed: 0 });
    expect(window.localStorage.getItem("pasu:market-locale")).toBe("ko-KR");
  });
});
