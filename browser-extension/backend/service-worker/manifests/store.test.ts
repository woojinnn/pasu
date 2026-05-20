import { beforeEach, describe, expect, it, vi } from "vitest";

// Shared in-memory backing store for `Browser.storage.local`. The mock
// implements `get`, `set`, and `remove` against this map so the store
// module sees a real round-trip.
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

import {
  clearAll,
  getAllManifests,
  getEndpointUrl,
  getHash,
  getManifest,
  putManifestRaw,
  setEndpointUrl,
  setHash,
  type PolicyManifest,
} from "./store";

function emptyManifest(id: string): PolicyManifest {
  return { id, schema_version: 1, requires: [], context_extensions: {} };
}

describe("manifest store", () => {
  beforeEach(async () => {
    mocks.localStore.clear();
    vi.clearAllMocks();
  });

  it("returns null for absent manifest / endpoint / hash", async () => {
    expect(await getManifest("swap")).toBeNull();
    expect(await getEndpointUrl()).toBeNull();
    expect(await getHash()).toBeNull();
    expect(await getAllManifests()).toEqual({});
  });

  it("round-trips a single manifest under its action key", async () => {
    await putManifestRaw("swap", emptyManifest("user::swap"));

    expect((await getManifest("swap"))!.id).toBe("user::swap");
    expect(Object.keys(await getAllManifests())).toEqual(["swap"]);
  });

  it("preserves existing manifests when writing additional actions", async () => {
    await putManifestRaw("swap", emptyManifest("user::swap"));
    await putManifestRaw("borrow", emptyManifest("user::borrow"));

    const all = await getAllManifests();
    expect(Object.keys(all).sort()).toEqual(["borrow", "swap"]);
    expect(all.swap.id).toBe("user::swap");
    expect(all.borrow.id).toBe("user::borrow");
  });

  it("setEndpointUrl and setHash are independently retrievable", async () => {
    await setEndpointUrl("http://localhost:8787");
    await setHash("sha256:abc");

    expect(await getEndpointUrl()).toBe("http://localhost:8787");
    expect(await getHash()).toBe("sha256:abc");
  });

  it("clearAll wipes manifests, endpoint, and hash but not unrelated keys", async () => {
    await putManifestRaw("swap", emptyManifest("user::swap"));
    await setEndpointUrl("http://localhost:8787");
    await setHash("sha256:abc");
    // Unrelated key the store must not touch.
    await mocks.browser.storage.local.set({ "unrelated:key": "keep me" });

    await clearAll();

    expect(await getManifest("swap")).toBeNull();
    expect(await getEndpointUrl()).toBeNull();
    expect(await getHash()).toBeNull();
    expect(mocks.localStore.get("unrelated:key")).toBe("keep me");
  });

  it("setEndpointUrl(null) clears the endpoint", async () => {
    await setEndpointUrl("http://localhost:8787");
    await setEndpointUrl(null);
    expect(await getEndpointUrl()).toBeNull();
  });

  it("putManifestRaw overwrites existing action entry", async () => {
    await putManifestRaw("swap", emptyManifest("user::swap"));
    await putManifestRaw("swap", emptyManifest("user::swap-v2"));

    expect((await getManifest("swap"))!.id).toBe("user::swap-v2");
  });
});
