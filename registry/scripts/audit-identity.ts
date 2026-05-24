/**
 * audit-identity.ts
 *
 * ScopeBall registry — contract identity 검증 (감사 Phase A2).
 *
 * A1(`audit-addresses.json`) 의 `present` pair 가 *올바른 프로토콜 컨트랙트*인지
 * 검증한다. "코드가 있다"(A1) ≠ "올바른 컨트랙트다"(A2).
 *
 * 방법 (zero-RPC — 로컬 oracle + A1 데이터만):
 *   1. bytecode hash 클러스터링 — A1 이 기록한 code_hash 로 동일 컨트랙트 그룹화.
 *   2. Uniswap oracle — `uniswap-deployments.json` (1차 출처 `deployments/<chain>.md`)
 *      의 (chain,addr)→deployment-key 매트릭스와 대조.
 *   3. Curve anchor — `docs/AUDIT_PRIOR_ART.md` (Phase 0) 가 1차 출처로 확정한
 *      Curve 싱글톤 주소 hardcode 대조.
 *   4. 클러스터 전파 — 클러스터에 anchored identity 가 유일하면 미anchored 멤버에 전파.
 *
 * verdict:  match        — oracle 확정, manifest 타입과 일치
 *           match-cluster — bytecode 클러스터로 anchored 멤버에서 전파
 *           mismatch     — 🔴 oracle 에 있으나 manifest 타입과 불일치 (mis-decode 위험)
 *           unlisted     — Uniswap: present 이나 deployments.json 미등재
 *           unverified   — oracle 없음 (Aerodrome / Curve pool·minor) → B2 에서 처리
 *
 * Aerodrome 와 Curve pool 은 1차 출처 oracle 이 아직 없다(B1 산출물) — A2 는
 * unverified 로 표기하고 B2 에서 aerodrome-deployments.json / curve pool 대조로 마감.
 *
 * 산출:  scripts/audit-identity.json + scripts/audit-identity.md
 * 실행:  cd registry && npx tsx scripts/audit-identity.ts   (A1 선행 필요)
 *
 * registry 무변경 — READ-ONLY.
 */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const REGISTRY_ROOT = resolve(__dirname, "..");
const A1_JSON = join(__dirname, "audit-addresses.json");
const UNI_DEPLOY = join(__dirname, "uniswap-deployments.json");
const OUT_JSON = join(__dirname, "audit-identity.json");
const OUT_MD = join(__dirname, "audit-identity.md");

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface A1Pair {
  chain_id: number;
  address: string;
  status: string;
  code_hash: string | null;
  manifests: string[];
}
interface A1Doc {
  pairs: A1Pair[];
}

type Verdict = "match" | "match-cluster" | "mismatch" | "unlisted" | "unverified";

interface IdResult {
  chain_id: number;
  address: string;
  manifest_dir: string; // 대표 protocol/contract
  expected: string; // manifest 가 기대하는 타입
  verdict: Verdict;
  identity: string | null; // oracle 가 확정한 실제 타입
  code_hash: string | null;
  cluster_size: number;
  manifests: string[];
  note?: string;
}

// ---------------------------------------------------------------------------
// Uniswap manifest dir → 허용 deployment key
// ---------------------------------------------------------------------------

const UNI_COMPAT: Record<string, string[]> = {
  "uniswap/v2": ["v2-router02"],
  "uniswap/v3": ["v3-swap-router", "v3-nfpm"],
  "uniswap/swap-router-02": ["swap-router-02"],
  "uniswap/v4": ["v4-pm", "v4-pool-manager"],
  "uniswap/universal-router": ["universal-router"],
  "uniswap/permit2": ["permit2"],
};

// ---------------------------------------------------------------------------
// Curve 싱글톤 anchor — docs/AUDIT_PRIOR_ART.md (Phase 0, 1차 출처 확정).
// key = `${chain_id}__${addr-lowercase}`.
// ---------------------------------------------------------------------------

