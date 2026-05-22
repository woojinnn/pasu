// Drift detection: the bundled `schema/method-catalog.json` MUST stay
// in lockstep with the live daemon catalog (`createMethodRegistry`).
// If they diverge, the dashboard's manifest editor will surface options
// the daemon can't actually serve (or miss options the daemon can).
//
// The fix when this test fails is one of:
//   1. Update `schema/method-catalog.json` to match the daemon (most common).
//   2. Roll back the daemon-side catalog change.
// `node scripts/regen-method-catalog.mjs` (Step 1f, separate PR) will
// automate (1) eventually; for now hand-edit the JSON.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { createMethodRegistry } from "../registry.js";
import type { MethodCatalog } from "../catalog.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const BUNDLED_PATH = resolve(__dirname, "../../../../schema/method-catalog.json");

describe("MethodCatalog bundled JSON ↔ daemon catalog", () => {
  it("daemon catalog matches schema/method-catalog.json byte-for-byte (semantically)", () => {
    const liveCatalog = createMethodRegistry({
      nowMs: () => 0, // doesn't affect catalog shape
    }).catalog();

    const raw = readFileSync(BUNDLED_PATH, "utf8");
    const bundled = JSON.parse(raw) as MethodCatalog;

    // Compare structurally so JSON whitespace/key-order doesn't trip
    // us up — we care about semantic equality.
    expect(normalize(liveCatalog)).toEqual(normalize(bundled));
  });

  it("daemon catalog enumerates every method the registry dispatches", () => {
    const registry = createMethodRegistry();
    const namesFromList = new Set(registry.listMethods());
    const namesFromCatalog = new Set(Object.keys(registry.catalog().methods));
    expect(namesFromCatalog).toEqual(namesFromList);
  });

  it("every catalog entry's name matches its key in the map", () => {
    const catalog = createMethodRegistry().catalog();
    for (const [key, entry] of Object.entries(catalog.methods)) {
      expect(entry.name).toBe(key);
    }
  });
});

/**
 * Sort keys + drop undefineds so deep-equality comparison is stable
 * across the daemon's in-memory shape (where TypeScript may include
 * optional fields as `undefined`) and the JSON shape (where they're
 * absent).
 */
function normalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(normalize);
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(value as Record<string, unknown>).sort()) {
      const v = (value as Record<string, unknown>)[k];
      if (v === undefined) continue;
      out[k] = normalize(v);
    }
    return out;
  }
  return value;
}
