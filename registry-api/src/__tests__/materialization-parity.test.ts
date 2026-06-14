/**
 * Phase 0 GATE — bundle-signing parity invariant.
 *
 * The bundle-signing design signs `canonicalize(bundle)` (= the `bundle_sha256`
 * preimage). For the extension's verify to ever succeed, the bytes it canonicalizes
 * at verify time must hash to the SAME `bundle_sha256` the build pipeline computed.
 * Two independent risks could break that, BOTH proven here over the whole corpus:
 *
 *   R1 (canonicalize parity): build-index uses `canonicalize@2`; the extension ships
 *       `canonicalize@3`. This test recomputes every bundle's sha with `canonicalize@3`
 *       and asserts it equals the on-disk `bundle_sha256` (computed with v2). If they
 *       differ, the two majors disagree on real data → every signature would fail.
 *
 *   R2 (materialization parity): for 3-ref (sourced) callkeys the PROXY re-materializes
 *       the bundle at request time via its OWN `materializeSourceBundle` (server.ts),
 *       a separate implementation from build-index's resolver. The proxy copies
 *       `entry.bundle_sha256` WITHOUT recomputation, so proxy-materialized ≠ build-
 *       materialized has never been asserted anywhere (the existing server test uses a
 *       dummy sha). This sweep recomputes the sha of the EXACT object the proxy would
 *       serve and asserts it matches.
 *
 * If this gate is GREEN, signing every served bundle is sound. If it RED, the failure
 * breakdown (concrete / ref-template / ref-materialized) tells us which assumption broke
 * and gates the rollout scope decision (plan C1).
 */
import { createHash } from "node:crypto";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import canonicalize from "canonicalize";
import {
  isRefRegistryEntry,
  materializeSourceBundle,
  type SourceContextDocument,
} from "../server";

const HERE = dirname(fileURLToPath(import.meta.url));
// REGISTRY_ROOT mirrors build-index's: registryV2/ holds the built index/, bundles/,
// contexts/. From registry-api/src/__tests__/ that is ../../../registryV2.
const REGISTRY_ROOT = resolve(HERE, "../../../registryV2");
const INDEX_ROOT = join(REGISTRY_ROOT, "index");

function sha256Hex(s: string): string {
  return "0x" + createHash("sha256").update(s, "utf8").digest("hex");
}

function walkJson(dir: string): string[] {
  const out: string[] = [];
  for (const ent of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, ent.name);
    if (ent.isDirectory()) out.push(...walkJson(full));
    else if (ent.name.endsWith(".json")) out.push(full);
  }
  return out;
}

const readCache = new Map<string, unknown>();
function readGenerated(ref: string): unknown {
  const path = join(REGISTRY_ROOT, ref);
  if (!readCache.has(path)) {
    readCache.set(path, JSON.parse(readFileSync(path, "utf8")));
  }
  return readCache.get(path);
}

/** The exact object the proxy serves as `response.bundle` for an index entry,
 * reproduced from the on-disk corpus exactly as registry-api/server.ts does. */
function servedBundle(entry: unknown): {
  bundle: unknown;
  kind: "concrete" | "ref-template" | "ref-materialized";
} {
  if (isRefRegistryEntry(entry)) {
    const template = readGenerated(entry.bundle_ref);
    if (entry.context_ref === undefined) {
      return { bundle: template, kind: "ref-template" };
    }
    const contextDoc = readGenerated(entry.context_ref) as SourceContextDocument;
    return {
      bundle: materializeSourceBundle(template, contextDoc),
      kind: "ref-materialized",
    };
  }
  return { bundle: (entry as { bundle: unknown }).bundle, kind: "concrete" };
}

interface Mismatch {
  file: string;
  kind: string;
  claimed: string;
  recomputed: string;
}

describe("Phase 0 gate — canonicalize + materialization parity (R1 + R2)", () => {
  const files = walkJson(INDEX_ROOT);

  it("the built corpus is present and non-trivial", () => {
    expect(files.length).toBeGreaterThan(100);
  });

  it("every served bundle hashes (canonicalize@3) to its on-disk bundle_sha256", () => {
    const mismatches: Mismatch[] = [];
    const byKind: Record<string, number> = {
      concrete: 0,
      "ref-template": 0,
      "ref-materialized": 0,
    };

    for (const file of files) {
      const entry = JSON.parse(readFileSync(file, "utf8")) as {
        bundle_sha256?: string;
      };
      const claimed = entry.bundle_sha256;
      if (typeof claimed !== "string") continue; // not a bundle-bearing entry
      const { bundle, kind } = servedBundle(entry);
      byKind[kind] = (byKind[kind] ?? 0) + 1;

      const canonical = canonicalize(bundle);
      if (typeof canonical !== "string") {
        mismatches.push({
          file,
          kind,
          claimed,
          recomputed: "<canonicalize returned non-string>",
        });
        continue;
      }
      const recomputed = sha256Hex(canonical);
      if (recomputed !== claimed) {
        mismatches.push({
          file: file.slice(REGISTRY_ROOT.length + 1),
          kind,
          claimed,
          recomputed,
        });
      }
    }

    if (mismatches.length > 0) {
      const breakdown = mismatches.reduce<Record<string, number>>((acc, m) => {
        acc[m.kind] = (acc[m.kind] ?? 0) + 1;
        return acc;
      }, {});
      const sample = mismatches.slice(0, 8);
      throw new Error(
        `PARITY GATE FAILED — ${mismatches.length}/${files.length} entries drift.\n` +
          `swept by kind: ${JSON.stringify(byKind)}\n` +
          `drift by kind: ${JSON.stringify(breakdown)}\n` +
          `(concrete drift ⇒ R1 canonicalize v2≠v3; ref-* drift ⇒ R2 proxy materialize ≠ build)\n` +
          `samples:\n${sample
            .map((m) => `  [${m.kind}] ${m.file}\n    claimed=${m.claimed}\n    recomp =${m.recomputed}`)
            .join("\n")}`,
      );
    }

    // Sanity: we actually exercised all three categories present in the corpus.
    expect(byKind.concrete).toBeGreaterThan(0);
    expect(byKind["ref-materialized"]).toBeGreaterThan(0);
  }, 180_000);
});
