#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const repoRoot = path.resolve(path.dirname(__filename), "../../..");

const args = new Map();
for (let i = 2; i < process.argv.length; i += 1) {
  const arg = process.argv[i];
  if (arg.startsWith("--")) {
    const next = process.argv[i + 1];
    if (next && !next.startsWith("--")) {
      args.set(arg, next);
      i += 1;
    } else {
      args.set(arg, "true");
    }
  }
}

const protocol = args.get("--protocol") ?? "uniswap";
const envPath = path.resolve(repoRoot, args.get("--env") ?? "crates/integration-tests/.env");
const universePath = path.resolve(
  repoRoot,
  args.get("--universe") ?? `registryV2/surface/${protocol}/_address_universe.json`,
);
const outPath = path.resolve(
  repoRoot,
  args.get("--out") ??
    `crates/integration-tests/onboarding/${protocol}/v2-lp-token-inventory-summary.json`,
);
const tokensRoot = path.resolve(repoRoot, args.get("--tokens-root") ?? "registryV2/tokens");
const writeTokens = args.get("--write-tokens") === "true";
const allowPartial = args.get("--allow-partial") === "true";
const timeoutMs = Number(args.get("--timeout-ms") ?? "15000");
const delayMs = Number(args.get("--delay-ms") ?? "230");

const SELECTORS = {
  token0: "0x0dfe1681",
  token1: "0xd21220a7",
  decimals: "0x313ce567",
  symbol: "0x95d89b41",
  name: "0x06fdde03",
};

const CHAIN_NAMES = new Map([
  [1, "ethereum"],
  [10, "optimism"],
  [8453, "base"],
  [42161, "arbitrum"],
]);

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function readEnv(file) {
  const env = {};
  if (!fs.existsSync(file)) return env;
  for (const line of fs.readFileSync(file, "utf8").split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const eq = trimmed.indexOf("=");
    if (eq < 0) continue;
    const key = trimmed.slice(0, eq).trim();
    let value = trimmed.slice(eq + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    env[key] = value;
  }
  return env;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function normalizeAddress(value) {
  if (typeof value !== "string") return "";
  const normalized = value.toLowerCase();
  return /^0x[0-9a-f]{40}$/.test(normalized) ? normalized : "";
}

function decodeAddress(hex) {
  if (typeof hex !== "string" || !hex.startsWith("0x") || hex.length < 42) return "";
  return normalizeAddress(`0x${hex.slice(-40)}`);
}

function decodeUint(hex) {
  if (typeof hex !== "string" || !hex.startsWith("0x") || hex === "0x") return null;
  return Number(BigInt(hex));
}

function decodeString(hex) {
  if (typeof hex !== "string" || !hex.startsWith("0x") || hex === "0x") return "";
  let body = hex.slice(2);
  try {
    if (body.length >= 128) {
      const offset = Number(BigInt(`0x${body.slice(0, 64)}`));
      const len = Number(BigInt(`0x${body.slice(offset * 2, offset * 2 + 64)}`));
      body = body.slice(offset * 2 + 64, offset * 2 + 64 + len * 2);
    } else {
      body = body.replace(/(?:00)+$/u, "");
    }
    return Buffer.from(body, "hex").toString("utf8").replace(/\u0000+$/u, "");
  } catch {
    return "";
  }
}

async function etherscanCall({ apiKey, chainId, address, data }) {
  const params = new URLSearchParams({
    chainid: String(chainId),
    module: "proxy",
    action: "eth_call",
    to: address,
    data,
    tag: "latest",
    apikey: apiKey,
  });
  const url = `https://api.etherscan.io/v2/api?${params.toString()}`;
  let lastError;
  for (let attempt = 1; attempt <= 4; attempt += 1) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
      const body = await response.json();
      if (typeof body.result === "string" && body.result.startsWith("0x")) return body.result;
      lastError = `${body.message ?? "ERROR"}: ${body.result ?? ""}`;
      if (/rate limit|Max rate|temporarily unavailable|timeout/i.test(lastError)) {
        await sleep(delayMs * attempt * 4);
        continue;
      }
      throw new Error(lastError);
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
      await sleep(delayMs * attempt * 4);
    }
  }
  throw new Error(lastError ?? "unknown Etherscan eth_call error");
}

