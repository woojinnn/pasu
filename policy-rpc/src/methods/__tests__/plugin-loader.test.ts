// Plugin loader tests — exercise the in-process discovery path
// without touching the real filesystem. We stub `readdir` and the
// dynamic-import function so every test runs in milliseconds and
// stays deterministic across machines.

import { describe, expect, it, vi } from "vitest";

import { loadPluginEntries } from "../plugin-loader.js";
import type { InProcessPlugin, MethodCatalogEntry } from "../catalog.js";

function makePlugin(name: string): InProcessPlugin {
  const catalog: MethodCatalogEntry = {
    name,
    params: {
      target: { type: "String", required: true },
    },
    returns: { kind: "scalar", type: "Long", from: "$.result.value" },
    origin: "plugin",
  };
  return {
    catalog,
    execute: async () => ({ value: 42 }),
  };
}

describe("loadPluginEntries", () => {
  it("returns [] when the plugin directory doesn't exist", async () => {
    const entries = await loadPluginEntries({
      dir: "/tmp/definitely-not-here",
      exists: () => false,
    });
    expect(entries).toEqual([]);
  });

  it("imports every valid plugin file and forces origin='plugin'", async () => {
    const entries = await loadPluginEntries({
      dir: "/fake/plugins",
      exists: () => true,
      readdir: vi.fn(async () => ["risk-score.js", "kyc.mjs"]) as any,
      importModule: async (url) => {
        // Two synthetic plugins; default-export shape matches what
        // a `export default plugin` would produce.
        if (url.endsWith("risk-score.js")) {
          return { default: makePlugin("risk.score") };
        }
        return { default: makePlugin("compliance.kyc") };
      },
    });
    expect(entries.map((e) => e.catalog.name).sort()).toEqual([
      "compliance.kyc",
      "risk.score",
    ]);
    // Even if a plugin tried to claim `origin: "bundled"`, the loader
    // forces it back so the dashboard's badge logic stays trustworthy.
    expect(entries.every((e) => e.catalog.origin === "plugin")).toBe(true);
  });

  it("skips files starting with _ or . (private / hidden)", async () => {
    const importSpy = vi.fn(async () => ({ default: makePlugin("ok.method") }));
    const entries = await loadPluginEntries({
      dir: "/fake/plugins",
      exists: () => true,
      readdir: vi.fn(async () => [
        "_internal.js",
        ".dotfile.js",
        "ok.js",
        "README.md",
      ]) as any,
      importModule: importSpy,
    });
    expect(entries.map((e) => e.catalog.name)).toEqual(["ok.method"]);
    expect(importSpy).toHaveBeenCalledTimes(1);
  });

  it("warns and skips when a plugin throws during import", async () => {
    const warn = vi.fn();
    const entries = await loadPluginEntries({
      dir: "/fake/plugins",
      exists: () => true,
      readdir: vi.fn(async () => ["broken.js", "ok.js"]) as any,
      importModule: async (url) => {
        if (url.endsWith("broken.js")) {
          throw new Error("syntax error");
        }
        return { default: makePlugin("ok.method") };
      },
      warn,
    });
    expect(entries.map((e) => e.catalog.name)).toEqual(["ok.method"]);
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining("plugin import failed"),
    );
  });

  it("warns and skips when the default export is malformed", async () => {
    const warn = vi.fn();
    const entries = await loadPluginEntries({
      dir: "/fake/plugins",
      exists: () => true,
      readdir: vi.fn(async () => [
        "no-execute.js",
        "no-catalog.js",
        "ok.js",
      ]) as any,
      importModule: async (url) => {
        if (url.endsWith("no-execute.js")) {
          return {
            default: { catalog: { name: "x", params: {}, returns: {} } },
          };
        }
        if (url.endsWith("no-catalog.js")) {
          return { default: { execute: async () => ({}) } };
        }
        return { default: makePlugin("ok.method") };
      },
      warn,
    });
    expect(entries.map((e) => e.catalog.name)).toEqual(["ok.method"]);
    // One warn per bad file.
    expect(warn).toHaveBeenCalledTimes(2);
  });
});
