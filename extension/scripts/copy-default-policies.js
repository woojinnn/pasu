#!/usr/bin/env node
// Copy the engine's default policy set + composed schema into
// extension/public/default-policies/ so the SW can fetch them at install
// time. Plan 6 will replace this static set with marketplace bundles.

const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..", "..");
const DEST = path.resolve(__dirname, "..", "public", "default-policies");

function read(rel) {
  return fs.readFileSync(path.join(REPO_ROOT, rel), "utf8");
}

function listCedarFiles(dir) {
  const files = [];
  function walk(d) {
    for (const entry of fs.readdirSync(d, { withFileTypes: true })) {
      const full = path.join(d, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.name.endsWith(".cedar")) files.push(full);
    }
  }
  walk(dir);
  return files.sort(); // deterministic order so the policy-set.json hashes stably
}

function main() {
  if (!fs.existsSync(DEST)) fs.mkdirSync(DEST, { recursive: true });

  const schemaParts = [
    "policy-schema/core.cedarschema",
    "policy-schema/actions/dex.cedarschema",
    "policy-schema/actions/other.cedarschema",
    "policy-schema/actions/permit2.cedarschema",
    "policy-schema/actions/eip2612.cedarschema",
    "policy-schema/actions/eip712_other.cedarschema",
    "policy-schema/actions/signature_base.cedarschema",
  ].filter((rel) => fs.existsSync(path.join(REPO_ROOT, rel)));
  const schema = schemaParts.map(read).join("\n\n");
  fs.writeFileSync(path.join(DEST, "schema.cedarschema"), schema);

  const policiesDir = path.join(REPO_ROOT, "policies");
  if (fs.existsSync(policiesDir)) {
    const files = listCedarFiles(policiesDir);
    const policySet = files.map((f) => ({
      id: `default::${path
        .relative(policiesDir, f)
        .replace(/\\/g, "/")
        .replace(/\.cedar$/, "")}`,
      text: fs.readFileSync(f, "utf8"),
    }));
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
