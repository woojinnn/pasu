import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    browser: {
      storage: {
        local: {
          get: vi.fn(async (key: string) => ({ [key]: localStore.get(key) })),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
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

import { handleDashboardRequest, isDashboardRequest } from "../api";

beforeEach(() => mocks.localStore.clear());

describe("dashboard api (post-ps2 surface)", () => {
  it("isDashboardRequest accepts only the remaining family", () => {
    expect(isDashboardRequest({ type: "dashboard:ping" })).toBe(true);
    expect(isDashboardRequest({ type: "dashboard:get-current-user" })).toBe(true);
    expect(isDashboardRequest({ type: "dashboard:put-raw" })).toBe(false);
    expect(isDashboardRequest({ type: "dashboard:list-sets" })).toBe(false);
  });

  it("ping → pong", async () => {
    await expect(handleDashboardRequest({ type: "dashboard:ping" })).resolves.toEqual({
      ok: true,
      data: "pong",
    });
  });

  it("set/get/clear current-user round-trips", async () => {
    const set = await handleDashboardRequest({ type: "dashboard:set-current-user", userId: "u1" });
    expect(set).toEqual({ ok: true, data: { userId: "u1" } });
    const got = await handleDashboardRequest({ type: "dashboard:get-current-user" });
    expect(got).toEqual({ ok: true, data: { userId: "u1" } });
    const cleared = await handleDashboardRequest({ type: "dashboard:clear-current-user" });
    expect(cleared).toEqual({ ok: true, data: null });
    const after = await handleDashboardRequest({ type: "dashboard:get-current-user" });
    expect(after).toEqual({ ok: true, data: { userId: null } });
  });

  it("set-current-user rejects an empty userId", async () => {
    const res = await handleDashboardRequest({ type: "dashboard:set-current-user", userId: "" });
    expect(res.ok).toBe(false);
  });
});
