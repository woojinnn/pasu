import { describe, expect, it } from "vitest";
import JSZip from "jszip";
import {
  validateBundleManifestPaths,
  validateBundleSandbox,
} from "../bundle-validator";

async function makeZip(files: Record<string, string>): Promise<JSZip> {
  const zip = new JSZip();
  for (const [k, v] of Object.entries(files)) zip.file(k, v);
  return JSZip.loadAsync(await zip.generateAsync({ type: "uint8array" }));
}

describe("validateBundleSandbox", () => {
  it("accepts the canonical bundle layout", async () => {
    const zip = await makeZip({
      "manifest.json": "{}",
      "params.schema.json": "{}",
      "policies/foo.cedar.tmpl": "permit(...);",
      "README.md": "# bundle",
    });
    expect(() => validateBundleSandbox(zip)).not.toThrow();
  });

  it("rejects schema fragments", async () => {
    const zip = await makeZip({
      "manifest.json": "{}",
      "params.schema.json": "{}",
      "schema-extensions/x.cedarschema": "entity Foo;",
    });
    expect(() => validateBundleSandbox(zip)).toThrow(/sandbox/);
  });

  it("rejects path traversal", async () => {
    const zip = new JSZip();
    zip.file("manifest.json", "{}");
    zip.file("params.schema.json", "{}");
    // JSZip lets us register the path verbatim.
    zip.file("../etc/passwd", "evil");
    const reloaded = await JSZip.loadAsync(
      await zip.generateAsync({ type: "uint8array" }),
    );
    expect(() => validateBundleSandbox(reloaded)).toThrow();
  });

  it("rejects scripts in policies/", async () => {
    const zip = await makeZip({
      "manifest.json": "{}",
      "params.schema.json": "{}",
      "policies/install.js": 'window.alert("pwned")',
    });
    expect(() => validateBundleSandbox(zip)).toThrow(/sandbox/);
  });
});

describe("validateBundleManifestPaths", () => {
  it("accepts canonical manifest", () => {
    expect(() =>
      validateBundleManifestPaths({
        params_schema: "params.schema.json",
        policies: [{ file: "policies/cap.cedar.tmpl" }],
      }),
    ).not.toThrow();
  });

  it("rejects manifest pointing params_schema at README", () => {
    expect(() =>
      validateBundleManifestPaths({
        params_schema: "README.md",
        policies: [],
      }),
    ).toThrow(/params_schema/);
  });

  it("rejects policy paths outside the policies/ tree", () => {
    expect(() =>
      validateBundleManifestPaths({
        params_schema: "params.schema.json",
        policies: [{ file: "README.md" }],
      }),
    ).toThrow(/non-policy/);
  });
});
