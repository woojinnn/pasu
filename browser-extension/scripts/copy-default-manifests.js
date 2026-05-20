#!/usr/bin/env node
// Copy `schema/policy-schema/extensions/<cat>/<action>.policy-rpc.json`
// into `browser-extension/public/default-manifests/` so the SW dev-seed
// path can fetch them via `Browser.runtime.getURL`.
//
// Each output file is named after the action (`<action>.policy-rpc.json`)
// plus a small `index.json` that lists every manifest so the runtime
// loader doesn't need a directory listing.
//
// Production builds skip this — the dev-seed module bails out when
// `NODE_ENV === "production"`, so shipping the defaults would just bloat
// the prod zip.

const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..", "..");
const SRC = path.resolve(REPO_ROOT, "schema", "policy-schema", "extensions");
const DEST = path.resolve(__dirname, "..", "public", "default-manifests");

function isProd() {
  return process.env.NODE_ENV === "production";
}

function listManifestFiles(dir) {
  const found = [];
  if (!fs.existsSync(dir)) return found;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) found.push(...listManifestFiles(full));
    else if (entry.name.endsWith(".policy-rpc.json")) found.push(full);
  }
  return found.sort();
}

function main() {
  if (isProd()) {
    console.log("[copy-default-manifests] NODE_ENV=production — skipping.");
    return;
  }
  if (!fs.existsSync(SRC)) {
    console.log(`[copy-default-manifests] no source dir at ${SRC} — skipping.`);
    return;
  }
  if (!fs.existsSync(DEST)) fs.mkdirSync(DEST, { recursive: true });

  // Wipe stale files so renamed actions don't leak between builds.
  for (const old of fs.readdirSync(DEST)) {
    fs.rmSync(path.join(DEST, old));
  }

  const files = listManifestFiles(SRC);
  const index = [];
  for (const file of files) {
    // Derive the action from the filename (`<action>.policy-rpc.json`).
    const base = path.basename(file).replace(/\.policy-rpc\.json$/, "");
    const dest = path.join(DEST, `${base}.policy-rpc.json`);
    const raw = fs.readFileSync(file, "utf8");
    // Parse + reserialise so malformed manifests fail the build early.
    const parsed = JSON.parse(raw);
    fs.writeFileSync(dest, JSON.stringify(parsed));
    index.push({ action: base, file: `${base}.policy-rpc.json` });
  }
  fs.writeFileSync(path.join(DEST, "index.json"), JSON.stringify(index));
  console.log(
    `[copy-default-manifests] copied ${index.length} default manifests → ${DEST}`,
  );
}

main();
