/**
 * build-index.test.ts — by-typed-data index emission (Phase A.1 Task 2)
 *
 * Exercises the REAL `scripts/build-index.ts` end-to-end via `tsx`, isolated
 * through the `BUILD_INDEX_REGISTRY_ROOT` env override (the script's
 * REGISTRY_ROOT is otherwise script-location-relative, NOT cwd-relative, so a
 * plain `cd` into a temp dir would read the real registryV2/).
 *
 * registryV2 has no vitest of its own — run from `browser-extension/` with its
 * bundled vitest, pointing `--root` at registryV2:
 *
 *   cd browser-extension
 *   node .yarn/releases/yarn-4.14.1.cjs vitest run \
 *     --root ../registryV2 scripts/__tests__/build-index.test.ts
 */

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// scripts/__tests__/build-index.test.ts → scripts/build-index.ts
const SCRIPTS_DIR = resolve(__dirname, "..");
const BUILD_INDEX = join(SCRIPTS_DIR, "build-index.ts");
// registryV2 root (scripts/.. ) → node_modules/.bin/tsx
const REGISTRY_V2_ROOT = resolve(SCRIPTS_DIR, "..");
const TSX_BIN = join(REGISTRY_V2_ROOT, "node_modules", ".bin", "tsx");

const PERMIT2 = "0x000000000022d473030f116ddee9f6b43ac78ba3";
const ERC20_TOKEN = "0x1111111111111111111111111111111111111111";
const ERC721_TOKEN = "0x2222222222222222222222222222222222222222";

interface RunResult {
  status: number;
  stdout: string;
  stderr: string;
}

/** Scaffold a temp registry root with manifests/, write the given manifests. */
function scaffold(manifests: Record<string, unknown>): string {
  const root = mkdtempSync(join(tmpdir(), "sb-build-index-"));
  const manifestsDir = join(root, "manifests");
  mkdirSync(manifestsDir, { recursive: true });
  let i = 0;
  for (const [name, body] of Object.entries(manifests)) {
    const fname = name.endsWith(".json") ? name : `m${i}.json`;
    writeFileSync(join(manifestsDir, fname), JSON.stringify(body, null, 2), "utf8");
    i += 1;
  }
  return root;
}

/** Run the real build-index.ts against a temp REGISTRY_ROOT. Never throws. */
function runBuild(registryRoot: string): RunResult {
  try {
    const stdout = execFileSync(TSX_BIN, [BUILD_INDEX], {
      env: { ...process.env, BUILD_INDEX_REGISTRY_ROOT: registryRoot },
      stdio: "pipe",
      encoding: "utf8",
    });
    return { status: 0, stdout: stdout ?? "", stderr: "" };
  } catch (e) {
    const err = e as { status?: number; stdout?: Buffer | string; stderr?: Buffer | string };
    return {
      status: err.status ?? 1,
      stdout: err.stdout?.toString() ?? "",
      stderr: err.stderr?.toString() ?? "",
    };
  }
}

function typedDataDir(root: string): string {
  return join(root, "index", "by-typed-data");
}

function callkeyDir(root: string): string {
  return join(root, "index", "by-callkey");
}

function listTypedData(root: string): string[] {
  const dir = typedDataDir(root);
  if (!existsSync(dir)) return [];
  return readdirSync(dir).sort();
}

function listCallkeys(root: string): string[] {
  const dir = callkeyDir(root);
  if (!existsSync(dir)) return [];
  return readdirSync(dir).sort();
}

function writeToken(
  root: string,
  chainId: number,
  address: string,
  ercKind: "erc20" | "erc721" | "erc1155" | "native",
  overrides: Record<string, unknown> = {},
): void {
  const dir = join(root, "tokens", String(chainId));
  mkdirSync(dir, { recursive: true });
  const lower = address.toLowerCase();
  writeFileSync(
    join(dir, `${lower}.json`),
    JSON.stringify(
      {
        erc_kind: ercKind,
        chainId,
        address: lower,
        symbol: "TEST",
        decimals: ercKind === "erc20" ? 18 : 0,
        name: "Test Token",
        source: "https://example.invalid/test-token",
        token_kind: { kind: "unknown" },
        ...overrides,
      },
      null,
      2,
    ),
    "utf8",
  );
}

