/**
 * check-surface-completeness.ts — Dambi Adapter Registry v3
 *
 * EXECUTABLE research-completeness gate. Converts the P0 "external
 * state-changing function 전수 + per-function COVER/EXCLUDE triage" step of the
 * onboarding manual (PROTOCOL_ONBOARDING_AND_TESTING.md §3) from PROSE into a
 * build-enforced invariant.
 *
 * Why this exists
 * ---------------
 * The §3 prose gate let an agent silently author adapters for "the easy
 * functions" and drop the rest (e.g. Morpho `setAuthorization`, the permission-
 * delegation primitive = Dambi's raison d'être). A prose checklist can be
 * skipped with no signal. This script makes the omission a build failure, the
 * same way `compose_per_policy` makes a missing Cedar registration a runtime
 * `MissingAction`. Research-completeness becomes machine-checked, not trusted.
 *
 * Ground truth (independent source — NOT circular)
 * ------------------------------------------------
 * Checking authored coverage against authored manifests would be circular
 * (forget a function in BOTH and it passes). The gate diffs against an
 * INDEPENDENT source: the contract's verified full ABI, fetched once from a 1st-
 * party source (Etherscan/Sourcify/BaseScan) and committed as a SNAPSHOT under
 * `surface/<protocol>/<contract>.abi.json` (provenance + raw ABI). The snapshot
 * is offline ground truth — the gate needs no API key. Re-fetch to verify.
 *
 * Three artifacts
 * ---------------
 *   surface/<protocol>/<contract>.abi.json       snapshot {source,url,chainId,address,fetchedAt,abi[]}
 *   surface/<protocol>/<contract>.coverage.json  triage   {chainId,address,functions{sel->{name,decision,reason}}, signed_structs{type->{decision,reason}}}
 *   manifests/**                                  the authored adapters
 *
 * Invariants (a violation of any is a build FAILURE → exit 1)
 * ----------------------------------------------------------
 *   I0 inventory      every `cover` contract in surface/<protocol>/_deployments.json
 *                     (the 1st-party deployment list = contract-level ground truth)
 *                     MUST have a surface snapshot; every `exclude` MUST have a reason.
 *                     Catches a CONTRACT research never found — the blind spot of I1
 *                     and the address-keyed real-tx pull (both bounded by the contract
 *                     set research produced). Opt-in: no _deployments.json → WARN.
 *                     Floor: as complete as the official list (a page CAN omit a
 *                     contract), so weaker than I1 (an ABI cannot omit a function).
 *   I1 completeness   every external-mutating selector in the snapshot
 *                     (type==="function" && stateMutability ∈ {nonpayable,payable})
 *                     MUST have a coverage.functions entry. ← blocks the original miss.
 *   I1' no-stale      every coverage.functions key MUST exist in the snapshot
 *                     (catches typos / removed functions).
 *   I2 cover→manifest every "cover" selector MUST have a manifest at (chain,address,selector).
 *   I3 manifest→cover every on-chain manifest selector at (chain,address) MUST be "cover".
 *   S1/S2 signed      every typed-data manifest primary_type MUST be a "cover" in
 *                     signed_structs, and vice-versa (best-effort EIP-712 half).
 *
 * Scope (incremental, no silent cap)
 * ----------------------------------
 * Only (chainId,address) pairs that HAVE a coverage file are enforced. Protocol
 * contracts with manifests but NO snapshot are reported as "ungated" (a visible
 * WARN, never silent — onboarding them is opt-in per protocol). Sourced
 * manifests are gate-scoped: token standards and supported protocol sources
 * count only when a coverage file explicitly marks that selector as "cover".
 *
 * Limits (honest)
 * ---------------
 *  - Proxy/diamond: a verified ABI may hide implementation functions. The
 *    snapshot is ground truth only for the surface it exposes. (Morpho Blue is a
 *    non-proxy singleton → clean.) For proxies, snapshot the implementation ABI.
 *  - EIP-712 typehashes are not in the function-selector ABI, so signed_structs
 *    cannot be auto-enumerated from the snapshot — they are hand-listed. The
 *    gate only cross-checks them against typed-data manifests.
 *
 * Run:
 *   $ npm run check:surface          # from registryV2/
 *   $ BUILD_INDEX_REGISTRY_ROOT=/tmp/x npm run check:surface   # test isolation
 *
 * Spec references:
 *   - crates/integration-tests/PROTOCOL_ONBOARDING_AND_TESTING.md §3 (surface-completeness gate)
 *   - surface/README.md (snapshot + coverage format, onboarding procedure)
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { toFunctionSelector } from "viem";

// ---------------------------------------------------------------------------
// Paths (mirror build-index.ts: script-relative, env-overridable for tests)
// ---------------------------------------------------------------------------

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REGISTRY_ROOT = process.env.BUILD_INDEX_REGISTRY_ROOT
  ? resolve(process.env.BUILD_INDEX_REGISTRY_ROOT)
  : resolve(__dirname, "..");
const MANIFESTS_DIR = join(REGISTRY_ROOT, "manifests");
const SURFACE_DIR = join(REGISTRY_ROOT, "surface");
const TOKENS_DIR = join(REGISTRY_ROOT, "tokens");

const SELECTOR_RE = /^0x[0-9a-fA-F]{8}$/;
const MUTABLE = new Set(["nonpayable", "payable"]);

/**
 * Reserved sentinel for selector-less BARE-ETH transfers (empty calldata + value):
 * a payable `receive()` / `fallback()` has no 4-byte function selector, so build-
 * index + the WASM route key such calls under the all-zero word `0x00000000`
 * (which still satisfies `SELECTOR_RE`). A coverage file may OPT IN to cover this
 * sentinel; I1' grounds it in the snapshot's receive/fallback entry (see
 * `hasNativeEntrypoint`) instead of the function-selector surface check.
 */
