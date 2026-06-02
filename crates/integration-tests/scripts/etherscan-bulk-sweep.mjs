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
const minRawTxs = Number(args.get("--min-raw-txs") ?? "20000");
const offset = Number(args.get("--offset") ?? "10000");
const pageLimit = Number(args.get("--page-limit") ?? "1");
const delayMs = Number(args.get("--delay-ms") ?? "230");
const timeoutMs = Number(args.get("--timeout-ms") ?? "15000");
const envPath = path.resolve(
  repoRoot,
  args.get("--env") ?? "crates/integration-tests/.env",
);
const deploymentsPath = path.resolve(
  repoRoot,
  args.get("--deployments") ?? `registryV2/surface/${protocol}/_deployments.json`,
);
const universePath = path.resolve(
  repoRoot,
  args.get("--universe") ?? `registryV2/surface/${protocol}/_address_universe.json`,
);
const targetSource = args.get("--target-source") ?? "deployments";
const surfaceRoot = path.resolve(
  repoRoot,
  args.get("--surface-root") ?? path.dirname(deploymentsPath),
);
const manifestsRoot = path.resolve(
  repoRoot,
  args.get("--manifests-root") ?? "registryV2/manifests",
);
const indexRoot = path.resolve(
  repoRoot,
  args.get("--index-root") ?? "registryV2/index/by-callkey",
);
const outPath = path.resolve(
  repoRoot,
  args.get("--out") ??
    `crates/integration-tests/onboarding/${protocol}/etherscan-bulk-summary.json`,
);

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

function walkJsonFiles(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walkJsonFiles(full, out);
    } else if (entry.isFile() && entry.name.endsWith(".json")) {
      out.push(full);
    }
  }
  return out;
}

function lowerAddress(value) {
  return typeof value === "string" ? value.toLowerCase() : "";
}

function selectorOf(input) {
  if (typeof input !== "string") return null;
  const normalized = input.startsWith("0x") ? input : `0x${input}`;
  return normalized.length >= 10 ? normalized.slice(0, 10).toLowerCase() : null;
}

function addCount(map, key, n = 1) {
  map.set(key, (map.get(key) ?? 0) + n);
}

