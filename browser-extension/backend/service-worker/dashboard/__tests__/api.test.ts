import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    applyEnabledIds: vi.fn(
      async (
        _ids: string[],
        _r: unknown,
      ): Promise<
        | { ok: true }
        | { ok: false; error: { kind: string; message: string } }
      > => ({ ok: true }),
    ),
    reinstallAllPolicies: vi.fn(async (_ids: readonly string[]) => {}),
    getCatalog: vi.fn(async () => ({
      policies: [],
      enabled: [],
      applied: [],
    })),
    browser: {
      storage: {
        local: {
          get: vi.fn(async (key: string) => ({ [key]: localStore.get(key) })),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
          }),
        },
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));
vi.mock("../../policies-loader", () => ({
  reinstallAllPolicies: mocks.reinstallAllPolicies,
}));
vi.mock("../../policy-selection", async () => {
  const actual =
    await vi.importActual<typeof import("../../policy-selection")>(
      "../../policy-selection",
    );
  return {
    ...actual,
    applyEnabledIds: mocks.applyEnabledIds,
    getCatalog: mocks.getCatalog,
  };
});

import { handleDashboardRequest, isDashboardRequest } from "../api";
import { listManaged } from "../storage";

const RAW_TEXT =
  '@id("dashboard::demo/x") @severity("warn") @reason("t") forbid (principal, action, resource);';

const TEMPLATE_TEXT =
  '@id("dashboard::demo/cap") @severity("deny") @reason("over cap") forbid (principal, action == Action::"swap", resource) when { context.inputAmountUsd > {{cap}} };';

describe("isDashboardRequest", () => {
  it("matches only objects whose type starts with 'dashboard:'", () => {
    expect(isDashboardRequest({ type: "dashboard:ping" })).toBe(true);
    expect(isDashboardRequest({ type: "policy-catalog" })).toBe(false);
    expect(isDashboardRequest(null)).toBe(false);
    expect(isDashboardRequest("dashboard:ping")).toBe(false);
  });
});

