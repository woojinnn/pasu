/**
 * gen-signing-key — produce a LOCAL dev signing keypair for `BUNDLE_SIGNING_MODE=local`.
 *
 * Writes the 32-byte P-256 secret key (hex) to scripts/deploy/keys/dev-signing-key.hex
 * (gitignored) and prints the matching PINNED public key (SPKI base64) on stdout — paste
 * that into the extension's dev `.env` as PINNED_BUNDLE_PUBLIC_KEY.
 *
 * Production keys live in Cloud KMS (never on disk); get the prod pinned key with
 * `gcloud kms keys versions get-public-key` (SPKI PEM → strip headers → base64).
 *
 *   npm run gen-signing-key
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { p256 } from "@noble/curves/nist.js";
import { publicKeySpkiBase64 } from "./sign-bundles.js";

const HERE = dirname(fileURLToPath(import.meta.url));
const KEYS_DIR = resolve(HERE, "deploy", "keys");
const KEY_PATH = join(KEYS_DIR, "dev-signing-key.hex");

const priv = p256.utils.randomSecretKey();
const privHex = Buffer.from(priv).toString("hex");

mkdirSync(KEYS_DIR, { recursive: true });
writeFileSync(KEY_PATH, privHex + "\n", "utf8");

console.error(`[gen-signing-key] wrote secret key → ${KEY_PATH} (gitignored, dev only)`);
console.error(`[gen-signing-key] PINNED public key (SPKI base64) — set as PINNED_BUNDLE_PUBLIC_KEY:`);
console.log(publicKeySpkiBase64(privHex));
