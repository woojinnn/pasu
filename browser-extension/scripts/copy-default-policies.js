#!/usr/bin/env node
// Copy the engine's default policy set + composed schema into
// extension/public/default-policies/ so the SW can fetch them at install
// time. Plan 6 will replace this static set with marketplace bundles.
//
// Also copies declarative adapter seed bundles (Phase 1B of the Adapter
// Marketplace PoC) from `crates/adapters/mappers/tests/fixtures/*.json`
// into `extension/public/seed-bundles/<bundle_id>.json` so the Rust fixture
// stays the single source of truth — the SW fetches these at boot via
// `ensureSeedBundlesInstalled()`.

const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..", "..");
const DEST = path.resolve(__dirname, "..", "public", "default-policies");
const SEED_BUNDLES_DEST = path.resolve(
  __dirname,
  "..",
  "public",
  "seed-bundles",
);

// Source → on-disk seed bundle filename. The filename is derived from the
// bundle's `id` field (`<path>@<version>`) so the on-disk name matches the
// canonical bundle id with `/` → `-`.
const SEED_BUNDLE_SOURCES = [
  path.join(
    REPO_ROOT,
    "crates",
    "adapters",
    "mappers",
    "tests",
    "fixtures",
    "uniswap-v2-swap-exact-tokens.json",
  ),
];

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

function copySeedBundles() {
  if (!fs.existsSync(SEED_BUNDLES_DEST)) {
    fs.mkdirSync(SEED_BUNDLES_DEST, { recursive: true });
  }
  // Wipe stale bundles so renames don't leave orphans in /dist.
  for (const entry of fs.readdirSync(SEED_BUNDLES_DEST)) {
    if (entry.endsWith(".json")) {
      fs.unlinkSync(path.join(SEED_BUNDLES_DEST, entry));
    }
  }
  let copied = 0;
  for (const src of SEED_BUNDLE_SOURCES) {
    if (!fs.existsSync(src)) continue;
    const raw = fs.readFileSync(src, "utf8");
    const bundle = JSON.parse(raw);
    if (typeof bundle.id !== "string" || !bundle.id.includes("@")) {
      throw new Error(
        `Seed bundle ${src} missing canonical "id" field (must be "<path>@<version>")`,
      );
    }
    // `id` of "uniswap/v2/swapExactTokensForTokens@1.0.0" → on-disk
    // "uniswap-v2-swap@1.0.0.json" wouldn't be reversible; preserve the id
    // verbatim except for "/" → "-" so the SW can still read it without
    // URL escaping path separators.
    const safeName = bundle.id.replace(/\//g, "-") + ".json";
    fs.writeFileSync(path.join(SEED_BUNDLES_DEST, safeName), raw);
    copied += 1;
  }
  console.log(`Copied ${copied} seed bundles → ${SEED_BUNDLES_DEST}`);
}

function main() {
  if (!fs.existsSync(DEST)) fs.mkdirSync(DEST, { recursive: true });

  const schemaParts = listSchemaFiles();
  const schema = schemaParts
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n\n");
  fs.writeFileSync(path.join(DEST, "schema.cedarschema"), schema);

  const policiesDir = path.join(REPO_ROOT, "policy-examples");
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
    console.log(`Wrote empty policy-set.json (no policy-examples/ dir found)`);
  }

  copySeedBundles();
}

main();
