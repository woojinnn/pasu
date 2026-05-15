// adapter-manifest.ts (vendored)
//
// TEMPORARY: this file is a vendored copy of the manifest types + parser that
// will live at `extension/src/lib/adapter-manifest.ts` once track-B publishes
// them. The phase-1 registry skeleton ships before that file exists in the
// repo, so we duplicate the shape here to unblock test coverage.
//
// Reconciliation plan (see tests/README.md): after PR #22 merges to main,
// delete this file and update `manifest-shape.test.ts` to import from
// `../../extension/src/lib/adapter-manifest.js`.
//
// The shape mirrors the canonical types verbatim. If the canonical types
// diverge, prefer the canonical types and update both this file and
// `build-manifest.js` to match.

type JsonRecord = Record<string, unknown>;

export const ADAPTER_MANIFEST_SCHEMA_VERSION = 1;

/** Root manifest document. */
export interface AdapterManifest {
  readonly schema_version: 1;
  /** ISO-8601 UTC timestamp emitted by the registry's build step. */
  readonly generated_at: string;
  readonly adapters: readonly AdapterEntry[];
}

/** One protocol — Uniswap V4, Curve, Aave, etc. */
export interface AdapterEntry {
  /** Stable kebab-case identifier (`uniswap-v4`). Used in URLs and cache keys. */
  readonly protocol: string;
  /** Human-readable name for UI. */
  readonly display_name: string;
  /** Mutable pointer into `versions[]` — the version the extension SHOULD use. */
  readonly stable_version: string;
  /** Optional second pointer for canary rollouts. Null in Phase 1. */
  readonly canary_version: string | null;
  /** All published versions, newest first by convention. */
  readonly versions: readonly AdapterVersion[];
}

/** A single published WASM artifact. URL is content-addressed (immutable). */
export interface AdapterVersion {
  /** Semver — `0.1.0`, `1.2.3-rc.1`. */
  readonly version: string;
  /** Path relative to the registry root, e.g. `/adapters/uniswap-v4/0.1.0/adapter.wasm`. */
  readonly url: string;
  /** Lowercase hex with 0x prefix. Required from day 1; extension verifies post-download. */
  readonly sha256: string;
  readonly size_bytes: number;
  /** Chains this adapter knows how to decode for. */
  readonly supported_chains: readonly number[];
  /** Pinpoint addresses on each chain so the SW can route by (chain, to) without booting the adapter. */
  readonly supported_addresses: readonly AdapterChainBinding[];
  /** Host-capability method names the adapter needs at runtime (e.g. `oracle.usd_value`). */
  readonly host_capabilities: readonly string[];
  /** Detached signature placeholder. Null until Phase 4 enforces signing. */
  readonly signature: string | null;
  /** Opaque id of the signer key. Null until Phase 4. */
  readonly signer_id: string | null;
  readonly published_at: string;
  /** Emergency kill-switch. Versions marked revoked must not be used even if cached. */
  readonly revoked: boolean;
}

/** Where on chain this adapter applies. */
export interface AdapterChainBinding {
  readonly chain_id: number;
  /** Lowercase 0x-prefixed 20-byte address. */
  readonly address: string;
}

export class AdapterManifestError extends Error {
  constructor(
    message: string,
    readonly path: string,
    readonly value_preview?: unknown,
  ) {
    super(`${path}: ${message}`);
    this.name = "AdapterManifestError";
  }
}

const HEX_ADDRESS = /^0x[0-9a-f]{40}$/;
const HEX_SHA256 = /^0x[0-9a-f]{64}$/;

export function parseAdapterManifest(value: unknown): AdapterManifest {
  const record = requireRecord(value, "$");
  const schemaVersion = requireNumber(record, "schema_version", "$.schema_version");
  if (schemaVersion !== ADAPTER_MANIFEST_SCHEMA_VERSION) {
    fail(
      "$.schema_version",
      `unsupported schema_version (got ${schemaVersion}, want ${ADAPTER_MANIFEST_SCHEMA_VERSION})`,
      schemaVersion,
    );
  }
  return {
    schema_version: ADAPTER_MANIFEST_SCHEMA_VERSION,
    generated_at: requireString(record, "generated_at", "$.generated_at"),
    adapters: parseArray(record, "adapters", "$.adapters", parseAdapterEntry),
  };
}

function parseAdapterEntry(value: unknown, path: string): AdapterEntry {
  const record = requireRecord(value, path);
  const versions = parseArray(record, "versions", `${path}.versions`, parseAdapterVersion);
  if (versions.length === 0) {
    fail(`${path}.versions`, "must contain at least one version", versions);
  }
  const stable = requireString(record, "stable_version", `${path}.stable_version`);
  if (!versions.some((v) => v.version === stable)) {
    fail(
      `${path}.stable_version`,
      `stable_version "${stable}" not in versions[]`,
      stable,
    );
  }
  const canary = requireNullableString(record, "canary_version", `${path}.canary_version`);
  if (canary !== null && !versions.some((v) => v.version === canary)) {
    fail(
      `${path}.canary_version`,
      `canary_version "${canary}" not in versions[]`,
      canary,
    );
  }
  return {
    protocol: requireString(record, "protocol", `${path}.protocol`),
    display_name: requireString(record, "display_name", `${path}.display_name`),
    stable_version: stable,
    canary_version: canary,
    versions,
  };
}

