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

import {
  KEY_ORIGINAL_ENABLED,
  KEY_PENDING_MIGRATION,
  clearOriginalEnabled,
  getOriginalEnabled,
  listPending,
  rewritePolicyText,
  rewritePolicyTextCustomToBase,
  setOriginalEnabled,
  setPending,
} from "./migration";

describe("rewritePolicyText", () => {
  it("rewrites known v0 enrichment refs to context.custom with a has-guard", () => {
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context.totalInputUsd.value > 100 };`;
    const after = rewritePolicyText(before, ["totalInputUsd"]);
    expect(after).toContain("context.custom has totalInputUsd");
    expect(after).toContain("context.custom.totalInputUsd.value > 100");
    expect(after).not.toContain("context.totalInputUsd");
  });

  it("leaves unknown identifiers alone", () => {
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context.someUnknown == 1 };`;
    const after = rewritePolicyText(before, ["totalInputUsd"]);
    expect(after).toBe(before);
  });

  it("only adds a single has-guard even when the field appears multiple times", () => {
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context.totalInputUsd.value > 100 && context.totalInputUsd.value < 1000 };`;
    const after = rewritePolicyText(before, ["totalInputUsd"]);
    // Both reads got rewritten...
    expect(
      (after.match(/context\.custom\.totalInputUsd/g) ?? []).length,
    ).toBe(2);
    // ...but the has-guard appears exactly once.
    expect(
      (after.match(/context\.custom has totalInputUsd/g) ?? []).length,
    ).toBe(1);
  });

  it("rewrites multiple known fields independently", () => {
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context.totalInputUsd.value > 100 && context.tokenRiskScore > 5 };`;
    const after = rewritePolicyText(before, ["totalInputUsd", "tokenRiskScore"]);
    expect(after).toContain("context.custom.totalInputUsd.value > 100");
    expect(after).toContain("context.custom.tokenRiskScore > 5");
    expect(after).toContain("context.custom has totalInputUsd");
    expect(after).toContain("context.custom has tokenRiskScore");
  });

  it("is a no-op when there are no when-clauses to guard", () => {
    const before = `forbid (principal, action == Action::"swap", resource);`;
    const after = rewritePolicyText(before, ["totalInputUsd"]);
    expect(after).toBe(before);
  });

  it("does not rewrite fields that already use context.custom (idempotent)", () => {
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context.custom has totalInputUsd && context.custom.totalInputUsd.value > 100 };`;
    const after = rewritePolicyText(before, ["totalInputUsd"]);
    expect(after).toBe(before);
  });
});

describe("rewritePolicyTextCustomToBase (Phase 8 inverse rewrite)", () => {
  // Promotes inputAmountNano/outputAmountNano back to the base context
  // shape so policies authored before Phase 8 keep installing once the
  // bundled cedarschema drops them from `SwapCustomContext`.

  it("strips context.custom prefix from named fields and rewrites the has-guard", () => {
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context has custom && context.custom has inputAmountNano && context.custom.inputAmountNano > 30000 };`;
    const after = rewritePolicyTextCustomToBase(before, ["inputAmountNano"]);
    expect(after).toContain("context has inputAmountNano");
    expect(after).toContain("context.inputAmountNano > 30000");
    expect(after).not.toContain("context.custom.inputAmountNano");
    expect(after).not.toContain("context.custom has inputAmountNano");
  });

  it("drops the umbrella `context has custom` guard when no custom refs remain", () => {
    // Single-field policy: after the rewrite there are zero
    // `context.custom.*` references, so the umbrella guard is dead
    // weight — the v8 schema doesn't even need to narrow `custom`.
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context has custom && context.custom has inputAmountNano && context.custom.inputAmountNano > 30000 };`;
    const after = rewritePolicyTextCustomToBase(before, ["inputAmountNano"]);
    expect(after).not.toContain("context has custom");
  });

  it("keeps the umbrella guard when OTHER custom fields are still referenced", () => {
    // Mixed policy: only inputAmountNano migrates back to base; the
    // remaining custom ref still needs the umbrella guard so type
    // narrowing works.
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context has custom && context.custom has inputAmountNano && context.custom has totalInputUsd && context.custom.inputAmountNano > 30000 && context.custom.totalInputUsd.value > 100 };`;
    const after = rewritePolicyTextCustomToBase(before, ["inputAmountNano"]);
    expect(after).toContain("context.inputAmountNano > 30000");
    expect(after).toContain("context.custom.totalInputUsd.value > 100");
    expect(after).toContain("context has custom");
  });

  it("is idempotent on policies already on the new shape", () => {
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context has inputAmountNano && context.inputAmountNano > 30000 };`;
    const after = rewritePolicyTextCustomToBase(before, ["inputAmountNano"]);
    expect(after).toBe(before);
  });

  it("never touches identifiers outside the supplied field list", () => {
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context has custom && context.custom has someOtherField && context.custom.someOtherField > 1 };`;
    const after = rewritePolicyTextCustomToBase(before, ["inputAmountNano"]);
    expect(after).toBe(before);
  });

  it("handles both amount-nano fields together", () => {
    const before = `forbid (principal, action == Action::"swap", resource)
      when { context has custom && context.custom has inputAmountNano && context.custom has outputAmountNano && context.custom.inputAmountNano > 30000 && context.custom.outputAmountNano > 100 };`;
    const after = rewritePolicyTextCustomToBase(before, [
      "inputAmountNano",
      "outputAmountNano",
    ]);
    expect(after).toContain("context.inputAmountNano > 30000");
    expect(after).toContain("context.outputAmountNano > 100");
    expect(after).not.toContain("context.custom");
  });
});