function mapToSortedArray(map, limit = undefined) {
  const rows = [...map.entries()]
    .map(([key, count]) => ({ key, count }))
    .sort((a, b) => b.count - a.count || a.key.localeCompare(b.key));
  return limit ? rows.slice(0, limit) : rows;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function addUnmatchedExample(map, key, tx, target, selector) {
  if (!tx?.hash || !tx?.input) return;
  const examples = map.get(key) ?? [];
  if (examples.length >= 3) return;
  examples.push({
    chainId: target.chainId,
    address: target.address,
    name: target.name,
    selector,
    txHash: String(tx.hash),
    from: tx.from,
    to: tx.to,
    value: tx.value,
    blockNumber: tx.blockNumber,
    timeStamp: tx.timeStamp,
    input: tx.input,
  });
  map.set(key, examples);
}

function addCallkeyMeta({ byCallkey, bySelector }, callkey, meta) {
  if (!byCallkey.has(callkey)) byCallkey.set(callkey, []);
  const existing = byCallkey
    .get(callkey)
    .some((row) => row.manifestId === meta.manifestId && row.file === meta.file);
  if (!existing) byCallkey.get(callkey).push(meta);
  if (!bySelector.has(meta.selector)) bySelector.set(meta.selector, []);
  const selectorExisting = bySelector
    .get(meta.selector)
    .some(
      (row) =>
        row.chainId === meta.chainId &&
        row.address === meta.address &&
        row.manifestId === meta.manifestId,
    );
  if (!selectorExisting) bySelector.get(meta.selector).push(meta);
}

function loadManifestCallkeys(manifestRoot, generatedIndexRoot) {
  const byCallkey = new Map();
  const bySelector = new Map();
  const maps = { byCallkey, bySelector };
  for (const file of walkJsonFiles(manifestRoot)) {
    let manifest;
    try {
      manifest = readJson(file);
    } catch {
      continue;
    }
    const selector = manifest?.match?.selector?.toLowerCase?.();
    const chainToAddresses = manifest?.match?.chain_to_addresses;
    if (!selector || !chainToAddresses || typeof chainToAddresses !== "object") {
      continue;
    }
    for (const [chainId, addresses] of Object.entries(chainToAddresses)) {
      if (!Array.isArray(addresses)) continue;
      for (const address of addresses) {
        const callkey = `${chainId}__${lowerAddress(address)}__${selector}`;
        const meta = {
          manifestId: manifest.id ?? path.relative(repoRoot, file),
          file: path.relative(repoRoot, file),
          selector,
          chainId: Number(chainId),
          address: lowerAddress(address),
          source: "manifest",
        };
        addCallkeyMeta(maps, callkey, meta);
      }
    }
  }

  if (fs.existsSync(generatedIndexRoot)) {
    for (const file of walkJsonFiles(generatedIndexRoot)) {
      const name = path.basename(file, ".json");
      const parts = name.split("__");
      if (parts.length !== 3) continue;
      const [chainIdRaw, address, selector] = parts;
      const chainId = Number(chainIdRaw);
      if (!chainId || !address || !selector) continue;
      let entry;
      try {
        entry = readJson(file);
      } catch {
        continue;
      }
      const manifestId = entry?.bundle_id ?? entry?.bundle?.id ?? path.relative(repoRoot, file);
      addCallkeyMeta(maps, `${chainId}__${address.toLowerCase()}__${selector.toLowerCase()}`, {
        manifestId,
        file: entry?.manifest_path ?? path.relative(repoRoot, file),
        selector: selector.toLowerCase(),
        chainId,
        address: address.toLowerCase(),
        source: "generated_index",
      });
    }
  }

  return { byCallkey, bySelector };
}

function loadCoverageDecisions(root) {
  const byCallkey = new Map();
  const selectorsByTarget = new Map();
  if (!fs.existsSync(root)) return { byCallkey, selectorsByTarget };
  for (const file of walkJsonFiles(root)) {
    if (!file.endsWith(".coverage.json")) continue;
    let coverage;
    try {
      coverage = readJson(file);
    } catch {
      continue;
    }
    const chainId = Number(coverage?.chainId);
    const address = lowerAddress(coverage?.address);
    const functions = coverage?.functions;
    if (!chainId || !address || !functions || typeof functions !== "object") {
      continue;
    }
    const targetKey = `${chainId}__${address}`;
    if (!selectorsByTarget.has(targetKey)) selectorsByTarget.set(targetKey, new Set());
    for (const [selector, row] of Object.entries(functions)) {
      const normalizedSelector = selector.toLowerCase();
      selectorsByTarget.get(targetKey).add(normalizedSelector);
      byCallkey.set(`${targetKey}__${normalizedSelector}`, {
        decision: row?.decision,
        name: row?.name,
        reason: row?.reason,
        file: path.relative(repoRoot, file),
      });
    }
  }
  return { byCallkey, selectorsByTarget };
}

function classifyUnmatched({ target, selector, selectorsByTarget, bySelector }) {
  const targetKey = `${target.chainId}__${target.address}`;
  const snapshotSelectors = selectorsByTarget.get(targetKey);
  if (!snapshotSelectors) {
    return {
      disposition: "missing_surface_coverage",
      actionable: true,
      reason: "target has no coverage snapshot, so this selector cannot be dispositioned",
    };
  }
  if (!snapshotSelectors.has(selector)) {
    const knownElsewhere = (bySelector.get(selector) ?? []).length > 0;
    return {
      disposition: knownElsewhere
        ? "selector_known_elsewhere_wrong_target"
        : "non_abi_or_text_calldata",
      actionable: false,
      reason: knownElsewhere
        ? "selector is covered on another address but is absent from this target ABI snapshot"
        : "selector is absent from this target ABI snapshot; examples are treated as misdirected, spam, or non-ABI calldata",
    };
  }
  return {
    disposition: "abi_selector_without_cover_or_exclude",
    actionable: true,
    reason: "selector is in this target coverage snapshot but has no matching manifest or explicit exclude",
  };
}

async function etherscanTxlist({ apiKey, chainId, address, page }) {
  const params = new URLSearchParams({
    chainid: String(chainId),
    module: "account",
    action: "txlist",
    address,
    startblock: "0",
    endblock: "99999999",
    page: String(page),
    offset: String(offset),
    sort: "desc",
    apikey: apiKey,
  });
  const url = `https://api.etherscan.io/v2/api?${params.toString()}`;
  let lastError;
  for (let attempt = 1; attempt <= 4; attempt += 1) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
      const body = await response.json();
      if (body.status === "1" && Array.isArray(body.result)) {
        return { ok: true, rows: body.result, message: body.message ?? "OK" };
      }
      const message = `${body.message ?? "ERROR"}: ${body.result ?? ""}`;
      if (/No transactions found/i.test(message)) {
        return { ok: true, rows: [], message };
      }
      lastError = message;
      if (/rate limit|Max rate|temporarily unavailable|timeout/i.test(message)) {
        await sleep(delayMs * attempt * 4);
        continue;
      }
      return { ok: false, rows: [], message };
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
      await sleep(delayMs * attempt * 4);
    }
  }
  return { ok: false, rows: [], message: lastError ?? "unknown error" };
}

