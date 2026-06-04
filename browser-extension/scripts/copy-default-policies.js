#!/usr/bin/env node
// Copy the engine's default policy set + composed schema into
// extension/public/default-policies/ so the SW can fetch them at install
// time. Plan 6 will replace this static set with marketplace bundles.

const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..", "..");
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
  const v2Dir = path.join(
    REPO_ROOT,
    "crates",
    "policy-engine",
    "tests",
    "fixtures",
    "default_policies_v2",
  );
  const destPath = path.join(DEST, "policy-set-v2.json");

  if (!fs.existsSync(v2Dir)) {
    // Mirror the v1 `[]` fallback so a release build with the fixture
    // pruned still produces a parseable (empty) asset and never bricks
    // `prepare:defaults`.
    fs.writeFileSync(destPath, "[]");
    console.log(
      `Wrote empty policy-set-v2.json (no default_policies_v2/ dir found)`,
    );
    return;
  }

  // A dir is a BUNDLE iff it directly holds a manifest.json; any other dir
  // (`phaseN/`, `phase1/A/`, …) is a grouping dir, recursed at ANY depth.
  // Supports flat `<v2Dir>/<id>/`, phased `<v2Dir>/<phaseN>/<id>/`, and nested
  // `<v2Dir>/<phaseN>/<sub>/<id>/` layouts alike. The emitted `id` is always the
  // bundle (policy) dir name, so the shipped asset stays a FLAT
  // `{id, policy, manifest}[]` regardless of nesting.
  function collectBundles(root) {
    const out = [];
    function walk(dir) {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        if (!entry.isDirectory()) continue;
        const full = path.join(dir, entry.name);
        if (fs.existsSync(path.join(full, "manifest.json"))) {
          // A bundle dir. Skip BLOCKED-BY-ACTION bundles (target an action
          // surface not yet in the schema, e.g. x402 Erc3009TransferWithAuth) —
          // they must not ship to the extension until that surface lands.
          const cedar = path.join(full, "policy.cedar");
          const blocked =
            fs.existsSync(cedar) &&
            fs.readFileSync(cedar, "utf8").includes("// BLOCKED-BY-ACTION");
          if (!blocked) out.push({ id: entry.name, dir: full });
        } else {
          walk(full);
        }
      }
    }
    walk(root);
    return out;
  }

  // Sort by id with plain string comparison (matches the previous `.sort()` on
  // dir names) so the asset hashes stably across builds.
  const bundles = collectBundles(v2Dir).sort((a, b) =>
    a.id < b.id ? -1 : a.id > b.id ? 1 : 0,
  );

  const set = bundles.map(({ id, dir }) => ({
    id,
    policy: fs.readFileSync(path.join(dir, "policy.cedar"), "utf8"),
    manifest: JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8")),
  }));

  fs.writeFileSync(destPath, JSON.stringify(set, null, 2));
  console.log(`Copied ${set.length} v2 policy bundles → ${DEST}`);
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
