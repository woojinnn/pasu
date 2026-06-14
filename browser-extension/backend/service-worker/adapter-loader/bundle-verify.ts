/**
 * bundle-verify — verify a registry v3 bundle's detached ECDSA P-256 signature
 * BEFORE installing its decoder into the WASM engine. Supply-chain integrity:
 * a compromised / MITM'd registry must not be able to inject a decode rule that
 * silently flips a pre-sign verdict.
 *
 * The signed message is `canonicalize(bundle)` (RFC 8785 JCS) — the same
 * preimage as the bundle's `bundle_sha256`. So verification is robust to the
 * proxy's serialization (a concrete bundle is served verbatim; a sourced one is
 * re-materialized + re-serialized) — both canonicalize to identical bytes
 * (proven for the whole corpus by registry-api's materialization-parity gate).
 *
 * SECURITY INVARIANTS:
 *  - The algorithm ({name:"ECDSA",hash:"SHA-256"}) and the verifying key are
 *    HARD-CODED / build-time pinned. The `.sig`'s `alg`/`key_id` fields are
 *    telemetry ONLY — a hostile registry cannot use them to downgrade.
 *  - The signature is fetched by the LOCALLY-RECOMPUTED hash of the bundle we
 *    actually parsed, never by the response's self-asserted `bundle_sha256`.
 *  - Hash the RAW response bundle (`parsedResponse.bundle`), NOT `parseBundleV3`'s
 *    output — the parser reconstructs a field-subset, whose canonical form would
 *    not match the signed preimage.
 *  - Staged rollout: when `require` is false (signing not yet enforced on this
 *    build channel) a missing / invalid signature is a NO-OP (install proceeds),
 *    so an unsigned dev / staging registry keeps working. When `require` is true,
 *    any verification failure is fail-closed by the caller.
 */
import canonicalize from "canonicalize";

declare const PINNED_BUNDLE_PUBLIC_KEY: string;
declare const DAMBI_REQUIRE_BUNDLE_SIGNATURE: string;

// Fail loud if the default-export interop ever regresses (e.g. a wrong import
// form yields the module namespace instead of the function).
if (typeof canonicalize !== "function") {
  throw new Error(
    "[bundle-verify] canonicalize default export is not a function — cannot verify bundle signatures",
  );
}

export interface BundleSigDefines {
  require: boolean;
  pinnedKeySpkiB64: string;
}

/** Read the webpack DefinePlugin build constants, defaulting safely when they
 * are absent (e.g. under vitest, which does not inject them). */
export function readBundleSigDefines(): BundleSigDefines {
  let require = false;
  let pinnedKeySpkiB64 = "";
  try {
    require = DAMBI_REQUIRE_BUNDLE_SIGNATURE === "true";
  } catch {
    /* identifier undefined outside a webpack build */
  }
  try {
    pinnedKeySpkiB64 =
      typeof PINNED_BUNDLE_PUBLIC_KEY === "string" ? PINNED_BUNDLE_PUBLIC_KEY : "";
  } catch {
    /* identifier undefined outside a webpack build */
  }
  return { require, pinnedKeySpkiB64 };
}

export type VerifyOutcome =
  | { ok: true; localSha?: string }
  | { ok: false; reason: string; localSha?: string };

export interface VerifyBundleParams {
  /** The RAW response bundle object — `parsedResponse.bundle`, NOT parseBundleV3 output. */
  bundle: unknown;
  /** The response's self-asserted `bundle_sha256` (defense-in-depth only). */
  claimedSha256?: string | undefined;
  baseUrl: string;
  fetchImpl: typeof fetch;
  require: boolean;
  pinnedKeySpkiB64: string;
}

function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}

function bufToHex(buf: ArrayBuffer): string {
  const b = new Uint8Array(buf);
  let s = "";
  for (let i = 0; i < b.length; i += 1) s += b[i].toString(16).padStart(2, "0");
  return s;
}

// importKey is async; memoize per SPKI so we import once per pinned key.
const keyCache = new Map<string, Promise<CryptoKey>>();
function importPinnedKey(spkiB64: string): Promise<CryptoKey> {
  let p = keyCache.get(spkiB64);
  if (!p) {
    p = crypto.subtle.importKey(
      "spki",
      base64ToBytes(spkiB64),
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["verify"],
    );
    keyCache.set(spkiB64, p);
  }
  return p;
}

export async function verifyBundleSignature(
  p: VerifyBundleParams,
): Promise<VerifyOutcome> {
  // OFF — signing not enforced on this channel; never block on a missing/invalid
  // sig (unsigned dev/staging registry keeps working).
  if (!p.require) return { ok: true };

  if (!p.pinnedKeySpkiB64) return { ok: false, reason: "no_pinned_key" };

  // Recompute the canonical preimage and its hash from the bytes we parsed.
  let canonical: string;
  let localSha: string;
  try {
    const c = canonicalize(p.bundle);
    if (typeof c !== "string") return { ok: false, reason: "canonicalize_failed" };
    canonical = c;
    const digest = await crypto.subtle.digest(
      "SHA-256",
      new TextEncoder().encode(canonical),
    );
    localSha = "0x" + bufToHex(digest);
  } catch {
    return { ok: false, reason: "canonicalize_failed" };
  }

  // Defense-in-depth: the response's self-asserted sha must match the bytes we
  // actually parsed (we still fetch the sig by localSha, never the claim).
  if (p.claimedSha256 && p.claimedSha256.toLowerCase() !== localSha) {
    return { ok: false, reason: "sha_mismatch", localSha };
  }

  // Fetch the detached signature by the RECOMPUTED hash (N4).
  let sigBytes: Uint8Array;
  try {
    const base = p.baseUrl.endsWith("/") ? p.baseUrl.slice(0, -1) : p.baseUrl;
    const res = await p.fetchImpl(`${base}/signatures/${localSha}.sig`);
    if (!res.ok) return { ok: false, reason: `sig_http_${res.status}`, localSha };
    const doc = (await res.json()) as { sig_b64?: unknown };
    if (typeof doc.sig_b64 !== "string") {
      return { ok: false, reason: "sig_malformed", localSha };
    }
    sigBytes = base64ToBytes(doc.sig_b64); // alg / key_id ignored (telemetry only, N2)
  } catch {
    return { ok: false, reason: "sig_fetch_error", localSha };
  }

  // Verify with the HARD-CODED algorithm + the build-time pinned key (N2).
  try {
    const key = await importPinnedKey(p.pinnedKeySpkiB64);
    const valid = await crypto.subtle.verify(
      { name: "ECDSA", hash: "SHA-256" },
      key,
      sigBytes,
      new TextEncoder().encode(canonical),
    );
    return valid
      ? { ok: true, localSha }
      : { ok: false, reason: "sig_invalid", localSha };
  } catch {
    return { ok: false, reason: "verify_error", localSha };
  }
}