/** A Permit2-shaped manifest: vc present in every chain_to_addresses entry. */
function permit2Manifest(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    type: "adapter_action",
    id: "uniswap/permit2/permitSingle@1.0.0",
    schema_version: "3",
    match: {
      selector: "0x2b67b570",
      chain_to_addresses: {
        "1": [PERMIT2],
        "10": [PERMIT2],
        "8453": [PERMIT2],
        "42161": [PERMIT2],
      },
      typed_data: {
        domain_name: "Permit2",
        verifying_contract: PERMIT2,
        primary_type: "PermitSingle",
        types: {
          PermitSingle: [
            { name: "spender", type: "address" },
            { name: "sigDeadline", type: "uint256" },
          ],
        },
      },
    },
    emit: { strategy: "single_emit" },
    ...overrides,
  };
}

function erc20TransferManifest(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    type: "adapter_action",
    id: "standard/erc20/transfer@1.0.0",
    schema_version: "3",
    match: {
      selector: "0xa9059cbb",
      chain_to_addresses_source: "tokens:erc20",
      chain_ids: [1],
    },
    abi_fragment: {
      function_name: "transfer",
      abi: {
        type: "function",
        name: "transfer",
        inputs: [
          { name: "to", type: "address" },
          { name: "amount", type: "uint256" },
        ],
      },
    },
    emit: { strategy: "single_emit" },
    ...overrides,
  };
}

let roots: string[] = [];

beforeEach(() => {
  roots = [];
});

afterEach(() => {
  for (const r of roots) {
    rmSync(r, { recursive: true, force: true });
  }
});

function track(root: string): string {
  roots.push(root);
  return root;
}

