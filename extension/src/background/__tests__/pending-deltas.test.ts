import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();

  const get = async (keys?: string | string[] | Record<string, unknown>) => {
    if (keys === undefined || keys === null)
      return Object.fromEntries(localStore.entries());
    const out: Record<string, unknown> = {};
    if (typeof keys === "string") {
      out[keys] = localStore.get(keys);
      return out;
    }
    if (Array.isArray(keys)) {
      for (const key of keys) out[key] = localStore.get(key);
      return out;
    }
    for (const [key, fallback] of Object.entries(keys)) {
      out[key] = localStore.has(key) ? localStore.get(key) : fallback;
    }
    return out;
  };

  return {
    localStore,
    browser: {
      storage: {
        local: {
          get: vi.fn(get),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [key, value] of Object.entries(entries))
              localStore.set(key, value);
          }),
        },
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import {
  commitByTxHash,
  pendingForActor,
  reservePending,
} from "../pending-deltas";

const ACTOR = "0x1111111111111111111111111111111111111111";

describe("pending-deltas", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
  });

  it("adds decimal swap volume deltas and integer count deltas per actor", async () => {
    await reservePending({
      requestId: "req-1",
      chainId: 1,
      actor: ACTOR,
      windowEntries: [
        { name: "swapVolumeUsd24h", value: "3500.0000" },
        { name: "swapCount24h", value: "1" },
      ],
      enqueuedAtMs: 1,
    });
    await reservePending({
      requestId: "req-2",
      chainId: 1,
      actor: ACTOR.toUpperCase(),
      windowEntries: [
        { name: "swapVolumeUsd24h", value: "0.2500" },
        { name: "swapCount24h", value: "2" },
      ],
      enqueuedAtMs: 2,
    });

    await expect(pendingForActor(ACTOR)).resolves.toEqual([
      { name: "swapVolumeUsd24h", value: "3500.2500" },
      { name: "swapCount24h", value: "3" },
    ]);
  });

  it("commits decimal swap volume deltas without coercing them through BigInt", async () => {
    await reservePending({
      requestId: "req-1",
      chainId: 1,
      actor: ACTOR,
      txHash: "0xabc",
      windowEntries: [
        { name: "swapVolumeUsd24h", value: "1.5000" },
        { name: "swapCount24h", value: "1" },
      ],
      enqueuedAtMs: 1,
    });

    await commitByTxHash("0xabc", {
      chainId: 1,
      actor: ACTOR,
      windowEntries: [
        { name: "swapVolumeUsd24h", value: "2.2500" },
        { name: "swapCount24h", value: "1" },
      ],
    });

    expect(mocks.localStore.get("windows:committed")).toEqual({
      [ACTOR]: {
        swapVolumeUsd24h: "2.2500",
        swapCount24h: "1",
      },
    });
  });
});