const env = { ...process.env, ...readEnv(envPath) };
const apiKey = env.ETHERSCAN_API_KEY;
if (!apiKey) {
  throw new Error(`ETHERSCAN_API_KEY missing in environment or ${envPath}`);
}

function loadTargets() {
  if (targetSource === "deployments") {
    const deployments = readJson(deploymentsPath);
    return (deployments.contracts ?? [])
      .filter((row) => row.decision === "cover")
      .map((row) => ({
        name: row.name,
        chainId: Number(row.chainId),
        address: lowerAddress(row.address),
        reason: row.reason,
        targetSource,
      }))
      .filter((row) => row.chainId && row.address);
  }
  if (targetSource === "universe-candidates") {
    const universe = readJson(universePath);
    return (universe.candidates ?? [])
      .map((row) => ({
        name: row.id ?? row.batch ?? "universe-candidate",
        chainId: Number(row.chainId),
        address: lowerAddress(row.address),
        reason: row.reason,
        decision: row.decision,
        batch: row.batch,
        targetSource,
      }))
      .filter((row) => row.chainId && row.address);
  }
  throw new Error(`unknown --target-source ${targetSource}`);
}

const coverTargets = loadTargets();
const { byCallkey, bySelector } = loadManifestCallkeys(manifestsRoot, indexRoot);
const { byCallkey: coverageByCallkey, selectorsByTarget } = loadCoverageDecisions(surfaceRoot);

const generatedAt = new Date().toISOString();
const selectorCounts = new Map();
const matchedSelectorCounts = new Map();
const unmatchedSelectorCounts = new Map();
const excludedSelectorCounts = new Map();
const addressSelectorCounts = new Map();
const unmatchedAddressSelectorCounts = new Map();
const excludedAddressSelectorCounts = new Map();
const unmatchedDispositionCounts = new Map();
const matchedCallkeys = new Map();
const unmatchedExamples = new Map();
const excludedExamples = new Map();
const txHashSeen = new Set();
const perAddress = [];
const errors = [];
let apiCallsUsed = 0;
let rawTxsSeen = 0;
let inputTxsSeen = 0;
let matchedInputTxs = 0;
let excludedInputTxs = 0;
let actionableUnmatchedInputTxs = 0;
let nonActionableUnmatchedInputTxs = 0;