describe("build-index by-typed-data emission", () => {
  it("(1) emits one by-typed-data entry per chain for a Permit2-shaped manifest", () => {
    const root = track(scaffold({ "permitSingle.json": permit2Manifest() }));
    const res = runBuild(root);
    expect(res.status, `stderr:\n${res.stderr}`).toBe(0);

    const files = listTypedData(root);
    // Order-independent: one entry per chain (emit order = chain_to_addresses
    // insertion order; listTypedData() applies its own .sort()).
    expect([...files].sort()).toEqual(
      [
        `1__${PERMIT2}__PermitSingle.json`,
        `10__${PERMIT2}__PermitSingle.json`,
        `42161__${PERMIT2}__PermitSingle.json`,
        `8453__${PERMIT2}__PermitSingle.json`,
      ].sort(),
    );

    // entry shape: matched + bundle_id + manifest_path + bundle_sha256 + bundle
    const entry = JSON.parse(readFileSync(join(typedDataDir(root), `1__${PERMIT2}__PermitSingle.json`), "utf8"));
    expect(entry.matched).toBe(true);
    expect(entry.bundle_id).toBe("uniswap/permit2/permitSingle@1.0.0");
    expect(entry.manifest_path).toBe("manifests/permitSingle.json");
    expect(typeof entry.bundle_sha256).toBe("string");
    expect(entry.bundle.match.typed_data.primary_type).toBe("PermitSingle");
  });

  it("(2) escapes a colon in primaryType to '__' in the filename", () => {
    // HyperLiquid-style primary type with EIP-712 colon.
    const vc = "0x1111111111111111111111111111111111111111";
    const manifest = {
      type: "adapter_action",
      id: "hyperliquid/usd-send/usdSend@1.0.0",
      schema_version: "3",
      match: {
        selector: "0xdeadbeef",
        chain_to_addresses: { "999": [vc] },
        typed_data: {
          domain_name: "HyperliquidSignTransaction",
          verifying_contract: vc,
          primary_type: "HyperliquidTransaction:UsdSend",
          types: { "HyperliquidTransaction:UsdSend": [{ name: "destination", type: "string" }] },
        },
      },
      emit: { strategy: "single_emit" },
    };
    const root = track(scaffold({ "usdSend.json": manifest }));
    const res = runBuild(root);
    expect(res.status, `stderr:\n${res.stderr}`).toBe(0);

    expect(listTypedData(root)).toEqual([`999__${vc}__HyperliquidTransaction__UsdSend.json`]);
  });

  it("(3) rejects a manifest whose typed_data.verifying_contract is absent from chain_to_addresses", () => {
    const vc = "0x2222222222222222222222222222222222222222";
    const manifest = permit2Manifest({
      match: {
        selector: "0x2b67b570",
        // vc below NOT in this map
        chain_to_addresses: { "1": [PERMIT2] },
        typed_data: {
          domain_name: "Permit2",
          verifying_contract: vc,
          primary_type: "PermitSingle",
          types: { PermitSingle: [{ name: "spender", type: "address" }] },
        },
      },
    });
    const root = track(scaffold({ "bad.json": manifest }));
    const res = runBuild(root);

    expect(res.status).not.toBe(0);
    // Validation message is on STDERR (process.exit(1) after console.error),
    // NOT on the thrown Error.message. Assert the stderr text.
    const combined = res.stderr + res.stdout;
    expect(combined).toMatch(/verifying_contract/);
    expect(combined).toMatch(/not in chain_to_addresses/);
    // no entry written
    expect(listTypedData(root)).toEqual([]);
  });

  it("(5) appends witness_type as a 4th filename segment when present", () => {
    // UniswapX-style Permit2-witness manifest: same (chain, vc, primary_type)
    // as a plain Permit2 sig, disambiguated by witness_type. The 4th segment
    // keeps the by-typed-data index file distinct.
    const manifest = permit2Manifest({
      match: {
        selector: "0x30f28b7a",
        chain_to_addresses: { "1": [PERMIT2] },
        typed_data: {
          domain_name: "Permit2",
          verifying_contract: PERMIT2,
          primary_type: "PermitWitnessTransferFrom",
          witness_type: "ExclusiveDutchOrder",
          types: {
            PermitWitnessTransferFrom: [
              { name: "spender", type: "address" },
              { name: "witness", type: "ExclusiveDutchOrder" },
            ],
          },
        },
      },
    });
    const root = track(scaffold({ "witness.json": manifest }));
    const res = runBuild(root);
    expect(res.status, `stderr:\n${res.stderr}`).toBe(0);

    expect(listTypedData(root)).toEqual([
      `1__${PERMIT2}__PermitWitnessTransferFrom__ExclusiveDutchOrder.json`,
    ]);

    // descriptor round-trips witness_type into the index entry's bundle.
    const entry = JSON.parse(
      readFileSync(
        join(
          typedDataDir(root),
          `1__${PERMIT2}__PermitWitnessTransferFrom__ExclusiveDutchOrder.json`,
        ),
        "utf8",
      ),
    );
    expect(entry.bundle.match.typed_data.witness_type).toBe(
      "ExclusiveDutchOrder",
    );
  });

  it("(6) without witness_type the filename stays the byte-identical 3-segment form", () => {
    // Backward compat: a typed_data block with NO witness_type produces exactly
    // the pre-T1 3-segment filename.
    const root = track(scaffold({ "permitSingle.json": permit2Manifest() }));
    const res = runBuild(root);
    expect(res.status, `stderr:\n${res.stderr}`).toBe(0);
    expect(listTypedData(root)).toContain(`1__${PERMIT2}__PermitSingle.json`);
    // No 4-segment variant leaked in.
    expect(
      listTypedData(root).some((f) => f.split("__").length > 3),
    ).toBe(false);
  });

  it("(7) two manifests colliding on (chain, vc, primary_type) but differing in witness_type both emit (no overwrite)", () => {
    const manifestA = permit2Manifest({
      id: "uniswapx/test/orderA@1.0.0",
      match: {
        selector: "0x00000001",
        chain_to_addresses: { "1": [PERMIT2] },
        typed_data: {
          domain_name: "Permit2",
          verifying_contract: PERMIT2,
          primary_type: "PermitWitnessTransferFrom",
          witness_type: "OrderA",
          types: {
            PermitWitnessTransferFrom: [{ name: "witness", type: "OrderA" }],
          },
        },
      },
    });
    const manifestB = permit2Manifest({
      id: "uniswapx/test/orderB@1.0.0",
      match: {
        selector: "0x00000002",
        chain_to_addresses: { "1": [PERMIT2] },
        typed_data: {
          domain_name: "Permit2",
          verifying_contract: PERMIT2,
          primary_type: "PermitWitnessTransferFrom",
          witness_type: "OrderB",
          types: {
            PermitWitnessTransferFrom: [{ name: "witness", type: "OrderB" }],
          },
        },
      },
    });
    const root = track(
      scaffold({ "orderA.json": manifestA, "orderB.json": manifestB }),
    );
    const res = runBuild(root);
    expect(res.status, `stderr:\n${res.stderr}`).toBe(0);

    expect([...listTypedData(root)].sort()).toEqual(
      [
        `1__${PERMIT2}__PermitWitnessTransferFrom__OrderA.json`,
        `1__${PERMIT2}__PermitWitnessTransferFrom__OrderB.json`,
      ].sort(),
    );
  });

  it("(8) rejects duplicate typed-data keys before overwrite", () => {
    const manifestA = permit2Manifest({ id: "uniswap/permit2/a@1.0.0" });
    const manifestB = permit2Manifest({ id: "uniswap/permit2/b@1.0.0" });
    const root = track(scaffold({ "a.json": manifestA, "b.json": manifestB }));
    const res = runBuild(root);

    expect(res.status).not.toBe(0);
    const combined = res.stderr + res.stdout;
    expect(combined).toMatch(/duplicate typed-data index key/);
    expect(combined).toMatch(/PermitSingle/);
  });

  it("(4) emits NO by-typed-data entry when a manifest has no typed_data", () => {
    const plain = {
      type: "adapter_action",
      id: "uniswap/v2-router-02/swap@1.0.0",
      schema_version: "3",
      match: {
        selector: "0x38ed1739",
        chain_to_addresses: { "1": ["0x7a250d5630b4cf539739df2c5dacb4c659f2488d"] },
      },
      emit: { strategy: "single_emit" },
    };
    const root = track(scaffold({ "swap.json": plain }));
    const res = runBuild(root);
    expect(res.status, `stderr:\n${res.stderr}`).toBe(0);

    // by-typed-data dir is created (wiped) but empty; by-callkey has the entry.
    expect(listTypedData(root)).toEqual([]);
    expect(listCallkeys(root).length).toBe(1);
  });
});

