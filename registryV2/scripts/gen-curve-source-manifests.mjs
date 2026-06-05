// Generate Curve source-materialized manifest templates.
//
// These are not concrete per-pool manifests. They are build-index templates
// consumed by protocol resolvers that provide one `$source.*` context per pool.

import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { toFunctionSelector } from "viem";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const CURVE_MANIFESTS = join(ROOT, "manifests", "curve");
const STABLESWAP_NG_MANIFESTS = join(CURVE_MANIFESTS, "stableswap-ng");
const TWOCRYPTO_MANIFESTS = join(CURVE_MANIFESTS, "twocrypto");
const CRYPTOSWAP_MANIFESTS = join(CURVE_MANIFESTS, "cryptoswap");

const COIN_COUNTS = [2, 3, 4, 5, 6, 7, 8];
const BASE_COIN_COUNTS = [2, 3, 4, 5, 6, 8];
const LEGACY_FACTORY_V2_COIN_COUNTS = [2, 3, 4];

const GROUPS = [
  ...LEGACY_FACTORY_V2_COIN_COUNTS.map((coinCount) => ({
    name: `factory-v2-${coinCount}coin-mainnet`,
    sourceKind: `curve:factory_v2_${coinCount}coin_mainnet`,
    chainIds: [1],
    coinCount,
    arrayKind: "fixed",
    templateDir: join(STABLESWAP_NG_MANIFESTS, "2btc"),
    outDir: join(STABLESWAP_NG_MANIFESTS, `zz-source-factory-v2-${coinCount}coin-mainnet`),
    idPrefix: `curve/stableswap/source/factory-v2-${coinCount}coin-mainnet`,
    oldIdPrefix: "curve/stableswap-ng/2btc",
    oldCoins: [
      "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599",
      "0x18084fba666a33d37592fa2633fd49a74dd93a88",
    ],
  })),
  ...LEGACY_FACTORY_V2_COIN_COUNTS.map((coinCount) => ({
    name: `factory-v2-${coinCount}coin-base`,
    sourceKind: `curve:factory_v2_${coinCount}coin_base`,
    chainIds: [8453],
    coinCount,
    arrayKind: "fixed",
    templateDir: join(STABLESWAP_NG_MANIFESTS, "2btc"),
    outDir: join(STABLESWAP_NG_MANIFESTS, `zz-source-factory-v2-${coinCount}coin-base`),
    idPrefix: `curve/stableswap/source/factory-v2-${coinCount}coin-base`,
    oldIdPrefix: "curve/stableswap-ng/2btc",
    oldCoins: [
      "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599",
      "0x18084fba666a33d37592fa2633fd49a74dd93a88",
    ],
  })),
  {
    name: "factory-crvusd-2coin-mainnet",
    sourceKind: "curve:factory_crvusd_2coin_mainnet",
    chainIds: [1],
    coinCount: 2,
    arrayKind: "fixed",
    templateDir: join(STABLESWAP_NG_MANIFESTS, "2btc"),
    outDir: join(STABLESWAP_NG_MANIFESTS, "zz-source-factory-crvusd-2coin-mainnet"),
    idPrefix: "curve/stableswap/source/factory-crvusd-2coin-mainnet",
    oldIdPrefix: "curve/stableswap-ng/2btc",
    oldCoins: [
      "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599",
      "0x18084fba666a33d37592fa2633fd49a74dd93a88",
    ],
  },
  ...COIN_COUNTS.map((coinCount) => ({
    name: `factory-stable-ng-${coinCount}coin-mainnet`,
    sourceKind: `curve:factory_stable_ng_${coinCount}coin_mainnet`,
    chainIds: [1],
    coinCount,
    arrayKind: "fixed",
    templateDir: join(STABLESWAP_NG_MANIFESTS, "2btc"),
    outDir: join(STABLESWAP_NG_MANIFESTS, `zz-source-factory-stable-ng-${coinCount}coin-mainnet`),
    idPrefix: `curve/stableswap-ng/source/factory-stable-ng-${coinCount}coin-mainnet`,
    oldIdPrefix: "curve/stableswap-ng/2btc",
    oldCoins: [
      "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599",
      "0x18084fba666a33d37592fa2633fd49a74dd93a88",
    ],
  })),
  ...BASE_COIN_COUNTS.map((coinCount) => ({
    name: `factory-stable-ng-${coinCount}coin-base`,
    sourceKind: `curve:factory_stable_ng_${coinCount}coin_base`,
    chainIds: [8453],
    coinCount,
    arrayKind: "dynamic",
    templateDir: join(STABLESWAP_NG_MANIFESTS, "base-superoethb"),
    outDir: join(STABLESWAP_NG_MANIFESTS, `zz-source-factory-stable-ng-${coinCount}coin-base`),
    idPrefix: `curve/stableswap-ng/source/factory-stable-ng-${coinCount}coin-base`,
    oldIdPrefix: "curve/stableswap-ng/base-superoethb",
    oldCoins: [
      "0x4200000000000000000000000000000000000006",
      "0xdbfefd2e8460a6ee4955a68582f85708baea60a3",
    ],
  })),
  {
    name: "factory-twocrypto-mainnet",
    sourceKind: "curve:factory_twocrypto_mainnet",
    chainIds: [1],
    coinCount: 2,
    arrayKind: "fixed",
    templateDir: join(TWOCRYPTO_MANIFESTS, "crvusd-cbbtc"),
    outDir: join(TWOCRYPTO_MANIFESTS, "zz-source-factory-twocrypto-mainnet"),
    idPrefix: "curve/twocrypto/source/factory-twocrypto-mainnet",
    oldIdPrefix: "curve/twocrypto/crvusd-cbbtc",
    amountsArg: "amounts",
    minAmountsArg: "min_amounts",
    oldCoins: [
      "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e",
      "0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf",
    ],
  },
  {
    name: "factory-twocrypto-base",
    sourceKind: "curve:factory_twocrypto_base",
    chainIds: [8453],
    coinCount: 2,
    arrayKind: "fixed",
    templateDir: join(TWOCRYPTO_MANIFESTS, "crvusd-cbbtc"),
    outDir: join(TWOCRYPTO_MANIFESTS, "zz-source-factory-twocrypto-base"),
    idPrefix: "curve/twocrypto/source/factory-twocrypto-base",
    oldIdPrefix: "curve/twocrypto/crvusd-cbbtc",
    amountsArg: "amounts",
    minAmountsArg: "min_amounts",
    oldCoins: [
      "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e",
      "0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf",
    ],
  },
  {
    name: "factory-crypto-mainnet",
    sourceKind: "curve:factory_crypto_mainnet",
    chainIds: [1],
    coinCount: 2,
    arrayKind: "fixed",
    templateDir: join(CRYPTOSWAP_MANIFESTS, "btcghoeth"),
    outDir: join(CRYPTOSWAP_MANIFESTS, "zz-source-factory-crypto-mainnet"),
    idPrefix: "curve/cryptoswap/source/factory-crypto-mainnet",
    oldIdPrefix: "curve/cryptoswap/btcghoeth",
    amountsArg: "amounts",
    minAmountsArg: "min_amounts",
    includeFiles: (file) => file !== "remove_liquidity-claim@1.0.0.json",
    oldCoins: [
      "0x40d16fc0246ad3160ccc09b8d0d3a2cd28ae6c2f",
      "0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf",
      "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
    ],
  },
  {
    name: "factory-crypto-base",
    sourceKind: "curve:factory_crypto_base",
    chainIds: [8453],
    coinCount: 2,
    arrayKind: "fixed",
    templateDir: join(CRYPTOSWAP_MANIFESTS, "btcghoeth"),
    outDir: join(CRYPTOSWAP_MANIFESTS, "zz-source-factory-crypto-base"),
    idPrefix: "curve/cryptoswap/source/factory-crypto-base",
    oldIdPrefix: "curve/cryptoswap/btcghoeth",
    amountsArg: "amounts",
    minAmountsArg: "min_amounts",
    includeFiles: (file) => file !== "remove_liquidity-claim@1.0.0.json",
    oldCoins: [
      "0x40d16fc0246ad3160ccc09b8d0d3a2cd28ae6c2f",
      "0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf",
      "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
    ],
  },
  {
    name: "factory-tricrypto-mainnet",
    sourceKind: "curve:factory_tricrypto_mainnet",
    chainIds: [1],
    coinCount: 3,
    arrayKind: "fixed",
    templateDir: join(CRYPTOSWAP_MANIFESTS, "btcghoeth"),
    outDir: join(CRYPTOSWAP_MANIFESTS, "zz-source-factory-tricrypto-mainnet"),
    idPrefix: "curve/cryptoswap/source/factory-tricrypto-mainnet",
    oldIdPrefix: "curve/cryptoswap/btcghoeth",
    amountsArg: "amounts",
    minAmountsArg: "min_amounts",
    oldCoins: [
      "0x40d16fc0246ad3160ccc09b8d0d3a2cd28ae6c2f",
      "0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf",
      "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
    ],
  },
  {
    name: "factory-tricrypto-base",
    sourceKind: "curve:factory_tricrypto_base",
    chainIds: [8453],
    coinCount: 3,
    arrayKind: "fixed",
    templateDir: join(CRYPTOSWAP_MANIFESTS, "btcghoeth"),
    outDir: join(CRYPTOSWAP_MANIFESTS, "zz-source-factory-tricrypto-base"),
    idPrefix: "curve/cryptoswap/source/factory-tricrypto-base",
    oldIdPrefix: "curve/cryptoswap/btcghoeth",
    amountsArg: "amounts",
    minAmountsArg: "min_amounts",
    oldCoins: [
      "0x40d16fc0246ad3160ccc09b8d0d3a2cd28ae6c2f",
      "0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf",
      "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
    ],
  },
];

