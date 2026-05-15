import { describe, expect, it } from "vitest";
import { promises as fs } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

import {
  AdapterManifestError,
  parseAdapterManifest,
} from "./_vendored/adapter-manifest.js";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REGISTRY_ROOT = path.resolve(HERE, "..");
const BUILD_SCRIPT = path.join(REGISTRY_ROOT, "scripts", "build-manifest.js");

const HEX_SHA256 = /^0x[0-9a-f]{64}$/;
const ZERO_SHA256 = `0x${"0".repeat(64)}`;

async function makeTempRegistry(): Promise<string> {
  const tmp = await fs.mkdtemp(
    path.join(os.tmpdir(), "adapter-registry-test-")
  );
  await fs.mkdir(path.join(tmp, "public", "adapters"), { recursive: true });
  await fs.mkdir(path.join(tmp, "scripts"), { recursive: true });
  await fs.copyFile(BUILD_SCRIPT, path.join(tmp, "scripts", "build-manifest.js"));
  return tmp;
}

async function writeVersionDir(
  registryRoot: string,
  protocol: string,
  version: string,
  metadata?: unknown
): Promise<string> {
  const versionDir = path.join(
    registryRoot,
    "public",
    "adapters",
    protocol,
    version
  );
  await fs.mkdir(versionDir, { recursive: true });
  await fs.writeFile(
    path.join(versionDir, "adapter.wasm"),
    `fake-wasm-bytes-${protocol}-${version}`
  );
  if (metadata !== undefined) {
    await fs.writeFile(
      path.join(versionDir, "metadata.json"),
      JSON.stringify(metadata)
    );
  }
  return versionDir;
}

function defaultMetadata(displayName: string): Record<string, unknown> {
  return {
    display_name: displayName,
    supported_chains: [1],
    supported_addresses: [],
    host_capabilities: [],
  };
}

function runBuildManifest(registryRoot: string): {
  status: number | null;
  stdout: string;
  stderr: string;
} {
  const scriptPath = path.join(registryRoot, "scripts", "build-manifest.js");
  const result = spawnSync(process.execPath, [scriptPath], {
    env: {
      ...process.env,
      MANIFEST_GENERATED_AT: "2026-05-15T00:00:00.000Z",
    },
    encoding: "utf8",
  });
  return {
    status: result.status,
    stdout: result.stdout,
    stderr: result.stderr,
  };
}

async function readGeneratedManifest(registryRoot: string): Promise<unknown> {
  const raw = await fs.readFile(
    path.join(registryRoot, "public", "manifest.json"),
    "utf8"
  );
  return JSON.parse(raw);
}

describe("parseAdapterManifest — empty manifest", () => {
  it("accepts a manifest with no adapters", () => {
    const manifest = parseAdapterManifest({
      schema_version: 1,
      generated_at: "2026-05-15T12:00:00.000Z",
      adapters: [],
    });
    expect(manifest.adapters).toHaveLength(0);
    expect(manifest.schema_version).toBe(1);
  });

  it("accepts the manifest currently shipped in public/", async () => {
    const raw = await fs.readFile(
      path.join(REGISTRY_ROOT, "public", "manifest.json"),
      "utf8"
    );
    const manifest = parseAdapterManifest(JSON.parse(raw));
    expect(manifest.adapters).toEqual([]);
  });
});

