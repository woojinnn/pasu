/**
 * check-surface-completeness.test.ts — source manifest surface accounting.
 *
 * Run from browser-extension/ with its bundled vitest:
 *
 *   node .yarn/releases/yarn-4.14.1.cjs vitest run \
 *     --root ../registryV2 scripts/__tests__/check-surface-completeness.test.ts
 */

import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const SCRIPTS_DIR = resolve(__dirname, "..");
const CHECK_SURFACE = join(SCRIPTS_DIR, "check-surface-completeness.ts");
const REGISTRY_V2_ROOT = resolve(SCRIPTS_DIR, "..");
const TSX_BIN = join(REGISTRY_V2_ROOT, "node_modules", ".bin", "tsx");

const NFT = "0x1111111111111111111111111111111111111111";
const DEBT = "0x2222222222222222222222222222222222222222";

interface RunResult {
  status: number;
  stdout: string;
  stderr: string;
}

function writeJson(path: string, body: unknown): void {
  writeFileSync(path, JSON.stringify(body, null, 2), "utf8");
}

function runSurfaceGate(registryRoot: string): RunResult {
  try {
    const stdout = execFileSync(TSX_BIN, [CHECK_SURFACE], {
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

function scaffoldTokenSourceRegistry(): string {
  const root = mkdtempSync(join(tmpdir(), "sb-surface-gate-"));
  mkdirSync(join(root, "manifests", "standard", "erc721"), { recursive: true });
  mkdirSync(join(root, "surface", "standard"), { recursive: true });
  mkdirSync(join(root, "tokens", "1"), { recursive: true });

  writeJson(join(root, "tokens", "1", `${NFT}.json`), {
    erc_kind: "erc721",
    chainId: 1,
    address: NFT,
    collection_name: "Fixture NFT",
    symbol: "FIX",
  });

  writeJson(join(root, "manifests", "standard", "erc721", "transferFrom@1.0.0.json"), {
    type: "adapter_action",
    id: "standard/erc721/transferFrom@1.0.0",
    schema_version: "3",
    match: {
      selector: "0x23b872dd",
      chain_to_addresses_source: "tokens:erc721",
      chain_ids: [1],
    },
    abi_fragment: {
      function_name: "transferFrom",
      abi: {
        name: "transferFrom",
        type: "function",
        stateMutability: "nonpayable",
        inputs: [
          { name: "from", type: "address" },
          { name: "to", type: "address" },
          { name: "tokenId", type: "uint256" },
        ],
        outputs: [],
      },
    },
    emit: { strategy: "single_emit", body: { domain: "unknown" }, live_inputs: {} },
    requires: { imperative: [], adapter_capabilities: [], host_capabilities: [], extension: ">=0.1.0" },
  });

  writeJson(join(root, "surface", "standard", "fixture-nft.abi.json"), {
    source: "test-fixture",
    chainId: 1,
    address: NFT,
    contract: "FixtureNFT",
    abi: [
      {
        name: "transferFrom",
        type: "function",
        stateMutability: "nonpayable",
        inputs: [
          { name: "from", type: "address" },
          { name: "to", type: "address" },
          { name: "tokenId", type: "uint256" },
        ],
        outputs: [],
      },
    ],
  });
  writeJson(join(root, "surface", "standard", "fixture-nft.coverage.json"), {
    contract: "FixtureNFT",
    chainId: 1,
    address: NFT,
    snapshot: "fixture-nft.abi.json",
    functions: {
      "0x23b872dd": {
        name: "transferFrom",
        decision: "cover",
        reason: "standard ERC721 transfer primitive",
      },
    },
  });

  return root;
}

function scaffoldExcludedDebtReceiptRegistry(): string {
  const root = mkdtempSync(join(tmpdir(), "sb-surface-gate-"));
  mkdirSync(join(root, "manifests", "standard", "erc20"), { recursive: true });
  mkdirSync(join(root, "surface", "standard"), { recursive: true });
  mkdirSync(join(root, "tokens", "1"), { recursive: true });

  writeJson(join(root, "tokens", "1", `${DEBT}.json`), {
    erc_kind: "erc20",
    chainId: 1,
    address: DEBT,
    name: "Fixture Debt Token",
    symbol: "debtFIX",
    decimals: 18,
    token_kind: { kind: "debt_receipt" },
  });

  writeJson(join(root, "manifests", "standard", "erc20", "approve@1.0.0.json"), {
    type: "adapter_action",
    id: "standard/erc20/approve@1.0.0",
    schema_version: "3",
    match: {
      selector: "0x095ea7b3",
      chain_to_addresses_source: "tokens:erc20",
      chain_ids: [1],
      semantic_token_kind_exclude: ["debt_receipt"],
    },
    abi_fragment: {
      function_name: "approve",
      abi: {
        name: "approve",
        type: "function",
        stateMutability: "nonpayable",
        inputs: [
          { name: "spender", type: "address" },
          { name: "amount", type: "uint256" },
        ],
        outputs: [{ name: "", type: "bool" }],
      },
    },
    emit: { strategy: "single_emit", body: { domain: "unknown" }, live_inputs: {} },
    requires: { imperative: [], adapter_capabilities: [], host_capabilities: [], extension: ">=0.1.0" },
  });

  writeJson(join(root, "surface", "standard", "fixture-debt.abi.json"), {
    source: "test-fixture",
    chainId: 1,
    address: DEBT,
    contract: "FixtureDebt",
    abi: [
      {
        name: "approve",
        type: "function",
        stateMutability: "nonpayable",
        inputs: [
          { name: "spender", type: "address" },
          { name: "amount", type: "uint256" },
        ],
        outputs: [{ name: "", type: "bool" }],
      },
    ],
  });
  writeJson(join(root, "surface", "standard", "fixture-debt.coverage.json"), {
    contract: "FixtureDebt",
    chainId: 1,
    address: DEBT,
    snapshot: "fixture-debt.abi.json",
    functions: {
      "0x095ea7b3": {
        name: "approve",
        decision: "exclude",
        reason: "non-transferable debt receipt compatibility method",
      },
    },
  });

  return root;
}

let roots: string[] = [];

beforeEach(() => {
  roots = [];
});

afterEach(() => {
  for (const root of roots) {
    rmSync(root, { recursive: true, force: true });
  }
});

describe("check-surface-completeness source manifests", () => {
  it("counts token-source manifests for gated token contracts without emitting ungated noise", () => {
    const root = scaffoldTokenSourceRegistry();
    roots.push(root);

    const result = runSurfaceGate(root);

    expect(result.status, `stdout:\n${result.stdout}\nstderr:\n${result.stderr}`).toBe(0);
    expect(result.stdout).toContain("PASS");
    expect(result.stdout).not.toContain("UNGATED");
  });

  it("does not count token-source manifests against gated token contracts excluded by semantic kind", () => {
    const root = scaffoldExcludedDebtReceiptRegistry();
    roots.push(root);

    const result = runSurfaceGate(root);

    expect(result.status, `stdout:\n${result.stdout}\nstderr:\n${result.stderr}`).toBe(0);
    expect(result.stdout).toContain("0 on-chain manifests");
    expect(result.stdout).toContain("PASS");
  });
});