function coinAddressRef(index) {
  return `$source.coins.${index}`;
}

function tokenKey(index) {
  return {
    key: {
      standard: "erc20",
      chain: "$chain",
      address: coinAddressRef(index),
    },
  };
}

function tokenAmountPairs(amountArg, coinCount) {
  return Array.from({ length: coinCount }, (_unused, index) => [
    tokenKey(index),
    `$args.${amountArg}[${index}]`,
  ]);
}

function coinCases(coinCount) {
  return Object.fromEntries(
    Array.from({ length: coinCount }, (_unused, index) => [String(index), coinAddressRef(index)]),
  );
}

function replaceDeep(value, group) {
  if (typeof value === "string") {
    const idx = group.oldCoins.indexOf(value.toLowerCase());
    if (idx !== -1) return coinAddressRef(idx);
    return value;
  }
  if (Array.isArray(value)) return value.map((item) => replaceDeep(item, group));
  if (value && typeof value === "object") {
    const out = {};
    for (const [key, nested] of Object.entries(value)) {
      out[key] = key === "n_coins" && nested === 2 ? group.coinCount : replaceDeep(nested, group);
    }
    return out;
  }
  return value;
}

function rewriteCoinCases(value, coinCount) {
  if (Array.isArray(value)) {
    for (const item of value) rewriteCoinCases(item, coinCount);
    return;
  }
  if (!value || typeof value !== "object") return;

  if (
    Object.prototype.hasOwnProperty.call(value, "$match") &&
    Object.prototype.hasOwnProperty.call(value, "$cases")
  ) {
    value.$cases = coinCases(coinCount);
  }

  for (const nested of Object.values(value)) rewriteCoinCases(nested, coinCount);
}

