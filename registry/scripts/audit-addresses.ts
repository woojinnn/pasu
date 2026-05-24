/**
 * audit-addresses.ts
 *
 * ScopeBall registry — on-chain bytecode 존재검증 (감사 Phase A1).
 *
 * registry/manifests/ 의 모든 bundle 에서 distinct (chain_id, to) pair 를 모아
 * 각 체인의 공개 RPC `eth_getCode` 로 그 주소에 컨트랙트 bytecode 가 있는지 검증한다.
 *
 *   - "0x"          → bogus  : 그 체인에 미배포 (EOA / 미존재) → dead callkey
 *   - len > 0       → present: bytecode 존재 (identity 는 A2 가 검증)
 *   - RPC 전부 실패  → unknown: 판정 불가 — 거짓 bogus 금지
 *
 * present pair 는 bytecode 의 sha256 fingerprint 를 기록한다 — A2 의
 * bytecode-hash 클러스터링(동일 코드 = 동일 컨트랙트) 입력. sha256 은 Ethereum
 * 해시가 아니라 단순 content fingerprint 이므로 Node 내장 crypto 로 충분(의존성 0).
 *
 * 산출:  scripts/audit-addresses.json (machine-readable, A2 입력)
 *        scripts/audit-addresses.md   (사람용 요약)
 *
 * 실행:  cd registry && npx tsx scripts/audit-addresses.ts
 *
 * exit code (감사 Phase E — `verify-addresses` 게이트 겸용):
 *   - bogus > 0                        → exit 1 (미배포 dead callkey)
 *   - unknown > 0 (--allow-unknown 無)  → exit 1 (RPC 판정 불가)
 *   - --allow-unknown 시 unknown 은 warn 만 (공개 RPC 간헐 실패 거짓차단 방지)
 *   - 그 외                            → exit 0
 *
 * 본 스크립트는 registry/manifests 를 READ-ONLY 로만 접근 — manifests/index 무변경.
 *
 * Spec reference: docs/TIER_AB_PLAYBOOK.md §10 V1 (on-chain 존재검증),
 *                 plan hidden-sauteeing-cerf.md Phase A1.
 */

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface BundleMatch {
  chain_ids: number[];
  to: string[];
  selector: string;
}
interface AdapterBundle {
  match?: BundleMatch;
  [key: string]: unknown;
}

type Status = "present" | "bogus" | "unknown";

