/**
 * build-index.ts — ScopeBall Adapter Registry v2
 *
 * Manifest schema v2 (registry/docs/SCHEMA_V2.md):
 *   {
 *     "type": "adapter_function",
 *     "id": "<publisher>/<contract>/<func>@<v>",
 *     "publisher": "<eth-name>",                  // optional for standard/*
 *     "schema_version": "2",                       // required, exactly "2"
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
 *     "abi_fragment": { ... },
 *     "emit":         { ... },
 *     "requires":     { ... }
 *   }
 *
 * Algorithm:
 *   1. walk `manifests/**\/*.json` (skip `_template/` subtrees)
 *   2. parseBundle() — strict schema validation. Reject if
 *        (a) schema_version !== "2"
 *        (b) `match` lacks both `chain_to_addresses` and `chain_to_addresses_source`
 *        (c) `match` has both (mutually exclusive)
 *        (d) selector / chainId / address regex mismatch
 *   3. If `chain_to_addresses_source` present:
 *        - parse "tokens:<kind>" with kind ∈ {erc20, erc721, erc1155}
 *        - for each chainId in `match.chain_ids`, walk `tokens/<chainId>/*.json`
 *          and select addresses whose `kind` field matches
 *        - build effective `chain_to_addresses`
 *   4. Substitute effective `chain_to_addresses` back into the bundle
 *      (delete `chain_to_addresses_source` and `chain_ids` from the inlined
 *      `bundle.match`) so the index entry is self-contained — clients see
 *      a uniform `chain_to_addresses` shape regardless of source mode.
 *   5. bundle_sha256 = "0x" + sha256(canonicalize(bundle))   (RFC 8785 JCS)
 *   6. for each (chainId, to) pair: write
 *      `index/by-callkey/<chainId>__<to.toLowerCase()>__<selector.toLowerCase()>.json`
 *      with entry { matched: true, bundle_id, manifest_path, bundle_sha256, bundle }
 *   7. wipeDir(`index/by-callkey/`) before write to prevent orphan
 *
 * Lint:
 *   - validateAssetRefs() — Aerodrome-audit post-mortem lint.
 *     Reject a bundle whose emit rule binds an erc20/erc721/erc1155 `asset.kind`
 *     literal without the required `.address` (and `.tokenId` for NFTs).
 *     The runtime engine fail-closes on such inputs; catching at build time
 *     keeps a hand-edited bundle from quietly slipping a malformed AssetRef.
 *
 * Spec references:
 *   - registry/docs/SCHEMA_V2.md
 *   - registry/docs/ERC_STANDARD_AUTO_ENUMERATE.md
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
  chain_to_addresses_source: TokenKindSource;
  chain_ids: ChainId[];
}

type BundleMatch = BundleMatchSpecific | BundleMatchSourced;

type TokenKind = "erc20" | "erc721" | "erc1155";
type TokenKindSource = `tokens:${TokenKind}`;

interface AdapterBundle {
  type: "adapter_function";
  id: string;
  schema_version: "2";
  publisher?: string;
  match: BundleMatch;
  [key: string]: unknown;
}

interface ResolvedBundle {
  type: "adapter_function";
  id: string;
  schema_version: "2";
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
  kind: TokenKind;
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
const SCHEMA_VERSION_REQUIRED = "2" as const;
const TOKEN_KINDS: ReadonlySet<TokenKind> = new Set(["erc20", "erc721", "erc1155"]);

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
// Token registry — load + index by (chainId, kind)
// ---------------------------------------------------------------------------

/** Map<chainId, Map<kind, Set<lowercased address>>> */
type TokensByChainKind = Map<ChainId, Map<TokenKind, Set<Hex>>>;