for (const target of coverTargets) {
  let targetRawTxs = 0;
  let targetInputTxs = 0;
  let targetMatched = 0;
  const targetSelectors = new Map();
  const targetUnmatched = new Map();
  const targetUnmatchedDispositions = new Map();
  const targetExcluded = new Map();
  for (let page = 1; page <= pageLimit; page += 1) {
    apiCallsUsed += 1;
    const res = await etherscanTxlist({
      apiKey,
      chainId: target.chainId,
      address: target.address,
      page,
    });
    if (!res.ok) {
      errors.push({
        chainId: target.chainId,
        address: target.address,
        name: target.name,
        page,
        message: res.message,
      });
      break;
    }
    targetRawTxs += res.rows.length;
    rawTxsSeen += res.rows.length;
    for (const tx of res.rows) {
      if (tx?.hash) txHashSeen.add(String(tx.hash).toLowerCase());
      const selector = selectorOf(tx?.input);
      if (!selector) continue;
      inputTxsSeen += 1;
      targetInputTxs += 1;
      addCount(selectorCounts, selector);
      addCount(targetSelectors, selector);
      const addressSelector = `${target.chainId}__${target.address}__${selector}`;
      addCount(addressSelectorCounts, addressSelector);
      const manifests = byCallkey.get(addressSelector) ?? [];
      if (manifests.length > 0) {
        matchedInputTxs += 1;
        targetMatched += 1;
        addCount(matchedSelectorCounts, selector);
        addCount(matchedCallkeys, `${addressSelector}__${manifests[0].manifestId}`);
      } else if (coverageByCallkey.get(addressSelector)?.decision === "exclude") {
        excludedInputTxs += 1;
        addCount(excludedSelectorCounts, selector);
        addCount(targetExcluded, selector);
        addCount(excludedAddressSelectorCounts, addressSelector);
        addUnmatchedExample(excludedExamples, addressSelector, tx, target, selector);
      } else {
        const classification = classifyUnmatched({
          target,
          selector,
          selectorsByTarget,
          bySelector,
        });
        if (classification.actionable) {
          actionableUnmatchedInputTxs += 1;
        } else {
          nonActionableUnmatchedInputTxs += 1;
        }
        addCount(unmatchedDispositionCounts, classification.disposition);
        addCount(targetUnmatchedDispositions, classification.disposition);
        addCount(unmatchedSelectorCounts, selector);
        addCount(targetUnmatched, selector);
        addCount(unmatchedAddressSelectorCounts, addressSelector);
        addUnmatchedExample(unmatchedExamples, addressSelector, tx, target, selector);
      }
    }
    if (res.rows.length < offset) break;
    await sleep(delayMs);
  }
  perAddress.push({
    name: target.name,
    chainId: target.chainId,
    address: target.address,
    rawTxs: targetRawTxs,
    inputTxs: targetInputTxs,
    matchedInputTxs: targetMatched,
    excludedInputTxs: mapToSortedArray(targetExcluded).reduce((sum, row) => sum + row.count, 0),
    unmatchedInputTxs:
      targetInputTxs -
      targetMatched -
      mapToSortedArray(targetExcluded).reduce((sum, row) => sum + row.count, 0),
    topSelectors: mapToSortedArray(targetSelectors, 12).map(({ key, count }) => ({
      selector: key,
      count,
      manifestIds: (byCallkey.get(`${target.chainId}__${target.address}__${key}`) ?? [])
        .map((m) => m.manifestId)
        .slice(0, 5),
    })),
    topUnmatchedSelectors: mapToSortedArray(targetUnmatched, 12).map(({ key, count }) => ({
      selector: key,
      count,
      disposition: classifyUnmatched({
        target,
        selector: key,
        selectorsByTarget,
        bySelector,
      }).disposition,
      knownElsewhere: (bySelector.get(key) ?? []).slice(0, 5).map((m) => ({
        chainId: m.chainId,
        address: m.address,
        manifestId: m.manifestId,
      })),
    })),
    unmatchedDispositions: mapToSortedArray(targetUnmatchedDispositions).map(
      ({ key, count }) => ({
        disposition: key,
        count,
      }),
    ),
    topExcludedSelectors: mapToSortedArray(targetExcluded, 12).map(({ key, count }) => {
      const coverage = coverageByCallkey.get(`${target.chainId}__${target.address}__${key}`);
      return {
        selector: key,
        count,
        name: coverage?.name,
        reason: coverage?.reason,
        coverageFile: coverage?.file,
      };
    }),
  });
  await sleep(delayMs);
}

const manifestSelectorsByCoverAddress = [];
for (const target of coverTargets) {
  for (const [callkey, manifests] of byCallkey.entries()) {
    const [chainId, address, selector] = callkey.split("__");
    if (Number(chainId) !== target.chainId || address !== target.address) continue;
    manifestSelectorsByCoverAddress.push({
      chainId: target.chainId,
      address: target.address,
      name: target.name,
      selector,
      txCount: addressSelectorCounts.get(`${target.chainId}__${target.address}__${selector}`) ?? 0,
      manifestIds: manifests.map((m) => m.manifestId),
    });
  }
}

