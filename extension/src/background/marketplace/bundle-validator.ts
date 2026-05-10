import type JSZip from "jszip";

const ALLOWED_TOPLEVEL = new Set([
  "manifest.json",
  "params.schema.json",
  "README.md",
  "README.txt",
  "LICENSE",
  "LICENSE.md",
  "LICENSE.txt",
  "CHANGELOG.md",
]);
const POLICY_PATH_RE = /^policies\/[A-Za-z0-9_.-]+\.cedar\.tmpl$/;

export interface BundleManifestPaths {
  params_schema: string;
  policies: { file: string }[];
}

/**
 * File-level sandbox enforcement: the zip MUST contain only manifest.json,
 * params.schema.json, optional README/LICENSE, and policies/<name>.cedar.tmpl.
 * Throws on any path traversal or unknown file.
 */
export function validateBundleSandbox(zip: JSZip): void {
  for (const [filePath, file] of Object.entries(zip.files)) {
    if (file.dir) continue;
    if (filePath.endsWith("/")) continue;
    if (filePath.includes("..") || filePath.startsWith("/")) {
      throw new Error(`bundle contains path traversal: ${filePath}`);
    }
    if (POLICY_PATH_RE.test(filePath)) continue;
    if (ALLOWED_TOPLEVEL.has(filePath)) continue;
    throw new Error(
      `bundle violates sandbox: file "${filePath}" not in policies/<name>.cedar.tmpl, ` +
        `manifest.json, params.schema.json, README.md, or LICENSE`,
    );
  }
}

/**
 * Manifest-level invariants checked at install time:
 *  - `params_schema` must literally equal "params.schema.json"
 *  - every `policies[].file` must match policies/<name>.cedar.tmpl
 *
 * Catches bundles that satisfy the file-level sandbox but try to point the
 * manifest at e.g. README.md as their schema.
 */
export function validateBundleManifestPaths(
  manifest: BundleManifestPaths,
): void {
  if (manifest.params_schema !== "params.schema.json") {
    throw new Error(
      `manifest.params_schema must be "params.schema.json", got "${manifest.params_schema}"`,
    );
  }
  for (const p of manifest.policies) {
    if (!POLICY_PATH_RE.test(p.file)) {
      throw new Error(`manifest references non-policy file: "${p.file}"`);
    }
  }
}
