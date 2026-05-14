import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    installPolicies: vi.fn(
      async (_input: {
        schema_text: string;
        policy_set: { id: string; text: string }[];
        manifests?: unknown[];
      }) => {},
    ),
    aggregatedPolicySet: vi.fn(
      async () => [] as { id: string; text: string }[],
    ),
    fetchedDefaults: "[]",
    fetchedSchema: "",
    browser: {
      runtime: { getURL: (p: string) => `chrome-extension://x/${p}` },
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
vi.mock("../wasm-bridge", () => ({ installPolicies: mocks.installPolicies }));
vi.mock("../marketplace/storage", () => ({
  aggregatedPolicySet: mocks.aggregatedPolicySet,
  listInstalled: vi.fn(async () => []),
}));

const fetchMock = vi.fn(async (url: string) => {
  if (url.endsWith("policy-set.json"))
    return new Response(mocks.fetchedDefaults);
  return new Response(mocks.fetchedSchema);
});
vi.stubGlobal("fetch", fetchMock);

const A =
  '@id("default::dex/a") @severity("deny") @reason("a") forbid (principal, action, resource);';
const B =
  '@id("default::dex/b") @severity("warn") @reason("b") forbid (principal, action, resource);';
const C =
  '@id("default::dex/c") @severity("warn") @reason("c") forbid (principal, action, resource);';

describe("policies-loader (filtered install)", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    mocks.fetchedDefaults = JSON.stringify([
      { id: "default::dex/a", text: A },
      { id: "default::dex/b", text: B },
      { id: "default::dex/c", text: C },
    ]);
    mocks.fetchedSchema = "";
    mocks.aggregatedPolicySet.mockResolvedValue([]);
    vi.resetModules();
  });

  it("on SW boot, ensureDefaultPoliciesInstalled installs only the storage-enabled subset", async () => {
    mocks.localStore.set("policy-selection:enabled-ids", [
      "default::dex/a",
      "default::dex/c",
    ]);
    const { ensureDefaultPoliciesInstalled } = await import(
      "../policies-loader"
    );
    await ensureDefaultPoliciesInstalled();
    expect(mocks.installPolicies).toHaveBeenCalledTimes(1);
    const call = mocks.installPolicies.mock.calls[0][0];
    expect(call.policy_set.map((p: { id: string }) => p.id).sort()).toEqual([
      "default::dex/a",
      "default::dex/c",
    ]);
  });

  it("passes filtered policy-rpc manifests to WASM install and exposes them as active", async () => {
    const manifestA = { id: "manifest-a", schema_version: 1 };
    const manifestB = { id: "manifest-b", schema_version: 1 };
    mocks.localStore.set("policy-selection:enabled-ids", ["default::dex/a"]);
    mocks.fetchedDefaults = JSON.stringify([
      { id: "default::dex/a", text: A, manifest: manifestA },
      { id: "default::dex/b", text: B, manifest: manifestB },
    ]);

    const { ensureDefaultPoliciesInstalled, getActivePolicyRpcManifests } =
      await import("../policies-loader");
    await ensureDefaultPoliciesInstalled();

    expect(mocks.installPolicies.mock.calls[0][0].manifests).toEqual([
      manifestA,
    ]);
    expect(getActivePolicyRpcManifests()).toEqual([manifestA]);
  });

  it("collects both singular and array policy-rpc manifest entries", async () => {
    const manifestA = { id: "manifest-a", schema_version: 1 };
    const manifestB = { id: "manifest-b", schema_version: 1 };
    const manifestC = { id: "manifest-c", schema_version: 1 };
    mocks.localStore.set("policy-selection:enabled-ids", ["default::dex/a"]);
    mocks.fetchedDefaults = JSON.stringify([
      {
        id: "default::dex/a",
        text: A,
        manifest: manifestA,
        manifests: [manifestB, manifestC],
      },
      { id: "default::dex/b", text: B, manifests: [{ id: "manifest-d" }] },
    ]);

    const { ensureDefaultPoliciesInstalled, getActivePolicyRpcManifests } =
      await import("../policies-loader");
    await ensureDefaultPoliciesInstalled();

    expect(mocks.installPolicies.mock.calls[0][0].manifests).toEqual([
      manifestA,
      manifestB,
      manifestC,
    ]);
    expect(getActivePolicyRpcManifests()).toEqual([
      manifestA,
      manifestB,
      manifestC,
    ]);
  });

  it("on SW boot with no enabled-ids, installs an empty policy_set", async () => {
    const { ensureDefaultPoliciesInstalled } = await import(
      "../policies-loader"
    );
    await ensureDefaultPoliciesInstalled();
    expect(mocks.installPolicies).toHaveBeenCalledTimes(1);
    expect(mocks.installPolicies.mock.calls[0][0].policy_set).toEqual([]);
  });

  it("reinstallAllPolicies(ids) installs exactly the passed ids — does NOT re-read storage", async () => {
    const infoSpy = vi.spyOn(console, "info").mockImplementation(() => {});
    // Set storage to simulate a stale or different value than the ids we pass.
    mocks.localStore.set("policy-selection:enabled-ids", ["default::dex/a"]);
    const { reinstallAllPolicies } = await import("../policies-loader");
    await reinstallAllPolicies(["default::dex/b", "default::dex/c"]);
    expect(mocks.installPolicies).toHaveBeenCalledTimes(1);
    expect(
      mocks.installPolicies.mock.calls[0][0].policy_set
        .map((p: { id: string }) => p.id)
        .sort(),
    ).toEqual(["default::dex/b", "default::dex/c"]);
    expect(infoSpy).toHaveBeenCalledWith("[Scopeball] policies installed", {
      requestedIds: ["default::dex/b", "default::dex/c"],
      installedIds: ["default::dex/b", "default::dex/c"],
      availableCount: 3,
    });
    infoSpy.mockRestore();
  });

  it("reinstallAllPolicies([]) installs an empty policy_set", async () => {
    const { reinstallAllPolicies } = await import("../policies-loader");
    await reinstallAllPolicies([]);
    expect(mocks.installPolicies).toHaveBeenCalledTimes(1);
    expect(mocks.installPolicies.mock.calls[0][0].policy_set).toEqual([]);
  });

  it("clears installed/inflight on rejection so the next call retries", async () => {
    mocks.installPolicies
      .mockRejectedValueOnce(new Error("install_failed: boom"))
      .mockResolvedValueOnce(undefined);
    const { reinstallAllPolicies } = await import("../policies-loader");
    await expect(reinstallAllPolicies(["default::dex/a"])).rejects.toThrow(
      /boom/,
    );
    await reinstallAllPolicies(["default::dex/a"]);
    expect(mocks.installPolicies).toHaveBeenCalledTimes(2);
  });

  it("aggregatedPolicySet contributions are filtered by ids alongside defaults", async () => {
    mocks.aggregatedPolicySet.mockResolvedValue([
      {
        id: "acme::v1/guard",
        text: '@id("acme::v1/guard") @severity("warn") @reason("g") forbid (principal, action, resource);',
      },
    ]);
    const { reinstallAllPolicies } = await import("../policies-loader");
    await reinstallAllPolicies(["default::dex/a", "acme::v1/guard"]);
    expect(mocks.installPolicies).toHaveBeenCalledTimes(1);
    const ids = mocks.installPolicies.mock.calls[0][0].policy_set
      .map((p: { id: string }) => p.id)
      .sort();
    expect(ids).toEqual(["acme::v1/guard", "default::dex/a"]);
  });

  it("ensureDefaultPoliciesInstalled also filters marketplace contributions", async () => {
    mocks.localStore.set("policy-selection:enabled-ids", ["acme::v1/guard"]);
    mocks.aggregatedPolicySet.mockResolvedValue([
      {
        id: "acme::v1/guard",
        text: '@id("acme::v1/guard") @severity("warn") @reason("g") forbid (principal, action, resource);',
      },
    ]);
    const { ensureDefaultPoliciesInstalled } = await import(
      "../policies-loader"
    );
    await ensureDefaultPoliciesInstalled();
    expect(mocks.installPolicies).toHaveBeenCalledTimes(1);
    expect(
      mocks.installPolicies.mock.calls[0][0].policy_set.map(
        (p: { id: string }) => p.id,
      ),
    ).toEqual(["acme::v1/guard"]);
  });

  it("reinstallAllPolicies during a still-resolving ensureDefaultPoliciesInstalled does not let the older IIFE stomp the newer one", async () => {
    mocks.localStore.set("policy-selection:enabled-ids", ["default::dex/a"]);

    // Hold the first installPolicies until we say so.
    let releaseFirst!: () => void;
    const firstStarted = new Promise<void>((resolveStarted) => {
      mocks.installPolicies.mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            resolveStarted();
            releaseFirst = resolve;
          }),
      );
    });
    mocks.installPolicies.mockResolvedValueOnce(undefined); // second call resolves immediately

    const { ensureDefaultPoliciesInstalled, reinstallAllPolicies } =
      await import("../policies-loader");
    const ensureP = ensureDefaultPoliciesInstalled();
    await firstStarted; // ensure's IIFE is parked inside installPolicies
    const reinstallP = reinstallAllPolicies(["default::dex/b"]);
    // Release the older (ensure) call first; new (reinstall) call should
    // already be queued behind it on the WASM side. After both settle,
    // installPolicies must have been called twice and the LAST call's
    // policy_set must be the reinstall ids.
    releaseFirst();
    await Promise.all([ensureP, reinstallP]);

    expect(mocks.installPolicies).toHaveBeenCalledTimes(2);
    const lastCallIds = mocks.installPolicies.mock.calls[1][0].policy_set.map(
      (p: { id: string }) => p.id,
    );
    expect(lastCallIds).toEqual(["default::dex/b"]);
  });
});