describe("dashboard/api", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    mocks.localStore.set("dashboard:current-user-id", "test-user");
    mocks.applyEnabledIds.mockResolvedValue({ ok: true });
    mocks.getCatalog.mockResolvedValue({
      policies: [],
      enabled: [],
      applied: [],
    });
  });

  it("ping returns version", async () => {
    const res = await handleDashboardRequest({ type: "dashboard:ping" });
    expect(res).toEqual({ ok: true, data: { version: 1 } });
  });

  it("put-raw stores, auto-enables, and reinstalls", async () => {
    const res = await handleDashboardRequest({
      type: "dashboard:put-raw",
      id: "dashboard::demo/x",
      text: RAW_TEXT,
    });
    expect(res.ok).toBe(true);
    expect(await listManaged()).toHaveLength(1);
    expect(mocks.applyEnabledIds).toHaveBeenCalledTimes(1);
    const [ids, reinstallFn] = mocks.applyEnabledIds.mock.calls[0];
    expect(ids).toContain("dashboard::demo/x");
    expect(reinstallFn).toBe(mocks.reinstallAllPolicies);
  });

  it("put-raw stores drafts without enabling them", async () => {
    const res = await handleDashboardRequest({
      type: "dashboard:put-raw",
      id: "dashboard::draft/x",
      text: RAW_TEXT,
      life: "draft",
    });

    expect(res.ok).toBe(true);
    expect(await listManaged()).toMatchObject([
      { id: "dashboard::draft/x", life: "draft" },
    ]);
    expect(mocks.applyEnabledIds).toHaveBeenCalledTimes(1);
    const [ids] = mocks.applyEnabledIds.mock.calls[0];
    expect(ids).not.toContain("dashboard::draft/x");
  });

  it("put-raw removes an existing enabled id when saving as draft", async () => {
    mocks.localStore.set("policy-selection:enabled-ids:test-user", [
      "dashboard::draft/x",
    ]);

    const res = await handleDashboardRequest({
      type: "dashboard:put-raw",
      id: "dashboard::draft/x",
      text: RAW_TEXT,
      life: "draft",
    });

    expect(res.ok).toBe(true);
    const [ids] = mocks.applyEnabledIds.mock.calls[0];
    expect(ids).not.toContain("dashboard::draft/x");
  });

  it("put-raw rejects unparseable text", async () => {
    const res = await handleDashboardRequest({
      type: "dashboard:put-raw",
      id: "dashboard::demo/garbage",
      text: "this is not cedar at all",
    });
    expect(res).toEqual({
      ok: false,
      error: { kind: "parse_failed", message: expect.any(String) },
    });
    expect(mocks.applyEnabledIds).not.toHaveBeenCalled();
  });

  it("put-raw rejects malformed id", async () => {
    const res = await handleDashboardRequest({
      type: "dashboard:put-raw",
      id: "no-prefix",
      text: RAW_TEXT,
    });
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.error.kind).toBe("invalid_id");
  });

  it("put-template renders, stores, auto-enables, and reinstalls", async () => {
    const res = await handleDashboardRequest({
      type: "dashboard:put-template",
      id: "dashboard::demo/cap",
      templateText: TEMPLATE_TEXT,
      paramsSchema: { cap: { type: "integer", min: 0, max: 1_000_000 } },
      paramValues: { cap: 1000 },
    });
    expect(res.ok).toBe(true);
    const list = await listManaged();
    expect(list).toHaveLength(1);
    expect(list[0].kind).toBe("template");
    expect(list[0].text).toContain("1000"); // rendered placeholder
    expect(mocks.applyEnabledIds).toHaveBeenCalledTimes(1);
  });

  it("put-template rejects when param schema mismatches values", async () => {
    const res = await handleDashboardRequest({
      type: "dashboard:put-template",
      id: "dashboard::demo/cap",
      templateText: TEMPLATE_TEXT,
      paramsSchema: { cap: { type: "integer", min: 0, max: 100 } },
      paramValues: { cap: 1000 },
    });
    expect(res.ok).toBe(false);
    if (!res.ok)
      expect(res.error.message).toMatch(/outside \[0, 100\]/);
  });

  it("delete removes the policy and un-enables it", async () => {
    await handleDashboardRequest({
      type: "dashboard:put-raw",
      id: "dashboard::demo/x",
      text: RAW_TEXT,
    });
    mocks.applyEnabledIds.mockClear();
    mocks.localStore.set("policy-selection:enabled-ids:test-user", ["dashboard::demo/x"]);

    const res = await handleDashboardRequest({
      type: "dashboard:delete",
      id: "dashboard::demo/x",
    });
    expect(res.ok).toBe(true);
    expect(await listManaged()).toEqual([]);
    expect(mocks.applyEnabledIds).toHaveBeenCalledTimes(1);
    const [ids] = mocks.applyEnabledIds.mock.calls[0];
    expect(ids).not.toContain("dashboard::demo/x");
  });

  it("set-enabled-ids forwards verbatim to applyEnabledIds", async () => {
    const res = await handleDashboardRequest({
      type: "dashboard:set-enabled-ids",
      ids: ["default::dex/a", "dashboard::demo/x"],
    });
    expect(res.ok).toBe(true);
    expect(mocks.applyEnabledIds).toHaveBeenCalledWith(
      ["default::dex/a", "dashboard::demo/x"],
      mocks.reinstallAllPolicies,
    );
  });

  it("set-enabled-ids rejects non-string array", async () => {
    const res = await handleDashboardRequest({
      type: "dashboard:set-enabled-ids",
      ids: [1, 2] as unknown as string[],
    });
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.error.kind).toBe("invalid_request");
  });

  it("propagates applyEnabledIds failure as the response error", async () => {
    mocks.applyEnabledIds.mockResolvedValueOnce({
      ok: false,
      error: { kind: "install_failed", message: "boom" },
    });
    const res = await handleDashboardRequest({
      type: "dashboard:put-raw",
      id: "dashboard::demo/x",
      text: RAW_TEXT,
    });
    expect(res).toEqual({
      ok: false,
      error: { kind: "install_failed", message: "boom" },
    });
  });

  it("rolls back storage when WASM rejects the policy (no prior entry)", async () => {
    // First put fails — storage must NOT keep the bad policy.
    mocks.applyEnabledIds.mockResolvedValueOnce({
      ok: false,
      error: { kind: "schema_failed", message: "attribute foo not found" },
    });
    const res = await handleDashboardRequest({
      type: "dashboard:put-raw",
      id: "dashboard::demo/bad",
      text: RAW_TEXT,
    });
    expect(res.ok).toBe(false);
    expect(await listManaged()).toEqual([]);
    // Rollback also runs applyEnabledIds a second time with the prior set.
    expect(mocks.applyEnabledIds).toHaveBeenCalledTimes(2);
    const [secondIds] = mocks.applyEnabledIds.mock.calls[1];
    expect(secondIds).not.toContain("dashboard::demo/bad");
  });

  it("restores the prior entry when an existing managed policy fails to re-install", async () => {
    // Plant a working policy first.
    await handleDashboardRequest({
      type: "dashboard:put-raw",
      id: "dashboard::demo/x",
      text: RAW_TEXT,
    });
    const before = await listManaged();
    expect(before).toHaveLength(1);
    const priorText = before[0].text;
    mocks.applyEnabledIds.mockClear();

    // Now attempt an update whose new text fails WASM validation.
    mocks.applyEnabledIds.mockResolvedValueOnce({
      ok: false,
      error: { kind: "schema_failed", message: "broken update" },
    });
    const res = await handleDashboardRequest({
      type: "dashboard:put-raw",
      id: "dashboard::demo/x",
      text: '@id("dashboard::demo/x") @severity("warn") @reason("v2") forbid (principal, action, resource);',
    });
    expect(res.ok).toBe(false);
    const after = await listManaged();
    expect(after).toHaveLength(1);
    expect(after[0].text).toBe(priorText);
    expect(mocks.applyEnabledIds).toHaveBeenCalledTimes(2);
  });
});
