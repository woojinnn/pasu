/**
 * build-index.ts
 *
 * ScopeBall Adapter Marketplace registry — index builder (PoC).
 *
 * Algorithm:
 *   1. `manifests/` recursive scan → Adapter Function Bundle JSON 파일 list
 *   2. 각 bundle 의 `match.{chain_ids[], to[], selector}` cross product 로 callkey 조합
 *   3. canonical_json(bundle) — RFC 8785 JSON Canonicalization Scheme (JCS)
 *   4. bundle_sha256 = "0x" + hex(sha256(canonical_json(bundle)))
 *   5. 각 callkey 에 대해 `index/by-callkey/<chain_id>__<lowercased_to>__<lowercased_selector>.json`
 *      파일 작성. 파일 내용에 `bundle` field 를 inline 시켜 client 가 1-step lookup 가능.
 *   6. 기존 `index/by-callkey/` 디렉토리 wipe 후 재기록 (orphan 방지).
 *
 * Spec reference:
 *   - /Users/jhy/Desktop/ScopeBall/ADAPTER_MARKETPLACE_ARCHITECTURE.md §6, §6.1, §6.2
 *   - RFC 8785 (JSON Canonicalization Scheme) — https://www.rfc-editor.org/rfc/rfc8785
 *
 * 실행:
 *   $ npm install      # tsx + canonicalize + @types/node 설치
 *   $ npm run build    # 본 스크립트 실행 → index 재생성
 */

