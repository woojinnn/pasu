import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  return {
    // Mutable per-case asset body served for `policy-set-v2.json`.
    fetchedV2: "[]",
    // When set, the fetch mock returns this Response instead of `fetchedV2`
    // (used to simulate a non-200 / network failure).
    fetchOverride: null as Response | (() => never) | null,
    // Mutable `chrome.storage.local["dashboard:policies"]` content — the
    // user-authored policies the dashboard saves (Option B merge).
    managed: [] as unknown[],
    // Mutable `chrome.storage.local["policy-selection:enabled-ids"]` — the
    // enabled-id allow-list the popup rewrites when a policy is toggled.
    enabledIds: [] as string[],
    browser: {
      runtime: { getURL: (p: string) => `chrome-extension://x/${p}` },
      storage: {
        local: {
          get: async (key: string) => {
            // Per-user namespacing: dashboard/storage + policy-selection key
            // their reads under `<base>:<userId>`, sourced from current-user.
            if (key === "dashboard:current-user-id")
              return { [key]: "test-user" };
            if (key === "dashboard:policies:test-user")
              return { [key]: mocks.managed };
            if (key === "policy-selection:enabled-ids:test-user")
              return { [key]: mocks.enabledIds };
            return { [key]: undefined };
          },
        },
        onChanged: { addListener: () => {} },
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

const fetchMock = vi.fn(async (url: string) => {
  if (mocks.fetchOverride) {
    if (typeof mocks.fetchOverride === "function") mocks.fetchOverride();
    return mocks.fetchOverride as Response;
  }
  if (url.endsWith("policy-set-v2.json")) return new Response(mocks.fetchedV2);
  return new Response("[]");
});
vi.stubGlobal("fetch", fetchMock);

const HIGH_SLIPPAGE = {
  id: "high-slippage-warning",
  policy:
    '@id("high-slippage-warning")\n@severity("warn")\nforbid(principal, action == Amm::Action::"Swap", resource)\nwhen { context.slippageBp > 100 };\n',
  manifest: {
    id: "high-slippage-warning",
    schema_version: 2,
    trigger: { where: { "action.tag": { eq: "swap" } } },
  },
};

const LARGE_SWAP = {
  id: "large-swap-usd-warning",
  policy:
    '@id("large-swap-usd-warning")\n@severity("warn")\nforbid(principal, action == Amm::Action::"Swap", resource)\nwhen { context has custom };\n',
  manifest: {
    id: "large-swap-usd-warning",
    schema_version: 2,
    trigger: { where: { "action.tag": { eq: "swap" } } },
    policy_rpc: [
      {
        id: "total-input-usd",
        method: "oracle.usd_value",
        params: { chain_id: "$.root.chain_id", recipient: "$.action.recipient" },
        outputs: [
          { kind: "context", field: "totalInputUsd", type: "Decimal", from: "$.result.usd" },
        ],
      },
    ],
    custom_context: { fields: { totalInputUsd: "decimal" } },
  },
};

describe("policies-loader-v2 (stateless fetch-and-hold)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.fetchedV2 = JSON.stringify([HIGH_SLIPPAGE, LARGE_SWAP]);
    mocks.fetchOverride = null;
    mocks.managed = [];
    mocks.enabledIds = [];
    vi.resetModules();
  });

  it("loadDefaultPolicySetV2 parses policy-set-v2.json into [{id, policy, manifest}]", async () => {
    const { loadDefaultPolicySetV2 } = await import("../policies-loader-v2");
    const bundles = await loadDefaultPolicySetV2();

    expect(bundles).toHaveLength(2);
    expect(bundles.map((b) => b.id)).toEqual([
      "high-slippage-warning",
      "large-swap-usd-warning",
    ]);
    // Each row carries the verbatim policy text + parsed manifest.
    expect(bundles[0]).toEqual(HIGH_SLIPPAGE);
    expect(bundles[1]).toEqual(LARGE_SWAP);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("fetches the policy-set-v2.json asset URL (not the v1 policy-set.json)", async () => {
    const { loadDefaultPolicySetV2 } = await import("../policies-loader-v2");
    await loadDefaultPolicySetV2();
    expect(fetchMock).toHaveBeenCalledWith(
      "chrome-extension://x/default-policies/policy-set-v2.json",
    );
  });

  it("caches after the first load — a second call does not re-fetch", async () => {
    const { loadDefaultPolicySetV2 } = await import("../policies-loader-v2");
    await loadDefaultPolicySetV2();
    await loadDefaultPolicySetV2();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("returns a defensive copy — mutating the result does not poison the cache", async () => {
    const { loadDefaultPolicySetV2 } = await import("../policies-loader-v2");
    const first = await loadDefaultPolicySetV2();
    first.pop();
    const second = await loadDefaultPolicySetV2();
    expect(second).toHaveLength(2);
  });

  it("getDefaultPolicyBundlesV2 maps the held set to {policy, manifest} (drops id)", async () => {
    const { loadDefaultPolicySetV2, getDefaultPolicyBundlesV2 } = await import(
      "../policies-loader-v2"
    );
    await loadDefaultPolicySetV2();
    const engineBundles = getDefaultPolicyBundlesV2();

    expect(engineBundles).toEqual([
      { policy: HIGH_SLIPPAGE.policy, manifest: HIGH_SLIPPAGE.manifest },
      { policy: LARGE_SWAP.policy, manifest: LARGE_SWAP.manifest },
    ]);
    // `id` is intentionally absent from the WASM arg shape.
    expect(engineBundles.every((b) => !("id" in b))).toBe(true);
  });

  it("getDefaultPolicyBundlesV2 returns [] before the cache is warmed", async () => {
    const { getDefaultPolicyBundlesV2 } = await import("../policies-loader-v2");
    expect(getDefaultPolicyBundlesV2()).toEqual([]);
  });

  it("is best-effort: a non-200 fetch resolves to [] instead of throwing", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    mocks.fetchOverride = new Response("nope", { status: 500 });
    const { loadDefaultPolicySetV2, getDefaultPolicyBundlesV2 } = await import(
      "../policies-loader-v2"
    );
    await expect(loadDefaultPolicySetV2()).resolves.toEqual([]);
    expect(getDefaultPolicyBundlesV2()).toEqual([]);
    warnSpy.mockRestore();
  });

  it("is best-effort: a fetch rejection resolves to [] instead of throwing", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    mocks.fetchOverride = () => {
      throw new Error("network down");
    };
    const { loadDefaultPolicySetV2 } = await import("../policies-loader-v2");
    await expect(loadDefaultPolicySetV2()).resolves.toEqual([]);
    warnSpy.mockRestore();
  });

  // ── Dashboard merge (Option B) ─────────────────────────────────────────

  it("merges dashboard-saved policies after the baked set, with a synthesized {id, schema_version:2} manifest", async () => {
    mocks.managed = [
      {
        id: "dashboard::block-non-usdt",
        kind: "raw",
        text:
          '@id("block-non-usdt")\n@severity("deny")\n' +
          'forbid(principal, action == Amm::Action::"Swap", resource)\n' +
          "when { !(context.tokenOut.key has address && " +
          'context.tokenOut.key.address == "0xdac17f958d2ee523a2206206994597c13d831ec7") };\n',
        updatedAtMs: 0,
        schemaVersion: 1,
      },
    ];
    mocks.enabledIds = ["dashboard::block-non-usdt"];
    const { loadDefaultPolicySetV2 } = await import("../policies-loader-v2");
    const bundles = await loadDefaultPolicySetV2();

    // baked 2 ∪ dashboard 1, dashboard appended after the baked set.
    expect(bundles).toHaveLength(3);
    expect(bundles.slice(0, 2).map((b) => b.id)).toEqual([
      "high-slippage-warning",
      "large-swap-usd-warning",
    ]);
    const dash = bundles[2];
    expect(dash.id).toBe("dashboard::block-non-usdt");
    expect(dash.policy).toContain("forbid(principal");
    // Empty trigger ⇒ matches every action ⇒ Cedar head is the sole filter.
    expect(dash.manifest).toEqual({
      id: "dashboard::block-non-usdt",
      schema_version: 2,
    });
  });

  it("getDefaultPolicyBundlesV2 includes dashboard bundles (id dropped) after warm", async () => {
    mocks.managed = [
      {
        id: "dashboard::x",
        kind: "raw",
        text: "forbid(principal, action, resource);\n",
        updatedAtMs: 0,
        schemaVersion: 1,
      },
    ];
    mocks.enabledIds = ["dashboard::x"];
    const { loadDefaultPolicySetV2, getDefaultPolicyBundlesV2 } = await import(
      "../policies-loader-v2"
    );
    await loadDefaultPolicySetV2();
    const engineBundles = getDefaultPolicyBundlesV2();

    expect(engineBundles).toHaveLength(3);
    expect(engineBundles[2]).toEqual({
      policy: "forbid(principal, action, resource);\n",
      manifest: { id: "dashboard::x", schema_version: 2 },
    });
    expect(engineBundles.every((b) => !("id" in b))).toBe(true);
  });

  it("excludes a dashboard policy that is NOT in the enabled-id allow-list (toggled off)", async () => {
    // Present in `dashboard:policies` but absent from
    // `policy-selection:enabled-ids` ⇒ the user toggled it off ⇒ it must NOT be
    // enforced. (Regression: a disabled policy kept blocking.)
    mocks.managed = [
      {
        id: "dashboard::toggled-off",
        kind: "raw",
        text: "forbid(principal, action, resource);\n",
        updatedAtMs: 0,
        schemaVersion: 1,
      },
    ];
    mocks.enabledIds = []; // nothing enabled
    const { loadDefaultPolicySetV2, getDefaultPolicyBundlesV2 } = await import(
      "../policies-loader-v2"
    );
    const bundles = await loadDefaultPolicySetV2();
    // Only the baked set survives; the disabled dashboard policy is dropped.
    expect(bundles.map((b) => b.id)).toEqual([
      "high-slippage-warning",
      "large-swap-usd-warning",
    ]);
    expect(getDefaultPolicyBundlesV2()).toHaveLength(2);
  });

  it("includes only the ENABLED subset when some dashboard policies are toggled off", async () => {
    mocks.managed = [
      { id: "dashboard::on", kind: "raw", text: "forbid(principal, action, resource);\n", updatedAtMs: 0, schemaVersion: 1 },
      { id: "dashboard::off", kind: "raw", text: "forbid(principal, action, resource);\n", updatedAtMs: 0, schemaVersion: 1 },
    ];
    mocks.enabledIds = ["dashboard::on"]; // 'off' is toggled off
    const { loadDefaultPolicySetV2 } = await import("../policies-loader-v2");
    const bundles = await loadDefaultPolicySetV2();
    expect(bundles.map((b) => b.id)).toEqual([
      "high-slippage-warning",
      "large-swap-usd-warning",
      "dashboard::on",
    ]);
  });

  it("a dashboard storage read error degrades to the baked set (best-effort)", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    // Make `listManaged` throw by handing back a non-array under the key.
    const origGet = mocks.browser.storage.local.get;
    mocks.browser.storage.local.get = (async () => {
      throw new Error("storage unavailable");
    }) as typeof origGet;
    try {
      const { loadDefaultPolicySetV2 } = await import("../policies-loader-v2");
      const bundles = await loadDefaultPolicySetV2();
      // Baked 2 survives; the dashboard read failure is swallowed.
      expect(bundles.map((b) => b.id)).toEqual([
        "high-slippage-warning",
        "large-swap-usd-warning",
      ]);
    } finally {
      mocks.browser.storage.local.get = origGet;
      warnSpy.mockRestore();
    }
  });
});