const NATIVE_TRANSFER_SELECTOR = "0x00000000";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type Hex = string;

interface AbiInput {
  name?: string;
  type: string;
  components?: AbiInput[];
}
interface AbiItem {
  type: string;
  name?: string;
  stateMutability?: string;
  inputs?: AbiInput[];
}
interface Snapshot {
  source: string;
  url?: string;
  chainId: number;
  address: Hex;
  contract?: string;
  abi: AbiItem[];
}
interface CoverageFn {
  name: string;
  decision: "cover" | "exclude";
  reason: string;
}
interface Coverage {
  contract?: string;
  chainId: number;
  /** Single contract address. Use this OR `addresses[]`. */
  address?: Hex;
  /**
   * Many pool addresses that SHARE the one `snapshot` ABI — for factory-
   * deployed protocols (Curve: one StableSwap-NG implementation ABI, N pool
   * addresses each called directly). I1 (snapshot completeness) runs once;
   * I2/I3/S1/S2 run against EACH address (every pool must carry the cover-
   * selector manifests). A pool missing a manifest fails the gate.
   */
  addresses?: Hex[];
  snapshot: string;
  functions: Record<string, CoverageFn>;
  signed_structs?: Record<string, { decision: "cover" | "exclude"; reason: string }>;
}

/**
 * `surface/<protocol>/_deployments.json` — the 1st-party DEPLOYMENT-LIST ground
 * truth for I0 (contract-inventory completeness). Per-contract snapshots (I1)
 * only enforce completeness WITHIN contracts you already found; they are blind
 * to a contract research never found (its address is never snapshotted, and the
 * adapter-blind real-tx pull queries BY address so its txs never enter a corpus).
 * This list is the independent source — the official deployment page — that
 * forces an explicit decision on EVERY deployed contract, the same way the
 * verified ABI forces a decision on every function.
 */
interface DeploymentEntry {
  name: string;
  chainId: number;
  address: Hex;
  decision: "cover" | "exclude";
  reason: string;
}
interface Deployments {
  protocol?: string;
  source?: string;
  url?: string;
  contracts: DeploymentEntry[];
}

/** On-chain + signed-struct manifest surface, grouped per (chainId, address). */
interface ContractManifests {
  onchainSelectors: Map<string, string[]>; // selector(lc) -> manifest rel-paths
  signedTypes: Map<string, string[]>; // EIP-712 primary_type -> manifest rel-paths
}

type TokenErcKind = "erc20" | "erc721" | "erc1155" | "native";
type GatedCoverageSelectors = Map<string, Set<string>>;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function safeExists(p: string): boolean {
  try {
    statSync(p);
    return true;
  } catch {
    return false;
  }
}

function walkJsonFiles(root: string, skipDirs: ReadonlySet<string> = new Set()): string[] {
  const out: string[] = [];
  if (!safeExists(root)) return out;
  const stack = [root];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    for (const entry of readdirSync(cur)) {
      const p = join(cur, entry);
      const s = statSync(p);
      if (s.isDirectory()) {
        if (skipDirs.has(entry)) continue;
        stack.push(p);
      } else if (s.isFile() && entry.endsWith(".json")) {
        out.push(p);
      }
    }
  }
  return out.sort();
}

