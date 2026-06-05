/**
 * check-tokens.ts
 *
 * Token-registry hygiene gate. `build-index.ts`'s token loader validates only
 * the structural minimum it needs to expand `tokens:erc*` sources (JSON object,
 * erc_kind, address format, native sentinel, positive chainId). The
 * TOKEN_INVENTORY_GUIDE "Referential Rule" + field requirements were left
 * "reviewer-enforced until a dedicated check:tokens gate exists" — this is that
 * gate.
 *
 * Two severities (mirrors the framework's opt-in-strict migration model):
 *   ERROR  — structural invariants (filename<->address, chainId field==dir,
 *            erc_kind/token_kind.kind validity). Always fatal.
 *   WARN   — completeness + referential integrity (source/symbol/decimals
 *            presence, underlying/peg_to refs resolve). Reported by default;
 *            promoted to ERROR with --strict.
 *
 * Usage:
 *   npm run check:tokens
 *   npm run check:tokens -- --chain 1
 *   npm run check:tokens -- --strict
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REGISTRY_ROOT = process.env.BUILD_INDEX_REGISTRY_ROOT
  ? resolve(process.env.BUILD_INDEX_REGISTRY_ROOT)
  : resolve(__dirname, "..");
const TOKENS_DIR = join(REGISTRY_ROOT, "tokens");

const ADDR_RE = /^0x[0-9a-f]{40}$/;
const ERC_KINDS = new Set(["erc20", "erc721", "erc1155", "native"]);
const NATIVE_SENTINEL = "0x0000000000000000000000000000000000000000";
// Authoritative top-level `token_kind.kind` set = the 10 serde-snake_case
// variants of the Rust `TokenKind` enum
// (crates/policy-server/asset-model/state/src/token/kind.rs). NOT the
// TOKEN_INVENTORY_GUIDE prose list — `governance` is a *nested* `BaseCategory`
// variant (token_kind.category.kind under `base`, cf. COMP/UNI), never a
// top-level kind; including it here would mask malformed tokens that put
// `kind:"governance"` at the top level (the Rust deserializer rejects those).
const TOKEN_KINDS = new Set([
  "base",
  "native_gas",
  "wrapped",
  "lp_share",
  "yield_receipt",
  "debt_receipt",
  "stake_receipt",
  "points_token",
  "maturity_note",
  "unknown",
]);
const MAX_LIST = 30;

interface Config {
  chain?: number;
  strict: boolean;
}

function safeExists(p: string): boolean {
  try {
    statSync(p);
    return true;
  } catch {
    return false;
  }
}

function parseArgs(argv: string[]): Config {
  const config: Config = { strict: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--chain") {
      const value = Number(argv[++i]);
      if (!Number.isInteger(value) || value < 1) throw new Error("--chain requires a positive integer");
      config.chain = value;
    } else if (arg === "--strict") {
      config.strict = true;
    } else if (arg === "-h" || arg === "--help") {
      console.log(
        "check-tokens\n\nUsage:\n  npm run check:tokens\n  npm run check:tokens -- --chain 1\n  npm run check:tokens -- --strict\n\n" +
          "ERROR (always fatal): filename<->address, chainId field==dir, erc_kind/token_kind.kind validity.\n" +
          "WARN (fatal with --strict): source/symbol/decimals presence, underlying/peg_to referential integrity.",
      );
      process.exit(0);
    } else {
      throw new Error(`unknown argument ${arg}`);
    }
  }
  return config;
}

/** chainId -> set of lowercased addresses present (any erc_kind). */
function loadTokenIndex(): Map<number, Set<string>> {
  const index = new Map<number, Set<string>>();
  if (!safeExists(TOKENS_DIR)) return index;
  for (const chainDir of readdirSync(TOKENS_DIR)) {
    const chainPath = join(TOKENS_DIR, chainDir);
    if (!statSync(chainPath).isDirectory()) continue;
    const chainId = Number(chainDir);
    if (!Number.isInteger(chainId) || chainId < 1) continue;
    const set = new Set<string>();
    for (const fname of readdirSync(chainPath)) {
      if (!fname.endsWith(".json")) continue;
      try {
        const obj = JSON.parse(readFileSync(join(chainPath, fname), "utf8")) as { address?: unknown };
        if (typeof obj.address === "string") set.add(obj.address.toLowerCase());
      } catch {
        /* malformed JSON is reported per-file in checkChain */
      }
    }
    index.set(chainId, set);
  }
  return index;
}

/** "eip155:8453" -> 8453; else undefined. */
function chainOf(caip: unknown): number | undefined {
  if (typeof caip !== "string") return undefined;
  const m = /^eip155:(\d+)$/.exec(caip);
  return m ? Number(m[1]) : undefined;
}

/** Recursively collect every {chain, address} TokenKey ref under token_kind. */
function collectRefs(node: unknown, out: Array<{ chain: unknown; address: string }>): void {
  if (Array.isArray(node)) {
    for (const item of node) collectRefs(item, out);
    return;
  }
  if (node && typeof node === "object") {
    const obj = node as Record<string, unknown>;
    if (typeof obj.address === "string" && typeof obj.chain === "string" && /^0x[0-9a-fA-F]{40}$/.test(obj.address)) {
      out.push({ chain: obj.chain, address: obj.address });
    }
    for (const value of Object.values(obj)) collectRefs(value, out);
  }
}

interface Tally {
  errors: string[];
  warns: string[];
  files: number;
}