describe("parseAdapterManifest — build-manifest.js fixture", () => {
  it("produces a parseable manifest with one fake adapter", async () => {
    const tmp = await makeTempRegistry();
    await writeVersionDir(tmp, "uniswap_v3", "0.1.0", {
      display_name: "Uniswap V3",
      supported_chains: [1],
      supported_addresses: [
        {
          chain_id: 1,
          address: "0xE592427A0AEce92De3Edee1F18E0157C05861564",
        },
      ],
      host_capabilities: ["abi_resolver.v1"],
    });

    const result = runBuildManifest(tmp);
    expect(result.status, result.stderr).toBe(0);

    const parsed = parseAdapterManifest(await readGeneratedManifest(tmp));
    expect(parsed.adapters).toHaveLength(1);

    const adapter = parsed.adapters[0]!;
    expect(adapter.protocol).toBe("uniswap_v3");
    expect(adapter.display_name).toBe("Uniswap V3");
    expect(adapter.stable_version).toBe("0.1.0");
    expect(adapter.canary_version).toBeNull();
    expect(adapter.versions).toHaveLength(1);

    const version = adapter.versions[0]!;
    expect(version.version).toBe("0.1.0");
    expect(version.url).toBe("/adapters/uniswap_v3/0.1.0/adapter.wasm");
    expect(version.sha256).toMatch(HEX_SHA256);
    expect(version.size_bytes).toBeGreaterThan(0);
    expect(version.supported_chains).toEqual([1]);
    expect(version.supported_addresses).toEqual([
      // address should be lowercased on output
      { chain_id: 1, address: "0xe592427a0aece92de3edee1f18e0157c05861564" },
    ]);
    expect(version.host_capabilities).toEqual(["abi_resolver.v1"]);
    expect(version.signature).toBeNull();
    expect(version.signer_id).toBeNull();
    expect(typeof version.published_at).toBe("string");
    expect(Number.isNaN(Date.parse(version.published_at))).toBe(false);
    expect(version.revoked).toBe(false);

    await fs.rm(tmp, { recursive: true, force: true });
  });

  it("re-running the script over an unchanged tree is idempotent", async () => {
    const tmp = await makeTempRegistry();
    await writeVersionDir(
      tmp,
      "uniswap_v3",
      "0.1.0",
      defaultMetadata("Uniswap V3")
    );

    expect(runBuildManifest(tmp).status).toBe(0);
    const first = await fs.readFile(
      path.join(tmp, "public", "manifest.json"),
      "utf8"
    );

    expect(runBuildManifest(tmp).status).toBe(0);
    const second = await fs.readFile(
      path.join(tmp, "public", "manifest.json"),
      "utf8"
    );

    expect(second).toBe(first);
    await fs.rm(tmp, { recursive: true, force: true });
  });

  it("flips revoked=true when the sentinel file is present", async () => {
    const tmp = await makeTempRegistry();
    const versionDir = await writeVersionDir(
      tmp,
      "uniswap_v3",
      "0.1.0",
      defaultMetadata("Uniswap V3")
    );
    await fs.writeFile(path.join(versionDir, ".revoked"), "");

    expect(runBuildManifest(tmp).status).toBe(0);
    const parsed = parseAdapterManifest(await readGeneratedManifest(tmp));
    expect(parsed.adapters[0]!.versions[0]!.revoked).toBe(true);

    await fs.rm(tmp, { recursive: true, force: true });
  });

  it("sorts multiple versions of one protocol by semver, with the highest as stable_version", async () => {
    const tmp = await makeTempRegistry();
    // Intentionally write the versions out of semver order.
    await writeVersionDir(tmp, "uniswap_v3", "1.0.0", defaultMetadata("Uniswap V3"));
    await writeVersionDir(tmp, "uniswap_v3", "0.1.0", defaultMetadata("Uniswap V3"));
    await writeVersionDir(tmp, "uniswap_v3", "0.10.0", defaultMetadata("Uniswap V3"));
    await writeVersionDir(tmp, "uniswap_v3", "0.2.0", defaultMetadata("Uniswap V3"));

    const result = runBuildManifest(tmp);
    expect(result.status, result.stderr).toBe(0);

    const parsed = parseAdapterManifest(await readGeneratedManifest(tmp));
    expect(parsed.adapters).toHaveLength(1);
    const adapter = parsed.adapters[0]!;
    expect(adapter.versions.map((v) => v.version)).toEqual([
      "0.1.0",
      "0.2.0",
      "0.10.0",
      "1.0.0",
    ]);
    // Highest semver should be picked when no channels.json is present.
    expect(adapter.stable_version).toBe("1.0.0");
    expect(adapter.canary_version).toBeNull();

    await fs.rm(tmp, { recursive: true, force: true });
  });

  it("honors channels.json overrides for stable and canary", async () => {
    const tmp = await makeTempRegistry();
    await writeVersionDir(tmp, "uniswap_v3", "0.1.0", defaultMetadata("Uniswap V3"));
    await writeVersionDir(tmp, "uniswap_v3", "0.2.0", defaultMetadata("Uniswap V3"));
    await writeVersionDir(tmp, "uniswap_v3", "1.0.0", defaultMetadata("Uniswap V3"));
    // Pin stable to an older version, with a canary pointer at the newest.
    await fs.writeFile(
      path.join(tmp, "public", "adapters", "uniswap_v3", "channels.json"),
      JSON.stringify({ stable: "0.2.0", canary: "1.0.0" })
    );

    const result = runBuildManifest(tmp);
    expect(result.status, result.stderr).toBe(0);

    const parsed = parseAdapterManifest(await readGeneratedManifest(tmp));
    const adapter = parsed.adapters[0]!;
    expect(adapter.stable_version).toBe("0.2.0");
    expect(adapter.canary_version).toBe("1.0.0");

    await fs.rm(tmp, { recursive: true, force: true });
  });

  it("errors when channels.json pins a non-existent version", async () => {
    const tmp = await makeTempRegistry();
    await writeVersionDir(tmp, "uniswap_v3", "0.1.0", defaultMetadata("Uniswap V3"));
    await fs.writeFile(
      path.join(tmp, "public", "adapters", "uniswap_v3", "channels.json"),
      JSON.stringify({ stable: "9.9.9" })
    );

    const result = runBuildManifest(tmp);
    expect(result.status).not.toBe(0);
    expect(result.stderr).toMatch(/9\.9\.9/);

    await fs.rm(tmp, { recursive: true, force: true });
  });

  it("emits multiple protocols in alphabetic order", async () => {
    const tmp = await makeTempRegistry();
    await writeVersionDir(tmp, "uniswap_v3", "0.1.0", defaultMetadata("Uniswap V3"));
    await writeVersionDir(tmp, "aave_v3", "0.1.0", defaultMetadata("Aave V3"));
    await writeVersionDir(tmp, "curve", "0.1.0", defaultMetadata("Curve"));

    const result = runBuildManifest(tmp);
    expect(result.status, result.stderr).toBe(0);

    const parsed = parseAdapterManifest(await readGeneratedManifest(tmp));
    expect(parsed.adapters.map((a) => a.protocol)).toEqual([
      "aave_v3",
      "curve",
      "uniswap_v3",
    ]);

    await fs.rm(tmp, { recursive: true, force: true });
  });

  it("rejects invalid metadata.json — supported_chains not an array", async () => {
    const tmp = await makeTempRegistry();
    await writeVersionDir(tmp, "uniswap_v3", "0.1.0", {
      display_name: "Uniswap V3",
      // Wrong type — should make build-manifest.js bail.
      supported_chains: "not-an-array",
      supported_addresses: [],
      host_capabilities: [],
    });

    const result = runBuildManifest(tmp);
    expect(result.status).not.toBe(0);
    expect(result.stderr).toMatch(/supported_chains/);

    await fs.rm(tmp, { recursive: true, force: true });
  });

  it("refuses to ship a protocol that has no display_name in any metadata.json", async () => {
    const tmp = await makeTempRegistry();
    // No metadata.json at all → no display_name source.
    await writeVersionDir(tmp, "uniswap_v3", "0.1.0");

    const result = runBuildManifest(tmp);
    expect(result.status).not.toBe(0);
    expect(result.stderr).toMatch(/display_name/);

    await fs.rm(tmp, { recursive: true, force: true });
  });
});

