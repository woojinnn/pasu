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
import {
  KEY_ORIGINAL_ENABLED,
  KEY_PENDING_MIGRATION,
  getOriginalEnabled,
  listPending,
} from "./migration";
import type { ManagedPolicy } from "../dashboard/storage";

const KEY_ENABLED_IDS = "policy-selection:enabled-ids";

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

  // Fix R: a v0 policy that's currently enabled must be force-disabled
  // when detected, otherwise `installFiltered` keeps trying to install
  // it against the enriched schema and the engine rejects every request.
  // The detector saves the prior enabled-state under
  // `migration:original-enabled` so a successful Rewrite + ack can put
  // the user's preference back.
  it("removes enabled v0 ids from policy-selection:enabled-ids and snapshots prior state", async () => {
    mocks.localStore.set(KEY_ENABLED_IDS, [
      "dashboard::v0",
      "dashboard::v1",
      "default::dex/keep",
    ]);
    const v0 = makePolicy("dashboard::v0", V0_TEXT);
    const v1 = makePolicy("dashboard::v1", V1_TEXT);

    await detectPendingMigrations({
      listManaged: async () => [v0, v1],
    });

    expect(await listPending()).toEqual(["dashboard::v0"]);
    // v0 is gone from enabled; v1 + unrelated id stay.
    expect(mocks.localStore.get(KEY_ENABLED_IDS)).toEqual([
      "dashboard::v1",
      "default::dex/keep",
    ]);
    // We snapshotted the prior enabled-state so ack can restore.
    expect(await getOriginalEnabled()).toEqual({ "dashboard::v0": true });
  });

  it("snapshots a disabled v0 id as `false` so ack does not auto-enable", async () => {
    // Enabled set does NOT contain the v0 id — user had it off.
    mocks.localStore.set(KEY_ENABLED_IDS, ["default::dex/keep"]);
    const v0 = makePolicy("dashboard::v0", V0_TEXT);

    await detectPendingMigrations({
      listManaged: async () => [v0],
    });

    expect(await listPending()).toEqual(["dashboard::v0"]);
    expect(mocks.localStore.get(KEY_ENABLED_IDS)).toEqual([
      "default::dex/keep",
    ]);
    expect(await getOriginalEnabled()).toEqual({ "dashboard::v0": false });
  });

  it("original-enabled snapshot is first-write-wins across re-runs", async () => {
    // Start: v0 is enabled. First detector run saves `{v0: true}` and
    // strips v0 from enabled-ids. A subsequent run sees v0 already
    // missing from enabled-ids but must NOT clobber the snapshot to
    // `false` — the user's original preference (`true`) must persist.
    mocks.localStore.set(KEY_ENABLED_IDS, ["dashboard::v0"]);
    const v0 = makePolicy("dashboard::v0", V0_TEXT);

    await detectPendingMigrations({ listManaged: async () => [v0] });
    await detectPendingMigrations({ listManaged: async () => [v0] });

    expect(await getOriginalEnabled()).toEqual({ "dashboard::v0": true });
    // Pending stays single-entry, enabled-ids stays without v0.
    expect(await listPending()).toEqual(["dashboard::v0"]);
    expect(mocks.localStore.get(KEY_ENABLED_IDS)).toEqual([]);
  });

  it("preserves the KEY_ORIGINAL_ENABLED storage key across runs", async () => {
    // Sanity: the constant is the documented key everyone in the SW
    // agrees on. Drift between the detector + ack handler would silently
    // break the restore path, so we anchor it here.
    expect(KEY_ORIGINAL_ENABLED).toBe("migration:original-enabled");
  });
});
