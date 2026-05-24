/**
 * gen-uniswap-ur.ts
 *
 * Universal Router manifest 생성 — per-chain split (감사 F1 처치).
 *
 * 문제(F1): UR `execute` 는 chain 별로 다수 배포 주소를 가진다. 과거 구현은
 *   1 manifest 에 chain_ids[13] × to[29] flat cross-product 를 넣어, build-index
 *   가 미배포 (chain,addr) 조합마다 dead callkey 를 생성했다 (감사 측정: 308 dead
 *   pair / 616 dead callkey = index 32%).
 *
 * 해법(per-chain split): chain 마다 manifest 1개. match.chain_ids = [그 chain],
 *   match.to = [그 chain 의 UR 주소들]. 단일 chain 이라 cross-product = 1×N = N,
 *   N 개 전부 그 chain 의 실 배포 → spurious callkey 0.
 *
 * canonical chain (전역 최소 chainId = 1 Ethereum) manifest 는 `execute@1.0.0.json`
 *   원본 파일명을 유지한다 — opcode_stream.rs / edge_v4.rs 의 include_str! 안전.
 *   그 외 chain 은 `execute-<slug>@1.0.0.json` (gen-uniswap-manifests.ts 와 동일 slug).
 *
 * execute-no-deadline (2-arg overload `execute(bytes,bytes[])`, selector 0x24856bc3)
 *   도 동일하게 chain 별. emit 은 execute 와 동일(opcode 의미 불변), abi 에서
 *   deadline input 만 제거.
 *
 * 입력: uniswap-deployments.json `universal-router` (per-chain 주소 배열).
 * 멱등 — 기존 execute*.json 전부 삭제 후 재생성. 재실행 안전.
 *
 * 실행: cd registry && npx tsx scripts/gen-uniswap-ur.ts
 */
import { readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REGISTRY_ROOT = resolve(HERE, "..");
const UR_DIR = join(REGISTRY_ROOT, "manifests", "uniswap", "universal-router");
const MATRIX_PATH = join(HERE, "uniswap-deployments.json");

/** 3-arg `execute(bytes,bytes[],uint256)` selector. */
const EXECUTE_SELECTOR = "0x3593564c";
/** 2-arg `execute(bytes,bytes[])` overload selector — Tier B universal_router.rs EXECUTE_SELECTOR. */
const EXECUTE_NO_DEADLINE_SELECTOR = "0x24856bc3";

/** 13 target chain → manifest 파일명 slug. gen-uniswap-manifests.ts CHAIN_SLUG 와 동일. */
const CHAIN_SLUG: Record<number, string> = {
  1: "ethereum",
  10: "optimism",
  56: "bnb",
  130: "unichain",
  137: "polygon",
  480: "worldchain",
  8453: "base",
  42161: "arbitrum",
  42220: "celo",
  43114: "avalanche",
  57073: "ink",
  81457: "blast",
  7777777: "zora",
};

interface UrManifest {
  id: string;
  match: { chain_ids: number[]; to: string[]; selector: string };
  abi_fragment: { abi: { inputs: Array<{ name: string; type: string }> } };
  [k: string]: unknown;
}

function exists(p: string): boolean {
  try {
    statSync(p);
    return true;
  } catch {
    return false;
  }
}

function fail(msg: string): never {
  console.error(`[ur] ERROR: ${msg}`);
  process.exit(1);
}

function main(): void {
  if (!exists(MATRIX_PATH)) fail(`주소 matrix 없음: ${MATRIX_PATH}`);
  const matrix = JSON.parse(readFileSync(MATRIX_PATH, "utf8")) as Record<
    string,
    Record<string, string[]>
  >;
  const ur = matrix["universal-router"];
  if (!ur || Object.keys(ur).length === 0) fail("matrix 에 universal-router deployments 없음");

  // template — 기존 execute@1.0.0.json 의 emit / abi_fragment / requires 재사용.
  // (최초 = flat cross-product, 재실행 시 = canonical per-chain — 둘 다 emit 동일.)
  const templatePath = join(UR_DIR, "execute@1.0.0.json");
  if (!exists(templatePath)) fail(`template ${templatePath} 없음`);
  const template = JSON.parse(readFileSync(templatePath, "utf8")) as UrManifest;

  const chainIds = Object.keys(ur)
    .map(Number)
    .sort((a, b) => a - b);
  for (const cid of chainIds) {
    if (!CHAIN_SLUG[cid]) fail(`chain ${cid} 가 13 target chain 외 — slug 미정의`);
    const addrs = ur[String(cid)];
    if (!Array.isArray(addrs) || addrs.length === 0) {
      fail(`universal-router["${cid}"] 가 비었거나 배열 아님`);
    }
  }
  const globalMin = chainIds[0];

  // no-deadline abi inputs — execute 에서 deadline 만 제거
  const noDeadlineInputs = template.abi_fragment.abi.inputs.filter((i) => i.name !== "deadline");
  if (noDeadlineInputs.length !== 2) {
    fail(`execute abi 에서 deadline 제거 후 input 2개 기대, got ${noDeadlineInputs.length}`);
  }

  // 멱등 — 기존 execute*.json 전부 삭제 (옛 cross-product + stale per-chain 제거)
  let removed = 0;
  for (const f of readdirSync(UR_DIR)) {
    if (/^execute.*\.json$/.test(f)) {
      rmSync(join(UR_DIR, f));
      removed++;
    }
  }

  let count = 0;
  for (const cid of chainIds) {
    const suffix = cid === globalMin ? "" : `-${CHAIN_SLUG[cid]}`;
    const to = ur[String(cid)];

    // execute@ — 3-arg (deadline 포함)
    const exec = JSON.parse(JSON.stringify(template)) as UrManifest;
    exec.id = `uniswap/universal-router/execute${suffix}@1.0.0`;
    exec.match = { chain_ids: [cid], to, selector: EXECUTE_SELECTOR };
    writeFileSync(
      join(UR_DIR, `execute${suffix}@1.0.0.json`),
      JSON.stringify(exec, null, 2) + "\n",
      "utf8",
    );
    count++;

    // execute-no-deadline@ — 2-arg overload
    const nod = JSON.parse(JSON.stringify(template)) as UrManifest;
    nod.id = `uniswap/universal-router/execute-no-deadline${suffix}@1.0.0`;
    nod.match = { chain_ids: [cid], to, selector: EXECUTE_NO_DEADLINE_SELECTOR };
    nod.abi_fragment.abi.inputs = noDeadlineInputs;
    writeFileSync(
      join(UR_DIR, `execute-no-deadline${suffix}@1.0.0.json`),
      JSON.stringify(nod, null, 2) + "\n",
      "utf8",
    );
    count++;
  }

  console.error(
    `[ur] removed ${removed} 기존 manifest → ${count} per-chain manifest 생성 ` +
      `(${chainIds.length} chain × {execute, execute-no-deadline})`,
  );
  console.error(`[ur] canonical: execute@1.0.0.json (chain ${globalMin})`);
  console.error("[ur] done");
}

main();