describe("parseAdapterManifest — malformed input rejection", () => {
  it("rejects an adapter version missing sha256", () => {
    const bad = {
      schema_version: 1,
      generated_at: "2026-05-15T12:00:00.000Z",
      adapters: [
        {
          protocol: "uniswap_v3",
          display_name: "Uniswap V3",
          stable_version: "0.1.0",
          canary_version: null,
          versions: [
            {
              version: "0.1.0",
              url: "/adapters/uniswap_v3/0.1.0/adapter.wasm",
              // sha256 omitted on purpose
              size_bytes: 16,
              supported_chains: [],
              supported_addresses: [],
              host_capabilities: [],
              signature: null,
              signer_id: null,
              published_at: "2026-05-15T12:00:00.000Z",
              revoked: false,
            },
          ],
        },
      ],
    };
    expect(() => parseAdapterManifest(bad)).toThrow(AdapterManifestError);
    expect(() => parseAdapterManifest(bad)).toThrow(/sha256/);
  });

  it("rejects an adapter version with a non-hex sha256", () => {
    const bad = {
      schema_version: 1,
      generated_at: "2026-05-15T12:00:00.000Z",
      adapters: [
        {
          protocol: "uniswap_v3",
          display_name: "Uniswap V3",
          stable_version: "0.1.0",
          canary_version: null,
          versions: [
            {
              version: "0.1.0",
              url: "/adapters/uniswap_v3/0.1.0/adapter.wasm",
              sha256: "not-a-hash",
              size_bytes: 16,
              supported_chains: [],
              supported_addresses: [],
              host_capabilities: [],
              signature: null,
              signer_id: null,
              published_at: "2026-05-15T12:00:00.000Z",
              revoked: false,
            },
          ],
        },
      ],
    };
    expect(() => parseAdapterManifest(bad)).toThrow(AdapterManifestError);
  });

  it("rejects unknown schema_version", () => {
    expect(() =>
      parseAdapterManifest({
        schema_version: 999,
        generated_at: "2026-05-15T12:00:00.000Z",
        adapters: [],
      })
    ).toThrow(AdapterManifestError);
  });

  it("rejects stable_version that is not in versions[]", () => {
    expect(() =>
      parseAdapterManifest({
        schema_version: 1,
        generated_at: "2026-05-15T12:00:00.000Z",
        adapters: [
          {
            protocol: "uniswap_v3",
            display_name: "Uniswap V3",
            stable_version: "0.9.9",
            canary_version: null,
            versions: [
              {
                version: "0.1.0",
                url: "/adapters/uniswap_v3/0.1.0/adapter.wasm",
                sha256: ZERO_SHA256,
                size_bytes: 16,
                supported_chains: [],
                supported_addresses: [],
                host_capabilities: [],
                signature: null,
                signer_id: null,
                published_at: "2026-05-15T12:00:00.000Z",
                revoked: false,
              },
            ],
          },
        ],
      })
    ).toThrow(/stable_version "0\.9\.9"/);
  });
});
