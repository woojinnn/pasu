// Generate token registry entries for Curve source-materialized pools.
//
// The filename is kept for compatibility with older notes, but the generator now
// covers every Curve pool family promoted through source materialization.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const UNIVERSE = join(ROOT, "surface", "curve", "_pool_universe.json");
const TOKENS = join(ROOT, "tokens");

const FAMILY_CONFIGS = [
  {
    family: "factory",
    curveIdPrefix: "factory-v2-",
    chains: new Set([1, 8453]),
    protocol: "curve_stableswap",
    source: {
      1: "https://api.curve.fi/api/getPools/ethereum/factory",
      8453: "https://api.curve.fi/api/getPools/base/factory",
    },
  },
  {
    family: "factory-crvusd",
    curveIdPrefix: "factory-crvusd-",
    chains: new Set([1]),
    protocol: "curve_stableswap",
    source: {
      1: "https://api.curve.fi/api/getPools/ethereum/factory-crvusd",
    },
  },
  {
    family: "factory-stable-ng",
    chains: new Set([1, 8453]),
    protocol: "curve_stableswap_ng",
    source: {
      1: "https://api.curve.fi/api/getPools/ethereum/factory-stable-ng",
      8453: "https://api.curve.fi/api/getPools/base/factory-stable-ng",
    },
  },
  {
    family: "factory-twocrypto",
    curveIdPrefix: "factory-twocrypto-",
    chains: new Set([1, 8453]),
    protocol: "curve_twocrypto",
    source: {
      1: "https://api.curve.fi/api/getPools/ethereum/factory-twocrypto",
      8453: "https://api.curve.fi/api/getPools/base/factory-twocrypto",
    },
  },
  {
    family: "factory-crypto",
    curveIdPrefix: "factory-crypto-",
    chains: new Set([1, 8453]),
    protocol: "curve_cryptoswap",
    source: {
      1: "https://api.curve.fi/api/getPools/ethereum/factory-crypto",
      8453: "https://api.curve.fi/api/getPools/base/factory-crypto",
    },
  },
  {
    family: "factory-tricrypto",
    curveIdPrefix: "factory-tricrypto-",
    chains: new Set([1, 8453]),
    protocol: "curve_tricrypto",
    source: {
      1: "https://api.curve.fi/api/getPools/ethereum/factory-tricrypto",
      8453: "https://api.curve.fi/api/getPools/base/factory-tricrypto",
    },
  },
];

const ADDR_RE = /^0x[0-9a-f]{40}$/;
const ZERO_RE = /^0x0{40}$/;

function tokenRef(chainId, address) {
  return {
    key: {
      standard: "erc20",
      chain: `eip155:${chainId}`,
      address,
    },
  };
}

function tokenPath(chainId, address) {
  return join(TOKENS, String(chainId), `${address}.json`);
}

function writeIfMissing(chainId, address, data) {
  const path = tokenPath(chainId, address);
  if (existsSync(path)) return false;
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, JSON.stringify(data, null, 2) + "\n", "utf8");
  return true;
}

function activeCoins(row) {
  return (row.coins ?? [])
    .map((coin) => String(coin).toLowerCase())
    .filter((coin) => ADDR_RE.test(coin) && !ZERO_RE.test(coin));
}

function matchingConfig(row) {
  if (row.decision !== "cover") return undefined;
  const chainId = row.chainId;
  const curveId = String(row.curve_id ?? "");
  return FAMILY_CONFIGS.find((config) => {
    if (!config.chains.has(chainId)) return false;
    if ((row.families ?? []).includes(config.family)) return true;
    return Boolean(config.curveIdPrefix && curveId.startsWith(config.curveIdPrefix));
  });
}

const universe = JSON.parse(readFileSync(UNIVERSE, "utf8"));
let lpWritten = 0;
let coinWritten = 0;
let skipped = 0;

for (const row of universe.candidates ?? []) {
  const config = matchingConfig(row);
  if (!config) continue;
  const chainId = row.chainId;
  const coins = activeCoins(row);
  if (coins.length < 2) {
    skipped++;
    continue;
  }

  const poolAddress = String(row.address).toLowerCase();
  const lpToken = String(row.lpTokenAddress ?? row.address).toLowerCase();
  if (!ADDR_RE.test(poolAddress) || !ADDR_RE.test(lpToken) || ZERO_RE.test(lpToken)) {
    skipped++;
    continue;
  }
  const source = config.source[chainId];

  const lp = {
    erc_kind: "erc20",
    chainId,
    address: lpToken,
    symbol: row.symbol ?? row.curve_id ?? "Curve LP",
    decimals: 18,
    name: row.name ?? row.curve_id ?? "Curve LP",
    source,
    token_kind: {
      kind: "lp_share",
      pool: {
        protocol: {
          name: config.protocol,
          chain: `eip155:${chainId}`,
        },
        pool_addr: poolAddress,
      },
      underlyings: coins.map((coin) => tokenRef(chainId, coin)),
      share_form: "fungible",
      shape: {
        kind: "pooled",
      },
    },
  };
  if (writeIfMissing(chainId, lpToken, lp)) lpWritten++;

  for (const coin of row.coinDetails ?? []) {
    const address = String(coin.address).toLowerCase();
    if (!ADDR_RE.test(address) || ZERO_RE.test(address)) continue;
    const decimals = Number(coin.decimals);
    const metadata = {
      erc_kind: "erc20",
      chainId,
      address,
      symbol: coin.symbol ?? address,
      decimals: Number.isFinite(decimals) ? decimals : 18,
      name: coin.name ?? coin.symbol ?? address,
      source,
    };
    if (writeIfMissing(chainId, address, metadata)) coinWritten++;
  }
}

console.log(
  `curve source tokens: wrote ${lpWritten} LP + ${coinWritten} coin token file(s), skipped ${skipped} row(s)`,
);
