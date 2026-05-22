/**
 * gen-uniswap-ur.ts
 *
 * Universal Router manifest 갱신 — per-address split 의 의도적 예외.
 *
 * UR 은 단일 함수 `execute` 에 다수(13 chain × v1.2/v2/v2.1) 배포 주소를 가진다.
 * per-address split 하면 ~250줄 `per_opcode_emit` 블록이 주소 수만큼 중복되므로,
 * UR 만 cross-product (1 manifest, chain_ids[13] × to[N]) 를 유지한다.
 *
 * 작업:
 *   1. execute@1.0.0.json 의 match.{chain_ids,to} 를 uniswap-deployments.json 의
 *      `universal-router` (1차 출처 검증된 주소) 로 갱신.
 *        → 기존 to[] 의 오염 항목 (0x7a250d56 = mainnet V2Router02, 0x4c82d1FB =
 *          test fixture) 은 matrix 에 없으므로 자동 제거.
 *   2. execute-no-deadline@1.0.0.json 신규 — 2-arg `execute(bytes,bytes[])`
 *      overload (selector 0x24856bc3). abi 에서 deadline input 만 제거, emit
 *      (per_opcode_emit) 은 동일 (opcode 의미 불변).
 *
 * 멱등 — uniswap-deployments.json 만 입력. 재실행 안전.
 *
 * 실행: cd registry && npx tsx scripts/gen-uniswap-ur.ts
 */
import { readFileSync, writeFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REGISTRY_ROOT = resolve(HERE, "..");
const UR_DIR = join(REGISTRY_ROOT, "manifests", "uniswap", "universal-router");
const MATRIX_PATH = join(HERE, "uniswap-deployments.json");

/** 2-arg `execute(bytes,bytes[])` selector — Tier B universal_router.rs EXECUTE_SELECTOR. */
const EXECUTE_NO_DEADLINE_SELECTOR = "0x24856bc3";

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

  // chain_ids = 정렬된 chain 키, to = distinct 주소 (chain 순회 + first-seen dedup)
  const chainIds = Object.keys(ur)
    .map(Number)
    .sort((a, b) => a - b);
  const seen = new Set<string>();
  const to: string[] = [];
  for (const cid of chainIds) {
    const addrs = ur[String(cid)];
    if (!Array.isArray(addrs)) fail(`universal-router["${cid}"] 가 배열 아님`);
    for (const addr of addrs) {
      const k = addr.toLowerCase();
      if (!seen.has(k)) {
        seen.add(k);
        to.push(addr);
      }
    }
  }

  // 1. execute@1.0.0.json — match 갱신
  const executePath = join(UR_DIR, "execute@1.0.0.json");
  if (!exists(executePath)) fail(`${executePath} 없음`);
  const execute = JSON.parse(readFileSync(executePath, "utf8")) as UrManifest;
  execute.match.chain_ids = chainIds;
  execute.match.to = to;
  writeFileSync(executePath, JSON.stringify(execute, null, 2) + "\n", "utf8");
  console.error(`[ur] execute@1.0.0.json — chain_ids ${chainIds.length}, to ${to.length}`);

  // 2. execute-no-deadline@1.0.0.json — 2-arg overload 신규
  const noDeadline = JSON.parse(JSON.stringify(execute)) as UrManifest; // 갱신된 execute deep copy
  noDeadline.id = "uniswap/universal-router/execute-no-deadline@1.0.0";
  noDeadline.match = { chain_ids: chainIds, to, selector: EXECUTE_NO_DEADLINE_SELECTOR };
  const inputs = noDeadline.abi_fragment.abi.inputs.filter((i) => i.name !== "deadline");
  if (inputs.length !== 2) {
    fail(`execute abi 에서 deadline 제거 후 input 2개 기대, got ${inputs.length}`);
  }
  noDeadline.abi_fragment.abi.inputs = inputs;
  writeFileSync(
    join(UR_DIR, "execute-no-deadline@1.0.0.json"),
    JSON.stringify(noDeadline, null, 2) + "\n",
    "utf8",
  );
  console.error(
    `[ur] execute-no-deadline@1.0.0.json — 신규 (selector ${EXECUTE_NO_DEADLINE_SELECTOR}, ` +
      `chain_ids ${chainIds.length}, to ${to.length})`,
  );
  console.error("[ur] done");
}

main();
