// Registry conflict-resolution tests — verifies that plugin and
// sidecar entries can extend the catalog with new methods but can
// NEVER override a bundled name. The dashboard's bundled
// `method-catalog.json` would otherwise diverge from the live
// registry, and the manifest editor would offer the wrong contract
// for the shadowed method.

import { describe, expect, it, vi } from "vitest";

import { createMethodRegistry } from "../registry.js";
import type {
  LoadedPluginEntry,
} from "../plugin-loader.js";
import type {
  LoadedSidecarEntry,
} from "../sidecar-loader.js";
import type { JsonObject } from "../../types.js";

function fakePluginEntry(name: string, value: number): LoadedPluginEntry {
  return {
    fn: async (): Promise<JsonObject> => ({ value }),
    catalog: {
      name,
      params: {},
      returns: { kind: "scalar", type: "Long", from: "$.result.value" },
      origin: "plugin",
    },
    source: `/fake/plugins/${name}.js`,
  };
}

function fakeSidecarEntry(name: string, value: number): LoadedSidecarEntry {
  return {
    fn: async (): Promise<JsonObject> => ({ value }),
    catalog: {
      name,
      params: {},
      returns: { kind: "scalar", type: "Long", from: "$.result.value" },
      origin: "sidecar",
    },
    source: {
      name: "test-sidecar",
      url: "http://localhost:9001",
      methodPrefix: "risk.",
    },
  };
}

describe("registry plugin + sidecar merge", () => {
  it("plugin entries with new names get registered", async () => {
    const registry = createMethodRegistry({
      pluginEntries: [fakePluginEntry("risk.internal_score", 7)],
    });
    expect(registry.listMethods()).toContain("risk.internal_score");
    const catalog = registry.catalog().methods["risk.internal_score"];
    expect(catalog.origin).toBe("plugin");
    const result = await registry.execute({
      id: "x",
      method: "risk.internal_score",
      params: {},
    });
    expect(result).toEqual({ id: "x", ok: true, result: { value: 7 } });
  });

  it("sidecar entries with new names get registered", async () => {
    const registry = createMethodRegistry({
      sidecarEntries: [fakeSidecarEntry("risk.kyc", 1)],
    });
    expect(registry.listMethods()).toContain("risk.kyc");
    expect(registry.catalog().methods["risk.kyc"].origin).toBe("sidecar");
  });

  it("plugin claiming a bundled name is rejected; bundled wins", async () => {
    const warn = vi.fn();
    const registry = createMethodRegistry({
      pluginEntries: [
        // Plugin tries to override `oracle.usd_value`.
        fakePluginEntry("oracle.usd_value", 99),
      ],
      warn,
    });
    expect(registry.catalog().methods["oracle.usd_value"].origin).toBe(
      "bundled",
    );
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining("bundled method already owns"),
    );
  });

  it("sidecar claiming a bundled name is rejected; bundled wins", async () => {
    const warn = vi.fn();
    const registry = createMethodRegistry({
      sidecarEntries: [fakeSidecarEntry("oracle.usd_value", 99)],
      warn,
    });
    expect(registry.catalog().methods["oracle.usd_value"].origin).toBe(
      "bundled",
    );
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining("already registered"),
    );
  });

  it("sidecar claiming a name a plugin already took is rejected; plugin wins", async () => {
    const warn = vi.fn();
    const registry = createMethodRegistry({
      pluginEntries: [fakePluginEntry("risk.score", 1)],
      sidecarEntries: [fakeSidecarEntry("risk.score", 2)],
      warn,
    });
    expect(registry.catalog().methods["risk.score"].origin).toBe("plugin");
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining("already registered"),
    );
  });

  it("catalog() output stays sorted across bundled + plugin + sidecar", () => {
    const registry = createMethodRegistry({
      pluginEntries: [fakePluginEntry("a.plugin_method", 1)],
      sidecarEntries: [fakeSidecarEntry("z.sidecar_method", 2)],
    });
    const names = Object.keys(registry.catalog().methods);
    expect(names).toEqual([...names].sort());
    // Both extensions appear in the catalog, distinguishable by origin.
    expect(registry.catalog().methods["a.plugin_method"].origin).toBe(
      "plugin",
    );
    expect(registry.catalog().methods["z.sidecar_method"].origin).toBe(
      "sidecar",
    );
  });
});
