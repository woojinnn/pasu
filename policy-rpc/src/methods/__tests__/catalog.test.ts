// Drift detection: the bundled `schema/method-catalog.json` must stay in
// lockstep with the live daemon catalog (`createMethodRegistry`) on the
// OPERATIONAL contract (name/description/params/returns/origin). If they
// diverge, the dashboard's manifest editor will surface options the daemon
// can't actually serve (or miss options the daemon can).
//
// The JSON's forward-looking SEMANTIC fields (readKind/server/stateDependency,
// ADR-009 sim-server enrichment) are JSON-only and excluded from the compare
// (see DOC_ONLY_FIELDS) — policy-rpc is being retired, so the daemon does not
// carry them.
//
// The fix when this test fails is one of:
//   1. Update `schema/method-catalog.json` operational fields to match daemon.
//   2. Roll back the daemon-side catalog change.

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
    const bundled = JSON.parse(raw) as MethodCatalog & {
      planned?: Record<string, unknown>;
    };

    // Compare structurally so JSON whitespace/key-order doesn't trip
    // us up — we care about semantic equality. Only the served
    // `methods` map must track the live registry; the sibling
    // `planned` section documents facts the daemon doesn't serve yet
    // (T1 placeholders), so it's intentionally NOT compared here.
    expect(normalize({ methods: liveCatalog.methods })).toEqual(
      normalize({ methods: bundled.methods }),
    );
  });

  it("planned placeholders are documented but NOT served by the registry", () => {
    const raw = readFileSync(BUNDLED_PATH, "utf8");
    const bundled = JSON.parse(raw) as MethodCatalog & {
      planned?: Record<string, { status?: string }>;
    };
    const served = new Set(Object.keys(bundled.methods));
    for (const [name, entry] of Object.entries(bundled.planned ?? {})) {
      // Each planned entry is clearly marked and must not collide with
      // a served method name.
      expect(entry.status).toBe("planned");
      expect(served.has(name)).toBe(false);
    }
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

// Forward-looking SEMANTIC fields that live ONLY in
// `schema/method-catalog.json` (read-kind, serving host, state dependency).
// They document the sim-server fact host (ADR-009); the legacy policy-rpc
// daemon does NOT set them and is being retired, so the catalog's enrichment
// is decoupled from the daemon. The drift test compares only the OPERATIONAL
// contract the daemon owns, so these keys are excluded from the comparison.
const DOC_ONLY_FIELDS = new Set(["readKind", "server", "stateDependency"]);

/**
 * Sort keys + drop undefineds so deep-equality comparison is stable
 * across the daemon's in-memory shape (where TypeScript may include
 * optional fields as `undefined`) and the JSON shape (where they're
 * absent). Also drops the JSON-only `DOC_ONLY_FIELDS` so the bundled
 * catalog may carry richer documentation than the daemon.
 */
function normalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(normalize);
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(value as Record<string, unknown>).sort()) {
      if (DOC_ONLY_FIELDS.has(k)) continue;
      const v = (value as Record<string, unknown>)[k];
      if (v === undefined) continue;
      out[k] = normalize(v);
    }
    return out;
  }
  return value;
}