describe("pending-migration list", () => {
  beforeEach(() => {
    mocks.localStore.clear();
    vi.clearAllMocks();
  });

  it("listPending returns [] when nothing has been recorded", async () => {
    expect(await listPending()).toEqual([]);
  });

  it("setPending / listPending round-trips ids in storage", async () => {
    await setPending(["policy:a", "policy:b"]);
    expect(await listPending()).toEqual(["policy:a", "policy:b"]);
    expect(mocks.localStore.get(KEY_PENDING_MIGRATION)).toEqual([
      "policy:a",
      "policy:b",
    ]);
  });

  it("setPending with [] clears the key", async () => {
    await setPending(["policy:a"]);
    await setPending([]);
    expect(await listPending()).toEqual([]);
  });
});

describe("original-enabled snapshot", () => {
  beforeEach(() => {
    mocks.localStore.clear();
    vi.clearAllMocks();
  });

  it("getOriginalEnabled returns {} when nothing has been recorded", async () => {
    expect(await getOriginalEnabled()).toEqual({});
  });

  it("setOriginalEnabled writes a Record<id, bool> under KEY_ORIGINAL_ENABLED", async () => {
    await setOriginalEnabled({
      "dashboard::v0": true,
      "dashboard::v1": false,
    });
    expect(mocks.localStore.get(KEY_ORIGINAL_ENABLED)).toEqual({
      "dashboard::v0": true,
      "dashboard::v1": false,
    });
  });

  it("setOriginalEnabled({}) removes the key entirely", async () => {
    await setOriginalEnabled({ a: true });
    await setOriginalEnabled({});
    expect(await getOriginalEnabled()).toEqual({});
    expect(mocks.localStore.has(KEY_ORIGINAL_ENABLED)).toBe(false);
  });

  it("clearOriginalEnabled(id) pops a single key but leaves the rest", async () => {
    await setOriginalEnabled({
      "dashboard::v0": true,
      "dashboard::v1": false,
    });
    await clearOriginalEnabled("dashboard::v0");
    expect(await getOriginalEnabled()).toEqual({ "dashboard::v1": false });
  });

  it("clearOriginalEnabled removes the key entirely when the last id is popped", async () => {
    await setOriginalEnabled({ "dashboard::v0": true });
    await clearOriginalEnabled("dashboard::v0");
    expect(await getOriginalEnabled()).toEqual({});
    expect(mocks.localStore.has(KEY_ORIGINAL_ENABLED)).toBe(false);
  });
});