describe("build-index token source expansion", () => {
  it("expands tokens:erc20 into concrete callkeys backed by a shared bundle ref", () => {
    const root = track(scaffold({ "transfer.json": erc20TransferManifest() }));
    writeToken(root, 1, ERC20_TOKEN, "erc20", { symbol: "T20" });
    writeToken(root, 1, ERC721_TOKEN, "erc721", { symbol: "NFT" });

    const res = runBuild(root);
    expect(res.status, `stderr:\n${res.stderr}`).toBe(0);

    expect(listCallkeys(root)).toEqual([`1__${ERC20_TOKEN}__0xa9059cbb.json`]);

    const entry = JSON.parse(
      readFileSync(join(callkeyDir(root), `1__${ERC20_TOKEN}__0xa9059cbb.json`), "utf8"),
    );
    expect(entry.schema_version).toBe("3-ref");
    expect(entry.bundle_ref).toMatch(/^bundles\/0x[0-9a-f]{64}\.json$/);
    expect(entry.context_ref).toBeUndefined();
    expect(entry.bundle).toBeUndefined();
    const bundle = JSON.parse(readFileSync(join(root, entry.bundle_ref), "utf8"));
    expect(bundle.match.chain_to_addresses).toEqual({ "1": [ERC20_TOKEN] });
    expect("chain_to_addresses_source" in bundle.match).toBe(false);
    expect("chain_ids" in bundle.match).toBe(false);
  });

  it("rejects tokens:erc20 when the requested chain has no token directory", () => {
    const root = track(scaffold({ "transfer.json": erc20TransferManifest() }));
    const res = runBuild(root);

    expect(res.status).not.toBe(0);
    const combined = res.stderr + res.stdout;
    expect(combined).toMatch(/tokens\/1\/ does not exist/);
  });

  it("rejects token metadata whose chainId disagrees with its directory", () => {
    const root = track(scaffold({ "transfer.json": erc20TransferManifest() }));
    writeToken(root, 1, ERC20_TOKEN, "erc20", { chainId: 8453 });

    const res = runBuild(root);
    expect(res.status).not.toBe(0);
    const combined = res.stderr + res.stdout;
    expect(combined).toMatch(/chainId field \(8453\) does not match directory \(1\)/);
  });
});

