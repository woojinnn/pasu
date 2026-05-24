/**
 * Phase 2B — `installBundle` pipeline.
 *
 * Spec: `ADAPTER_LOADER_ARCHITECTURE.md` §7.3:869-887.
 *
 * Pipeline (demo-stage integrity layer):
 *   1. Shape validation — `parseBundle` (Phase 0 hand-written validator)
 *      surfaces field-level errors before we touch crypto.
 *   2. RFC 8785 canonical JSON + SHA-256.
 *   3. Compare against the `bundle_sha256` the registry sent us
 *      (`/Users/jhy/Desktop/ScopeBall/scopeball/registry/index/by-callkey/…`).
 *      Mismatch → `InstallError("bundle_hash_mismatch")` — the JIT fetcher
 *      maps this to a 5-minute `integrity_failed` negative-cache entry.
 *   4. Mount the bundle into the WASM engine via
 *      `mountDeclarativeBundle` (Phase 1B).
 *
 * Note on pass-through: the loader (`mountDeclarativeBundle`) already
 * stringifies + forwards raw bytes to WASM. We canonicalise *only* for the
 * hash check and never reuse the canonical form downstream — this keeps
 * the bytes-to-WASM invariant the Phase 1B comment talks about
 * (parser is shape-only, original bytes preserved for stable hashing).
 *
 * What is NOT in scope here (per spec §7.4):
 *   - Signature verification (§10.1+)
 *   - Sourcify integration
 *   - merkle proofs
 */
import canonicalize from "canonicalize";
import {
  mountDeclarativeBundle,
  type MountResult,
} from "./declarative-adapter-loader";
import { BundleParseError, parseBundle } from "./bundle-schema";

export type InstallErrorCode =
  | "bundle_hash_mismatch"
  | "schema_invalid"
  | "wasm_install_failed";

export interface InstallErrorDetails {
  /** Set when code === "bundle_hash_mismatch". */
  expected?: string;
  computed?: string;
  /** Set when code === "schema_invalid". */
  schemaError?: string;
  /** Set when code === "wasm_install_failed". */
  cause?: unknown;
}

export class InstallError extends Error {
  constructor(
    readonly code: InstallErrorCode,
    readonly details: InstallErrorDetails = {},
  ) {
    super(`install[${code}] ${formatDetails(code, details)}`);
    this.name = "InstallError";
    if (details.cause !== undefined) {
      (this as { cause?: unknown }).cause = details.cause;
    }
  }
}

function formatDetails(
  code: InstallErrorCode,
  details: InstallErrorDetails,
): string {
  switch (code) {
    case "bundle_hash_mismatch":
      return `expected=${details.expected ?? "?"} computed=${details.computed ?? "?"}`;
    case "schema_invalid":
      return details.schemaError ?? "shape validation failed";
    case "wasm_install_failed":
      return details.cause instanceof Error
        ? details.cause.message
        : String(details.cause ?? "unknown");
  }
}

/**
 * Compute the canonical-JSON SHA-256 used by the adapter-loader. Exported for
 * the jit-fetcher and the test harness — both need to round-trip an
 * in-memory bundle through the same hash function the registry's
 * `build-index.ts` uses.
 *
 * Format: `"0x" + 64 lower-case hex`. Mirrors the registry exactly.
 */
export async function canonicalSha256(bundle: unknown): Promise<string> {
  const canonical = canonicalize(bundle);
  if (typeof canonical !== "string") {
    throw new InstallError("schema_invalid", {
      schemaError: "canonicalize returned non-string (bundle has cycles or undefined values)",
    });
  }
  const bytes = new TextEncoder().encode(canonical);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return "0x" + bytesToHex(new Uint8Array(digest));
}

function bytesToHex(buf: Uint8Array): string {
  const out = new Array<string>(buf.length);
  for (let i = 0; i < buf.length; i++) {
    out[i] = buf[i].toString(16).padStart(2, "0");
  }
  return out.join("");
}

/**
 * Install a registry-fetched bundle into the WASM engine after verifying
 * integrity against `expectedSha256`.
 *
 * Inputs:
 *   - `bundleJson`: the parsed bundle object (`bundle` field from the
 *     registry index response). We accept the object — not the string —
 *     because the canonical hash is independent of whitespace/ordering by
 *     definition of JCS.
 *   - `expectedSha256`: the `bundle_sha256` field from the same index
 *     response.
 *
 * Error mapping (consumed by jit-fetcher):
 *   - `BundleParseError`        → `InstallError("schema_invalid")`
 *   - hash mismatch             → `InstallError("bundle_hash_mismatch")`
 *   - downstream WASM failure   → `InstallError("wasm_install_failed")`
 */
export async function installBundle(
  bundleJson: unknown,
  expectedSha256: string,
): Promise<MountResult> {
  // 1. Schema validation. We re-throw inside `mountDeclarativeBundle` too,
  // but doing it here means a malformed registry payload aborts before we
  // burn cycles canonicalising it.
  try {
    parseBundle(bundleJson);
  } catch (err) {
    if (err instanceof BundleParseError) {
      throw new InstallError("schema_invalid", { schemaError: err.message });
    }
    throw err;
  }

  // 2. Compute canonical hash.
  const computed = await canonicalSha256(bundleJson);

  // 3. Integrity check. Case-insensitive because the registry emits
  // lowercase hex and we shouldn't fail on uppercase input.
  if (!hashEquals(computed, expectedSha256)) {
    throw new InstallError("bundle_hash_mismatch", {
      expected: expectedSha256,
      computed,
    });
  }

  // 4. WASM mount. `mountDeclarativeBundle` accepts raw text — we
  // re-stringify here. Note: this is a *different* serialisation than
  // `canonicalize(...)` used in step 2; the engine deserialises into Rust
  // structs so byte ordering doesn't matter past this point.
  const text = JSON.stringify(bundleJson);
  try {
    return await mountDeclarativeBundle(text);
  } catch (err) {
    throw new InstallError("wasm_install_failed", { cause: err });
  }
}

function hashEquals(a: string, b: string): boolean {
  return a.toLowerCase() === b.toLowerCase();
}
