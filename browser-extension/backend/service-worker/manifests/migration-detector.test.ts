// SW migration auto-detection (Fix O).
//
// The migration handler reads from `migration:pending` and the rewrite
// banner renders it, but production code never populated the queue —
// only test fixtures did. As a result a real user upgrade through Phase
// 5 left v0 policies sitting in storage with no UI prompt to migrate.
//
// `detectPendingMigrations()` scans every managed policy text for
// `context.<knownEnrichmentField>` references (NOT
// `context.custom.<field>` — those are already migrated) and pushes the
// matching ids onto `migration:pending`. It runs once per SW startup,
// AFTER hydrate, so the banner reflects state on the next dashboard
// open.

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

import { detectPendingMigrations } from "./migration-detector";
import { KEY_PENDING_MIGRATION, listPending } from "./migration";
import type { ManagedPolicy } from "../dashboard/storage";

function makePolicy(id: string, text: string): ManagedPolicy {
  return {
    id,
    kind: "raw",
    text,
    updatedAtMs: 1_700_000_000_000,
    schemaVersion: 1,
  };
}

const V0_TEXT = `@id("user/x") @severity("deny") forbid (principal, action == Action::"swap", resource)
  when { context.totalInputUsd.value > 100 };`;
const V1_TEXT = `@id("user/y") @severity("deny") forbid (principal, action == Action::"swap", resource)
  when { context has custom && context.custom has totalInputUsd &&
         context.custom.totalInputUsd.value > 100 };`;
const NO_REF_TEXT = `@id("user/z") @severity("deny") forbid (principal, action == Action::"swap", resource)
  when { context has feeBps && context.feeBps > 30 };`;

describe("detectPendingMigrations", () => {
  beforeEach(() => {
    mocks.localStore.clear();
    vi.clearAllMocks();
  });

  it("queues ids whose text uses top-level context.<v0-field>", async () => {
    const v0 = makePolicy("dashboard::v0", V0_TEXT);
    const v1 = makePolicy("dashboard::v1", V1_TEXT);
    const no = makePolicy("dashboard::no", NO_REF_TEXT);

    await detectPendingMigrations({
      listManaged: async () => [v0, v1, no],
    });

    expect(await listPending()).toEqual(["dashboard::v0"]);
  });

  it("is idempotent — re-running does not duplicate existing ids", async () => {
    const v0 = makePolicy("dashboard::v0", V0_TEXT);

    await detectPendingMigrations({ listManaged: async () => [v0] });
    await detectPendingMigrations({ listManaged: async () => [v0] });
    await detectPendingMigrations({ listManaged: async () => [v0] });

    expect(await listPending()).toEqual(["dashboard::v0"]);
  });

  it("merges with previously-set pending ids without duplicating", async () => {
    // Pre-seed an unrelated pending id (e.g. a prior session left this).
    mocks.localStore.set(KEY_PENDING_MIGRATION, ["dashboard::seeded"]);

    const v0 = makePolicy("dashboard::v0", V0_TEXT);
    await detectPendingMigrations({ listManaged: async () => [v0] });
    // Re-run: the second call must NOT reappend `dashboard::v0`.
    await detectPendingMigrations({ listManaged: async () => [v0] });

    const pending = await listPending();
    expect(pending.sort()).toEqual(["dashboard::seeded", "dashboard::v0"]);
  });

  it("ignores already-migrated `context.custom.<field>` references", async () => {
    const v1 = makePolicy("dashboard::v1", V1_TEXT);
    await detectPendingMigrations({ listManaged: async () => [v1] });
    expect(await listPending()).toEqual([]);
  });

  it("matches every field in the shared V0_KNOWN_FIELDS list", async () => {
    // Build one policy per field; assert every id ends up pending.
    // Import the constant through the same relative path the SW code
    // uses so the root tsconfig (which doesn't carry the dashboard's
    // `@scopeball/sdk` alias) still resolves it.
    const { V0_KNOWN_FIELDS } = await import("../../../sdk/extension-client");
    const policies = V0_KNOWN_FIELDS.map((field, i) =>
      makePolicy(
        `dashboard::field-${i}`,
        `forbid (principal, action == Action::"swap", resource)
         when { context.${field} == 0 };`,
      ),
    );
    await detectPendingMigrations({ listManaged: async () => policies });
    const expected = policies.map((p) => p.id).sort();
    expect((await listPending()).sort()).toEqual(expected);
  });

  it("real `listManaged` round-trip: seed via storage, run detector, see id in pending", async () => {
    // Exercises the integration boundary: a managed policy committed
    // through `upsertManaged` (mirrors the SW `dashboard:put-raw`
    // handler's write path) must be visible to the detector reading
    // through `listManaged()` from the same storage namespace.
    const { upsertManaged } = await import("../dashboard/storage");
    await upsertManaged({
      id: "dashboard::seeded-v0",
      kind: "raw",
      text: `@severity("deny") forbid (principal, action == Action::"swap", resource)
        when { context.totalInputUsd.value > 100 };`,
      updatedAtMs: 1_700_000_000_000,
      schemaVersion: 1,
    });

    await detectPendingMigrations();
    expect(await listPending()).toContain("dashboard::seeded-v0");
  });
});