const summary = {
  generatedAt,
  protocol,
  source:
    targetSource === "deployments"
      ? "Etherscan v2 account txlist, adapter-blind by P0 cover deployment addresses"
      : "Etherscan v2 account txlist, adapter-blind by P0 address-universe candidate addresses",
  targetSource,
  apiCallsUsed,
  targetsQueried: coverTargets.length,
  coverAddressesQueried: coverTargets.length,
  offset,
  pageLimit,
  targetRawTxFloor: minRawTxs,
  floorMet: rawTxsSeen >= minRawTxs,
  rawTxsSeen,
  uniqueTxHashesSeen: txHashSeen.size,
  inputTxsSeen,
  matchedInputTxs,
  excludedInputTxs,
  unmatchedInputTxs: inputTxsSeen - matchedInputTxs - excludedInputTxs,
  actionableUnmatchedInputTxs,
  nonActionableUnmatchedInputTxs,
  unmatchedDispositions: mapToSortedArray(unmatchedDispositionCounts).map(({ key, count }) => ({
    disposition: key,
    count,
  })),
  uniqueSelectorsSeen: selectorCounts.size,
  uniqueMatchedSelectorsSeen: matchedSelectorCounts.size,
  uniqueExcludedSelectorsSeen: excludedSelectorCounts.size,
  uniqueUnmatchedSelectorsSeen: unmatchedSelectorCounts.size,
  matchedCallkeys: matchedCallkeys.size,
  errors,
  topSelectors: mapToSortedArray(selectorCounts, 80).map(({ key, count }) => ({
    selector: key,
    count,
  })),
  topMatchedSelectors: mapToSortedArray(matchedSelectorCounts, 80).map(({ key, count }) => ({
    selector: key,
    count,
  })),
  topExcludedSelectors: mapToSortedArray(excludedSelectorCounts, 80).map(({ key, count }) => ({
    selector: key,
    count,
  })),
  topUnmatchedSelectors: mapToSortedArray(unmatchedSelectorCounts, 80).map(
    ({ key, count }) => ({
      selector: key,
      count,
      knownElsewhere: (bySelector.get(key) ?? []).slice(0, 8).map((m) => ({
        chainId: m.chainId,
        address: m.address,
        manifestId: m.manifestId,
      })),
    }),
  ),
  topUnmatchedAddressSelectors: mapToSortedArray(unmatchedAddressSelectorCounts, 120).map(
    ({ key, count }) => {
      const [chainId, address, selector] = key.split("__");
      const target = coverTargets.find(
        (row) => row.chainId === Number(chainId) && row.address === address,
      );
      const classification = target
        ? classifyUnmatched({
            target,
            selector,
            selectorsByTarget,
            bySelector,
          })
        : {
            disposition: "unknown_target",
            reason: "target not found in cover target list",
          };
      return {
        chainId: Number(chainId),
        address,
        name: target?.name,
        selector,
        count,
        disposition: classification.disposition,
        dispositionReason: classification.reason,
        knownElsewhere: (bySelector.get(selector) ?? []).slice(0, 8).map((m) => ({
          chainId: m.chainId,
          address: m.address,
          manifestId: m.manifestId,
        })),
        examples: unmatchedExamples.get(key) ?? [],
      };
    },
  ),
  topExcludedAddressSelectors: mapToSortedArray(excludedAddressSelectorCounts, 120).map(
    ({ key, count }) => {
      const [chainId, address, selector] = key.split("__");
      const target = coverTargets.find(
        (row) => row.chainId === Number(chainId) && row.address === address,
      );
      const coverage = coverageByCallkey.get(key);
      return {
        chainId: Number(chainId),
        address,
        name: target?.name,
        selector,
        count,
        functionName: coverage?.name,
        reason: coverage?.reason,
        coverageFile: coverage?.file,
        examples: excludedExamples.get(key) ?? [],
      };
    },
  ),
  manifestSelectorsByCoverAddress: manifestSelectorsByCoverAddress.sort(
    (a, b) =>
      b.txCount - a.txCount ||
      a.chainId - b.chainId ||
      a.address.localeCompare(b.address) ||
      a.selector.localeCompare(b.selector),
  ),
  perAddress: perAddress.sort(
    (a, b) =>
      b.rawTxs - a.rawTxs ||
      a.chainId - b.chainId ||
      a.address.localeCompare(b.address),
  ),
};

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, `${JSON.stringify(summary, null, 2)}\n`);
console.log(
  JSON.stringify(
    {
      out: path.relative(repoRoot, outPath),
      coverAddressesQueried: summary.coverAddressesQueried,
      apiCallsUsed: summary.apiCallsUsed,
      rawTxsSeen: summary.rawTxsSeen,
      floorMet: summary.floorMet,
      inputTxsSeen: summary.inputTxsSeen,
      matchedInputTxs: summary.matchedInputTxs,
      excludedInputTxs: summary.excludedInputTxs,
      unmatchedInputTxs: summary.unmatchedInputTxs,
      actionableUnmatchedInputTxs: summary.actionableUnmatchedInputTxs,
      nonActionableUnmatchedInputTxs: summary.nonActionableUnmatchedInputTxs,
      uniqueSelectorsSeen: summary.uniqueSelectorsSeen,
      errors: summary.errors.length,
    },
    null,
    2,
  ),
);

if (!summary.floorMet) {
  process.exitCode = 2;
}
