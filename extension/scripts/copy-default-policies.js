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
  const core = path.join(REPO_ROOT, "policy-schema", "core.cedarschema");
  if (fs.existsSync(core)) files.push(core);

  const actionsDir = path.join(REPO_ROOT, "policy-schema", "actions");
  if (fs.existsSync(actionsDir)) {
    files.push(...listFilesWithExtension(actionsDir, ".cedarschema"));
  }

  return files;
}

function main() {
  if (!fs.existsSync(DEST)) fs.mkdirSync(DEST, { recursive: true });

  const schemaParts = listSchemaFiles();
  const schema = schemaParts
    .map((file) => fs.readFileSync(file, "utf8"))
    .join("\n\n");
  fs.writeFileSync(path.join(DEST, "schema.cedarschema"), schema);

  const policiesDir = path.join(REPO_ROOT, "policies");
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
    console.log(`Wrote empty policy-set.json (no policies/ dir found)`);
  }
}

main();