function loadTokensIndex(): TokensByChainKind {
  const out: TokensByChainKind = new Map();
  if (!safeExists(TOKENS_DIR)) return out;

  // tokens/<chainId>/*.json
  for (const chainDir of readdirSync(TOKENS_DIR)) {
    const chainPath = join(TOKENS_DIR, chainDir);
    if (!statSync(chainPath).isDirectory()) continue;
    const chainId = Number(chainDir);
    if (!Number.isInteger(chainId) || chainId < 1) {
      throw new Error(`tokens/: invalid chain directory name "${chainDir}" — expected positive integer`);
    }

    const perKind = new Map<TokenKind, Set<Hex>>();
    perKind.set("erc20", new Set());
    perKind.set("erc721", new Set());
    perKind.set("erc1155", new Set());

    for (const fname of readdirSync(chainPath)) {
      if (!fname.endsWith(".json")) continue;
      const fpath = join(chainPath, fname);
      const meta = loadTokenMetadata(fpath, chainId);
      perKind.get(meta.kind)!.add(meta.address.toLowerCase());
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

  const kind = obj.kind;
  if (typeof kind !== "string" || !TOKEN_KINDS.has(kind as TokenKind)) {
    throw new Error(
      `tokens/: ${path} has missing or invalid "kind" — expected one of erc20/erc721/erc1155, got ${JSON.stringify(kind)}`,
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
    throw new Error(`tokens/: ${path} has missing or invalid "chainId" — expected positive integer, got ${JSON.stringify(chainId)}`);
  }
  if (chainId !== expectedChainId) {
    throw new Error(`tokens/: ${path} chainId field (${chainId}) does not match directory (${expectedChainId})`);
  }

  return { kind: kind as TokenKind, chainId, address, ...obj };
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

  if (obj.type !== "adapter_function") {
    throw new Error(`manifests/: ${path} type !== "adapter_function" (got ${JSON.stringify(obj.type)})`);
  }
  if (typeof obj.id !== "string" || obj.id.length === 0) {
    throw new Error(`manifests/: ${path} has missing or invalid "id"`);
  }
  if (obj.schema_version !== SCHEMA_VERSION_REQUIRED) {
    throw new Error(
      `manifests/: ${path} schema_version !== "${SCHEMA_VERSION_REQUIRED}" (got ${JSON.stringify(obj.schema_version)}). ` +
        `registry v2 rejects pre-v2 bundles — migrate to chain_to_addresses map first.`,
    );
  }

  validateMatchShape(path, obj.match);

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
    if (typeof m.chain_to_addresses !== "object" || m.chain_to_addresses === null || Array.isArray(m.chain_to_addresses)) {
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
      throw new Error(
        `manifests/: ${path} match.chain_to_addresses_source must be a string`,
      );
    }
    const parts = m.chain_to_addresses_source.split(":");
    if (parts.length !== 2 || parts[0] !== "tokens" || !TOKEN_KINDS.has(parts[1] as TokenKind)) {
      throw new Error(
        `manifests/: ${path} match.chain_to_addresses_source must be one of "tokens:erc20" | "tokens:erc721" | "tokens:erc1155", got ${JSON.stringify(m.chain_to_addresses_source)}`,
      );
    }
    if (!Array.isArray(m.chain_ids) || m.chain_ids.length === 0) {
      throw new Error(`manifests/: ${path} match.chain_ids must be a non-empty array when chain_to_addresses_source is set`);
    }
    for (const [i, cid] of m.chain_ids.entries()) {
      if (typeof cid !== "number" || !Number.isInteger(cid) || cid < 1) {
        throw new Error(`manifests/: ${path} match.chain_ids[${i}] must be a positive integer, got ${JSON.stringify(cid)}`);
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Source resolution — sourced bundles → effective chain_to_addresses
// ---------------------------------------------------------------------------

function resolveBundle(bundle: AdapterBundle, tokens: TokensByChainKind, manifestPath: string): ResolvedBundle {
  if ("chain_to_addresses" in bundle.match) {
    // already concrete
    return bundle as ResolvedBundle;
  }

  const sourced = bundle.match as BundleMatchSourced;
  const kind = sourced.chain_to_addresses_source.split(":")[1] as TokenKind;

  const effective: Record<string, Hex[]> = {};
  let totalAddresses = 0;
  for (const chainId of sourced.chain_ids) {
    const perKind = tokens.get(chainId);
    if (!perKind) {
      throw new Error(
        `manifests/: ${manifestPath} match.chain_to_addresses_source references chain ${chainId} but tokens/${chainId}/ does not exist`,
      );
    }
    const addresses = Array.from(perKind.get(kind)!).sort();
    if (addresses.length === 0) {
      // not an error — a chain with no tokens of this kind simply produces zero callkeys for this chain.
      console.error(
        `[build-index] WARN ${manifestPath}: chain ${chainId} has no tokens of kind=${kind} — 0 callkeys for this (chain, selector)`,
      );
      continue;
    }
    effective[String(chainId)] = addresses;
    totalAddresses += addresses.length;
  }

  if (totalAddresses === 0) {
    throw new Error(
      `manifests/: ${manifestPath} match.chain_to_addresses_source resolved to 0 addresses across all chain_ids — at least one token of kind=${kind} required`,
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
// AssetRef lint (post-Aerodrome audit, ported)
// ---------------------------------------------------------------------------

const ERC_KINDS: ReadonlySet<string> = new Set(["erc20", "erc721", "erc1155"]);
const NFT_KINDS: ReadonlySet<string> = new Set(["erc721", "erc1155"]);

function collectFieldsMaps(node: unknown, out: Record<string, unknown>[]): void {
  if (Array.isArray(node)) {
    for (const child of node) collectFieldsMaps(child, out);
    return;
  }
  if (node === null || typeof node !== "object") return;
  for (const [key, value] of Object.entries(node as Record<string, unknown>)) {
    if (key === "fields" && value !== null && typeof value === "object" && !Array.isArray(value)) {
      out.push(value as Record<string, unknown>);
    }
    collectFieldsMaps(value, out);
  }
}

function validateAssetRefs(manifestPath: string, bundle: AdapterBundle): void {
  const fieldsMaps: Record<string, unknown>[] = [];
  collectFieldsMaps(bundle, fieldsMaps);

  const violations: string[] = [];
  for (const fm of fieldsMaps) {
    for (const key of Object.keys(fm)) {
      if (!key.endsWith(".asset.kind") && !key.endsWith("token.kind") && !key.endsWith("token0.kind") && !key.endsWith("token1.kind")) {
        continue;
      }
      const prefix = key.slice(0, -".kind".length);
      const kindNode = fm[key];
      const literal =
        kindNode !== null && typeof kindNode === "object" && !Array.isArray(kindNode)
          ? (kindNode as Record<string, unknown>).literal
          : undefined;
      if (typeof literal !== "string" || !ERC_KINDS.has(literal)) continue;
      if (!(`${prefix}.address` in fm)) {
        violations.push(`${prefix}: kind="${literal}" but no "${prefix}.address" binding`);
      }
      if (NFT_KINDS.has(literal) && !(`${prefix}.tokenId` in fm)) {
        violations.push(`${prefix}: kind="${literal}" but no "${prefix}.tokenId" binding`);
      }
    }
  }
  if (violations.length === 0) return;

  const detail = violations.map((v) => `                - ${v}`).join("\n");
  throw new Error(
    `manifests/: ${manifestPath} emits an erc-kind AssetRef without a required ` +
      `address/tokenId — the evaluate stage cannot deserialize it:\n${detail}`,
  );
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
    console.error(`[build-index] no manifests found in ${MANIFESTS_DIR}`);
    process.exit(1);
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
      validateAssetRefs(manifestPath, bundle);
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
