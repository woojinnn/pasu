#!/usr/bin/env node
// Copy the engine's default policy set + composed schema into
// extension/public/default-policies/ so the SW can fetch them at install
// time. Plan 6 will replace this static set with marketplace bundles.

const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..", "..");
const EXT_ROOT = path.resolve(__dirname, "..");
const DEST = path.resolve(__dirname, "..", "public", "default-policies");

function listFilesWithExtension(dir, extension) {
  const files = [];
  function walk(d) {
    for (const entry of fs.readdirSync(d, { withFileTypes: true })) {
      const full = path.join(d, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.name.endsWith(extension)) files.push(full);
    }
  }
  walk(dir);
  return files.sort(); // deterministic order so the policy-set.json hashes stably
}

function listCedarFiles(dir) {
  return listFilesWithExtension(dir, ".cedar");
}

function listSchemaFiles() {
  const files = [];
  const core = path.join(REPO_ROOT, "schema", "policy-schema", "core.cedarschema");
  if (fs.existsSync(core)) files.push(core);

  const actionsDir = path.join(REPO_ROOT, "schema", "policy-schema", "actions");
  if (fs.existsSync(actionsDir)) {
    files.push(...listFilesWithExtension(actionsDir, ".cedarschema"));
  }

  return files;
}

// Phase 1 / P2 — emit the default v2 policy set alongside the v1
// `policy-set.json`. v2 is STATELESS: the SW holds these bundles in memory
// and passes them INLINE to `evaluate_action_v2_json` per call (no install
// step). The canonical source of truth is the Rust fixture dir
// `crates/policy-engine/tests/fixtures/default_policies_v2/<id>/{manifest.json,
// policy.cedar}`, proven consistent by `default_policies_v2.rs`. We enumerate
// DIRECTORIES (not `.cedar` files), sort for byte-stable output, and ship the
// policy text + manifest verbatim (no JS-side transform — validity is the
// Rust fixture gate's job).
function copyDefaultPoliciesV2() {
  const destPath = path.join(DEST, "policy-set-v2.json");

  // Ship the "day1-safety" bundle as the baked default v2 set. The bundle lives
  // under `default-bundles/day1-safety/` (package.json + policies/<id>/{policy.cedar,
  // manifest.json}); we project each policy onto the `{id, policy, manifest}` shape
  // that `loadBakedSetV2()` / `evaluate_action_v2_json` consume.
  //
  // The `manifest` MUST be included: WASM `PlanActionInput.manifests` and
  // `BundleInput.manifest` are `Vec<ManifestV2>` / `ManifestV2` (NON-Option). The
  // SW builds the plan's `manifests = bundles.map(b => b.manifest)`, so a bundle
  // with no `manifest` serializes a `null` element and `plan_action_rpc_v2_json`
  // throws `invalid type: null, expected struct ManifestV2`. That throw makes
  // `tryV2VerdictPath` return undefined → EVERY decoded tx fails closed to a
  // `__engine::no_decoder` warn, so the baked deny/warn policies never fire.
  // Dropping the manifest here (the old `{id, policy}` projection) was that bug.
  // Mirrors the v1 path below + the dashboard's `managedToV2Bundle` synthesis.
  // Enumerate via package.json's `policies[]` so order + id match the bundle's
  // canonical manifest; fall back to dir-scan if package.json is absent.
  const bundleDir = path.join(EXT_ROOT, "default-bundles", "day1-safety");
  if (!fs.existsSync(bundleDir)) {
    fs.writeFileSync(destPath, "[]");
    console.log("Wrote empty policy-set-v2.json (default-bundles/day1-safety not found)");
    return;
  }

  const pkgPath = path.join(bundleDir, "package.json");
  let ids;
  if (fs.existsSync(pkgPath)) {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
    ids = (pkg.policies || []).map((p) => p.id);
  } else {
    ids = fs
      .readdirSync(path.join(bundleDir, "policies"), { withFileTypes: true })
      .filter((e) => e.isDirectory())
      .map((e) => e.name)
      .sort();
  }

  const policySet = ids.map((id) => {
    const policyDir = path.join(bundleDir, "policies", id);
    const policy = fs.readFileSync(path.join(policyDir, "policy.cedar"), "utf8");
    // Include the policy's manifest so the shipped bundle is `{id, policy, manifest}`.
    // Fall back to a minimal empty-trigger manifest (`{id, schema_version: 2}`,
    // matches every action — the Cedar head self-filters) when a policy has no
    // manifest.json, so the set can NEVER contain a manifest-less (null) bundle.
    const manifestPath = path.join(policyDir, "manifest.json");
    const manifest = fs.existsSync(manifestPath)
      ? JSON.parse(fs.readFileSync(manifestPath, "utf8"))
      : { id, schema_version: 2 };
    return { id, policy, manifest };
  });

  fs.writeFileSync(destPath, JSON.stringify(policySet, null, 2));
  console.log(
    `Wrote policy-set-v2.json with ${policySet.length} day1-safety policies → ${destPath}`,
  );
}

function main() {
  if (!fs.existsSync(DEST)) fs.mkdirSync(DEST, { recursive: true });

  const schemaParts = listSchemaFiles();
  const schema = schemaParts
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n\n");
  fs.writeFileSync(path.join(DEST, "schema.cedarschema"), schema);

  const policiesDir = path.join(REPO_ROOT, "policy-rpc", "examples", "policies");
  if (fs.existsSync(policiesDir)) {
    const files = listCedarFiles(policiesDir);
    const policySet = files.map((f) => {
      const entry = {
        id: `default::${path
          .relative(policiesDir, f)
          .replace(/\\/g, "/")
          .replace(/\.cedar$/, "")}`,
        text: fs.readFileSync(f, "utf8"),
      };
      const manifestPath = f.replace(/\.cedar$/, ".policy-rpc.json");
      if (fs.existsSync(manifestPath)) {
        const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
        if (Array.isArray(manifest)) entry.manifests = manifest;
        else entry.manifest = manifest;
      }
      return entry;
    });
    fs.writeFileSync(
      path.join(DEST, "policy-set.json"),
      JSON.stringify(policySet, null, 2),
    );
    console.log(
      `Copied ${schemaParts.length} schema parts + ${policySet.length} policies → ${DEST}`,
    );
  } else {
    fs.writeFileSync(path.join(DEST, "policy-set.json"), "[]");
    console.log(
      `Wrote empty policy-set.json (no policy-rpc/examples/policies/ dir found)`,
    );
  }

  copyDefaultPoliciesV2();
}

main();