function parseAdapterVersion(value: unknown, path: string): AdapterVersion {
  const record = requireRecord(value, path);
  const sha256 = requireString(record, "sha256", `${path}.sha256`).toLowerCase();
  if (!HEX_SHA256.test(sha256)) {
    fail(`${path}.sha256`, "expected 0x-prefixed 32-byte hex string", sha256);
  }
  return {
    version: requireString(record, "version", `${path}.version`),
    url: requireString(record, "url", `${path}.url`),
    sha256,
    size_bytes: requireNonNegativeInt(record, "size_bytes", `${path}.size_bytes`),
    supported_chains: parseChainArray(record, "supported_chains", `${path}.supported_chains`),
    supported_addresses: parseArray(
      record,
      "supported_addresses",
      `${path}.supported_addresses`,
      parseChainBinding,
    ),
    host_capabilities: parseStringArray(
      record,
      "host_capabilities",
      `${path}.host_capabilities`,
    ),
    signature: requireNullableString(record, "signature", `${path}.signature`),
    signer_id: requireNullableString(record, "signer_id", `${path}.signer_id`),
    published_at: requireString(record, "published_at", `${path}.published_at`),
    revoked: requireBool(record, "revoked", `${path}.revoked`),
  };
}

function parseChainBinding(value: unknown, path: string): AdapterChainBinding {
  const record = requireRecord(value, path);
  const address = requireString(record, "address", `${path}.address`).toLowerCase();
  if (!HEX_ADDRESS.test(address)) {
    fail(`${path}.address`, "expected 0x-prefixed 20-byte hex address", address);
  }
  return {
    chain_id: requireNonNegativeInt(record, "chain_id", `${path}.chain_id`),
    address,
  };
}

function parseArray<T>(
  record: JsonRecord,
  key: string,
  path: string,
  parseItem: (value: unknown, path: string) => T,
): readonly T[] {
  const value = requireField(record, key, path);
  if (!Array.isArray(value)) fail(path, "expected array", value);
  return value.map((item, index) => parseItem(item, `${path}[${index}]`));
}

function parseStringArray(record: JsonRecord, key: string, path: string): readonly string[] {
  const value = requireField(record, key, path);
  if (!Array.isArray(value)) fail(path, "expected array", value);
  return value.map((item, index) => {
    if (typeof item !== "string") fail(`${path}[${index}]`, "expected string", item);
    return item;
  });
}

function parseChainArray(record: JsonRecord, key: string, path: string): readonly number[] {
  const value = requireField(record, key, path);
  if (!Array.isArray(value)) fail(path, "expected array", value);
  return value.map((item, index) => {
    if (typeof item !== "number" || !Number.isInteger(item) || item < 0) {
      fail(`${path}[${index}]`, "expected non-negative integer chain id", item);
    }
    return item;
  });
}

function requireRecord(value: unknown, path: string): JsonRecord {
  if (typeof value === "object" && value !== null && !Array.isArray(value)) {
    return value as JsonRecord;
  }
  fail(path, "expected object", value);
}

function requireField(record: JsonRecord, key: string, path: string): unknown {
  if (Object.prototype.hasOwnProperty.call(record, key)) return record[key];
  fail(path, "missing required field", record);
}

function requireString(record: JsonRecord, key: string, path: string): string {
  const value = requireField(record, key, path);
  if (typeof value === "string") return value;
  fail(path, "expected string", value);
}

function requireNullableString(
  record: JsonRecord,
  key: string,
  path: string,
): string | null {
  const value = requireField(record, key, path);
  if (value === null || typeof value === "string") return value;
  fail(path, "expected string or null", value);
}

function requireNumber(record: JsonRecord, key: string, path: string): number {
  const value = requireField(record, key, path);
  if (typeof value === "number" && Number.isFinite(value)) return value;
  fail(path, "expected finite number", value);
}

function requireNonNegativeInt(record: JsonRecord, key: string, path: string): number {
  const n = requireNumber(record, key, path);
  if (!Number.isInteger(n) || n < 0) fail(path, "expected non-negative integer", n);
  return n;
}

function requireBool(record: JsonRecord, key: string, path: string): boolean {
  const value = requireField(record, key, path);
  if (typeof value === "boolean") return value;
  fail(path, "expected boolean", value);
}

function fail(path: string, message: string, value: unknown): never {
  throw new AdapterManifestError(message, path, value);
}
