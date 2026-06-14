/**
 * sign-bundles — detached ECDSA P-256 signatures over each unique registry bundle.
 *
 * The signed message is `canonicalize(bundle)` (RFC 8785 JCS); its SHA-256 is
 * exactly the `bundle_sha256` that build-index.ts stamps on every index entry
 * (proven for the whole corpus by registry-api's materialization-parity gate).
 * Signing that 32-byte digest therefore signs the canonical bundle. The browser
 * extension verifies with:
 *
 *     crypto.subtle.verify({name:"ECDSA",hash:"SHA-256"}, pinnedKey, sig, canonicalize(bundle))
 *
 * which re-hashes the canonical message to the same digest. So we never need the
 * canonical bytes here — only `bundle_sha256`, read straight from the built index.
 *
 * Output: one detached sidecar per unique bundle, `signatures/<sha>.sig`:
 *     { "alg": "ECDSA_P256_SHA256", "key_id": "<label>", "sig_b64": "<base64 P1363 r||s>" }
 * The extension treats `alg`/`key_id` as telemetry ONLY — it hard-codes the
 * algorithm and the pinned key, so a malicious registry cannot downgrade them.
 *
 * Modes (BUNDLE_SIGNING_MODE):
 *   local (default) — sign with a local P-256 secret key (@noble/curves). dev / CI-test.
 *   kms             — sign each digest via Google Cloud KMS asymmetricSign (DER → P1363).
 *
 *   npm run sign                 # local mode (BUNDLE_SIGNING_KEY_PATH or dev key)
 *   BUNDLE_SIGNING_MODE=kms KMS_KEY_NAME=projects/.../cryptoKeyVersions/1 npm run sign
 *   npm run sign -- --force      # re-sign even when a .sig already exists
 */
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { p256 } from "@noble/curves/nist.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_ROOT = resolve(HERE, "..");

export const SIG_ALG = "ECDSA_P256_SHA256";

/** Fixed ASN.1 SubjectPublicKeyInfo prefix for an uncompressed prime256v1 point. */
const P256_SPKI_PREFIX = Buffer.from(
  "3059301306072a8648ce3d020106082a8648ce3d030107034200",
  "hex",
);

export interface SignBundlesOptions {
  registryRoot?: string;
  mode?: "local" | "kms";
  /** local mode: 32-byte secret key, hex (0x optional). */
  privKeyHex?: string;
  /** kms mode: full crypto key VERSION resource name. */
  kmsKeyName?: string;
  /** label stored in each .sig (telemetry only; the extension ignores it). */
  keyId?: string;
  /** re-sign even when a .sig already exists (key rotation / corpus repair). */
  force?: boolean;
  log?: (msg: string) => void;
}

export interface SignBundlesResult {
  total: number;
  signed: number;
  skipped: number;
}

// ---- helpers ----------------------------------------------------------------

function walkJson(dir: string): string[] {
  if (!existsSync(dir)) return [];
  const out: string[] = [];
  for (const ent of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, ent.name);
    if (ent.isDirectory()) out.push(...walkJson(full));
    else if (ent.name.endsWith(".json")) out.push(full);
  }
  return out;
}

/** Every unique `bundle_sha256` referenced by the built index/ tree. */
export function collectBundleShas(registryRoot: string): string[] {
  const shas = new Set<string>();
  for (const file of walkJson(join(registryRoot, "index"))) {
    let entry: { bundle_sha256?: unknown };
    try {
      entry = JSON.parse(readFileSync(file, "utf8"));
    } catch {
      continue;
    }
    const sha = entry.bundle_sha256;
    if (typeof sha === "string" && /^0x[0-9a-f]{64}$/.test(sha)) shas.add(sha);
  }
  return [...shas].sort();
}

function digestBytes(sha256Hex: string): Uint8Array {
  const hex = sha256Hex.startsWith("0x") ? sha256Hex.slice(2) : sha256Hex;
  return Uint8Array.from(Buffer.from(hex, "hex"));
}

function privKeyBytes(hex: string): Uint8Array {
  const h = hex.trim().replace(/^0x/, "");
  if (!/^[0-9a-fA-F]{64}$/.test(h)) {
    throw new Error("local signing key must be 32-byte hex (64 chars)");
  }
  return Uint8Array.from(Buffer.from(h, "hex"));
}