import { createHash } from "node:crypto";
import { mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import canonicalize from "canonicalize";

// ---------------------------------------------------------------------------
// Types — Adapter Function Bundle 의 PoC 단계 minimum field set
// ---------------------------------------------------------------------------

interface BundleMatch {
  chain_ids: number[];
  to: string[];
  selector: string;
}

interface AdapterBundle {
  type: string;
  id: string;
  match: BundleMatch;
  // emit / abi_fragment / requires 등 다른 field 는 본 스크립트 입장에서 opaque.
  [key: string]: unknown;
}

interface IndexEntry {
  matched: true;
  bundle_id: string;
  manifest_path: string;
  bundle_sha256: string;
  bundle: AdapterBundle;
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REGISTRY_ROOT = resolve(__dirname, "..");
const MANIFESTS_DIR = join(REGISTRY_ROOT, "manifests");
const INDEX_BY_CALLKEY_DIR = join(REGISTRY_ROOT, "index", "by-callkey");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function walkJsonFiles(root: string): string[] {
  const out: string[] = [];
  if (!safeExists(root)) return out;
  const stack: string[] = [root];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    for (const entry of readdirSync(cur)) {
      const p = join(cur, entry);
      const s = statSync(p);
      if (s.isDirectory()) {
        // Skip `_template/` dirs — these hold pool-type emit-rule templates
        // with placeholder addresses (0x1111…/0xaaaa…) consumed by
        // `gen-curve-pools.ts` per-pool generation. They are NOT live bundles
        // and must never be installed or indexed (placeholder addresses would
        // create dead callkeys and tripping audit-addresses bogus gate).
        if (entry === "_template") continue;
        stack.push(p);
      } else if (s.isFile() && entry.endsWith(".json")) {
        out.push(p);
      }
    }
  }
  return out.sort();
}

function safeExists(p: string): boolean {
  try {
    statSync(p);
    return true;
  } catch {
    return false;
  }
}

function sha256Hex(s: string): string {
  return createHash("sha256").update(s, "utf8").digest("hex");
}

// Round 3 audit (P1) — tighten registry-side validation so the index
// builder rejects malformed bundles before they get a callkey entry.
// The runtime `parseBundle` enforces the same shape, but catching this
// at build time keeps a hand-edited bundle from quietly slipping a
// "0x" + N-where-N != 8 selector or a non-EVM address into the index.
const SELECTOR_RE = /^0x[0-9a-fA-F]{8}$/;
const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

function loadBundle(path: string): AdapterBundle {
  const raw = readFileSync(path, "utf8");
  const json = JSON.parse(raw) as AdapterBundle;
  if (typeof json !== "object" || json === null) {
    throw new Error(`bundle not an object: ${path}`);
  }
  if (json.type !== "adapter_function") {
    throw new Error(`bundle.type !== "adapter_function": ${path}`);
  }
  if (!json.match || !Array.isArray(json.match.chain_ids) || !Array.isArray(json.match.to)) {
    throw new Error(`bundle.match shape invalid: ${path}`);
  }
  for (const [i, cid] of json.match.chain_ids.entries()) {
    if (typeof cid !== "number" || !Number.isInteger(cid) || cid < 1) {
      throw new Error(
        `bundle.match.chain_ids[${i}] must be positive integer: ${path}`,
      );
    }
  }
  for (const [i, to] of json.match.to.entries()) {
    if (typeof to !== "string" || !ADDRESS_RE.test(to)) {
      throw new Error(
        `bundle.match.to[${i}] expected EVM address "0x" + 40 hex, got "${to}": ${path}`,
      );
    }
  }
  if (typeof json.match.selector !== "string" || !SELECTOR_RE.test(json.match.selector)) {
    throw new Error(
      `bundle.match.selector expected "0x" + 8 hex, got "${json.match.selector}": ${path}`,
    );
  }
  return json;
}

function computeBundleSha256(bundle: AdapterBundle): string {
  // RFC 8785 JCS — `canonicalize` package returns canonical JSON string (UTF-8).
  const canonical = canonicalize(bundle);
  if (typeof canonical !== "string") {
    throw new Error("canonicalize returned non-string");
  }
  return "0x" + sha256Hex(canonical);
}

function callkeyFilename(chainId: number, to: string, selector: string): string {
  // 파일명: lowercased to + lowercased selector. spec §6.2 의 예시는 case 유지지만
  // PoC client lookup 의 case-sensitivity 문제를 회피하기 위해 lowercased 결정.
  return `${chainId}__${to.toLowerCase()}__${selector.toLowerCase()}.json`;
}

function wipeDir(p: string): void {
  if (safeExists(p)) {
    rmSync(p, { recursive: true, force: true });
  }
  mkdirSync(p, { recursive: true });
}

// ---------------------------------------------------------------------------
// Asset-address lint — post-Aerodrome audit.
//
// `AssetRef`'s deserialize validation (policy-engine) requires an `address`
// for every `erc20`/`erc721`/`erc1155` asset (and a `tokenId` for NFTs). A
// bundle whose emit rule produces such an asset without those fields cannot
// be deserialized at the evaluate stage — the engine fail-closes with an
// opaque `__engine::invalid_input_json` (a false `fail`). Reject such bundles
// here, at index-build time, before they ever reach a client.
//
// `KNOWN_DEFERRED_ASSET_ADDRESS` lists manifests with a pre-existing gap
// scheduled for a follow-up fix. They warn instead of throwing so the
// Aerodrome remediation can ship without blocking on Uniswap/Curve. This is a
// ratchet: a NEW violation (manifest not on the list) still fails the build,
// and each entry is deleted as its bundle is fixed.
// ---------------------------------------------------------------------------

const ERC_KINDS: ReadonlySet<string> = new Set(["erc20", "erc721", "erc1155"]);
const NFT_KINDS: ReadonlySet<string> = new Set(["erc721", "erc1155"]);

const KNOWN_DEFERRED_ASSET_ADDRESS: ReadonlySet<string> = new Set([
  "manifests/curve/stableswap/frxeth/addLiquidity-2@1.0.0.json",
  "manifests/curve/stableswap/frxeth/removeLiquidity-2@1.0.0.json",
  "manifests/curve/stableswap/frxeth/removeLiquidityImbalance-2@1.0.0.json",
  "manifests/curve/stableswap/frxeth/removeLiquidityOneCoin@1.0.0.json",
  "manifests/uniswap/swap-router-02/unwrapWETH9@1.0.0.json",
  "manifests/uniswap/swap-router-02/wrapETH@1.0.0.json",
  "manifests/uniswap/v2/addLiquidity@1.0.0.json",
  "manifests/uniswap/v2/addLiquidityETH@1.0.0.json",
  "manifests/uniswap/v2/removeLiquidity@1.0.0.json",
  "manifests/uniswap/v2/removeLiquidityETH@1.0.0.json",
  "manifests/uniswap/v2/removeLiquidityETHSupportingFeeOnTransferTokens@1.0.0.json",
  "manifests/uniswap/v2/removeLiquidityETHWithPermit@1.0.0.json",
  "manifests/uniswap/v2/removeLiquidityETHWithPermitSupportingFeeOnTransferTokens@1.0.0.json",
  "manifests/uniswap/v2/removeLiquidityWithPermit@1.0.0.json",
  "manifests/uniswap/v3/decreaseLiquidity@1.0.0.json",
  "manifests/uniswap/v3/increaseLiquidity@1.0.0.json",
  "manifests/uniswap/v3/unwrapWETH9@1.0.0.json",
]);

/** Collect every object that is the value of a key literally named `fields`. */
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

/**
 * Reject a bundle whose emit rule binds an `erc20`/`erc721`/`erc1155` asset
 * `kind` without the required `address` (or `tokenId` for NFTs). Deferred
 * manifests warn; everything else throws and fails the build.
 */
function validateAssetRefs(manifestPath: string, bundle: AdapterBundle): void {
  const fieldsMaps: Record<string, unknown>[] = [];
  collectFieldsMaps(bundle, fieldsMaps);

  const violations: string[] = [];
  for (const fm of fieldsMaps) {
    for (const key of Object.keys(fm)) {
      if (!key.endsWith(".asset.kind")) continue;
      const prefix = key.slice(0, -".kind".length); // "<path>.asset"
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
  if (KNOWN_DEFERRED_ASSET_ADDRESS.has(manifestPath)) {
    console.error(`[build-index] WARN ${manifestPath}: deferred asset-address gap\n${detail}`);
    return;
  }
  throw new Error(
    `bundle ${manifestPath} emits an erc-kind AssetRef without a required ` +
      `address/tokenId — the evaluate stage cannot deserialize it ` +
      `(add to KNOWN_DEFERRED_ASSET_ADDRESS only with a tracked follow-up):\n${detail}`,
  );
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

function main(): void {
  const files = walkJsonFiles(MANIFESTS_DIR);
  if (files.length === 0) {
    console.error(`[build-index] no manifests found in ${MANIFESTS_DIR}`);
    process.exit(1);
  }

  console.error(`[build-index] manifests root: ${MANIFESTS_DIR}`);
  console.error(`[build-index] index root:     ${INDEX_BY_CALLKEY_DIR}`);
  console.error(`[build-index] manifests:      ${files.length}`);

  // orphan 방지 — 기존 index/by-callkey/ wipe
  wipeDir(INDEX_BY_CALLKEY_DIR);

  let indexCount = 0;
  for (const file of files) {
    const bundle = loadBundle(file);
    const manifestPath = relative(REGISTRY_ROOT, file).split(/[\\/]/).join("/");
    validateAssetRefs(manifestPath, bundle);
    const bundleSha256 = computeBundleSha256(bundle);
    const callkeyCount = bundle.match.chain_ids.length * bundle.match.to.length;

    console.error(
      `[build-index] ${bundle.id}\n` +
        `              manifest:  ${manifestPath}\n` +
        `              sha256:    ${bundleSha256}\n` +
        `              callkeys:  ${callkeyCount}`,
    );

    for (const chainId of bundle.match.chain_ids) {
      for (const to of bundle.match.to) {
        const entry: IndexEntry = {
          matched: true,
          bundle_id: bundle.id,
          manifest_path: manifestPath,
          bundle_sha256: bundleSha256,
          bundle,
        };
        const fname = callkeyFilename(chainId, to, bundle.match.selector);
        const outPath = join(INDEX_BY_CALLKEY_DIR, fname);
        // index 파일도 pretty-print JSON 으로 commit (PoC 디버깅 편의)
        writeFileSync(outPath, JSON.stringify(entry, null, 2) + "\n", "utf8");
        indexCount++;
      }
    }
  }

  console.error(`[build-index] done — ${indexCount} index file(s) written`);
}

main();