const ckey = (chainId: number, addr: string): string => `${chainId}__${addr.toLowerCase()}`;

/** First path segment under manifests/ — the protocol grouping for reports. */
function protocolOf(manifestPath: string): string {
  const normalized = manifestPath.split(/[\\/]/).join("/");
  const rel = normalized.startsWith("manifests/")
    ? normalized.slice("manifests/".length)
    : relative(MANIFESTS_DIR, manifestPath).split(/[\\/]/).join("/");
  const seg = rel.split(/[/\\]/)[0];
  return seg || "(root)";
}

/** Protocol subdir of a surface path ("surface/lido/x.coverage.json" -> "lido"). */
function surfaceProtocolOf(coverageRelPath: string): string {
  const parts = coverageRelPath.replace(/\\/g, "/").split("/"); // ["surface","lido",...]
  return parts[1] ?? "(root)";
}

const ADDR_RE = /^0x[0-9a-fA-F]{40}$/;
const TOKEN_SOURCE_RE = /^tokens:(erc20|erc721|erc1155|native)$/;
const UNISWAP_V2_PAIR_SOURCE = "uniswap:v2_pair_candidates";
const UNISWAP_V2_PAIR_BATCH = "uniswap-v2-child-universe-deferred";
const UNISWAP_V3_POOL_SOURCE = "uniswap:v3_pool_candidates";
const UNISWAP_V3_POOL_BATCH = "uniswap-v3-child-universe-deferred";
// Balancer V3 liquidity manifests are source-materialized but, unlike Curve/Uniswap
// (one callkey per pool), they materialize to the SINGLE V3 Router; the per-pool
// token list rides as `$source.pool_tokens` context, not as per-pool callkeys.
const BALANCER_V3_POOL_TOKENS_SOURCE = "balancer_v3:pool_tokens";
const BALANCER_V3_ROUTER_MAINNET = "0xae563e3f8219521950555f5962419c8919758ea2";
const BALANCER_V3_COMPOSITE_TOKENS_SOURCE = "balancer_v3:composite_pool_tokens";
const BALANCER_V3_COMPOSITE_MAINNET = "0xb21a277466e7db6934556a1ce12eb3f032815c8a";

function coverageAddresses(cov: Coverage): Hex[] {
  return (
    cov.addresses && cov.addresses.length > 0
      ? cov.addresses
      : cov.address
        ? [cov.address]
        : []
  ).map((a) => a.toLowerCase());
}

function buildGatedCoverageSelectors(units: SurfaceUnit[]): GatedCoverageSelectors {
  const out: GatedCoverageSelectors = new Map();
  for (const { coverage: cov } of units) {
    for (const addr of coverageAddresses(cov)) {
      const key = ckey(cov.chainId, addr);
      let selectors = out.get(key);
      if (!selectors) {
        selectors = new Set<string>();
        out.set(key, selectors);
      }
      for (const [sel, fn] of Object.entries(cov.functions ?? {})) {
        if (fn.decision === "cover") selectors.add(sel.toLowerCase());
      }
    }
  }
  return out;
}

const TOKEN_ADDRESS_CACHE = new Map<string, Hex[]>();

function tokenAddressesForKind(chainId: number, ercKind: TokenErcKind): Hex[] {
  const cacheKey = `${chainId}:${ercKind}`;
  const cached = TOKEN_ADDRESS_CACHE.get(cacheKey);
  if (cached) return cached;

  const out: Hex[] = [];
  const dir = join(TOKENS_DIR, String(chainId));
  if (safeExists(dir)) {
    for (const fname of readdirSync(dir)) {
      if (!fname.endsWith(".json")) continue;
      const path = join(dir, fname);
      const obj = JSON.parse(readFileSync(path, "utf8")) as Record<string, unknown>;
      const address = String(obj.address ?? "").toLowerCase();
      if (obj.erc_kind === ercKind && ADDR_RE.test(address)) out.push(address);
    }
  }

  out.sort();
  TOKEN_ADDRESS_CACHE.set(cacheKey, out);
  return out;
}

const UNISWAP_SOURCE_CACHE = new Map<string, Hex[]>();

