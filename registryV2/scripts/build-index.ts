/**
 * build-index.ts — ScopeBall Adapter Registry v3
 *
 * Forked from `registry/scripts/build-index.ts` (v2) and extended for the
 * schema v3 surface (PDF FSM spec). The build-index step is intentionally a
 * thin pass-through — manifest fields that v3 adds (`emit.body`, `live_inputs`,
 * `recurse`) are copied verbatim into the index entry. Deep schema validation
 * (ActionBody enum well-formedness, DataSource variant correctness, recurse
 * strategy compatibility) is delegated to a separate validate step.
 *
 * Manifest schema v3 (registryV2/docs/SCHEMA_V3.md):
 *   {
 *     "type": "adapter_action",                    // v3: renamed from "adapter_function"
 *     "id": "<publisher>/<contract>/<func>@<v>",
 *     "publisher": "<eth-name>",                   // optional for standard/*
 *     "schema_version": "3",                        // required, exactly "3"
 *
 *     "match": {
 *       "selector": "0x<8 hex>",
 *
 *       // ── exactly one of the two `to` modes ──
 *
 *       // mode A — explicit chain → addresses map (Uniswap etc.)
 *       "chain_to_addresses": {
 *         "<chainId>": ["0x<40 hex>", ...]
 *       }
 *
 *       // mode B — auto-enumerate from `tokens/<chainId>/<addr>.json`
 *       // (ERC20 / ERC721 / ERC1155 standard manifests)
 *       "chain_to_addresses_source": "tokens:erc20" | "tokens:erc721" | "tokens:erc1155",
 *       "chain_ids": [<chainId>, ...]
 *     },
 *
 *     "abi_fragment": { ... },                      // v3: unchanged from v2
 *     "emit":         {
 *       "strategy": "single_emit" | "array_emit" | "multicall_recurse" | "opcode_stream_dispatch" | ...,
 *       "body":     { ... },                        // v3 NEW: hierarchical ActionBody (PDF spec)
 *       "live_inputs": { ... },                     // v3 NEW: LiveField source descriptors
 *       ...
 *     },
 *     "recurse":      { ... },                      // v3 NEW: Multicall handler descriptor (optional)
 *     "requires":     { ... }                       // v3: unchanged from v2
 *   }
 *
 * Algorithm:
 *   1. walk `manifests/**\/*.json` (skip `_template/` subtrees)
 *   2. parseBundle() — schema validation:
 *        (a) type === "adapter_action"
 *        (b) schema_version === "3"
 *        (c) match.selector regex + match has exactly one of
 *            chain_to_addresses / chain_to_addresses_source
 *        (d) selector / chainId / address regex mismatch reject
 *   3. If `chain_to_addresses_source` present:
 *        - parse "tokens:<kind>" with kind ∈ {erc20, erc721, erc1155}
 *        - for each chainId in `match.chain_ids`, walk `tokens/<chainId>/*.json`
 *          and select addresses whose `erc_kind` field matches
 *        - build effective `chain_to_addresses`
 *   4. Substitute effective `chain_to_addresses` back into the bundle
 *      (delete `chain_to_addresses_source` and `chain_ids` from the inlined
 *      `bundle.match`) so the index entry is self-contained.
 *   5. bundle_sha256 = "0x" + sha256(canonicalize(bundle))   (RFC 8785 JCS)
 *   6. for each (chainId, to) pair: write
 *      `index/by-callkey/<chainId>__<to.toLowerCase()>__<selector.toLowerCase()>.json`
 *      with entry { matched: true, bundle_id, manifest_path, bundle_sha256, bundle }
 *   7. wipeDir(`index/by-callkey/`) before write to prevent orphan
 *
 * v3 differences from v2:
 *   - `type` literal changed: `adapter_function` → `adapter_action`
 *   - `schema_version` required value changed: `"2"` → `"3"`
 *   - v2's `validateAssetRefs` lint is dropped — v3's hierarchical `emit.body`
 *     produces fully-typed ActionBody payloads, so the v2 flat `fields` map
 *     lint surface no longer applies. A future `validate-emit-body.ts` script
 *     can run deep ActionBody / DataSource schema enforcement.
 *   - `emit.body`, `emit.live_inputs`, `recurse` are pass-through — copied
 *     verbatim into the index entry without structural validation.
 *   - tokens/<chainId>/<addr>.json now uses `erc_kind` (ERC contract kind:
 *     erc20/erc721/erc1155) to disambiguate from the semantic `TokenKind`
 *     enum (Base, NativeGas, Wrapped, LpShare, YieldReceipt, ... 10 variants)
 *     declared in `crates/policy-server/asset-model/state/src/token/kind.rs`. The build-index
 *     step only consumes `erc_kind` for auto-enumerate; the semantic
 *     `token_kind` field (if present) is treated as opaque metadata.
 *
 * Spec references:
 *   - registryV2/docs/SCHEMA_V3.md
 *   - registryV2/docs/ERC_AUTO_ENUMERATE_V3.md
 *   - registryV2/docs/TOKEN_SCHEMA_V3.md
 *   - registryV2/docs/SOURCE_CATALOG.md
 *   - RFC 8785 (JSON Canonicalization Scheme) — https://www.rfc-editor.org/rfc/rfc8785
 *
 * Run:
 *   $ npm install
 *   $ npm run build
 */