interface PairResult {
  chain_id: number;
  address: string;
  status: Status;
  code_len: number; // bytecode 바이트 수. bogus=0, unknown=-1
  code_hash: string | null; // present 시 sha256(bytecode) — A2 클러스터 키
  rpc: string | null; // 성공한 endpoint
  manifests: string[]; // 이 pair 를 callkey 로 쓰는 manifest 경로 (역색인)
  note?: string;
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REGISTRY_ROOT = resolve(__dirname, "..");
const MANIFESTS_DIR = join(REGISTRY_ROOT, "manifests");
const OUT_JSON = join(__dirname, "audit-addresses.json");
const OUT_MD = join(__dirname, "audit-addresses.md");

// ---------------------------------------------------------------------------
// RPC 매트릭스 — registry 가 쓰는 20 체인. 체인당 공개 endpoint 2~3 (fallback).
// 출처: PHASE13-CURVE-RESEARCH.md §8 (13 체인) + 공개 RPC.
// ---------------------------------------------------------------------------

const RPC: Record<number, string[]> = {
  1: ["https://ethereum-rpc.publicnode.com", "https://eth.llamarpc.com"],
  10: ["https://optimism-rpc.publicnode.com", "https://mainnet.optimism.io"],
  56: ["https://bsc-rpc.publicnode.com", "https://bsc-dataseed.bnbchain.org"],
  100: ["https://gnosis-rpc.publicnode.com", "https://rpc.gnosischain.com"],
  130: ["https://unichain-rpc.publicnode.com", "https://mainnet.unichain.org"],
  137: ["https://polygon-bor-rpc.publicnode.com", "https://polygon-rpc.com"],
  196: ["https://rpc.xlayer.tech", "https://xlayerrpc.okx.com"],
  250: ["https://rpcapi.fantom.network", "https://rpc.fantom.network", "https://fantom-rpc.publicnode.com"],
  252: ["https://rpc.frax.com", "https://fraxtal-rpc.publicnode.com"],
  324: ["https://mainnet.era.zksync.io"],
  480: ["https://worldchain-mainnet.gateway.tenderly.co", "https://480.rpc.thirdweb.com"],
  2222: ["https://kava-evm-rpc.publicnode.com", "https://evm.kava.io"],
  5000: ["https://rpc.mantle.xyz", "https://mantle-rpc.publicnode.com"],
  8453: ["https://base-rpc.publicnode.com", "https://mainnet.base.org"],
  42161: ["https://arbitrum-one-rpc.publicnode.com", "https://arb1.arbitrum.io/rpc"],
  42220: ["https://celo-rpc.publicnode.com", "https://forno.celo.org"],
  43114: ["https://avalanche-c-chain-rpc.publicnode.com", "https://api.avax.network/ext/bc/C/rpc"],
  57073: ["https://rpc-gel.inkonchain.com", "https://rpc-qnd.inkonchain.com"],
  81457: ["https://rpc.blast.io", "https://blast-rpc.publicnode.com"],
  7777777: ["https://rpc.zora.energy"],
};

const CHAIN_NAME: Record<number, string> = {
  1: "Ethereum", 10: "Optimism", 56: "BSC", 100: "Gnosis", 130: "Unichain",
  137: "Polygon", 196: "X-Layer", 250: "Fantom", 252: "Fraxtal", 324: "zkSync Era",
  480: "World Chain", 2222: "Kava", 5000: "Mantle", 8453: "Base", 42161: "Arbitrum",
  42220: "Celo", 43114: "Avalanche", 57073: "Ink", 81457: "Blast", 7777777: "Zora",
};

const CONCURRENCY = 5;
const REQUEST_TIMEOUT_MS = 12_000;

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
        // Skip `_template/` dirs — pool-type emit-rule templates with
        // placeholder addresses (0x1111…/0xaaaa…) consumed by
        // `gen-curve-pools.ts`. These are NOT real on-chain pools and would
        // falsely trip the bogus gate.
        if (entry === "_template") continue;
        stack.push(p);
      } else if (s.isFile() && entry.endsWith(".json")) out.push(p);
    }
  }
  return out.sort();
}

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

/** 모든 manifest 에서 distinct (chain_id, to) pair 를 모은다 — chain_ids × to 전개. */
function collectPairs(files: string[]): Map<string, PairResult> {
  const out = new Map<string, PairResult>();
  for (const f of files) {
    let bundle: AdapterBundle;
    try {
      bundle = JSON.parse(readFileSync(f, "utf8")) as AdapterBundle;
    } catch (e) {
      console.error(`[audit-addresses] WARN parse 실패 ${f}: ${(e as Error).message}`);
      continue;
    }
    const m = bundle.match;
    if (!m || !Array.isArray(m.chain_ids) || !Array.isArray(m.to)) continue;
    const rel = relative(REGISTRY_ROOT, f).split(/[\\/]/).join("/");
    for (const c of m.chain_ids) {
      for (const t of m.to) {
        const addr = String(t).toLowerCase();
        const key = `${c}__${addr}`;
        let pr = out.get(key);
        if (!pr) {
          pr = {
            chain_id: c, address: addr, status: "unknown",
            code_len: -1, code_hash: null, rpc: null, manifests: [],
          };
          out.set(key, pr);
        }
        if (!pr.manifests.includes(rel)) pr.manifests.push(rel);
      }
    }
  }
  return out;
}

/** 단일 RPC endpoint 에 eth_getCode 호출. 실패 시 throw. */
async function rpcGetCode(url: string, addr: string): Promise<string> {
  const res = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "eth_getCode", params: [addr, "latest"] }),
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const body = (await res.json()) as { result?: unknown; error?: unknown };
  if (body.error) throw new Error(`RPC error ${JSON.stringify(body.error)}`);
  const result = body.result;
  if (typeof result !== "string" || !result.startsWith("0x")) {
    throw new Error(`unexpected result ${String(result).slice(0, 48)}`);
  }
  return result;
}

