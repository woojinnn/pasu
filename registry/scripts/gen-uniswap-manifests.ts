/**
 * gen-uniswap-manifests.ts
 *
 * ScopeBall Uniswap registry — per-address-split manifest generator.
 *
 * 문제: bundle `match:{chain_ids[], to[], selector}` 는 "chain → 주소" 매핑을 표현하지
 *       못한다. build-index 는 `chain_ids × to` cross-product 로 callkey 를 만들므로,
 *       한 manifest 에 여러 chain + 단일 mainnet 주소를 넣으면 비-mainnet chain 의
 *       callkey 가 mainnet 주소를 가리켜 영구히 miss 한다.
 *
 * 해법(per-address split): 함수 × distinct 배포 주소마다 manifest 1개.
 *       match.chain_ids = 그 주소를 공유하는 chain 집합, match.to = [그 1 주소].
 *       → 모든 callkey 가 실 배포에 대응. spurious callkey 0.
 *
 * 입력:
 *   - scripts/uniswap-deployments.json
 *       Phase 1 (protocol-researcher) 산출 per-chain 배포 주소 matrix.
 *       { "<routerKey>": { "<chainId>": "<address>" }, ... }
 *   - manifests/uniswap/{v2,v3,v4,swap-router-02,permit2}/*.json
 *       기존 manifest = template. emit / abi_fragment / requires 는 그대로 복제,
 *       match 만 per-address 로 재작성. v3/ 는 V3 SwapRouter+NFPM, v4/ 는
 *       V4 PositionManager+PoolManager 가 혼재 — match.to[0] 으로 판별.
 *
 * 출력:
 *   - 각 함수마다 주소 group 별 manifest. canonical group(전역 최소 chainId 를 포함하는
 *     group)은 원본 파일명을 유지(overwrite), 그 외 group 은
 *     `<func>-<chainSlug>@<ver>.json` 신규 파일.
 *
 * universal-router/ 는 제외 — 단일 함수 execute 에 19 주소라 per_opcode_emit(~250줄)
 *   중복을 피하려고 cross-product 를 유지한다(별도 수작업으로 to[] 만 정리).
 *
 * 멱등 아님 — 1회성 authoring 도구. 재실행 시 먼저 `git checkout manifests/uniswap/`.
 *   (split 된 파일은 match.to 가 비-mainnet 주소라 v3/ router 판별이 실패하며 즉시 abort.)
 *
 * 실행: cd registry && npx tsx scripts/gen-uniswap-manifests.ts
 */