import { createHash } from "node:crypto";
import { mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import canonicalize from "canonicalize";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ChainId = number;
type Hex = string;

interface BundleMatchSpecific {
  selector: Hex;
  chain_to_addresses: Record<string, Hex[]>;
}

interface BundleMatchSourced {
  selector: Hex;
  chain_to_addresses_source: TokenErcKindSource;
  chain_ids: ChainId[];
}

type BundleMatch = BundleMatchSpecific | BundleMatchSourced;

/** ERC contract kind (registry-level, distinct from semantic TokenKind). */
type TokenErcKind = "erc20" | "erc721" | "erc1155";
type TokenErcKindSource = `tokens:${TokenErcKind}`;

interface AdapterBundle {
  type: "adapter_action";
  id: string;
  schema_version: "3";
  publisher?: string;
  match: BundleMatch;
  [key: string]: unknown;
}

interface ResolvedBundle {
  type: "adapter_action";
  id: string;
  schema_version: "3";
  publisher?: string;
  match: BundleMatchSpecific;
  [key: string]: unknown;
}

interface IndexEntry {
  matched: true;
  bundle_id: string;
  manifest_path: string;
  bundle_sha256: Hex;
  bundle: ResolvedBundle;
}

interface TokenMetadata {
  erc_kind: TokenErcKind;
  chainId: ChainId;
  address: Hex;
  [key: string]: unknown;
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REGISTRY_ROOT = resolve(__dirname, "..");
const MANIFESTS_DIR = join(REGISTRY_ROOT, "manifests");
const TOKENS_DIR = join(REGISTRY_ROOT, "tokens");
const INDEX_BY_CALLKEY_DIR = join(REGISTRY_ROOT, "index", "by-callkey");

// ---------------------------------------------------------------------------
// Regex constants
// ---------------------------------------------------------------------------

const SELECTOR_RE = /^0x[0-9a-fA-F]{8}$/;
const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;
const SCHEMA_VERSION_REQUIRED = "3" as const;
const ADAPTER_TYPE_REQUIRED = "adapter_action" as const;
const TOKEN_ERC_KINDS: ReadonlySet<TokenErcKind> = new Set(["erc20", "erc721", "erc1155"]);

// ---------------------------------------------------------------------------
// Filesystem helpers
// ---------------------------------------------------------------------------

function safeExists(p: string): boolean {
  try {
    statSync(p);
    return true;
  } catch {
    return false;
  }
}

function walkJsonFiles(root: string, opts: { skipDirs?: ReadonlySet<string> } = {}): string[] {
  const skipDirs = opts.skipDirs ?? new Set<string>();
  const out: string[] = [];
  if (!safeExists(root)) return out;
  const stack: string[] = [root];
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

function wipeDir(p: string): void {
  if (safeExists(p)) {
    rmSync(p, { recursive: true, force: true });
  }
  mkdirSync(p, { recursive: true });
}

function sha256Hex(s: string): string {
  return createHash("sha256").update(s, "utf8").digest("hex");
}

// ---------------------------------------------------------------------------
// Token registry — load + index by (chainId, erc_kind)
// ---------------------------------------------------------------------------

/** Map<chainId, Map<erc_kind, Set<lowercased address>>> */
type TokensByChainKind = Map<ChainId, Map<TokenErcKind, Set<Hex>>>;

function loadTokensIndex(): TokensByChainKind {
  const out: TokensByChainKind = new Map();
  if (!safeExists(TOKENS_DIR)) return out;

  // tokens/<chainId>/*.json
  for (const chainDir of readdirSync(TOKENS_DIR)) {
    const chainPath = join(TOKENS_DIR, chainDir);
    const s = statSync(chainPath);
    if (!s.isDirectory()) continue;
    const chainId = Number(chainDir);
    if (!Number.isInteger(chainId) || chainId < 1) {
      throw new Error(`tokens/: invalid chain directory name "${chainDir}" — expected positive integer`);
    }

    const perKind = new Map<TokenErcKind, Set<Hex>>();
    perKind.set("erc20", new Set());
    perKind.set("erc721", new Set());
    perKind.set("erc1155", new Set());

    for (const fname of readdirSync(chainPath)) {
      if (!fname.endsWith(".json")) continue;
      const fpath = join(chainPath, fname);
      const meta = loadTokenMetadata(fpath, chainId);
      perKind.get(meta.erc_kind)!.add(meta.address.toLowerCase());
    }

    out.set(chainId, perKind);
  }
  return out;
}

function loadTokenMetadata(path: string, expectedChainId: ChainId): TokenMetadata {
  const raw = readFileSync(path, "utf8");
  let json: unknown;
  try {
    json = JSON.parse(raw);
  } catch (e) {
    throw new Error(`tokens/: invalid JSON in ${path}: ${(e as Error).message}`);
  }
  if (typeof json !== "object" || json === null || Array.isArray(json)) {
    throw new Error(`tokens/: ${path} must be a JSON object`);
  }
  const obj = json as Record<string, unknown>;

  const ercKind = obj.erc_kind;
  if (typeof ercKind !== "string" || !TOKEN_ERC_KINDS.has(ercKind as TokenErcKind)) {
    throw new Error(
      `tokens/: ${path} has missing or invalid "erc_kind" — expected one of erc20/erc721/erc1155, got ${JSON.stringify(ercKind)}`,
    );
  }

  const address = obj.address;
  if (typeof address !== "string" || !ADDRESS_RE.test(address)) {
    throw new Error(
      `tokens/: ${path} has missing or invalid "address" — expected "0x" + 40 hex, got ${JSON.stringify(address)}`,
    );
  }

  const chainId = obj.chainId;
  if (typeof chainId !== "number" || !Number.isInteger(chainId) || chainId < 1) {
    throw new Error(
      `tokens/: ${path} has missing or invalid "chainId" — expected positive integer, got ${JSON.stringify(chainId)}`,
    );
  }
  if (chainId !== expectedChainId) {
    throw new Error(`tokens/: ${path} chainId field (${chainId}) does not match directory (${expectedChainId})`);
  }

  return { erc_kind: ercKind as TokenErcKind, chainId, address, ...obj };
}

// ---------------------------------------------------------------------------
// Bundle parsing + validation
// ---------------------------------------------------------------------------

function loadBundle(path: string): AdapterBundle {
  const raw = readFileSync(path, "utf8");
  let json: unknown;
  try {
    json = JSON.parse(raw);
  } catch (e) {
    throw new Error(`manifests/: invalid JSON in ${path}: ${(e as Error).message}`);
  }
  if (typeof json !== "object" || json === null || Array.isArray(json)) {
    throw new Error(`manifests/: ${path} must be a JSON object`);
  }
  const obj = json as Record<string, unknown>;

  if (obj.type !== ADAPTER_TYPE_REQUIRED) {
    throw new Error(
      `manifests/: ${path} type !== "${ADAPTER_TYPE_REQUIRED}" (got ${JSON.stringify(obj.type)}). ` +
        `v3 renamed the discriminator from "adapter_function" — update the manifest.`,
    );
  }
  if (typeof obj.id !== "string" || obj.id.length === 0) {
    throw new Error(`manifests/: ${path} has missing or invalid "id"`);
  }
  if (obj.schema_version !== SCHEMA_VERSION_REQUIRED) {
    throw new Error(
      `manifests/: ${path} schema_version !== "${SCHEMA_VERSION_REQUIRED}" (got ${JSON.stringify(obj.schema_version)}). ` +
        `registry v3 rejects pre-v3 bundles — migrate to hierarchical emit.body + live_inputs.source first.`,
    );
  }

  validateMatchShape(path, obj.match);
  validateEmitShape(path, obj.emit);

  return json as AdapterBundle;
}

function validateMatchShape(path: string, match: unknown): asserts match is BundleMatch {
  if (typeof match !== "object" || match === null || Array.isArray(match)) {
    throw new Error(`manifests/: ${path} match must be a JSON object`);
  }
  const m = match as Record<string, unknown>;

  if (typeof m.selector !== "string" || !SELECTOR_RE.test(m.selector)) {
    throw new Error(
      `manifests/: ${path} match.selector expected "0x" + 8 hex, got ${JSON.stringify(m.selector)}`,
    );
  }

  const hasMap = "chain_to_addresses" in m;
  const hasSource = "chain_to_addresses_source" in m;

  if (hasMap === hasSource) {
    throw new Error(
      `manifests/: ${path} match must have exactly one of "chain_to_addresses" or "chain_to_addresses_source" (found ${hasMap && hasSource ? "both" : "neither"})`,
    );
  }

  if (hasMap) {
    if (
      typeof m.chain_to_addresses !== "object" ||
      m.chain_to_addresses === null ||
      Array.isArray(m.chain_to_addresses)
    ) {
      throw new Error(`manifests/: ${path} match.chain_to_addresses must be an object`);
    }
    const map = m.chain_to_addresses as Record<string, unknown>;
    if (Object.keys(map).length === 0) {
      throw new Error(`manifests/: ${path} match.chain_to_addresses must have at least one chain entry`);
    }
    for (const [chainKey, addresses] of Object.entries(map)) {
      const chainId = Number(chainKey);
      if (!Number.isInteger(chainId) || chainId < 1) {
        throw new Error(
          `manifests/: ${path} match.chain_to_addresses key "${chainKey}" must stringify a positive integer`,
        );
      }
      if (!Array.isArray(addresses) || addresses.length === 0) {
        throw new Error(
          `manifests/: ${path} match.chain_to_addresses["${chainKey}"] must be a non-empty array`,
        );
      }
      for (const [i, addr] of addresses.entries()) {
        if (typeof addr !== "string" || !ADDRESS_RE.test(addr)) {
          throw new Error(
            `manifests/: ${path} match.chain_to_addresses["${chainKey}"][${i}] expected "0x" + 40 hex, got ${JSON.stringify(addr)}`,
          );
        }
      }
    }
  } else {
    // hasSource — chain_to_addresses_source + chain_ids
    if (typeof m.chain_to_addresses_source !== "string") {
      throw new Error(`manifests/: ${path} match.chain_to_addresses_source must be a string`);
    }
    const parts = m.chain_to_addresses_source.split(":");
    if (parts.length !== 2 || parts[0] !== "tokens" || !TOKEN_ERC_KINDS.has(parts[1] as TokenErcKind)) {
      throw new Error(
        `manifests/: ${path} match.chain_to_addresses_source must be one of "tokens:erc20" | "tokens:erc721" | "tokens:erc1155", got ${JSON.stringify(m.chain_to_addresses_source)}`,
      );
    }
    if (!Array.isArray(m.chain_ids) || m.chain_ids.length === 0) {
      throw new Error(
        `manifests/: ${path} match.chain_ids must be a non-empty array when chain_to_addresses_source is set`,
      );
    }
    for (const [i, cid] of m.chain_ids.entries()) {
      if (typeof cid !== "number" || !Number.isInteger(cid) || cid < 1) {
        throw new Error(
          `manifests/: ${path} match.chain_ids[${i}] must be a positive integer, got ${JSON.stringify(cid)}`,
        );
      }
    }
  }
}

/**
 * v3 emit shape: `{ strategy: <string>, body?: <ActionBody>, live_inputs?: <map>, ... }`.
 *
 * Build-index does NOT deeply validate `body` / `live_inputs` / per-strategy
 * keys — those are pass-through. A separate `validate-emit-body.ts` step
 * enforces full ActionBody / DataSource schema conformance.
 *
 * The only hard requirement here is `emit.strategy` being a non-empty string —
 * an empty `emit` block (or one missing the discriminator) almost always
 * indicates a hand-edit accident.
 */
function validateEmitShape(path: string, emit: unknown): void {
  if (emit === undefined) {
    throw new Error(`manifests/: ${path} missing required "emit" block`);
  }
  if (typeof emit !== "object" || emit === null || Array.isArray(emit)) {
    throw new Error(`manifests/: ${path} emit must be a JSON object`);
  }
  const e = emit as Record<string, unknown>;
  if (typeof e.strategy !== "string" || e.strategy.length === 0) {
    throw new Error(
      `manifests/: ${path} emit.strategy must be a non-empty string (e.g. "single_emit", "array_emit", "multicall_recurse", "opcode_stream_dispatch")`,
    );
  }
}

// ---------------------------------------------------------------------------
// Source resolution — sourced bundles → effective chain_to_addresses
// ---------------------------------------------------------------------------

function resolveBundle(
  bundle: AdapterBundle,
  tokens: TokensByChainKind,
  manifestPath: string,
): ResolvedBundle {
  if ("chain_to_addresses" in bundle.match) {
    // already concrete
    return bundle as ResolvedBundle;
  }

  const sourced = bundle.match as BundleMatchSourced;
  const ercKind = sourced.chain_to_addresses_source.split(":")[1] as TokenErcKind;

  const effective: Record<string, Hex[]> = {};
  let totalAddresses = 0;
  for (const chainId of sourced.chain_ids) {
    const perKind = tokens.get(chainId);
    if (!perKind) {
      throw new Error(
        `manifests/: ${manifestPath} match.chain_to_addresses_source references chain ${chainId} but tokens/${chainId}/ does not exist`,
      );
    }
    const addresses = Array.from(perKind.get(ercKind)!).sort();
    if (addresses.length === 0) {
      // not an error — a chain with no tokens of this kind simply produces zero callkeys.
      console.error(
        `[build-index] WARN ${manifestPath}: chain ${chainId} has no tokens of erc_kind=${ercKind} — 0 callkeys for this (chain, selector)`,
      );
      continue;
    }
    effective[String(chainId)] = addresses;
    totalAddresses += addresses.length;
  }

  if (totalAddresses === 0) {
    throw new Error(
      `manifests/: ${manifestPath} match.chain_to_addresses_source resolved to 0 addresses across all chain_ids — at least one token of erc_kind=${ercKind} required`,
    );
  }

  // Build resolved bundle — replace sourced match with concrete map, drop source/chain_ids fields
  const resolvedMatch: BundleMatchSpecific = {
    selector: sourced.selector,
    chain_to_addresses: effective,
  };

  const { match: _omit, ...rest } = bundle;
  return { ...rest, match: resolvedMatch } as ResolvedBundle;
}

// ---------------------------------------------------------------------------
// SHA-256 + callkey filename
// ---------------------------------------------------------------------------

function computeBundleSha256(bundle: ResolvedBundle): Hex {
  const canonical = canonicalize(bundle);
  if (typeof canonical !== "string") {
    throw new Error("canonicalize returned non-string");
  }
  return "0x" + sha256Hex(canonical);
}

function callkeyFilename(chainId: ChainId, to: Hex, selector: Hex): string {
  return `${chainId}__${to.toLowerCase()}__${selector.toLowerCase()}.json`;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

function main(): void {
  const skipDirs = new Set(["_template"]);
  const manifestFiles = walkJsonFiles(MANIFESTS_DIR, { skipDirs });
  if (manifestFiles.length === 0) {
    console.error(`[build-index] no manifests found in ${MANIFESTS_DIR} — registry empty (expected during Phase 3A scaffold)`);
    // Phase 3A scaffold: zero manifests is not a fatal condition. The
    // index/by-callkey/ directory is wiped + recreated empty, ready for
    // Phase 3C-F manifest authoring.
    wipeDir(INDEX_BY_CALLKEY_DIR);
    return;
  }

  console.error(`[build-index] registry root: ${REGISTRY_ROOT}`);
  console.error(`[build-index] manifests:    ${manifestFiles.length}`);

  const tokens = loadTokensIndex();
  const tokenChainCount = tokens.size;
  let tokenTotal = 0;
  for (const perKind of tokens.values()) {
    for (const set of perKind.values()) tokenTotal += set.size;
  }
  console.error(`[build-index] tokens:       ${tokenTotal} across ${tokenChainCount} chain(s)`);

  // orphan 방지
  wipeDir(INDEX_BY_CALLKEY_DIR);

  let totalCallkeys = 0;
  let totalErrors = 0;
  for (const file of manifestFiles) {
    const manifestPath = relative(REGISTRY_ROOT, file).split(/[\\/]/).join("/");
    try {
      const bundle = loadBundle(file);
      const resolved = resolveBundle(bundle, tokens, manifestPath);
      const bundleSha256 = computeBundleSha256(resolved);

      const pairs = Object.entries(resolved.match.chain_to_addresses);
      const pairCount = pairs.reduce((acc, [, addrs]) => acc + addrs.length, 0);

      console.error(
        `[build-index] ${resolved.id}\n` +
          `              manifest:  ${manifestPath}\n` +
          `              sha256:    ${bundleSha256}\n` +
          `              callkeys:  ${pairCount}`,
      );

      for (const [chainKey, addresses] of pairs) {
        const chainId = Number(chainKey);
        for (const to of addresses) {
          const entry: IndexEntry = {
            matched: true,
            bundle_id: resolved.id,
            manifest_path: manifestPath,
            bundle_sha256: bundleSha256,
            bundle: resolved,
          };
          const fname = callkeyFilename(chainId, to, resolved.match.selector);
          const outPath = join(INDEX_BY_CALLKEY_DIR, fname);
          writeFileSync(outPath, JSON.stringify(entry, null, 2) + "\n", "utf8");
          totalCallkeys++;
        }
      }
    } catch (e) {
      totalErrors++;
      console.error(`[build-index] FAIL ${manifestPath}: ${(e as Error).message}`);
    }
  }

  if (totalErrors > 0) {
    console.error(`[build-index] FAILED — ${totalErrors} manifest(s) rejected, ${totalCallkeys} callkey(s) written`);
    process.exit(1);
  }
  console.error(`[build-index] done — ${totalCallkeys} callkey(s) written across ${manifestFiles.length} manifest(s)`);
}

main();
