/**
 * check-surface-completeness.ts — ScopeBall Adapter Registry v3
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
 * delegation primitive = ScopeBall's raison d'être). A prose checklist can be
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
 * WARN, never silent — onboarding them is opt-in per protocol). ERC token
 * standards (manifests using `chain_to_addresses_source: "tokens:*"`) are
 * counted only for already-gated token contracts, so canonical token manifests
 * can satisfy I2/I3 without turning every token list entry into an ungated WARN.
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
const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;
const MUTABLE = new Set(["nonpayable", "payable"]);
const TOKEN_ERC_KINDS = new Set(["erc20", "erc721", "erc1155"]);

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type Hex = string;
type TokenErcKind = "erc20" | "erc721" | "erc1155";

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
  address: Hex;
  snapshot: string;
  functions: Record<string, CoverageFn>;
  signed_structs?: Record<string, { decision: "cover" | "exclude"; reason: string }>;
}

/** On-chain + signed-struct manifest surface, grouped per (chainId, address). */
interface ContractManifests {
  onchainSelectors: Map<string, string[]>; // selector(lc) -> manifest rel-paths
  signedTypes: Map<string, string[]>; // EIP-712 primary_type -> manifest rel-paths
}

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
  const rel = relative(MANIFESTS_DIR, manifestPath);
  const seg = rel.split(/[/\\]/)[0];
  return seg || "(root)";
}

function semanticTokenKind(meta: Record<string, unknown>): string | undefined {
  const tokenKind = meta.token_kind;
  if (typeof tokenKind !== "object" || tokenKind === null || Array.isArray(tokenKind)) return undefined;
  const kind = (tokenKind as Record<string, unknown>).kind;
  return typeof kind === "string" ? kind : undefined;
}

function tokenPassesSourceFilter(meta: Record<string, unknown>, match: Record<string, unknown>): boolean {
  const excluded = match.semantic_token_kind_exclude;
  if (!Array.isArray(excluded) || excluded.length === 0) return true;
  const kind = semanticTokenKind(meta);
  return kind === undefined || !excluded.includes(kind);
}

function readTokenAddresses(chainId: number, ercKind: TokenErcKind, match: Record<string, unknown>): Hex[] {
  const chainPath = join(TOKENS_DIR, String(chainId));
  if (!safeExists(chainPath)) return [];

  const addresses: Hex[] = [];
  for (const fname of readdirSync(chainPath)) {
    if (!fname.endsWith(".json")) continue;
    const path = join(chainPath, fname);
    const obj = JSON.parse(readFileSync(path, "utf8")) as Record<string, unknown>;
    if (obj.erc_kind !== ercKind) continue;
    if (obj.chainId !== chainId) continue;
    if (!tokenPassesSourceFilter(obj, match)) continue;
    const address = obj.address;
    if (typeof address === "string" && ADDRESS_RE.test(address)) {
      addresses.push(address.toLowerCase());
    }
  }
  return addresses.sort();
}

function collectSourceManifestSurface(
  path: string,
  rel: string,
  obj: any,
  match: Record<string, unknown>,
  gatedKeys: ReadonlySet<string>,
  get: (key: string) => ContractManifests,
  protocolByKey: Map<string, string>,
): void {
  const sourceSpec = match.chain_to_addresses_source;
  if (typeof sourceSpec !== "string" || !sourceSpec.startsWith("tokens:")) return;

  const ercKind = sourceSpec.slice("tokens:".length);
  if (!TOKEN_ERC_KINDS.has(ercKind)) return;
  if (!Array.isArray(match.chain_ids)) return;

  const selector = String(match.selector ?? "").toLowerCase();
  const abi = obj?.abi_fragment?.abi;
  const hasConcreteOnchainSelector =
    SELECTOR_RE.test(selector) &&
    abi &&
    typeof abi === "object" &&
    abi.type === "function" &&
    toFunctionSelector(abi).toLowerCase() === selector;
  if (!hasConcreteOnchainSelector) return;

  for (const chainRaw of match.chain_ids) {
    const chainId = Number(chainRaw);
    if (!Number.isInteger(chainId)) continue;
    for (const addr of readTokenAddresses(chainId, ercKind as TokenErcKind, match)) {
      const key = ckey(chainId, addr);
      if (!gatedKeys.has(key)) continue;
      const v = get(key);
      const arr = v.onchainSelectors.get(selector) ?? [];
      arr.push(rel);
      v.onchainSelectors.set(selector, arr);
      if (!protocolByKey.has(key)) protocolByKey.set(key, protocolOf(path));
    }
  }
}

