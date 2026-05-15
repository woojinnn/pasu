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

async function makeTempRegistry(): Promise<string> {
  const tmp = await fs.mkdtemp(
    path.join(os.tmpdir(), "adapter-registry-test-")
  );
  await fs.mkdir(path.join(tmp, "public", "adapters"), { recursive: true });
  await fs.mkdir(path.join(tmp, "scripts"), { recursive: true });
  await fs.copyFile(BUILD_SCRIPT, path.join(tmp, "scripts", "build-manifest.js"));
  return tmp;
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
    const versionDir = path.join(
      tmp,
      "public",
      "adapters",
      "uniswap_v3",
      "0.1.0"
    );
    await fs.mkdir(versionDir, { recursive: true });
    await fs.writeFile(path.join(versionDir, "adapter.wasm"), "fake-wasm-bytes");
    await fs.writeFile(
      path.join(versionDir, "metadata.json"),
      JSON.stringify({
        display_name: "Uniswap V3",
        supported_chains: [1],
        supported_addresses: [
          {
            chain_id: 1,
            address: "0xE592427A0AEce92De3Edee1F18E0157C05861564",
          },
        ],
        host_capabilities: ["abi_resolver.v1"],
      })
    );

    const result = runBuildManifest(tmp);
    expect(result.status, result.stderr).toBe(0);

    const parsed = parseAdapterManifest(await readGeneratedManifest(tmp));
    expect(parsed.adapters).toHaveLength(1);

    const adapter = parsed.adapters[0]!;
    expect(adapter.protocol).toBe("uniswap_v3");
    expect(adapter.display_name).toBe("Uniswap V3");
    expect(adapter.stable_version).toBe("0.1.0");
    expect(adapter.versions).toHaveLength(1);

    const version = adapter.versions[0]!;
    expect(version.version).toBe("0.1.0");
    expect(version.wasm_url).toBe("/adapters/uniswap_v3/0.1.0/adapter.wasm");
    expect(version.sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(version.supported_chains).toEqual([1]);
    expect(version.supported_addresses).toEqual([
      { chain_id: 1, address: "0xE592427A0AEce92De3Edee1F18E0157C05861564" },
    ]);
    expect(version.host_capabilities).toEqual(["abi_resolver.v1"]);
    expect(version.revoked).toBe(false);

    await fs.rm(tmp, { recursive: true, force: true });
  });

  it("re-running the script over an unchanged tree is idempotent", async () => {
    const tmp = await makeTempRegistry();
    const versionDir = path.join(
      tmp,
      "public",
      "adapters",
      "uniswap_v3",
      "0.1.0"
    );
    await fs.mkdir(versionDir, { recursive: true });
    await fs.writeFile(path.join(versionDir, "adapter.wasm"), "fake-wasm-bytes");

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
    const versionDir = path.join(
      tmp,
      "public",
      "adapters",
      "uniswap_v3",
      "0.1.0"
    );
    await fs.mkdir(versionDir, { recursive: true });
    await fs.writeFile(path.join(versionDir, "adapter.wasm"), "fake-wasm-bytes");
    await fs.writeFile(path.join(versionDir, ".revoked"), "");

    expect(runBuildManifest(tmp).status).toBe(0);
    const parsed = parseAdapterManifest(await readGeneratedManifest(tmp));
    expect(parsed.adapters[0]!.versions[0]!.revoked).toBe(true);

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
          versions: [
            {
              version: "0.1.0",
              wasm_url: "/adapters/uniswap_v3/0.1.0/adapter.wasm",
              // sha256 omitted on purpose
              supported_chains: [],
              supported_addresses: [],
              host_capabilities: [],
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
          versions: [
            {
              version: "0.1.0",
              wasm_url: "/adapters/uniswap_v3/0.1.0/adapter.wasm",
              sha256: "not-a-hash",
              supported_chains: [],
              supported_addresses: [],
              host_capabilities: [],
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
            versions: [
              {
                version: "0.1.0",
                wasm_url: "/adapters/uniswap_v3/0.1.0/adapter.wasm",
                sha256: "0".repeat(64),
                supported_chains: [],
                supported_addresses: [],
                host_capabilities: [],
                revoked: false,
              },
            ],
          },
        ],
      })
    ).toThrow(/stable_version=0\.9\.9/);
  });
});
