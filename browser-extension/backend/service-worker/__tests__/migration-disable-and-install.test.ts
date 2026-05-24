// Fix R integration test.
//
// Proves the end-to-end claim that the detector + install pipeline
// keeps the engine green when v0 policies are present in storage:
//
//   1. Seed `policy-selection:enabled-ids` with a mix of v0 + valid ids.
//   2. Seed `dashboard:policies` with a v0-shaped policy (referenced
//      under `context.<oldField>` instead of `context.custom.<oldField>`).
//   3. Run `detectPendingMigrations()` — the new boot-time first step.
//   4. Run `installFiltered()` via `ensureDefaultPoliciesInstalled()`.
//
// Assertion: `installPolicies()` is called with a `policy_set` that
// does NOT include the v0 id, so WASM never sees a v0 text and the
// install succeeds. Without Fix R the install would carry the v0
// policy, fail closed against the enriched schema, and the orchestrator
// would reject every request.

import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    installPolicies: vi.fn(
      async (_input: {
        schema_text: string;
        policy_set: { id: string; text: string }[];
        manifests?: unknown;
      }) => {},
    ),
    browser: {
      runtime: { getURL: (p: string) => `chrome-extension://x/${p}` },
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
            for (const [k, v] of Object.entries(entries)) {
              localStore.set(k, v);
            }
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
vi.mock("../wasm-bridge", () => ({ installPolicies: mocks.installPolicies }));
vi.mock("../adapter-loader/storage", () => ({
  aggregatedPolicySet: vi.fn(async () => []),
  listInstalled: vi.fn(async () => []),
}));

const fetchMock = vi.fn(async (url: string) => {
  if (url.endsWith("policy-set.json")) return new Response("[]");
  return new Response("");
});
vi.stubGlobal("fetch", fetchMock);

const V0_TEXT = `@id("dashboard::v0") @severity("deny") forbid (principal, action == Action::"swap", resource)
  when { context.totalInputUsd.value > 100 };`;
const VALID_TEXT = `@id("dashboard::v1") @severity("deny") forbid (principal, action == Action::"swap", resource)
  when { context has custom && context.custom has totalInputUsd && context.custom.totalInputUsd.value > 100 };`;

describe("Fix R: detector strips v0 ids before install", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    vi.resetModules();
  });

  it("post-detector ensureDefaultPoliciesInstalled never sees the v0 id in policy_set", async () => {
    // Seed managed-policy storage with a v0 + v1 entry through the
    // real `upsertManaged` path so the detector reads them via
    // `listManaged()` exactly as it would in production.
    const { upsertManaged } = await import("../dashboard/storage");
    await upsertManaged({
      id: "dashboard::v0",
      kind: "raw",
      text: V0_TEXT,
      updatedAtMs: 1_700_000_000_000,
      schemaVersion: 1,
    });
    await upsertManaged({
      id: "dashboard::v1",
      kind: "raw",
      text: VALID_TEXT,
      updatedAtMs: 1_700_000_000_000,
      schemaVersion: 1,
    });
    // Both ids enabled — this is the exact pre-Fix-R production state
    // that broke the engine.
    mocks.localStore.set("policy-selection:enabled-ids", [
      "dashboard::v0",
      "dashboard::v1",
    ]);

    const { detectPendingMigrations } = await import(
      "../manifests/migration-detector"
    );
    await detectPendingMigrations();

    // Storage post-detector: v0 is on the pending list AND removed from
    // the enabled set; original-enabled snapshot remembers v0 was on.
    expect(mocks.localStore.get("migration:pending")).toEqual([
      "dashboard::v0",
    ]);
    expect(mocks.localStore.get("policy-selection:enabled-ids")).toEqual([
      "dashboard::v1",
    ]);
    expect(mocks.localStore.get("migration:original-enabled")).toEqual({
      "dashboard::v0": true,
    });

    const { ensureDefaultPoliciesInstalled } = await import(
      "../policies-loader"
    );
    await ensureDefaultPoliciesInstalled();

    expect(mocks.installPolicies).toHaveBeenCalledTimes(1);
    const call = mocks.installPolicies.mock.calls[0][0];
    const installedIds = call.policy_set.map((p: { id: string }) => p.id);
    // The v0 id is NOT in the install payload. The v1 id IS, so the
    // engine still serves real verdicts for the user's other policies.
    expect(installedIds).not.toContain("dashboard::v0");
    expect(installedIds).toContain("dashboard::v1");
  });
});
