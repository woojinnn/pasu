/**
 * sign-bundles.test.ts — registryV2 has no vitest of its own; run from
 * browser-extension/ with its bundled vitest, pointing --root at registryV2:
 *
 *   cd browser-extension
 *   node .yarn/releases/yarn-4.14.1.cjs vitest run \
 *     --root ../registryV2 scripts/__tests__/sign-bundles.test.ts
 *
 * Proves the sign step end-to-end with the REAL crypto the extension verifies
 * with: a locally-signed `.sig` (P1363 over the bundle_sha256 digest) verifies
 * under WebCrypto `subtle.verify({ECDSA,SHA-256}, spkiPinnedKey, sig, canonicalize(bundle))`.
 */
import { createHash, webcrypto } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import canonicalize from "canonicalize";
import { p256 } from "@noble/curves/nist.js";
import {
  SIG_ALG,
  collectBundleShas,
  publicKeySpkiBase64,
  signBundles,
} from "../sign-bundles.js";

const subtle = webcrypto.subtle;

function sha256Hex(s: string): string {
  return "0x" + createHash("sha256").update(s, "utf8").digest("hex");
}

function bundleSha(bundle: unknown): string {
  return sha256Hex(canonicalize(bundle) as string);
}

let root: string;

/** Build a temp registry root with by-callkey index entries (concrete-inline). */
function seedRoot(entries: { seg: string; bundle: Record<string, unknown> }[]): void {
  const dir = join(root, "index", "by-callkey");
  mkdirSync(dir, { recursive: true });
  for (const { seg, bundle } of entries) {
    const entry = {
      matched: true,
      bundle_id: String(bundle.id ?? seg),
      manifest_path: `manifests/${seg}.json`,
      bundle_sha256: bundleSha(bundle),
      bundle,
    };
    writeFileSync(join(dir, `${seg}.json`), JSON.stringify(entry, null, 2) + "\n");
  }
}

function readSig(sha: string): { alg: string; key_id: string; sig_b64: string } {
  return JSON.parse(readFileSync(join(root, "signatures", `${sha}.sig`), "utf8"));
}

// A fresh ephemeral signing key per test run.
let privHex: string;
let spkiB64: string;

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "signtest-"));
  privHex = Buffer.from(p256.utils.randomSecretKey()).toString("hex");
  spkiB64 = publicKeySpkiBase64(privHex);
});
afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

const A = { type: "adapter_action", id: "a@1", schema_version: "3", x: 1 };
const B = { type: "adapter_action", id: "b@1", schema_version: "3", y: 2 };

describe("sign-bundles", () => {
  it("signs each unique bundle_sha256 → one .sig per unique sha", async () => {
    seedRoot([
      { seg: "1__0xaa__0x01", bundle: A },
      { seg: "1__0xbb__0x02", bundle: B },
    ]);
    const r = await signBundles({ registryRoot: root, mode: "local", privKeyHex: privHex });
    expect(r.total).toBe(2);
    expect(r.signed).toBe(2);
    expect(existsSync(join(root, "signatures", `${bundleSha(A)}.sig`))).toBe(true);
    expect(existsSync(join(root, "signatures", `${bundleSha(B)}.sig`))).toBe(true);
    const sig = readSig(bundleSha(A));
    expect(sig.alg).toBe(SIG_ALG);
    expect(typeof sig.sig_b64).toBe("string");
  });

  it("dedups: two callkeys sharing a bundle produce ONE .sig", async () => {
    seedRoot([
      { seg: "1__0xaa__0x01", bundle: A },
      { seg: "10__0xcc__0x01", bundle: A }, // same bundle, different callkey
    ]);
    const r = await signBundles({ registryRoot: root, mode: "local", privKeyHex: privHex });
    expect(collectBundleShas(root)).toEqual([bundleSha(A)]);
    expect(r.total).toBe(1);
    expect(r.signed).toBe(1);
  });

  it("is idempotent: a second run skips existing .sig", async () => {
    seedRoot([{ seg: "1__0xaa__0x01", bundle: A }]);
    await signBundles({ registryRoot: root, mode: "local", privKeyHex: privHex });
    const second = await signBundles({ registryRoot: root, mode: "local", privKeyHex: privHex });
    expect(second.signed).toBe(0);
    expect(second.skipped).toBe(1);
  });

  it("--force re-signs existing", async () => {
    seedRoot([{ seg: "1__0xaa__0x01", bundle: A }]);
    await signBundles({ registryRoot: root, mode: "local", privKeyHex: privHex });
    const forced = await signBundles({
      registryRoot: root,
      mode: "local",
      privKeyHex: privHex,
      force: true,
    });
    expect(forced.signed).toBe(1);
    expect(forced.skipped).toBe(0);
  });

  it("the .sig verifies under WebCrypto over canonicalize(bundle) with the pinned SPKI key", async () => {
    seedRoot([{ seg: "1__0xaa__0x01", bundle: A }]);
    await signBundles({ registryRoot: root, mode: "local", privKeyHex: privHex });

    const sig = readSig(bundleSha(A));
    const sigBytes = Buffer.from(sig.sig_b64, "base64");
    const pubKey = await subtle.importKey(
      "spki",
      Buffer.from(spkiB64, "base64"),
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["verify"],
    );
    const message = new TextEncoder().encode(canonicalize(A) as string);
    const ok = await subtle.verify({ name: "ECDSA", hash: "SHA-256" }, pubKey, sigBytes, message);
    expect(ok).toBe(true);

    // tampered message must NOT verify
    const tampered = new TextEncoder().encode(canonicalize({ ...A, x: 999 }) as string);
    const bad = await subtle.verify({ name: "ECDSA", hash: "SHA-256" }, pubKey, sigBytes, tampered);
    expect(bad).toBe(false);
  });
});
