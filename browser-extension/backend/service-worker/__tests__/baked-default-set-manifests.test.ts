import { describe, it, expect } from "vitest";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";

// Regression guard for the manifest-drop bug.
//
// `scripts/copy-default-policies.js::copyDefaultPoliciesV2()` once shipped the
// day1-safety bundles as `{ id, policy }` — WITHOUT each policy's manifest. The
// SW then builds the plan's `manifests = bundles.map(b => b.manifest)`, so a
// manifest-less bundle serializes a `null` element, and the WASM
// `plan_action_rpc_v2_json` (whose `manifests` is `Vec<ManifestV2>`, NON-Option,
// see crates/policy-engine-wasm/src/action_eval_exports.rs) throws
// `invalid type: null, expected struct ManifestV2`. That throw makes
// `tryV2VerdictPath` return undefined → EVERY decoded transaction falls closed to
// a `__engine::no_decoder` warn, so the baked deny/warn policies never fire
// (a hard-deny silently degrades to an approvable warn).
//
// The existing wasm-bridge / policies-loader tests MOCK the WASM + the fetched
// asset, so none of them exercises the actually-shipped `policy-set-v2.json`
// through the engine — which is why the regression shipped unseen. This test
// regenerates the set from source via the real copy script and asserts the
// load-bearing invariant: EVERY shipped v2 bundle carries a manifest OBJECT.

const EXT_ROOT = path.resolve(__dirname, "../../..");

interface ShippedBundle {
  id: string;
  policy: string;
  manifest?: { id?: unknown; schema_version?: unknown } | null;
}

describe("baked default v2 policy set — manifest invariant", () => {
  it("every shipped bundle in policy-set-v2.json carries a non-null manifest", () => {
    // Regenerate from source so the assertion reflects the live copy script, not
    // a possibly-stale build artifact.
    execFileSync("node", ["scripts/copy-default-policies.js"], {
      cwd: EXT_ROOT,
      stdio: "ignore",
    });

    const setPath = path.join(
      EXT_ROOT,
      "public/default-policies/policy-set-v2.json",
    );
    const set = JSON.parse(readFileSync(setPath, "utf8")) as ShippedBundle[];

    expect(set.length).toBeGreaterThan(0);
    for (const b of set) {
      expect(typeof b.id).toBe("string");
      expect(typeof b.policy).toBe("string");
      // The bug was exactly this field being absent → null in the WASM Vec.
      expect(
        b.manifest,
        `bundle "${b.id}" has no manifest — it would serialize a null into the ` +
          `WASM plan's manifests Vec<ManifestV2> and throw, fail-closing every ` +
          `decoded tx to no_decoder. copyDefaultPoliciesV2 must emit {id, policy, manifest}.`,
      ).toBeTruthy();
      expect(typeof b.manifest).toBe("object");
      // A minimal ManifestV2 is `{ id, schema_version: 2 }`; real ones add a
      // trigger / policy_rpc. Either way these two fields must be present + sane.
      expect(b.manifest?.id).toBe(b.id);
      expect(b.manifest?.schema_version).toBe(2);
    }
  });
});
