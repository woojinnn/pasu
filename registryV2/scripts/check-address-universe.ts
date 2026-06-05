/**
 * check-address-universe.ts
 *
 * Protocol-agnostic gate for pool/vault/factory child address universes.
 *
 * `check:surface` proves function completeness for contracts we already found.
 * This gate proves the higher-level address set is not a no-op: every
 * `surface/<protocol>/_*_universe.json` artifact must be non-empty, every
 * candidate must be dispositioned, and (optionally, for P4) every covered
 * address must have generated callkeys.
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REGISTRY_ROOT = process.env.BUILD_INDEX_REGISTRY_ROOT
  ? resolve(process.env.BUILD_INDEX_REGISTRY_ROOT)
  : resolve(__dirname, "..");
const SURFACE_DIR = join(REGISTRY_ROOT, "surface");
const CALLKEY_DIR = join(REGISTRY_ROOT, "index", "by-callkey");

const ADDR_RE = /^0x[0-9a-f]{40}$/;
const DECISIONS = new Set(["cover", "exclude", "defer"]);
const UNIVERSE_FILE_RE = /^_.*_universe\.json$/;

interface Config {
  protocol?: string;
  requireCoverLinkage: boolean;
}

interface SourceRecord {
  name?: unknown;
  url?: unknown;
  query?: unknown;
  count?: unknown;
}

interface CandidateRecord {
  chainId?: unknown;
  address?: unknown;
  decision?: unknown;
  disposition?: unknown;
  reason?: unknown;
  batch?: unknown;
  defer_batch?: unknown;
  boundary?: unknown;
}

interface UniverseRecord {
  protocol?: unknown;
  kind?: unknown;
  source?: unknown;
  sources?: unknown;
  expected_count?: unknown;
  expectedCount?: unknown;
  source_count?: unknown;
  sourceCount?: unknown;
  batch?: unknown;
  batch_boundary?: unknown;
  batchBoundary?: unknown;
  candidates?: unknown;
  addresses?: unknown;
  pools?: unknown;
}

interface Artifact {
  path: string;
  relPath: string;
  protocol: string;
  data: UniverseRecord;
}

function safeExists(path: string): boolean {
  try {
    statSync(path);
    return true;
  } catch {
    return false;
  }
}

function parseArgs(argv: string[]): Config {
  const config: Config = { requireCoverLinkage: false };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--protocol") {
      const value = argv[++i];
      if (!value) throw new Error("--protocol requires a value");
      config.protocol = value;
    } else if (arg === "--require-cover-linkage") {
      config.requireCoverLinkage = true;
    } else if (arg === "-h" || arg === "--help") {
      usage(0);
    } else {
      throw new Error(`unknown argument ${arg}`);
    }
  }
  return config;
}

function usage(code: number): never {
  const text = `check-address-universe

Usage:
  npm run check:universe
  npm run check:universe -- --protocol curve
  npm run check:universe -- --protocol curve --require-cover-linkage

Artifacts:
  registryV2/surface/<protocol>/_*_universe.json

Candidate rows must include chainId, lowercase address, decision/disposition
(cover|exclude|defer), and reason. Defer rows also need a batch boundary.`;
  console.log(text);
  process.exit(code);
}

function loadArtifacts(config: Config): Artifact[] {
  const artifacts: Artifact[] = [];
  if (!safeExists(SURFACE_DIR)) return artifacts;

  const protocols = config.protocol
    ? [config.protocol]
    : readdirSync(SURFACE_DIR).filter((entry) => {
        const path = join(SURFACE_DIR, entry);
        return statSync(path).isDirectory();
      });

  for (const protocol of protocols) {
    const dir = join(SURFACE_DIR, protocol);
    if (!safeExists(dir)) continue;
    for (const file of readdirSync(dir).filter((entry) => UNIVERSE_FILE_RE.test(entry)).sort()) {
      const path = join(dir, file);
      const data = JSON.parse(readFileSync(path, "utf8")) as UniverseRecord;
      artifacts.push({
        path,
        relPath: relative(REGISTRY_ROOT, path),
        protocol,
        data,
      });
    }
  }
  return artifacts;
}

function candidatesOf(data: UniverseRecord): CandidateRecord[] {
  const raw = data.candidates ?? data.addresses ?? data.pools;
  return Array.isArray(raw) ? (raw as CandidateRecord[]) : [];
}

function sourceCountOf(data: UniverseRecord): number | undefined {
  for (const value of [data.source_count, data.sourceCount, data.expected_count, data.expectedCount]) {
    if (typeof value === "number" && Number.isFinite(value)) return value;
  }
  if (Array.isArray(data.sources)) {
    let total = 0;
    let sawCount = false;
    for (const src of data.sources as SourceRecord[]) {
      if (typeof src.count === "number" && Number.isFinite(src.count)) {
        total += src.count;
        sawCount = true;
      }
    }
    if (sawCount) return total;
  }
  return undefined;
}

function hasGlobalBatchBoundary(data: UniverseRecord): boolean {
  if (typeof data.batch_boundary === "string" && data.batch_boundary.trim()) return true;
  if (typeof data.batchBoundary === "string" && data.batchBoundary.trim()) return true;
  if (data.batch && typeof data.batch === "object") return true;
  return false;
}

function hasCandidateBatchBoundary(candidate: CandidateRecord): boolean {
  for (const value of [candidate.batch, candidate.defer_batch, candidate.boundary]) {
    if (typeof value === "string" && value.trim()) return true;
    if (value && typeof value === "object") return true;
  }
  return false;
}

function loadCallkeyPrefixes(): Set<string> {
  const prefixes = new Set<string>();
  if (!safeExists(CALLKEY_DIR)) return prefixes;
  for (const file of readdirSync(CALLKEY_DIR)) {
    if (!file.endsWith(".json")) continue;
    const parts = file.replace(/\.json$/, "").split("__");
    if (parts.length !== 3) continue;
    prefixes.add(`${parts[0]}__${parts[1]}`.toLowerCase());
  }
  return prefixes;
}

function checkArtifact(artifact: Artifact, callkeyPrefixes: Set<string>, config: Config): string[] {
  const failures: string[] = [];
  const { data, protocol, relPath } = artifact;
  const candidates = candidatesOf(data);
  const sourceCount = sourceCountOf(data);

  if (typeof data.protocol === "string" && data.protocol !== protocol) {
    failures.push(`${relPath}: protocol field ${JSON.stringify(data.protocol)} != directory ${protocol}`);
  }
  if (!data.source && !Array.isArray(data.sources)) {
    failures.push(`${relPath}: missing source or sources provenance`);
  }
  if (sourceCount !== undefined && sourceCount <= 0) {
    failures.push(`${relPath}: source count must be > 0, got ${sourceCount}`);
  }
  if (candidates.length === 0) {
    failures.push(`${relPath}: candidates/addresses/pools array is empty or missing`);
  }

  const seen = new Set<string>();
  let cover = 0;
  let exclude = 0;
  let defer = 0;

  for (const [idx, candidate] of candidates.entries()) {
    const row = `${relPath}: candidate[${idx}]`;
    const chainId = candidate.chainId;
    const address = String(candidate.address ?? "");
    const decision = String(candidate.decision ?? candidate.disposition ?? "");
    const reason = String(candidate.reason ?? "").trim();

    if (!Number.isInteger(chainId)) {
      failures.push(`${row}: chainId must be an integer`);
      continue;
    }
    if (!ADDR_RE.test(address)) {
      failures.push(`${row}: address must be lowercase 0x+40hex, got ${JSON.stringify(address)}`);
      continue;
    }
    const key = `${chainId}__${address}`;
    if (seen.has(key)) {
      failures.push(`${row}: duplicate address key ${key}`);
    }
    seen.add(key);

    if (!DECISIONS.has(decision)) {
      failures.push(`${row}: decision/disposition must be cover|exclude|defer`);
      continue;
    }
    if (!reason) {
      failures.push(`${row}: ${decision} requires a reason`);
    }
    if (decision === "cover") {
      cover++;
      if (config.requireCoverLinkage && !callkeyPrefixes.has(key)) {
        failures.push(`${row}: cover address ${key} has no generated by-callkey entry`);
      }
    } else if (decision === "exclude") {
      exclude++;
    } else {
      defer++;
      if (!hasGlobalBatchBoundary(data) && !hasCandidateBatchBoundary(candidate)) {
        failures.push(`${row}: defer requires batch/defer_batch/boundary or a top-level batch boundary`);
      }
    }
  }

  if (sourceCount !== undefined && sourceCount < candidates.length) {
    failures.push(`${relPath}: source count ${sourceCount} is smaller than candidate rows ${candidates.length}`);
  }
  console.log(
    `  ${relPath}: ${candidates.length} candidates · ${cover} cover · ${exclude} exclude · ${defer} defer` +
      (sourceCount === undefined ? "" : ` · source_count=${sourceCount}`),
  );
  return failures;
}

function main(): void {
  const config = parseArgs(process.argv.slice(2));
  const artifacts = loadArtifacts(config);
  const failures: string[] = [];

  console.log("address-universe gate");
  console.log(`  registry root: ${REGISTRY_ROOT}`);

  if (config.protocol && artifacts.length === 0) {
    failures.push(
      `surface/${config.protocol}/ is missing _*_universe.json; ` +
        "pool/factory/vault-heavy onboarding cannot claim P0/P4 completion without it",
    );
  }

  const callkeyPrefixes = config.requireCoverLinkage ? loadCallkeyPrefixes() : new Set<string>();
  if (config.requireCoverLinkage && callkeyPrefixes.size === 0) {
    failures.push("registryV2/index/by-callkey is empty or missing; run npm run build before --require-cover-linkage");
  }

  for (const artifact of artifacts) {
    failures.push(...checkArtifact(artifact, callkeyPrefixes, config));
  }

  if (artifacts.length === 0 && !config.protocol) {
    console.log("  no address-universe artifacts found; pass in global scan mode");
  }

  if (failures.length > 0) {
    console.error(`\nFAIL - ${failures.length} address-universe violation(s):`);
    for (const failure of failures) console.error(`  x ${failure}`);
    process.exit(1);
  }

  console.log("\nPASS - address universe gate found no violations.");
}

main();