function actionParams(body) {
  const amm = body.emit?.body?.amm;
  if (!amm || typeof amm !== "object") return undefined;
  const action = amm.action;
  if (action === "add_liquidity") return amm.add_liquidity?.params;
  if (action === "remove_liquidity") return amm.remove_liquidity?.params;
  return undefined;
}

function rewriteActionArrays(file, body, group) {
  const params = actionParams(body);
  if (!params) return;
  const amountArg = group.amountsArg ?? "_amounts";
  const minAmountsArg = group.minAmountsArg ?? "_min_amounts";

  if (file.startsWith("add_liquidity")) {
    params.tokens = tokenAmountPairs(amountArg, group.coinCount);
  } else if (file.startsWith("remove_liquidity_imbalance")) {
    params.amounts_out = tokenAmountPairs(amountArg, group.coinCount);
  } else if (
    (file.startsWith("remove_liquidity@") || file.startsWith("remove_liquidity-")) &&
    !file.startsWith("remove_liquidity_one_coin") &&
    !file.startsWith("remove_liquidity_imbalance")
  ) {
    params.min_out = tokenAmountPairs(minAmountsArg, group.coinCount);
  }
}

function rewriteAbiArrays(input, group) {
  if (!/^uint256(?:\[\d+\]|\[\])$/.test(input.type)) return;
  input.type = group.arrayKind === "fixed" ? `uint256[${group.coinCount}]` : "uint256[]";
}

function rewriteSelector(body) {
  const abi = body.abi_fragment?.abi;
  if (!abi || !Array.isArray(abi.inputs)) return;
  const signature = `${abi.name}(${abi.inputs.map((input) => input.type).join(",")})`;
  body.match.selector = toFunctionSelector(signature);
}

let written = 0;
for (const group of GROUPS) {
  mkdirSync(group.outDir, { recursive: true });
  const files = readdirSync(group.templateDir)
    .filter((file) => file.endsWith(".json"))
    .filter((file) => (group.includeFiles ? group.includeFiles(file) : true))
    .sort();
  for (const file of files) {
    const raw = JSON.parse(readFileSync(join(group.templateDir, file), "utf8"));
    const body = replaceDeep(raw, group);
    body.id = String(body.id).replace(group.oldIdPrefix, group.idPrefix);
    body.match = {
      selector: raw.match.selector,
      chain_to_addresses_source: group.sourceKind,
      chain_ids: group.chainIds,
    };
    for (const input of body.abi_fragment?.abi?.inputs ?? []) rewriteAbiArrays(input, group);
    rewriteSelector(body);
    rewriteCoinCases(body, group.coinCount);
    rewriteActionArrays(file, body, group);
    body.source_materialize = {
      kind: "per_address_context",
      source: group.sourceKind,
      note: "build-index substitutes $source.* and appends a per-pool id suffix",
    };
    writeFileSync(join(group.outDir, file), JSON.stringify(body, null, 2) + "\n", "utf8");
    written++;
  }
  console.log(`${group.name}: wrote ${files.length} templates`);
}
console.log(`curve source manifests: wrote ${written} templates`);