const CURVE_ANCHORS: Record<string, string> = (() => {
  const m: Record<string, string> = {};
  const put = (chain: number, addr: string, label: string) => {
    m[`${chain}__${addr.toLowerCase()}`] = label;
  };
  // Router NG 14-chain (PHASE13 §1.1)
  const routerNg: [number, string][] = [
    [1, "0x45312ea0eFf7E09C83CBE249fa1d7598c4C8cd4e"],
    [10, "0x0DCDED3545D565bA3B19E683431381007245d983"],
    [100, "0x0DCDED3545D565bA3B19E683431381007245d983"],
    [137, "0x0DCDED3545D565bA3B19E683431381007245d983"],
    [250, "0x0DCDED3545D565bA3B19E683431381007245d983"],
    [2222, "0x0DCDED3545D565bA3B19E683431381007245d983"],
    [43114, "0x0DCDED3545D565bA3B19E683431381007245d983"],
    [56, "0xA72C85C258A81761433B4e8da60505Fe3Dd551CC"],
    [252, "0x56C526b0159a258887e0d79ec3a80dfb940d0cD7"],
    [324, "0x7C915390e109CA66934f1eB285854375D1B127FA"],
    [8453, "0x4f37A9d177470499A2dD084621020b023fcffc1F"],
    [5000, "0x4f37A9d177470499A2dD084621020b023fcffc1F"],
    [42161, "0x2191718CD32d02B8E60BAdFFeA33E4B5DD9A0A0D"],
    [196, "0xBFab8ebc836E1c4D81837798FC076D219C9a1855"],
  ];
  for (const [c, a] of routerNg) put(c, a, "curve/router-ng");
  // crvUSD Controller 3 market (PHASE13 §4.3, mainnet)
  put(1, "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce", "curve/crvusd");
  put(1, "0xEC0820EfafC41D8943EE8dE495fC9Ba8495B15cf", "curve/crvusd");
  put(1, "0x4e59541306910ad6dc1dac0ac9dfb29bd9f15c67", "curve/crvusd");
  // 부속 contract (PHASE13 §4.4·§6, mainnet)
  put(1, "0x5f3b5DfeB7B28CDbD7FaBa78963EE202a494e2A2", "curve/vecrv");
  put(1, "0x2F50D538606Fa9EDD2B11E2446BEb18C9D5846bb", "curve/gauge-controller");
  put(1, "0xbFcF63294aD7105dEa65aA58F8AE5BE2D9D0952A", "curve/gauge");
  put(1, "0x182B723a58739a9c974cFDB385ceaDb237453c28", "curve/gauge");
  put(1, "0x2932a86df44Fe8D2A706d8e9c5d51c24883423F5", "curve/gauge");
  put(1, "0xa1F8A6807c402E4A15ef4EBa36528A3FED24E577", "curve/stableswap"); // frxETH/ETH pool
  return m;
})();

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** manifest 경로 → `<protocol>/<contract>` dir prefix. */
function manifestDir(path: string): string {
  const parts = path.split("/");
  return parts.length >= 3 ? `${parts[1]}/${parts[2]}` : path;
}

