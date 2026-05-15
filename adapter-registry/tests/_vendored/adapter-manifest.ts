// adapter-manifest.ts (vendored)
//
// TEMPORARY: this file is a vendored copy of the manifest types + parser that
// will live at `extension/src/lib/adapter-manifest.ts` once track-B publishes
// them. The phase-1 registry skeleton ships before that file exists in the
// repo, so we duplicate the shape here to unblock test coverage.
//
// Reconciliation plan: as soon as the canonical module lands, delete this file
// and import from `../../extension/src/lib/adapter-manifest` instead. The
// `manifest-shape.test.ts` consumer is the only user.
//
// The shape mirrors the task brief for the registry phase 1 deliverables
// and the `build-manifest.js` output. If the canonical types diverge, prefer
// the canonical types and update `build-manifest.js` to match.

export type AdapterSupportedAddress = {
  readonly chain_id: number;
  readonly address: string;
};

export type AdapterVersionEntry = {
  readonly version: string;
  readonly wasm_url: string;
  readonly sha256: string;
  readonly supported_chains: readonly number[];
  readonly supported_addresses: readonly AdapterSupportedAddress[];
  readonly host_capabilities: readonly string[];
  readonly revoked: boolean;
};

export type AdapterManifestEntry = {
  readonly protocol: string;
  readonly display_name: string;
  readonly stable_version: string;
  readonly versions: readonly AdapterVersionEntry[];
};

export type AdapterManifest = {
  readonly schema_version: 1;
  readonly generated_at: string;
  readonly adapters: readonly AdapterManifestEntry[];
};

export class AdapterManifestError extends Error {
  constructor(message: string, public readonly path: string) {
    super(`adapter manifest invalid at ${path}: ${message}`);
    this.name = "AdapterManifestError";
  }
}

// --- parser ---------------------------------------------------------------
// Defensive parse: rejects unknown shapes rather than coercing, because the
// extension verifies the wasm against `sha256` and a bad manifest means we
// could fetch an artifact we cannot validate.

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function isHexSha256(v: unknown): v is string {
  return typeof v === "string" && /^[0-9a-f]{64}$/.test(v);
}

function isIsoDateString(v: unknown): v is string {
  return typeof v === "string" && !Number.isNaN(Date.parse(v));
}

function parseSupportedAddress(
  v: unknown,
  path: string
): AdapterSupportedAddress {
  if (!isObject(v)) throw new AdapterManifestError("expected object", path);
  if (typeof v.chain_id !== "number" || !Number.isInteger(v.chain_id)) {
    throw new AdapterManifestError("chain_id must be an integer", `${path}.chain_id`);
  }
  if (typeof v.address !== "string" || v.address.length === 0) {
    throw new AdapterManifestError("address must be a non-empty string", `${path}.address`);
  }
  return { chain_id: v.chain_id, address: v.address };
}

function parseVersionEntry(v: unknown, path: string): AdapterVersionEntry {
  if (!isObject(v)) throw new AdapterManifestError("expected object", path);
  if (typeof v.version !== "string" || v.version.length === 0) {
    throw new AdapterManifestError("version required", `${path}.version`);
  }
  if (typeof v.wasm_url !== "string" || v.wasm_url.length === 0) {
    throw new AdapterManifestError("wasm_url required", `${path}.wasm_url`);
  }
  if (!isHexSha256(v.sha256)) {
    throw new AdapterManifestError(
      "sha256 must be 64 hex chars",
      `${path}.sha256`
    );
  }
  if (!Array.isArray(v.supported_chains)) {
    throw new AdapterManifestError("supported_chains must be an array", `${path}.supported_chains`);
  }
  const supportedChains = v.supported_chains.map((entry, idx) => {
    if (typeof entry !== "number" || !Number.isInteger(entry) || entry < 0) {
      throw new AdapterManifestError(
        "must be a non-negative integer",
        `${path}.supported_chains[${idx}]`
      );
    }
    return entry;
  });
  if (!Array.isArray(v.supported_addresses)) {
    throw new AdapterManifestError(
      "supported_addresses must be an array",
      `${path}.supported_addresses`
    );
  }
  const supportedAddresses = v.supported_addresses.map((entry, idx) =>
    parseSupportedAddress(entry, `${path}.supported_addresses[${idx}]`)
  );
  if (!Array.isArray(v.host_capabilities)) {
    throw new AdapterManifestError(
      "host_capabilities must be an array",
      `${path}.host_capabilities`
    );
  }
  const hostCapabilities = v.host_capabilities.map((entry, idx) => {
    if (typeof entry !== "string" || entry.length === 0) {
      throw new AdapterManifestError(
        "must be a non-empty string",
        `${path}.host_capabilities[${idx}]`
      );
    }
    return entry;
  });
  if (typeof v.revoked !== "boolean") {
    throw new AdapterManifestError("revoked must be a boolean", `${path}.revoked`);
  }
  return {
    version: v.version,
    wasm_url: v.wasm_url,
    sha256: v.sha256,
    supported_chains: supportedChains,
    supported_addresses: supportedAddresses,
    host_capabilities: hostCapabilities,
    revoked: v.revoked,
  };
}

function parseAdapterEntry(v: unknown, path: string): AdapterManifestEntry {
  if (!isObject(v)) throw new AdapterManifestError("expected object", path);
  if (typeof v.protocol !== "string" || v.protocol.length === 0) {
    throw new AdapterManifestError("protocol required", `${path}.protocol`);
  }
  if (typeof v.display_name !== "string" || v.display_name.length === 0) {
    throw new AdapterManifestError("display_name required", `${path}.display_name`);
  }
  if (typeof v.stable_version !== "string" || v.stable_version.length === 0) {
    throw new AdapterManifestError("stable_version required", `${path}.stable_version`);
  }
  if (!Array.isArray(v.versions) || v.versions.length === 0) {
    throw new AdapterManifestError("versions must be a non-empty array", `${path}.versions`);
  }
  const versions = v.versions.map((entry, idx) =>
    parseVersionEntry(entry, `${path}.versions[${idx}]`)
  );
  if (!versions.some((entry) => entry.version === v.stable_version)) {
    throw new AdapterManifestError(
      `stable_version=${String(v.stable_version)} not found among versions`,
      `${path}.stable_version`
    );
  }
  return {
    protocol: v.protocol,
    display_name: v.display_name,
    stable_version: v.stable_version,
    versions,
  };
}

export function parseAdapterManifest(input: unknown): AdapterManifest {
  if (!isObject(input)) {
    throw new AdapterManifestError("expected object", "$");
  }
  if (input.schema_version !== 1) {
    throw new AdapterManifestError(
      `unsupported schema_version: ${String(input.schema_version)}`,
      "$.schema_version"
    );
  }
  if (!isIsoDateString(input.generated_at)) {
    throw new AdapterManifestError(
      "generated_at must be an ISO-8601 string",
      "$.generated_at"
    );
  }
  if (!Array.isArray(input.adapters)) {
    throw new AdapterManifestError("adapters must be an array", "$.adapters");
  }
  const adapters = input.adapters.map((entry, idx) =>
    parseAdapterEntry(entry, `$.adapters[${idx}]`)
  );
  return {
    schema_version: 1,
    generated_at: input.generated_at,
    adapters,
  };
}