function uniswapCandidateAddresses(chainId: number, batch: string): Hex[] {
  const cacheKey = `${chainId}:${batch}`;
  const cached = UNISWAP_SOURCE_CACHE.get(cacheKey);
  if (cached) return cached;

  const out: Hex[] = [];
  const path = join(SURFACE_DIR, "uniswap", "_address_universe.json");
  if (safeExists(path)) {
    const parsed = JSON.parse(readFileSync(path, "utf8")) as
      | { candidates?: Array<Record<string, unknown>> }
      | Array<Record<string, unknown>>;
    const candidates = Array.isArray(parsed) ? parsed : Array.isArray(parsed.candidates) ? parsed.candidates : [];
    for (const candidate of candidates) {
      const candidateChain = candidate.chainId ?? candidate.chain_id;
      const address = String(candidate.address ?? "").toLowerCase();
      if (candidateChain !== chainId) continue;
      if (candidate.decision !== "cover") continue;
      if (candidate.batch !== batch) continue;
      if (ADDR_RE.test(address) && !/^0x0{40}$/i.test(address)) out.push(address);
    }
  }

  out.sort();
  UNISWAP_SOURCE_CACHE.set(cacheKey, out);
  return out;
}

function gatedSourceAddresses(
  sourceSpec: string,
  chainIds: unknown,
  selector: string,
  gatedCoverageSelectors: GatedCoverageSelectors,
): Record<string, Hex[]> {
  if (!Array.isArray(chainIds)) return {};

  const tokenMatch = sourceSpec.match(TOKEN_SOURCE_RE);
  const out: Record<string, Hex[]> = {};
  for (const chainIdRaw of chainIds) {
    const chainId = Number(chainIdRaw);
    if (!Number.isInteger(chainId) || chainId < 1) continue;

    let candidates: Hex[] = [];
    if (tokenMatch) {
      candidates = tokenAddressesForKind(chainId, tokenMatch[1] as TokenErcKind);
    } else if (sourceSpec === UNISWAP_V2_PAIR_SOURCE) {
      candidates = uniswapCandidateAddresses(chainId, UNISWAP_V2_PAIR_BATCH);
    } else if (sourceSpec === UNISWAP_V3_POOL_SOURCE) {
      candidates = uniswapCandidateAddresses(chainId, UNISWAP_V3_POOL_BATCH);
    } else if (sourceSpec === BALANCER_V3_POOL_TOKENS_SOURCE) {
      // Materializes to the single V3 Router (mainnet); cover selectors live on
      // the Router and the pool->tokens map rides as $source context.
      candidates = chainId === 1 ? [BALANCER_V3_ROUTER_MAINNET] : [];
    } else if (sourceSpec === BALANCER_V3_COMPOSITE_TOKENS_SOURCE) {
      // Materializes to the single CompositeLiquidityRouter v2 (mainnet).
      candidates = chainId === 1 ? [BALANCER_V3_COMPOSITE_MAINNET] : [];
    } else {
      continue;
    }

    const selected = candidates.filter((addr) => gatedCoverageSelectors.get(ckey(chainId, addr))?.has(selector));
    if (selected.length > 0) out[String(chainId)] = selected;
  }
  return out;
}

// ---------------------------------------------------------------------------
// Step 1 — collect the authored manifest surface, grouped per (chainId,address)
// ---------------------------------------------------------------------------