describe("build-index protocol source materialization", () => {
  it("emits a small route entry plus template/context refs for a sourced Curve pool", () => {
    const pool = "0x3333333333333333333333333333333333333333";
    const coin0 = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const coin1 = "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const root = track(
      scaffold({
        "curve-source.json": {
          type: "adapter_action",
          id: "curve/stableswap-ng/source/test/exchange@1.0.0",
          schema_version: "3",
          match: {
            selector: "0x3df02124",
            chain_to_addresses_source: "curve:factory_stable_ng_2coin_mainnet",
            chain_ids: [1],
          },
          source_materialize: { kind: "per_address_context" },
          abi_fragment: {
            function_name: "exchange",
            abi: {
              type: "function",
              name: "exchange",
              inputs: [
                { name: "i", type: "int128" },
                { name: "j", type: "int128" },
                { name: "_dx", type: "uint256" },
                { name: "_min_dy", type: "uint256" },
              ],
            },
          },
          emit: {
            strategy: "single_emit",
            body: {
              token_in: {
                $match: "$args.i",
                $cases: {
                  "0": "$source.coins.0",
                  "1": "$source.coins.1",
                },
              },
            },
          },
        },
      }),
    );
    const surfaceDir = join(root, "surface", "curve");
    mkdirSync(surfaceDir, { recursive: true });
    writeFileSync(
      join(surfaceDir, "_pool_universe.json"),
      JSON.stringify(
        {
          protocol: "curve",
          source: "test",
          source_count: 1,
          candidates: [
            {
              chainId: 1,
              address: pool,
              decision: "cover",
              reason: "test",
              batch: "test",
              families: ["factory-stable-ng"],
              curve_id: "factory-stable-ng-0",
              name: "Test Pool",
              symbol: "TEST",
              lpTokenAddress: pool,
              coins: [coin0, coin1],
            },
          ],
        },
        null,
        2,
      ),
      "utf8",
    );

    const res = runBuild(root);
    expect(res.status, `stderr:\n${res.stderr}`).toBe(0);
    expect(listCallkeys(root)).toEqual([`1__${pool}__0x3df02124.json`]);

    const entry = JSON.parse(
      readFileSync(join(callkeyDir(root), `1__${pool}__0x3df02124.json`), "utf8"),
    );
    expect(entry.schema_version).toBe("3-ref");
    expect(entry.bundle_id).toMatch(
      /^curve\/stableswap-ng\/source\/test\/exchange\/1-factory-stable-ng-0-33333333@1\.0\.0$/,
    );
    expect(entry.bundle_ref).toMatch(/^bundles\/0x[0-9a-f]{64}\.json$/);
    expect(entry.context_ref).toBe(
      `contexts/curve/factory_stable_ng_2coin_mainnet/1/${pool}.json`,
    );
    expect(entry.bundle).toBeUndefined();

    const template = JSON.parse(readFileSync(join(root, entry.bundle_ref), "utf8"));
    expect(template.match.chain_to_addresses_source).toBe(
      "curve:factory_stable_ng_2coin_mainnet",
    );
    expect(template.source_materialize).toEqual({ kind: "per_address_context" });
    expect(template.emit.body.token_in.$cases).toEqual({
      "0": "$source.coins.0",
      "1": "$source.coins.1",
    });

    const context = JSON.parse(readFileSync(join(root, entry.context_ref), "utf8"));
    expect(context.schema_version).toBe("3-source-context");
    expect(context.chain_id).toBe(1);
    expect(context.address).toBe(pool);
    expect(context.context.coins).toEqual([coin0, coin1]);
    expect(context.context.id_suffix).toBe("1-factory-stable-ng-0-33333333");
  });
});