// ---------------------------------------------------------------------------
// Step 1 — collect the authored manifest surface, grouped per (chainId,address)
// ---------------------------------------------------------------------------

function collectManifestSurface(gatedKeys: ReadonlySet<string>): {
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

    if ("chain_to_addresses_source" in m) {
      collectSourceManifestSurface(path, rel, obj, m, gatedKeys, get, protocolByKey);
      continue;
    }
    const c2a = m.chain_to_addresses;
    if (!c2a || typeof c2a !== "object") continue;

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

function main(): void {
  const failures: string[] = [];
  const warnings: string[] = [];
  const summary: string[] = [];

  const units = loadSurfaceUnits();
  const gatedKeys = new Set<string>();

  for (const u of units) {
    gatedKeys.add(ckey(u.coverage.chainId, u.coverage.address));
  }

  const { byContract, protocolByKey } = collectManifestSurface(gatedKeys);

  for (const u of units) {
    const { coverage: cov, snapshot: snap, coveragePath } = u;
    const key = ckey(cov.chainId, cov.address);
    const label = `${cov.contract ?? snap.contract ?? "?"} [${cov.chainId}/${cov.address}]`;

    // snapshot/coverage address agreement
    if (cov.address.toLowerCase() !== snap.address.toLowerCase() || cov.chainId !== snap.chainId) {
      failures.push(`${label}: coverage (chainId,address) != snapshot (${snap.chainId}/${snap.address})`);
    }

    const surface = mutatingSelectors(snap);
    const fns = cov.functions ?? {};
    const manifests = byContract.get(key) ?? { onchainSelectors: new Map(), signedTypes: new Map() };

    // I1 — completeness: every snapshot mutating selector is triaged
    for (const [sel, name] of surface) {
      if (!fns[sel]) {
        failures.push(`${label}: I1 un-triaged external-mutating selector ${sel} (${name}) — add cover|exclude to ${coveragePath}`);
      }
    }
    // I1' — no stale coverage entries
    for (const sel of Object.keys(fns)) {
      if (!surface.has(sel)) {
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
    // I2 — every cover selector has a manifest
    for (const [sel, fn] of Object.entries(fns)) {
      if (fn.decision !== "cover") continue;
      if (!manifests.onchainSelectors.has(sel)) {
        failures.push(`${label}: I2 cover selector ${sel} (${fn.name}) has NO manifest at this (chain,address)`);
      }
    }
    // I3 — every on-chain manifest selector is a cover in coverage
    for (const [sel, paths] of manifests.onchainSelectors) {
      const fn = fns[sel];
      if (!fn) {
        failures.push(`${label}: I3 manifest ${paths[0]} selector ${sel} not triaged in coverage`);
      } else if (fn.decision !== "cover") {
        failures.push(`${label}: I3 manifest ${paths[0]} exists for ${sel} (${fn.name}) but coverage decision=${fn.decision}`);
      }
    }
    // S1/S2 — EIP-712 signed structs (best-effort)
    const signed = cov.signed_structs ?? {};
    for (const [ptype, paths] of manifests.signedTypes) {
      const s = signed[ptype];
      if (!s || s.decision !== "cover") {
        failures.push(`${label}: S1 typed-data manifest ${paths[0]} primary_type "${ptype}" is not a cover in signed_structs`);
      }
    }
    for (const [ptype, s] of Object.entries(signed)) {
      if (s.decision === "cover" && !manifests.signedTypes.has(ptype)) {
        failures.push(`${label}: S2 signed_structs cover "${ptype}" has NO typed-data manifest`);
      }
    }

    const cov_n = Object.values(fns).filter((f) => f.decision === "cover").length;
    const exc_n = Object.values(fns).filter((f) => f.decision === "exclude").length;
    summary.push(
      `  ✓ ${label}: ${surface.size} surface · ${cov_n} cover · ${exc_n} exclude · ` +
        `${manifests.onchainSelectors.size} on-chain manifests · ${manifests.signedTypes.size} signed-struct`,
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