function collectManifestSurface(gatedCoverageSelectors: GatedCoverageSelectors): {
  byContract: Map<string, ContractManifests>;
  protocolByKey: Map<string, string>;
} {
  const byContract = new Map<string, ContractManifests>();
  const protocolByKey = new Map<string, string>();

  const get = (key: string): ContractManifests => {
    let v = byContract.get(key);
    if (!v) {
      v = { onchainSelectors: new Map(), signedTypes: new Map() };
      byContract.set(key, v);
    }
    return v;
  };

  for (const path of walkJsonFiles(MANIFESTS_DIR, new Set(["_template"]))) {
    const rel = relative(REGISTRY_ROOT, path);
    let obj: any;
    try {
      obj = JSON.parse(readFileSync(path, "utf8"));
    } catch (e) {
      throw new Error(`surface-gate: invalid JSON in ${rel}: ${(e as Error).message}`);
    }
    const m = obj?.match;
    if (!m || typeof m !== "object") continue;

    const selector = String(m.selector ?? "").toLowerCase();
    const isTypedData = m.typed_data && typeof m.typed_data === "object";
    const primaryType: string | undefined = isTypedData ? m.typed_data.primary_type : undefined;
    const verifying: string | undefined = isTypedData
      ? String(m.typed_data.verifying_contract ?? "").toLowerCase()
      : undefined;
    const abi = obj?.abi_fragment?.abi;
    const hasConcreteOnchainSelector =
      SELECTOR_RE.test(selector) &&
      abi &&
      typeof abi === "object" &&
      abi.type === "function" &&
      toFunctionSelector(abi).toLowerCase() === selector;

    const sourceSpec = typeof m.chain_to_addresses_source === "string" ? m.chain_to_addresses_source : undefined;
    if (sourceSpec) {
      if (hasConcreteOnchainSelector) {
        const effective = gatedSourceAddresses(sourceSpec, m.chain_ids, selector, gatedCoverageSelectors);
        for (const [chainStr, addrs] of Object.entries(effective)) {
          const chainId = Number(chainStr);
          if (!Number.isInteger(chainId)) continue;
          for (const addrRaw of addrs) {
            const addr = String(addrRaw).toLowerCase();
            const key = ckey(chainId, addr);
            const v = get(key);
            const arr = v.onchainSelectors.get(selector) ?? [];
            if (!arr.includes(rel)) arr.push(rel);
            v.onchainSelectors.set(selector, arr);
            if (!protocolByKey.has(key)) protocolByKey.set(key, protocolOf(path));
          }
        }
      }
      continue;
    }

    const c2a = m.chain_to_addresses;
    if (!c2a || typeof c2a !== "object") continue;

    for (const [chainStr, addrs] of Object.entries(c2a)) {
      const chainId = Number(chainStr);
      if (!Number.isInteger(chainId)) continue;
      for (const addrRaw of addrs as Hex[]) {
        const addr = String(addrRaw).toLowerCase();
        if (isTypedData) {
          // Off-chain signed-struct manifest: selector is a sentinel placeholder.
          // Route by EIP-712 primary_type against the verifying contract.
          const key = ckey(chainId, verifying || addr);
          if (primaryType) {
            const v = get(key);
            const arr = v.signedTypes.get(primaryType) ?? [];
            arr.push(rel);
            v.signedTypes.set(primaryType, arr);
            if (!protocolByKey.has(key)) protocolByKey.set(key, protocolOf(path));
          }
        }
        if (!isTypedData || hasConcreteOnchainSelector) {
          const key = ckey(chainId, addr);
          const v = get(key);
          const arr = v.onchainSelectors.get(selector) ?? [];
          arr.push(rel);
          v.onchainSelectors.set(selector, arr);
          if (!protocolByKey.has(key)) protocolByKey.set(key, protocolOf(path));
        }
      }
    }
  }
  return { byContract, protocolByKey };
}

// ---------------------------------------------------------------------------
// Step 2 — load committed snapshot + coverage pairs from surface/
// ---------------------------------------------------------------------------

interface SurfaceUnit {
  coveragePath: string;
  coverage: Coverage;
  snapshot: Snapshot;
}

function loadSurfaceUnits(): SurfaceUnit[] {
  const units: SurfaceUnit[] = [];
  for (const path of walkJsonFiles(SURFACE_DIR)) {
    if (!path.endsWith(".coverage.json")) continue;
    const rel = relative(REGISTRY_ROOT, path);
    const coverage = JSON.parse(readFileSync(path, "utf8")) as Coverage;
    if (typeof coverage.snapshot !== "string") {
      throw new Error(`surface-gate: ${rel} missing "snapshot" field`);
    }
    const snapPath = join(dirname(path), coverage.snapshot);
    if (!safeExists(snapPath)) {
      throw new Error(`surface-gate: ${rel} references snapshot ${coverage.snapshot} which does not exist`);
    }
    const snapshot = JSON.parse(readFileSync(snapPath, "utf8")) as Snapshot;
    units.push({ coveragePath: rel, coverage, snapshot });
  }
  return units;
}

/** Load `surface/<protocol>/_deployments.json` ground-truth lists (I0, opt-in). */
function loadDeploymentLists(): Map<string, { path: string; deployments: Deployments }> {
  const out = new Map<string, { path: string; deployments: Deployments }>();
  if (!safeExists(SURFACE_DIR)) return out;
  for (const proto of readdirSync(SURFACE_DIR)) {
    const dir = join(SURFACE_DIR, proto);
    if (!statSync(dir).isDirectory()) continue;
    const dpath = join(dir, "_deployments.json");
    if (!safeExists(dpath)) continue;
    const deployments = JSON.parse(readFileSync(dpath, "utf8")) as Deployments;
    out.set(proto, { path: relative(REGISTRY_ROOT, dpath), deployments });
  }
  return out;
}

