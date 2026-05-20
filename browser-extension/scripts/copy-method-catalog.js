#!/usr/bin/env node
// Copy `schema/method-catalog.json` into `browser-extension/public/`
// so the dashboard can fetch it at runtime via
// `Browser.runtime.getURL("method-catalog.json")`.
//
// The catalog is the bundled "what methods does this extension know
// about" snapshot — the dashboard uses it as the default for the
// manifest editor's method/param/return dropdowns. When the user
// configures a policy-rpc endpoint, the dashboard ALSO fetches that
// daemon's `GET /v1/methods` and merges the two so user-added methods
// (plugins) show up alongside.
//
// Unlike `copy-default-manifests.js`, this runs in BOTH dev and prod
// builds: even prod manifests need the catalog to drive the editor
// UX. The catalog itself is small (~5KB) so the bundle hit is
// negligible.

const fs = require("fs");
const path = require("path");

const REPO_ROOT = path.resolve(__dirname, "..", "..");
const SRC = path.resolve(REPO_ROOT, "schema", "method-catalog.json");
const DEST_DIR = path.resolve(__dirname, "..", "public");
const DEST = path.join(DEST_DIR, "method-catalog.json");

function main() {
  if (!fs.existsSync(SRC)) {
    console.warn(`[copy-method-catalog] source not found at ${SRC} — skipping.`);
    return;
  }
  if (!fs.existsSync(DEST_DIR)) fs.mkdirSync(DEST_DIR, { recursive: true });

  const raw = fs.readFileSync(SRC, "utf8");
  // Parse + reserialise so malformed catalogs fail the build early
  // (we'd rather break here than at runtime in the dashboard).
  const parsed = JSON.parse(raw);
  fs.writeFileSync(DEST, JSON.stringify(parsed));
  console.log(
    `[copy-method-catalog] copied ${Object.keys(parsed.methods).length} method entries → ${DEST}`,
  );
}

main();