/** endpoint fallback + 3회 backoff retry. 전부 실패 시 null. */
async function getCode(chainId: number, addr: string): Promise<{ code: string; rpc: string } | null> {
  const endpoints = RPC[chainId] ?? [];
  for (const url of endpoints) {
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        return { code: await rpcGetCode(url, addr), rpc: url };
      } catch {
        await sleep(250 * (attempt + 1));
      }
    }
  }
  return null;
}

/** 한 pair 를 검증해 status/code_len/code_hash 를 채운다. */
async function classifyPair(pair: PairResult): Promise<void> {
  const got = await getCode(pair.chain_id, pair.address);
  if (got === null) {
    pair.status = "unknown";
    pair.code_len = -1;
    pair.note = RPC[pair.chain_id] ? "RPC 도달 실패 (retry 소진)" : "RPC endpoint 미설정";
    return;
  }
  pair.rpc = got.rpc;
  if (got.code === "0x") {
    pair.status = "bogus";
    pair.code_len = 0;
    pair.note = "bytecode 없음 — dead callkey";
  } else {
    pair.status = "present";
    pair.code_len = (got.code.length - 2) / 2;
    pair.code_hash = "sha256:" + createHash("sha256").update(got.code.slice(2), "hex").digest("hex");
  }
}

/** bounded-concurrency 워커 풀. */
async function runPool<T>(items: T[], concurrency: number, worker: (item: T) => Promise<void>): Promise<void> {
  let next = 0;
  const lane = async (): Promise<void> => {
    while (next < items.length) {
      await worker(items[next++]);
    }
  };
  await Promise.all(Array.from({ length: Math.min(concurrency, items.length) }, lane));
}