/** local: P1363 r||s directly from the prehashed digest. */
function signLocal(digest: Uint8Array, priv: Uint8Array): Uint8Array {
  return p256.sign(digest, priv, { prehash: false });
}

/** kms: asymmetricSign returns DER; convert to P1363 (WebCrypto verify format). */
async function signKms(digest: Uint8Array, keyName: string): Promise<Uint8Array> {
  const { KeyManagementServiceClient } = await import("@google-cloud/kms");
  const client = new KeyManagementServiceClient();
  const [resp] = await client.asymmetricSign({
    name: keyName,
    digest: { sha256: Buffer.from(digest) },
  });
  if (!resp.signature) {
    throw new Error(`KMS returned no signature for ${keyName}`);
  }
  const der = Uint8Array.from(resp.signature as Buffer);
  return p256.Signature.fromBytes(der, "der").toBytes("compact");
}

function localKeyId(priv: Uint8Array): string {
  const pub = p256.getPublicKey(priv, false);
  return "local-" + createHash("sha256").update(pub).digest("hex").slice(0, 12);
}

/** SPKI(base64) public key for a local secret — the value pinned in the extension. */
export function publicKeySpkiBase64(privKeyHex: string): string {
  const pubU = p256.getPublicKey(privKeyBytes(privKeyHex), false); // 65-byte uncompressed
  return Buffer.concat([P256_SPKI_PREFIX, Buffer.from(pubU)]).toString("base64");
}

function readLocalKey(): string {
  const path =
    process.env.BUNDLE_SIGNING_KEY_PATH ??
    join(DEFAULT_ROOT, "scripts", "deploy", "keys", "dev-signing-key.hex");
  if (!existsSync(path)) {
    throw new Error(
      `local signing key not found at ${path}. Generate one with: npm run gen-signing-key`,
    );
  }
  return readFileSync(path, "utf8");
}

// ---- main -------------------------------------------------------------------

export async function signBundles(
  opts: SignBundlesOptions = {},
): Promise<SignBundlesResult> {
  const root = opts.registryRoot ?? DEFAULT_ROOT;
  const mode =
    opts.mode ?? (process.env.BUNDLE_SIGNING_MODE === "kms" ? "kms" : "local");
  const log = opts.log ?? (() => {});
  const sigDir = join(root, "signatures");

  let priv: Uint8Array | undefined;
  let kmsKeyName: string | undefined;
  let keyId = opts.keyId;
  if (mode === "local") {
    priv = privKeyBytes(opts.privKeyHex ?? readLocalKey());
    keyId = keyId ?? localKeyId(priv);
  } else {
    kmsKeyName = opts.kmsKeyName ?? process.env.KMS_KEY_NAME;
    if (!kmsKeyName) {
      throw new Error("kms mode requires KMS_KEY_NAME (key version resource name)");
    }
    keyId = keyId ?? kmsKeyName;
  }

  const shas = collectBundleShas(root);
  mkdirSync(sigDir, { recursive: true });

  let signed = 0;
  let skipped = 0;
  for (const sha of shas) {
    const outPath = join(sigDir, `${sha}.sig`);
    if (!opts.force && existsSync(outPath)) {
      skipped++;
      continue;
    }
    const digest = digestBytes(sha);
    const sig =
      mode === "local"
        ? signLocal(digest, priv as Uint8Array)
        : await signKms(digest, kmsKeyName as string);
    const doc = {
      alg: SIG_ALG,
      key_id: keyId,
      sig_b64: Buffer.from(sig).toString("base64"),
    };
    writeFileSync(outPath, JSON.stringify(doc, null, 2) + "\n", "utf8");
    signed++;
  }
  log(`[${mode}] ${signed} signed, ${skipped} skipped, ${shas.length} total`);
  return { total: shas.length, signed, skipped };
}

// CLI -------------------------------------------------------------------------

const isMain =
  process.argv[1] !== undefined &&
  import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) {
  signBundles({
    force: process.argv.includes("--force"),
    log: (m) => console.error(`[sign-bundles] ${m}`),
  })
    .then((r) => {
      if (r.total === 0) {
        console.error(
          "[sign-bundles] WARN: no bundle_sha256 found — was the index built (npm run build)?",
        );
      }
    })
    .catch((e) => {
      console.error("[sign-bundles] FATAL:", e instanceof Error ? e.message : e);
      process.exit(1);
    });
}