// ---------------------------------------------------------------------------
// Step 3 — the gate
// ---------------------------------------------------------------------------

function mutatingSelectors(snap: Snapshot): Map<string, string> {
  const out = new Map<string, string>(); // selector(lc) -> fn name
  for (const item of snap.abi) {
    if (item.type !== "function") continue;
    if (!MUTABLE.has(item.stateMutability ?? "nonpayable")) continue;
    const sel = toFunctionSelector(item as any).toLowerCase();
    out.set(sel, item.name ?? "(anonymous)");
  }
  return out;
}

/**
 * Does the snapshot expose a value-receiving entrypoint with NO 4-byte selector —
 * a `receive()` (payable by EVM definition) or a payable `fallback()`? Bare-ETH
 * calls (empty calldata + value) hit these and are routed under the reserved
 * `0x00000000` native-transfer sentinel, not by a function selector. This grounds
 * an opt-in `0x00000000` coverage entry in 1st-party ABI so I1' can accept it as a
 * real surface (e.g. Lido: ETH→stETH fallback stake / ETH→wstETH receive stake).
 */
function hasNativeEntrypoint(snap: Snapshot): boolean {
  return snap.abi.some(
    (item) =>
      item.type === "receive" ||
      (item.type === "fallback" && item.stateMutability === "payable"),
  );
}