import { readdirSync, readFileSync, writeFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REGISTRY_ROOT = resolve(HERE, "..");
const MANIFESTS_UNISWAP = join(REGISTRY_ROOT, "manifests", "uniswap");
const MATRIX_PATH = join(HERE, "uniswap-deployments.json");

/** 13 target chains → deployment 식별 slug (manifest id/파일명 suffix 용). */
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

/**
 * v3/ 디렉토리는 V3 SwapRouter + V3 NFPM manifest 가 혼재한다. 원본 manifest 의
 * match.to[0] (mainnet 주소)로 어느 router 인지 판별한다. split 후 재실행하면
 * to 가 비-mainnet 주소라 여기서 매치 실패 → 즉시 abort (멱등 가드).
 */
const V3_DISCRIMINATOR: Record<string, string> = {
  "0xe592427a0aece92de3edee1f18e0157c05861564": "v3-swap-router",
  "0xc36442b4a4522e871399cd717abdd847ab11fe88": "v3-nfpm",
};

/**
 * v4/ 디렉토리는 V4 PositionManager + V4 PoolManager manifest 가 혼재한다.
 * v3/ 의 V3_DISCRIMINATOR 와 동일 패턴 — 원본 manifest 의 match.to[0]
 * (chain 1 mainnet 주소)로 어느 컨트랙트인지 판별한다. split 후 재실행하면
 * to 가 비-mainnet 주소라 여기서 매치 실패 → 즉시 abort (멱등 가드).
 */
const V4_DISCRIMINATOR: Record<string, string> = {
  "0xbd216513d74c8cf14cf4747e6aaa6420ff64ee9e": "v4-pm",
  "0x000000000004444c5dc75cb358380d2e3de08a90": "v4-pool-manager",
};

/** 디렉토리 → matrix router key. universal-router 는 의도적으로 제외. */
const DIR_ROUTER: Record<string, string> = {
  v2: "v2-router02",
  "swap-router-02": "swap-router-02",
  permit2: "permit2",
};

const PROCESS_DIRS = ["v2", "v3", "v4", "swap-router-02", "permit2"] as const;
const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

interface Manifest {
  type: string;
  id: string;
  match: { chain_ids: number[]; to: string[]; selector: string };
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
  console.error(`[gen] ERROR: ${msg}`);
  process.exit(1);
}

function main(): void {
  if (!exists(MATRIX_PATH)) {
    fail(
      `주소 matrix 없음: ${MATRIX_PATH}\n` +
        `       Phase 1 (protocol-researcher) 가 uniswap-deployments.json 을 먼저 작성해야 합니다.`,
    );
  }
  const matrix = JSON.parse(readFileSync(MATRIX_PATH, "utf8")) as Record<
    string,
    Record<string, unknown>
  >;

  let srcCount = 0;
  let genCount = 0;

  for (const dir of PROCESS_DIRS) {
    const dirPath = join(MANIFESTS_UNISWAP, dir);
    if (!exists(dirPath)) continue;
    // readdirSync 는 eager — 루프 중 새로 쓴 split 파일은 재처리되지 않는다.
    const files = readdirSync(dirPath)
      .filter((f) => f.endsWith(".json"))
      .sort();

    for (const fname of files) {
      const manifest = JSON.parse(
        readFileSync(join(dirPath, fname), "utf8"),
      ) as Manifest;
      srcCount++;

      // --- router key 판별 ---
      let routerKey: string;
      if (dir === "v3") {
        const to0 = (manifest.match?.to?.[0] ?? "").toLowerCase();
        routerKey = V3_DISCRIMINATOR[to0];
        if (!routerKey) {
          fail(
            `${dir}/${fname}: match.to[0]=${to0} 가 V3 SwapRouter/NFPM 판별 주소가 아님 ` +
              `— 비-멱등 재실행 의심. 'git checkout manifests/uniswap/' 후 재실행하세요.`,
          );
        }
      } else if (dir === "v4") {
        const to0 = (manifest.match?.to?.[0] ?? "").toLowerCase();
        routerKey = V4_DISCRIMINATOR[to0];
        if (!routerKey) {
          fail(
            `${dir}/${fname}: match.to[0]=${to0} 가 V4 PositionManager/PoolManager 판별 주소가 아님 ` +
              `— 비-멱등 재실행 의심. 'git checkout manifests/uniswap/' 후 재실행하세요.`,
          );
        }
      } else {
        routerKey = DIR_ROUTER[dir];
      }

      // --- id / 파일명 정합성 ---
      const atIdx = manifest.id.lastIndexOf("@");
      if (atIdx < 0) fail(`${dir}/${fname}: manifest.id 에 @version 없음 (${manifest.id})`);
      const idPath = manifest.id.slice(0, atIdx); // uniswap/<router>/<func>
      const version = manifest.id.slice(atIdx + 1); // 1.0.0
      const slashIdx = idPath.lastIndexOf("/");
      const idPrefix = idPath.slice(0, slashIdx); // uniswap/<router>
      const funcName = idPath.slice(slashIdx + 1); // <func>
      if (fname !== `${funcName}@${version}.json`) {
        fail(
          `${dir}/${fname}: 파일명이 id 의 함수/버전(${funcName}@${version})과 불일치 ` +
            `— canonical overwrite 가 orphan 을 남길 수 있음.`,
        );
      }

      // --- matrix 에서 배포 주소 조회 ---
      const deployments = matrix[routerKey];
      if (!deployments || Object.keys(deployments).length === 0) {
        fail(`${dir}/${fname}: matrix 에 router '${routerKey}' deployments 없음.`);
      }

      // --- 주소별 chain group ---
      const byAddr = new Map<string, { addr: string; chains: number[] }>();
      const allChains: number[] = [];
      for (const [cidStr, addrRaw] of Object.entries(deployments)) {
        const cid = Number(cidStr);
        if (!Number.isInteger(cid) || cid < 1) {
          fail(`router '${routerKey}': bad chainId ${cidStr}`);
        }
        if (typeof addrRaw !== "string" || !ADDRESS_RE.test(addrRaw)) {
          fail(`router '${routerKey}' chain ${cidStr}: bad address ${String(addrRaw)}`);
        }
        if (!CHAIN_SLUG[cid]) {
          fail(`router '${routerKey}': chain ${cid} 가 13 target chain 외 (slug 없음).`);
        }
        const key = addrRaw.toLowerCase();
        if (!byAddr.has(key)) byAddr.set(key, { addr: addrRaw, chains: [] });
        byAddr.get(key)!.chains.push(cid);
        allChains.push(cid);
      }
      const globalMin = Math.min(...allChains);

      // --- group 별 manifest emit ---
      const groups = [...byAddr.values()].sort(
        (a, b) => Math.min(...a.chains) - Math.min(...b.chains),
      );
      const outNames: string[] = [];
      for (const g of groups) {
        const chainsSorted = [...g.chains].sort((a, b) => a - b);
        const isCanonical = g.chains.includes(globalMin);
        const suffix = isCanonical ? "" : `-${CHAIN_SLUG[Math.min(...g.chains)]}`;
        const out = JSON.parse(JSON.stringify(manifest)) as Manifest; // deep copy
        out.id = `${idPrefix}/${funcName}${suffix}@${version}`;
        out.match = {
          chain_ids: chainsSorted,
          to: [g.addr],
          selector: manifest.match.selector,
        };
        const outName = `${funcName}${suffix}@${version}.json`;
        writeFileSync(
          join(dirPath, outName),
          JSON.stringify(out, null, 2) + "\n",
          "utf8",
        );
        outNames.push(outName);
        genCount++;
      }
      console.error(
        `[gen] ${dir}/${fname}  (${routerKey})  → ${groups.length}: ${outNames.join(", ")}`,
      );
    }
  }
  console.error(
    `[gen] done — ${srcCount} source manifest → ${genCount} per-address manifest`,
  );
}

main();