function loadV2Candidates() {
  const universe = readJson(universePath);
  return (universe.candidates ?? [])
    .filter((row) => row.batch === "uniswap-v2-child-universe-deferred")
    .map((row) => ({
      chainId: Number(row.chainId),
      address: normalizeAddress(row.address),
      decision: row.decision,
      reason: row.reason,
      batch: row.batch,
    }))
    .filter((row) => row.chainId && row.address);
}

function tokenDocument(row) {
  const symbol = row.symbol || "UNI-V2";
  const name = row.name || `Uniswap V2 ${row.address}`;
  return {
    erc_kind: "erc20",
    chainId: row.chainId,
    address: row.address,
    symbol,
    decimals: row.decimals ?? 18,
    name,
    source: "Etherscan v2 eth_call over representative Dune PairCreated candidates",
    token_kind: {
      kind: "lp_share",
      pool: {
        protocol: {
          name: "uniswap_v2",
          chain: `eip155:${row.chainId}`,
        },
        pool_addr: row.address,
      },
      underlyings: [row.token0, row.token1].filter(Boolean).map((address) => ({
        key: {
          standard: "erc20",
          chain: `eip155:${row.chainId}`,
          address,
        },
      })),
      share_form: "fungible",
      shape: {
        kind: "constant_product",
      },
    },
  };
}

function writeToken(row) {
  const dir = path.join(tokensRoot, String(row.chainId));
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(dir, `${row.address}.json`);
  fs.writeFileSync(file, `${JSON.stringify(tokenDocument(row), null, 2)}\n`, "utf8");
  return path.relative(repoRoot, file);
}

async function main() {
  const env = { ...process.env, ...readEnv(envPath) };
  const apiKey = env.ETHERSCAN_API_KEY;
  if (!apiKey) throw new Error(`ETHERSCAN_API_KEY missing in environment or ${envPath}`);

  const rows = [];
  const errors = [];
  for (const candidate of loadV2Candidates()) {
    const row = { ...candidate, chain: CHAIN_NAMES.get(candidate.chainId) ?? String(candidate.chainId) };
    try {
      row.token0 = decodeAddress(
        await etherscanCall({
          apiKey,
          chainId: candidate.chainId,
          address: candidate.address,
          data: SELECTORS.token0,
        }),
      );
      row.token1 = decodeAddress(
        await etherscanCall({
          apiKey,
          chainId: candidate.chainId,
          address: candidate.address,
          data: SELECTORS.token1,
        }),
      );
      row.decimals = decodeUint(
        await etherscanCall({
          apiKey,
          chainId: candidate.chainId,
          address: candidate.address,
          data: SELECTORS.decimals,
        }),
      );
      row.symbol = decodeString(
        await etherscanCall({
          apiKey,
          chainId: candidate.chainId,
          address: candidate.address,
          data: SELECTORS.symbol,
        }),
      );
      row.name = decodeString(
        await etherscanCall({
          apiKey,
          chainId: candidate.chainId,
          address: candidate.address,
          data: SELECTORS.name,
        }),
      );
      if (!row.token0 || !row.token1) throw new Error("token0/token1 eth_call returned empty address");
      if (writeTokens) row.tokenFile = writeToken(row);
    } catch (error) {
      row.error = error instanceof Error ? error.message : String(error);
      errors.push({ chainId: candidate.chainId, address: candidate.address, error: row.error });
    }
    rows.push(row);
    await sleep(delayMs);
  }

  const summary = {
    generatedAt: new Date().toISOString(),
    protocol,
    source: "Etherscan v2 proxy eth_call over representative Uniswap V2 pair universe candidates",
    universePath: path.relative(repoRoot, universePath),
    writeTokens,
    allowPartial,
    candidatesSeen: rows.length,
    enriched: rows.filter((row) => row.token0 && row.token1 && !row.error).length,
    errors,
    rows,
  };
  fs.mkdirSync(path.dirname(outPath), { recursive: true });
  fs.writeFileSync(outPath, `${JSON.stringify(summary, null, 2)}\n`, "utf8");
  console.log(
    `uniswap v2 lp inventory: ${summary.enriched}/${summary.candidatesSeen} enriched` +
      (writeTokens ? " with token files" : ""),
  );
  console.log(`summary: ${path.relative(repoRoot, outPath)}`);
  if (errors.length > 0) {
    console.error(JSON.stringify(errors, null, 2));
    if (!allowPartial) process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