function main(): void {
  const failures: string[] = [];
  const warnings: string[] = [];
  const summary: string[] = [];

  const units = loadSurfaceUnits();
  const { byContract, protocolByKey } = collectManifestSurface(buildGatedCoverageSelectors(units));
  const gatedKeys = new Set<string>();
  const gatedByProtocol = new Map<string, Set<string>>(); // protocol -> ckeys (for I0)

  for (const u of units) {
    const { coverage: cov, snapshot: snap, coveragePath } = u;
    const label = `${cov.contract ?? snap.contract ?? "?"} [${cov.chainId}]`;

    // Effective pool addresses sharing this snapshot's ABI. Single-address mode
    // (cov.address) and multi-address mode (cov.addresses[], factory pools)
    // both normalise to a lowercased list.
    const effectiveAddresses = coverageAddresses(cov);
    if (effectiveAddresses.length === 0) {
      failures.push(`${label}: coverage has neither "address" nor a non-empty "addresses[]"`);
      continue;
    }
    for (const a of effectiveAddresses) gatedKeys.add(ckey(cov.chainId, a));
    const unitProto = surfaceProtocolOf(coveragePath);
    let pset = gatedByProtocol.get(unitProto);
    if (!pset) {
      pset = new Set<string>();
      gatedByProtocol.set(unitProto, pset);
    }
    for (const a of effectiveAddresses) pset.add(ckey(cov.chainId, a));

    // Chain / address agreement. In multi-address mode the snapshot is the
    // shared IMPLEMENTATION ABI (its address is the impl, not a pool), so only
    // the chainId is checked; in single-address mode the snapshot IS the contract.
    if (cov.chainId !== snap.chainId) {
      failures.push(`${label}: coverage chainId ${cov.chainId} != snapshot chainId ${snap.chainId}`);
    }
    if (!cov.addresses && cov.address && cov.address.toLowerCase() !== snap.address.toLowerCase()) {
      failures.push(`${label}: coverage address ${cov.address} != snapshot address ${snap.address}`);
    }

    const surface = mutatingSelectors(snap);
    const fns = cov.functions ?? {};

    // I1 — completeness: every snapshot mutating selector is triaged (once).
    for (const [sel, name] of surface) {
      if (!fns[sel]) {
        failures.push(`${label}: I1 un-triaged external-mutating selector ${sel} (${name}) — add cover|exclude to ${coveragePath}`);
      }
    }
    // I1' — no stale / malformed coverage entries (once).
    for (const sel of Object.keys(fns)) {
      if (sel === NATIVE_TRANSFER_SELECTOR) {
        // Selector-less native-transfer sentinel (bare-ETH receive()/fallback()).
        // It is NOT an ABI function selector, so it is absent from `surface`;
        // ground it in the snapshot's receive/fallback entry instead of the
        // function-surface check (opt-in — only files that cover it reach here).
        if (!hasNativeEntrypoint(snap)) {
          failures.push(
            `${label}: I1' native-transfer sentinel ${sel} (${fns[sel].name}) requires a receive() or payable fallback() entry in the snapshot to ground it`,
          );
        }
      } else if (!surface.has(sel)) {
        failures.push(`${label}: I1' coverage selector ${sel} (${fns[sel].name}) is NOT an external-mutating fn in the snapshot (stale/typo)`);
      }
      if (!SELECTOR_RE.test(sel)) {
        failures.push(`${label}: I1' coverage key ${sel} is not a 0x+8hex selector`);
      }
      const d = fns[sel].decision;
      if (d !== "cover" && d !== "exclude") {
        failures.push(`${label}: ${sel} (${fns[sel].name}) decision must be "cover"|"exclude", got ${JSON.stringify(d)}`);
      }
      if (d === "exclude" && !String(fns[sel].reason ?? "").trim()) {
        failures.push(`${label}: ${sel} (${fns[sel].name}) is exclude but has no reason`);
      }
    }

    // I2 / I3 / S1 / S2 — per pool address. Every listed pool must independently
    // carry the cover-selector manifests; a pool missing one fails the gate.
    const signed = cov.signed_structs ?? {};
    const multi = effectiveAddresses.length > 1;
    let onchainManifestTotal = 0;
    let signedManifestTotal = 0;
    for (const addr of effectiveAddresses) {
      const at = multi ? ` @${addr}` : "";
      const manifests =
        byContract.get(ckey(cov.chainId, addr)) ?? { onchainSelectors: new Map(), signedTypes: new Map() };
      onchainManifestTotal += manifests.onchainSelectors.size;
      signedManifestTotal += manifests.signedTypes.size;

      // I2 — every cover selector has a manifest at this pool.
      for (const [sel, fn] of Object.entries(fns)) {
        if (fn.decision !== "cover") continue;
        if (!manifests.onchainSelectors.has(sel)) {
          failures.push(`${label}: I2 cover selector ${sel} (${fn.name}) has NO manifest at ${cov.chainId}/${addr}`);
        }
      }
      // I3 — every on-chain manifest selector at this pool is a cover.
      for (const [sel, paths] of manifests.onchainSelectors) {
        const fn = fns[sel];
        if (!fn) {
          failures.push(`${label}${at}: I3 manifest ${paths[0]} selector ${sel} not triaged in coverage`);
        } else if (fn.decision !== "cover") {
          failures.push(`${label}${at}: I3 manifest ${paths[0]} exists for ${sel} (${fn.name}) but coverage decision=${fn.decision}`);
        }
      }
      // S1 — every typed-data manifest at this pool is covered in signed_structs.
      for (const [ptype, paths] of manifests.signedTypes) {
        const s = signed[ptype];
        if (!s || s.decision !== "cover") {
          failures.push(`${label}${at}: S1 typed-data manifest ${paths[0]} primary_type "${ptype}" is not a cover in signed_structs`);
        }
      }
      // S2 — every signed_structs cover has a typed-data manifest at this pool.
      for (const [ptype, s] of Object.entries(signed)) {
        if (s.decision === "cover" && !manifests.signedTypes.has(ptype)) {
          failures.push(`${label}: S2 signed_structs cover "${ptype}" has NO typed-data manifest at ${cov.chainId}/${addr}`);
        }
      }
    }

    const cov_n = Object.values(fns).filter((f) => f.decision === "cover").length;
    const exc_n = Object.values(fns).filter((f) => f.decision === "exclude").length;
    const addrDesc = multi ? `${effectiveAddresses.length} pools` : effectiveAddresses[0];
    summary.push(
      `  ✓ ${label} (${addrDesc}): ${surface.size} surface · ${cov_n} cover · ${exc_n} exclude · ` +
        `${onchainManifestTotal} on-chain manifests · ${signedManifestTotal} signed-struct`,
    );
  }

  // I0 — contract-inventory completeness (per protocol, opt-in via _deployments.json).
  // Catches a contract research NEVER FOUND — the blind spot of I1 + the
  // address-keyed real-tx pull (both bounded by the contract set research produced).
  const deploymentLists = loadDeploymentLists();
  const protocolsAwaitingInventory = new Set<string>(gatedByProtocol.keys());
  for (const [proto, { path: dpath, deployments }] of deploymentLists) {
    protocolsAwaitingInventory.delete(proto);
    const gated = gatedByProtocol.get(proto) ?? new Set<string>();
    const contracts = Array.isArray(deployments.contracts) ? deployments.contracts : [];
    if (contracts.length === 0) {
      failures.push(`I0 ${dpath}: "contracts" is empty or missing`);
      continue;
    }
    const listed = new Set<string>();
    let i0cover = 0;
    let i0excl = 0;
    for (const c of contracts) {
      if (!Number.isInteger(c.chainId) || !ADDR_RE.test(String(c.address ?? ""))) {
        failures.push(`I0 ${dpath}: "${c.name ?? "(noname)"}" needs an integer chainId + 0x+40hex address`);
        continue;
      }
      if (c.decision !== "cover" && c.decision !== "exclude") {
        failures.push(`I0 ${dpath}: "${c.name}" decision must be cover|exclude, got ${JSON.stringify(c.decision)}`);
        continue;
      }
      const k = ckey(c.chainId, c.address);
      listed.add(k);
      if (c.decision === "cover") {
        i0cover++;
        if (!gated.has(k)) {
          failures.push(
            `I0 ${dpath}: deployment "${c.name}" (${c.chainId}/${c.address}) is COVER but has NO ` +
              `surface snapshot/coverage — research missed a user-facing contract, or mark it exclude:<reason>`,
          );
        }
      } else {
        i0excl++;
        if (!String(c.reason ?? "").trim()) {
          failures.push(`I0 ${dpath}: deployment "${c.name}" is exclude but has no reason`);
        }
      }
    }
    // I0' no-stale: a gated contract absent from the deployment list — stale list
    // or address mismatch. WARN (not fail): the snapshot may legitimately lead.
    for (const k of gated) {
      if (!listed.has(k)) {
        warnings.push(`I0' ${proto}: gated contract ${k} is not in ${dpath} (stale deployment list or address mismatch?)`);
      }
    }
    summary.push(
      `  ✓ [I0] ${proto}: ${contracts.length} deployed · ${i0cover} cover · ${i0excl} exclude ` +
        `(contract-inventory enforced vs ${deployments.source ?? "1st-party list"})`,
    );
  }
  // Protocols with surface units but NO deployment list — contract-inventory NOT
  // enforced (only per-contract function coverage is). Visible WARN, never silent.
  for (const proto of [...protocolsAwaitingInventory].sort()) {
    warnings.push(
      `${proto}: contract-inventory NOT enforced (no surface/${proto}/_deployments.json 1st-party ground truth) — ` +
        `function coverage is gated per known contract, but a MISSED contract would be invisible`,
    );
  }

  // Ungated protocol contracts (have manifests, no snapshot) — visible, never silent.
  const ungatedByProtocol = new Map<string, number>();
  for (const key of byContract.keys()) {
    if (gatedKeys.has(key)) continue;
    const proto = protocolByKey.get(key) ?? "(unknown)";
    ungatedByProtocol.set(proto, (ungatedByProtocol.get(proto) ?? 0) + 1);
  }
  if (ungatedByProtocol.size > 0) {
    const total = [...ungatedByProtocol.values()].reduce((a, b) => a + b, 0);
    const parts = [...ungatedByProtocol.entries()].sort().map(([p, n]) => `${p}=${n}`);
    warnings.push(
      `${total} protocol contract(s) UNGATED (no surface/ snapshot — completeness NOT enforced): ${parts.join(", ")}`,
    );
  }

  // Report
  console.log("surface-completeness gate");
  console.log(`  registry root: ${REGISTRY_ROOT}`);
  console.log(`  gated contracts: ${units.length}`);
  if (summary.length) console.log(summary.join("\n"));
  if (warnings.length) {
    console.log("\nWARN (informational, not a failure):");
    for (const w of warnings) console.log(`  ⚠ ${w}`);
  }
  if (failures.length) {
    console.error(`\nFAIL — ${failures.length} surface-completeness violation(s):`);
    for (const f of failures) console.error(`  ✗ ${f}`);
    console.error(
      "\nThe research surface is incomplete or inconsistent. Every external-mutating function\n" +
        "must be explicitly COVER (with a manifest) or EXCLUDE (with a reason). See surface/README.md.",
    );
    process.exit(1);
  }
  console.log("\nPASS — every gated contract's external surface is fully triaged and consistent.");
}

main();
