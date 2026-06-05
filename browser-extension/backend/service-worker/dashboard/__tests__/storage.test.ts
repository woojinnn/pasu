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
        },
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import {
  aggregatedManagedPolicySet,
  DASHBOARD_ID_RE,
  deleteManaged,
  listManaged,
  MAX_ENTRIES,
  MAX_TEXT_BYTES,
  upsertManaged,
  type ManagedPolicy,
} from "../storage";

const sampleText = (id: string) =>
  `@id("${id}") @severity("warn") @reason("test") forbid (principal, action, resource);`;

function makePolicy(id: string, text = sampleText(id)): ManagedPolicy {
  return {
    id,
    kind: "raw",
    text,
    updatedAtMs: 0,
    schemaVersion: 1,
  };
}

describe("dashboard/storage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    mocks.localStore.set("dashboard:current-user-id", "test-user");
  });

  it("returns an empty list on a fresh install", async () => {
    expect(await listManaged()).toEqual([]);
  });

  it("DASHBOARD_ID_RE matches well-formed ids and rejects bad ones", () => {
    expect(DASHBOARD_ID_RE.test("dashboard::user/forbid-permits")).toBe(true);
    expect(DASHBOARD_ID_RE.test("dashboard::a")).toBe(true);
    expect(DASHBOARD_ID_RE.test("dashboard::newrule(0)")).toBe(true);
    expect(DASHBOARD_ID_RE.test("dashboard::newrule(42)")).toBe(true);
    expect(DASHBOARD_ID_RE.test("user/forbid-permits")).toBe(false);
    expect(DASHBOARD_ID_RE.test("dashboard::")).toBe(false);
    expect(DASHBOARD_ID_RE.test("dashboard::foo bar")).toBe(false);
  });

  it("rejects upsert with an invalid id", async () => {
    await expect(upsertManaged(makePolicy("invalid/no-prefix"))).rejects.toThrow(
      /invalid_id/,
    );
  });

  it("rejects upsert when text exceeds MAX_TEXT_BYTES", async () => {
    const big = "a".repeat(MAX_TEXT_BYTES + 1);
    await expect(
      upsertManaged(makePolicy("dashboard::big", big)),
    ).rejects.toThrow(/text_too_large/);
  });

  it("upsert / list / delete round-trip", async () => {
    const p = makePolicy("dashboard::x");
    await upsertManaged(p);
    expect(await listManaged()).toEqual([p]);

    const p2 = { ...p, text: sampleText("dashboard::x") + " // edited" };
    await upsertManaged(p2);
    const after = await listManaged();
    expect(after).toHaveLength(1);
    expect(after[0].text).toContain("edited");

    await deleteManaged("dashboard::x");
    expect(await listManaged()).toEqual([]);
  });

  it("enforces MAX_ENTRIES cap", async () => {
    const existing: ManagedPolicy[] = [];
    for (let i = 0; i < MAX_ENTRIES; i++) {
      existing.push(makePolicy(`dashboard::p${i}`));
    }
    mocks.localStore.set("dashboard:policies:test-user", existing);
    await expect(
      upsertManaged(makePolicy("dashboard::overflow")),
    ).rejects.toThrow(/too_many_entries/);
  });

  it("aggregatedManagedPolicySet returns loader-shaped entries", async () => {
    await upsertManaged({
      ...makePolicy("dashboard::a"),
      manifest: { id: "m1" },
    });
    await upsertManaged({
      ...makePolicy("dashboard::b"),
      manifests: [{ id: "m2" }],
    });
    const out = await aggregatedManagedPolicySet();
    expect(out).toEqual([
      {
        id: "dashboard::a",
        text: sampleText("dashboard::a"),
        manifest: { id: "m1" },
      },
      {
        id: "dashboard::b",
        text: sampleText("dashboard::b"),
        manifests: [{ id: "m2" }],
      },
    ]);
  });
});