function checkChain(chainId: number, index: Map<number, Set<string>>, tally: Tally): void {
  const chainPath = join(TOKENS_DIR, String(chainId));
  for (const fname of readdirSync(chainPath).filter((f) => f.endsWith(".json")).sort()) {
    tally.files++;
    const rel = `tokens/${chainId}/${fname}`;
    let obj: Record<string, unknown>;
    try {
      const parsed = JSON.parse(readFileSync(join(chainPath, fname), "utf8"));
      if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
        tally.errors.push(`${rel}: not a JSON object`);
        continue;
      }
      obj = parsed as Record<string, unknown>;
    } catch (e) {
      tally.errors.push(`${rel}: invalid JSON (${(e as Error).message})`);
      continue;
    }

    const address = typeof obj.address === "string" ? obj.address : "";
    const ercKind = typeof obj.erc_kind === "string" ? obj.erc_kind : "";

    // --- ERROR: structural invariants ---
    if (!ADDR_RE.test(address)) {
      tally.errors.push(`${rel}: address must be lowercase 0x+40hex, got ${JSON.stringify(obj.address)}`);
    } else if (fname !== `${address}.json`) {
      tally.errors.push(`${rel}: filename must equal "${address}.json" (lowercased address)`);
    }
    if (!ERC_KINDS.has(ercKind)) {
      tally.errors.push(`${rel}: erc_kind must be erc20|erc721|erc1155|native, got ${JSON.stringify(obj.erc_kind)}`);
    }
    if (obj.chainId !== chainId) {
      tally.errors.push(`${rel}: chainId field ${JSON.stringify(obj.chainId)} != directory ${chainId}`);
    }
    const tokenKind = obj.token_kind;
    if (tokenKind !== undefined) {
      const kind = (tokenKind as { kind?: unknown })?.kind;
      if (typeof kind !== "string" || !TOKEN_KINDS.has(kind)) {
        tally.errors.push(`${rel}: token_kind.kind must be one of ${[...TOKEN_KINDS].join("|")}, got ${JSON.stringify(kind)}`);
      }
    }

    // --- WARN: completeness ---
    if (typeof obj.source !== "string" || !obj.source.trim()) {
      tally.warns.push(`${rel}: missing "source" provenance`);
    }
    if (ercKind === "erc20" || ercKind === "native") {
      if (typeof obj.symbol !== "string" || !obj.symbol.trim()) tally.warns.push(`${rel}: missing "symbol"`);
      if (typeof obj.decimals !== "number" || !Number.isInteger(obj.decimals)) tally.warns.push(`${rel}: missing/invalid "decimals"`);
    }
    // token_kind itself is optional semantic metadata (plain allowlist tokens
    // omit it); only its referential integrity is checked, below.

    // --- WARN: referential integrity (underlying / peg_to token refs) ---
    if (tokenKind !== undefined) {
      const refs: Array<{ chain: unknown; address: string }> = [];
      collectRefs(tokenKind, refs);
      for (const ref of refs) {
        const refAddr = ref.address.toLowerCase();
        if (refAddr === NATIVE_SENTINEL) continue; // native sentinel is intrinsic
        const refChain = chainOf(ref.chain);
        if (refChain === undefined) {
          tally.warns.push(`${rel}: token_kind ref has non-eip155 chain ${JSON.stringify(ref.chain)}`);
          continue;
        }
        if (!index.get(refChain)?.has(refAddr)) {
          tally.warns.push(`${rel}: token_kind references tokens/${refChain}/${refAddr}.json which is not registered`);
        }
      }
    }
  }
}

function main(): void {
  const config = parseArgs(process.argv.slice(2));
  console.log("token-registry gate");
  console.log(`  registry root: ${REGISTRY_ROOT}`);
  console.log(`  mode: ${config.strict ? "strict (WARN promoted to ERROR)" : "default (WARN reported)"}`);

  if (!safeExists(TOKENS_DIR)) {
    console.log("  no tokens/ directory; nothing to check.");
    return;
  }

  const index = loadTokenIndex();
  const chains = config.chain
    ? [config.chain]
    : [...index.keys()].sort((a, b) => a - b);

  const tally: Tally = { errors: [], warns: [], files: 0 };
  for (const chainId of chains) {
    if (!safeExists(join(TOKENS_DIR, String(chainId)))) {
      tally.errors.push(`tokens/${chainId}/ does not exist`);
      continue;
    }
    checkChain(chainId, index, tally);
  }

  console.log(`  scanned ${tally.files} token file(s) across chain(s) ${chains.join(", ")}`);

  const printList = (label: string, items: string[]) => {
    console.error(`\n${label} (${items.length}):`);
    for (const item of items.slice(0, MAX_LIST)) console.error(`  x ${item}`);
    if (items.length > MAX_LIST) console.error(`  … and ${items.length - MAX_LIST} more`);
  };

  if (tally.warns.length > 0) printList(config.strict ? "WARN (strict -> fatal)" : "WARN", tally.warns);
  if (tally.errors.length > 0) printList("ERROR", tally.errors);

  const fatal = tally.errors.length + (config.strict ? tally.warns.length : 0);
  if (fatal > 0) {
    console.error(`\nFAIL - ${tally.errors.length} error(s)` + (config.strict ? `, ${tally.warns.length} warn(s)` : "") + ".");
    process.exit(1);
  }
  console.log(
    `\nPASS - 0 errors` +
      (tally.warns.length > 0 ? ` (${tally.warns.length} warn(s) reported; run with --strict to enforce)` : "") +
      ".",
  );
}

main();