/** uniswap-deployments.json → (chain__addr) → deployment key. */
function buildUniOracle(): Map<string, string> {
  const raw = JSON.parse(readFileSync(UNI_DEPLOY, "utf8")) as Record<
    string,
    Record<string, string | string[]>
  >;
  const oracle = new Map<string, string>();
  for (const [depKey, perChain] of Object.entries(raw)) {
    if (depKey.startsWith("_")) continue; // _comment
    for (const [chain, val] of Object.entries(perChain)) {
      const addrs = Array.isArray(val) ? val : [val];
      for (const a of addrs) {
        oracle.set(`${chain}__${a.toLowerCase()}`, depKey);
      }
    }
  }
  return oracle;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

function main(): void {
  const a1 = JSON.parse(readFileSync(A1_JSON, "utf8")) as A1Doc;
  const present = a1.pairs.filter((p) => p.status === "present");
  const uniOracle = buildUniOracle();

  // bytecode hash 클러스터 — code_hash → pair index 목록
  const clusters = new Map<string, number[]>();
  present.forEach((p, i) => {
    const h = p.code_hash ?? `__nohash_${i}`;
    if (!clusters.has(h)) clusters.set(h, []);
    clusters.get(h)!.push(i);
  });

  // 1차 — oracle anchor
  const results: IdResult[] = present.map((p) => {
    const dirs = [...new Set(p.manifests.map(manifestDir))];
    const dir = dirs[0] ?? "?";
    const key = `${p.chain_id}__${p.address}`;
    const clusterSize = clusters.get(p.code_hash ?? "")?.length ?? 1;
    const base: IdResult = {
      chain_id: p.chain_id,
      address: p.address,
      manifest_dir: dirs.join(","),
      expected: dir,
      verdict: "unverified",
      identity: null,
      code_hash: p.code_hash,
      cluster_size: clusterSize,
      manifests: p.manifests,
    };

    if (dir.startsWith("uniswap/")) {
      const depKey = uniOracle.get(key);
      if (depKey) {
        const compat = UNI_COMPAT[dir] ?? [];
        base.identity = `uniswap/${depKey}`;
        base.verdict = compat.includes(depKey) ? "match" : "mismatch";
        if (base.verdict === "mismatch") {
          base.note = `deployments.json 은 ${depKey} 인데 manifest 는 ${dir}`;
        }
      } else {
        base.verdict = "unlisted";
        base.note = "present 이나 uniswap-deployments.json 미등재";
      }
    } else if (dir.startsWith("curve/")) {
      const cv = CURVE_ANCHORS[key];
      if (cv) {
        base.identity = cv;
        base.verdict = cv === dir ? "match" : "mismatch";
        if (base.verdict === "mismatch") base.note = `anchor 는 ${cv} 인데 manifest 는 ${dir}`;
      } else {
        base.verdict = "unverified";
        base.note = "Curve pool/minor — oracle 없음 → B2";
      }
    } else {
      // aerodrome — oracle 미보유 (B1 산출)
      base.verdict = "unverified";
      base.note = "Aerodrome — ground-truth 미작성 → B2 (aerodrome-deployments.json)";
    }
    return base;
  });

  // 2차 — 클러스터 전파: 클러스터 내 anchored identity 가 유일하면 미anchored 에 전파
  for (const idxs of clusters.values()) {
    if (idxs.length < 2) continue;
    const anchored = new Set(
      idxs.map((i) => results[i]).filter((r) => r.verdict === "match" && r.identity).map((r) => r.identity!),
    );
    if (anchored.size !== 1) continue;
    const id = [...anchored][0];
    for (const i of idxs) {
      const r = results[i];
      if (r.verdict === "unlisted" || r.verdict === "unverified") {
        r.identity = id;
        // 클러스터 전파 identity 가 manifest 기대와 호환되는지 확인
        const compatOk =
          id.startsWith("uniswap/") &&
          (UNI_COMPAT[r.expected] ?? []).includes(id.slice("uniswap/".length));
        r.verdict = compatOk || id === r.expected ? "match-cluster" : "mismatch";
        r.note =
          (r.verdict === "mismatch" ? "🔴 클러스터 전파 결과 타입 불일치 — " : "bytecode 클러스터 전파 — ") +
          `동일 bytecode 컨트랙트 = ${id}`;
      }
    }
  }

  writeReports(results, clusters.size, present.length);
}

function writeReports(results: IdResult[], clusterCount: number, presentCount: number): void {
  const cnt = (v: Verdict) => results.filter((r) => r.verdict === v).length;
  const summary = {
    match: cnt("match"),
    "match-cluster": cnt("match-cluster"),
    mismatch: cnt("mismatch"),
    unlisted: cnt("unlisted"),
    unverified: cnt("unverified"),
  };

  writeFileSync(
    OUT_JSON,
    JSON.stringify(
      { audited_at: new Date().toISOString(), present_pairs: presentCount, distinct_bytecode: clusterCount, summary, results },
      null,
      2,
    ) + "\n",
    "utf8",
  );

  const L: string[] = [];
  L.push("# audit-identity — contract identity 검증 (Phase A2)");
  L.push("");
  L.push(`- 측정: ${new Date().toISOString()}`);
  L.push(`- present pair ${presentCount} → distinct bytecode ${clusterCount}`);
  L.push(
    `- **match ${summary.match} / match-cluster ${summary["match-cluster"]} / mismatch ${summary.mismatch} / unlisted ${summary.unlisted} / unverified ${summary.unverified}**`,
  );
  L.push("");

  const mism = results.filter((r) => r.verdict === "mismatch");
  L.push(`## 🔴 mismatch — oracle 와 manifest 타입 불일치 — ${mism.length}건`);
  L.push("");
  if (mism.length === 0) L.push("_없음._");
  else {
    L.push("| chain | address | manifest | identity | note |");
    L.push("|---|---|---|---|---|");
    for (const r of mism)
      L.push(`| ${r.chain_id} | \`${r.address}\` | ${r.expected} | ${r.identity ?? ""} | ${r.note ?? ""} |`);
  }
  L.push("");

  const unlisted = results.filter((r) => r.verdict === "unlisted").sort((a, b) => a.chain_id - b.chain_id);
  L.push(`## ⚠️ unlisted — Uniswap present 이나 deployments.json 미등재 — ${unlisted.length}건`);
  L.push("");
  if (unlisted.length === 0) L.push("_없음._");
  else {
    L.push("| chain | address | manifest | cluster |");
    L.push("|---|---|---|---|");
    for (const r of unlisted)
      L.push(`| ${r.chain_id} | \`${r.address}\` | ${r.expected} | ${r.cluster_size} |`);
  }
  L.push("");

  // unverified — 타입별 집계
  const unv = results.filter((r) => r.verdict === "unverified");
  const byType = new Map<string, number>();
  for (const r of unv) byType.set(r.expected, (byType.get(r.expected) ?? 0) + 1);
  L.push(`## unverified — oracle 미보유 → B2 처리 — ${unv.length}건`);
  L.push("");
  L.push("| manifest 타입 | pair 수 |");
  L.push("|---|---|");
  for (const [t, c] of [...byType.entries()].sort((a, b) => b[1] - a[1])) L.push(`| ${t} | ${c} |`);
  L.push("");

  // match 타입별 집계
  const matched = results.filter((r) => r.verdict === "match" || r.verdict === "match-cluster");
  const mByType = new Map<string, number>();
  for (const r of matched) mByType.set(r.expected, (mByType.get(r.expected) ?? 0) + 1);
  L.push(`## ✅ match / match-cluster — 타입별 — ${matched.length}건`);
  L.push("");
  L.push("| manifest 타입 | pair 수 |");
  L.push("|---|---|");
  for (const [t, c] of [...mByType.entries()].sort((a, b) => b[1] - a[1])) L.push(`| ${t} | ${c} |`);
  L.push("");

  writeFileSync(OUT_MD, L.join("\n") + "\n", "utf8");

  console.error(
    `[audit-identity] present ${presentCount} / distinct bytecode ${clusterCount}\n` +
      `[audit-identity] match ${summary.match} · match-cluster ${summary["match-cluster"]} · ` +
      `mismatch ${summary.mismatch} · unlisted ${summary.unlisted} · unverified ${summary.unverified}`,
  );
  console.error(`[audit-identity] → scripts/audit-identity.json + scripts/audit-identity.md`);
}

main();