/** 시작 전 sanity — Ethereum WETH(코드 있음) + zero-addr(코드 없음). RPC 정상 확인. */
async function selfCheck(): Promise<void> {
  const weth = await getCode(1, "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");
  const zero = await getCode(1, "0x0000000000000000000000000000000000000000");
  if (!weth || weth.code === "0x") {
    throw new Error("self-check 실패: Ethereum WETH 에 bytecode 가 있어야 함 — RPC 이상");
  }
  if (!zero || zero.code !== "0x") {
    throw new Error("self-check 실패: zero-address 는 bytecode 가 없어야 함");
  }
  console.error("[audit-addresses] self-check OK (WETH=present, zero-addr=bogus)");
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

function buildMarkdown(list: PairResult[], manifestCount: number): string {
  const total = list.length;
  const cnt = (s: Status) => list.filter((p) => p.status === s).length;
  const byChain = new Map<number, { present: number; bogus: number; unknown: number }>();
  for (const p of list) {
    const e = byChain.get(p.chain_id) ?? { present: 0, bogus: 0, unknown: 0 };
    e[p.status]++;
    byChain.set(p.chain_id, e);
  }

  const lines: string[] = [];
  lines.push("# audit-addresses — on-chain bytecode 존재검증 (Phase A1)");
  lines.push("");
  lines.push(`- 측정: ${new Date().toISOString()}`);
  lines.push(`- manifest ${manifestCount} → distinct (chain,addr) pair ${total}`);
  lines.push(`- **present ${cnt("present")} / bogus ${cnt("bogus")} / unknown ${cnt("unknown")}**`);
  lines.push("");
  lines.push("## 체인별");
  lines.push("");
  lines.push("| chain | name | present | bogus | unknown |");
  lines.push("|---|---|---|---|---|");
  for (const c of [...byChain.keys()].sort((a, b) => a - b)) {
    const e = byChain.get(c)!;
    lines.push(`| ${c} | ${CHAIN_NAME[c] ?? "?"} | ${e.present} | ${e.bogus} | ${e.unknown} |`);
  }
  lines.push("");

  const bogus = list.filter((p) => p.status === "bogus").sort((a, b) => a.chain_id - b.chain_id);
  lines.push(`## 🔴 bogus — bytecode 없음 (dead callkey) — ${bogus.length}건`);
  lines.push("");
  if (bogus.length === 0) {
    lines.push("_없음._");
  } else {
    lines.push("| chain | address | manifest 수 | 대표 manifest |");
    lines.push("|---|---|---|---|");
    for (const p of bogus) {
      lines.push(
        `| ${p.chain_id} ${CHAIN_NAME[p.chain_id] ?? ""} | \`${p.address}\` | ${p.manifests.length} | ${p.manifests[0] ?? ""} |`,
      );
    }
  }
  lines.push("");

  const unknown = list.filter((p) => p.status === "unknown").sort((a, b) => a.chain_id - b.chain_id);
  lines.push(`## ⚠️ unknown — RPC 판정 불가 — ${unknown.length}건`);
  lines.push("");
  if (unknown.length === 0) {
    lines.push("_없음._");
  } else {
    lines.push("| chain | address | note |");
    lines.push("|---|---|---|");
    for (const p of unknown) {
      lines.push(`| ${p.chain_id} ${CHAIN_NAME[p.chain_id] ?? ""} | \`${p.address}\` | ${p.note ?? ""} |`);
    }
  }
  lines.push("");
  return lines.join("\n");
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  const files = walkJsonFiles(MANIFESTS_DIR);
  const pairMap = collectPairs(files);
  const list = [...pairMap.values()].sort(
    (a, b) => a.chain_id - b.chain_id || a.address.localeCompare(b.address),
  );
  console.error(
    `[audit-addresses] ${files.length} manifest → ${list.length} distinct (chain,addr) pair`,
  );

  await selfCheck();

  let done = 0;
  await runPool(list, CONCURRENCY, async (pair) => {
    await classifyPair(pair);
    done++;
    if (done % 25 === 0 || done === list.length) {
      console.error(`[audit-addresses] ${done}/${list.length}`);
    }
  });

  const summary = {
    present: list.filter((p) => p.status === "present").length,
    bogus: list.filter((p) => p.status === "bogus").length,
    unknown: list.filter((p) => p.status === "unknown").length,
  };
  const out = {
    audited_at: new Date().toISOString(),
    registry_root: REGISTRY_ROOT,
    total_manifests: files.length,
    total_pairs: list.length,
    summary,
    pairs: list,
  };
  writeFileSync(OUT_JSON, JSON.stringify(out, null, 2) + "\n", "utf8");
  writeFileSync(OUT_MD, buildMarkdown(list, files.length), "utf8");

  console.error(
    `[audit-addresses] done — present ${summary.present} / bogus ${summary.bogus} / unknown ${summary.unknown}`,
  );
  console.error(`[audit-addresses] → ${relative(REGISTRY_ROOT, OUT_JSON)}  +  ${relative(REGISTRY_ROOT, OUT_MD)}`);

  // --- gate (Phase E) — `verify-addresses` 로 호출 시 exit code 로 차단 ---
  const allowUnknown = process.argv.includes("--allow-unknown");
  if (summary.bogus > 0) {
    console.error(
      `[audit-addresses] GATE FAIL — bogus ${summary.bogus} (미배포 dead callkey). audit-addresses.md 참조.`,
    );
    process.exit(1);
  }
  if (summary.unknown > 0 && !allowUnknown) {
    console.error(
      `[audit-addresses] GATE FAIL — unknown ${summary.unknown} (RPC 판정 불가). ` +
        `RPC 복구 후 재시도하거나 --allow-unknown 으로 허용.`,
    );
    process.exit(1);
  }
  if (summary.unknown > 0) {
    console.error(
      `[audit-addresses] WARN — unknown ${summary.unknown} (--allow-unknown — RPC 실패 무시).`,
    );
  }
}

main().catch((e) => {
  console.error(`[audit-addresses] FATAL: ${(e as Error).message}`);
  process.exit(1);
});
