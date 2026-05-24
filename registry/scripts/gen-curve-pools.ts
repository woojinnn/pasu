/**
 * gen-curve-pools.ts
 *
 * ScopeBall Curve registry — pool-coverage 확장 generator (감사 처치 F7).
 *
 * 목적: registry 의 검증된 Curve pool manifest 를 type-template 로 clone 해
 *   curated 한 추가 pool 의 manifest set 을 생성한다. emit 규칙은 발명하지
 *   않는다 — template manifest 의 주소 literal 만 pool 별 값으로 교체.
 *
 * 핵심 원칙(misdecode 방지):
 *   1. registry 에 type-template 이 있는 pool-type 만 다룬다. 없으면 그 pool 은
 *      애초에 CURATED 목록에서 제외(스크립트가 만들지 않음).
 *   2. 한 pool 의 ABI 가 template pool 과 byte-identical 인 것만 채택했다 —
 *      모든 CURATED entry 는 사전에 on-chain 검증(아래)을 통과한 것.
 *   3. 치환은 "old→new 주소 맵"의 단일-패스 string replace 다. emit rule 의
 *      트리 구조(select_from_literal_array / inputTokens[k] / pool.address …)는
 *      1 byte 도 건드리지 않는다 — template 의 주소 배치(역순 포함)가 그대로
 *      보존되고 값만 교체된다.
 *
 * pool-type → template 매핑 (registryId 는 Curve API getPools 로 확인):
 *   - "tricrypto-ng"        ← cryptoswap/tricryptousdc/  (registryId factory-tricrypto,
 *                             impl tricrypto-1; LP token == pool 주소)
 *   - "factory-crvusd-plain" ← stableswap-ng/crvusd-usdc/ (registryId factory-crvusd;
 *                             EIP-1167 minimal proxy, impl
 *                             0x67fe41a94e779ccfa22cff02cc2957dc9c0e4286 —
 *                             crvusd-usdc/usdt 와 byte-identical; LP token == pool)
 *   ※ registry dir 'stableswap-ng' 는 misnomer — 실제 ABI 는 factory-crvusd plain
 *     pool 의 것이다(add_liquidity 2-arg). 진짜 stableswap-NG(plainstableng impl)
 *     pool 은 add_liquidity 가 3-arg(+receiver) 라 selector 가 달라 호환 불가 —
 *     CURATED 에 넣지 않았다.
 *
 * 사전 검증(CURATED 확정 근거, 1차 출처):
 *   - Curve API  https://api.curve.finance/v1/getPools/big/{ethereum,base}
 *     — registryId / coins / usdTotal / implementation.
 *   - on-chain `coins(uint256)` (selector 0xc6610657) — Ethereum
 *     https://ethereum-rpc.publicnode.com, Base https://base-rpc.publicnode.com:
 *     pool 의 실 coin 주소를 직접 읽어 API 와 cross-check, coin 수 확정.
 *   - on-chain `eth_getCode` — EIP-1167 proxy 면 impl 로 follow 후, template 의
 *     6 selector 가 전부 bytecode 에 존재하는지 확인(ABI 동일성 실측).
 *   → 이 검증을 통과하지 못한 pool 은 SKIPPED 에 사유와 함께 기록.
 *
 * 멱등 — 한 pool 의 출력 dir 6 파일을 매 실행 덮어쓴다. 재실행 안전.
 *   (단 CURATED 에서 pool 을 빼도 옛 dir 은 남는다 — 그 경우 수동 rm.)
 *
 * 실행:  cd registry && npx tsx scripts/gen-curve-pools.ts
 *        이어서  npm run build  (index 재생성·schema·asset-address 검증)
 *               npx tsx scripts/audit-addresses.ts  (새 주소 on-chain 존재검증)
 */

import { mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REGISTRY_ROOT = resolve(HERE, "..");
const CURVE_DIR = join(REGISTRY_ROOT, "manifests", "curve");

const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

// ---------------------------------------------------------------------------
// Template 정의 — registry 의 검증된 pool manifest set.
//
//   files     : template dir 의 6 manifest 파일명. 출력도 동일 파일명.
//   templateDir: manifests/curve/ 하위 상대경로.
//   oldPool   : template pool/LP 주소(소문자). NG·factory-crvusd 는 LP == pool.
//   oldCoins  : template pool 의 coins[] (on-chain coins() 순서, 소문자).
//   idPoolSeg : template id 의 pool segment (치환 대상). e.g. "tricryptousdc".
//   idPrefix  : template id 의 pool 앞 prefix. e.g. "curve/cryptoswap".
//   outSubdir : 출력 dir 의 manifests/curve/ 하위 prefix. e.g. "cryptoswap".
// ---------------------------------------------------------------------------

interface TemplateDef {
  files: string[];
  templateDir: string;
  oldPool: string;
  oldCoins: string[];
  idPoolSeg: string;
  idPrefix: string;
  outSubdir: string;
}

const TEMPLATES: Record<string, TemplateDef> = {
  // tricrypto-NG (factory-tricrypto, impl tricrypto-1). LP token == pool.
  "tricrypto-ng": {
    files: [
      "addLiquidity-v2@1.0.0.json",
      "exchange-noEth@1.0.0.json",
      "exchange-receiver@1.0.0.json",
      "exchange-v2@1.0.0.json",
      "removeLiquidity-v2@1.0.0.json",
      "removeLiquidityOneCoin-v2@1.0.0.json",
    ],
    templateDir: "cryptoswap/tricryptousdc",
    oldPool: "0x7f86bf177dd4f3494b841a37e810a34dd56c829b",
    oldCoins: [
      "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // USDC
      "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599", // WBTC
      "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // WETH
    ],
    idPoolSeg: "tricryptousdc",
    idPrefix: "curve/cryptoswap",
    outSubdir: "cryptoswap",
  },
  // factory-crvusd plain 2-coin pool. registry dir name 'stableswap-ng' 은
  // misnomer지만 ABI 는 이 template 이 진실. LP token == pool.
  "factory-crvusd-plain": {
    files: [
      "addLiquidity-ng@1.0.0.json",
      "exchange-ng@1.0.0.json",
      "exchange-noReceiver@1.0.0.json",
      "removeLiquidity-ng@1.0.0.json",
      "removeLiquidityImbalance-ng@1.0.0.json",
      "removeLiquidityOneCoin-ng@1.0.0.json",
    ],
    templateDir: "stableswap-ng/crvusd-usdc",
    oldPool: "0x4dece678ceceb27446b35c672dc7d61f30bad69e",
    oldCoins: [
      "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // USDC
      "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e", // crvUSD
    ],
    idPoolSeg: "crvusd-usdc",
    idPrefix: "curve/stableswap-ng",
    outSubdir: "stableswap-ng",
  },
};

// ---------------------------------------------------------------------------
// CURATED — 추가할 pool. 전부 on-chain 검증 통과분(2026-05-22 측정).
//
//   chainId / address : on-chain pool 주소.
//   poolName          : 출력 dir / id 의 pool segment (소문자, 영숫자+하이픈).
//   template          : TEMPLATES 키.
//   coins             : on-chain coins() 가 반환한 coin 주소(index 순서, 소문자).
//                       generator 가 oldCoins[k] → coins[k] 로 치환.
//   tvlUsd            : Curve API usdTotal (참고용).
// ---------------------------------------------------------------------------

interface CuratedPool {
  chainId: number;
  address: string;
  poolName: string;
  template: keyof typeof TEMPLATES;
  coins: string[];
  tvlUsd: number;
}

const CURATED: CuratedPool[] = [
  // --- tricrypto-NG (Ethereum factory-tricrypto, impl tricrypto-1) ---------
  {
    chainId: 1,
    address: "0xf5f5b97624542d72a9e06f04804bf81baa15e2b4",
    poolName: "crvusdtwbtcweth",
    template: "tricrypto-ng",
    coins: [
      "0xdac17f958d2ee523a2206206994597c13d831ec7", // USDT
      "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599", // WBTC
      "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // WETH
    ],
    tvlUsd: 13_774_000,
  },
  {
    chainId: 1,
    address: "0x2889302a794da87fbf1d6db415c1492194663d13",
    poolName: "crvcrvusdtbtcwsteth",
    template: "tricrypto-ng",
    coins: [
      "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e", // crvUSD
      "0x18084fba666a33d37592fa2633fd49a74dd93a88", // tBTC
      "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0", // wstETH
    ],
    tvlUsd: 3_154_000,
  },
  {
    chainId: 1,
    address: "0xdb6925ea42897ca786a045b252d95aa7370f44b4",
    poolName: "crvrsreusdeth",
    template: "tricrypto-ng",
    coins: [
      "0xe72b141df173b999ae7c1adcbf60cc9833ce56a8", // ETH+
      "0xa0d69e286b938e21cbf7e51d71f6a4c8918f482f", // eUSD
      "0x320623b8e4ff03373931769a31fc52a4e78b5d70", // RSR
    ],
    tvlUsd: 2_550_000,
  },
  {
    chainId: 1,
    address: "0x4ebdf703948ddcea3b11f675b4d1fba9d2414a14",
    poolName: "crvusdethcrv",
    template: "tricrypto-ng",
    coins: [
      "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e", // crvUSD
      "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // WETH
      "0xd533a949740bb3306d119cc777fa900ba034cd52", // CRV
    ],
    tvlUsd: 2_360_000,
  },
  {
    chainId: 1,
    address: "0x8a4f252812dff2a8636e4f7eb249d8fc2e3bd77f",
    poolName: "btcghoeth",
    template: "tricrypto-ng",
    coins: [
      "0x40d16fc0246ad3160ccc09b8d0d3a2cd28ae6c2f", // GHO
      "0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf", // cbBTC
      "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // WETH
    ],
    tvlUsd: 2_039_000,
  },
  {
    chainId: 1,
    address: "0x2570f1bd5d2735314fc102eb12fc1afe9e6e7193",
    poolName: "trylsd",
    template: "tricrypto-ng",
    coins: [
      "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0", // wstETH
      "0xae78736cd615f374d3085123a210448e74fc6393", // rETH
      "0xac3e018457b222d93114458476f3e3416abbe38f", // sfrxETH
    ],
    tvlUsd: 261_000,
  },
  {
    chainId: 1,
    address: "0xdae4135dac6c62937728d145f8048b2bab2ce55c",
    poolName: "3pros",
    template: "tricrypto-ng",
    coins: [
      "0xbe1936a67f503e0eaf2434b0cf9f4e3d7100008a", // PROS
      "0xdac17f958d2ee523a2206206994597c13d831ec7", // USDT
      "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599", // WBTC
    ],
    tvlUsd: 140_000,
  },
  // --- tricrypto-NG (Base factory-tricrypto) — 사용자 명시 대상 ------------
  {
    chainId: 8453,
    address: "0x6e53131f68a034873b6bfa15502af094ef0c5854",
    poolName: "tricrypto-base",
    template: "tricrypto-ng",
    coins: [
      "0x417ac0e078398c154edfadd9ef675d30be60af93", // crvUSD
      "0x236aa50979d5f3de3bd1eeb40e81137f22ab794b", // tBTC
      "0x4200000000000000000000000000000000000006", // WETH
    ],
    tvlUsd: 595_000,
  },
  // --- factory-crvusd plain 2-coin (Ethereum) -----------------------------
  {
    chainId: 1,
    address: "0xb7ecb2aa52aa64a717180e030241bc75cd946726",
    poolName: "2btc",
    template: "factory-crvusd-plain",
    coins: [
      "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599", // WBTC
      "0x18084fba666a33d37592fa2633fd49a74dd93a88", // tBTC
    ],
    tvlUsd: 10_504_000,
  },
  {
    chainId: 1,
    address: "0x9c3b46c0ceb5b9e304fcd6d88fc50f7dd24b31bc",
    poolName: "frxeth-ng",
    template: "factory-crvusd-plain",
    coins: [
      "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // WETH
      "0x5e8422345238f34275888049021821e8e08caa1f", // frxETH
    ],
    tvlUsd: 8_641_000,
  },
  {
    chainId: 1,
    address: "0x0cd6f267b2086bea681e922e19d40512511be538",
    poolName: "crvusdfrax",
    template: "factory-crvusd-plain",
    coins: [
      "0x853d955acef822db058eb8505911ed77f175b99e", // FRAX
      "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e", // crvUSD
    ],
    tvlUsd: 234_000,
  },
];

// ---------------------------------------------------------------------------
// SKIPPED — 검토했으나 제외한 pool. 보고/추적용. generator 는 만들지 않는다.
// ---------------------------------------------------------------------------

const SKIPPED: { chainId: number; address: string; name: string; reason: string }[] = [
  {
    chainId: 1,
    address: "0x66da369fc5dbba0774da70546bd20f2b242cd34d",
    name: "crvDBRINV (factory-tricrypto, $896K)",
    reason:
      "tricryptousdc template 6 selector 중 0xce7d6503(exchange-receiver)·0x394747c5" +
      "(exchange use_eth) 가 bytecode 에 없음. codeLen 21585 (다른 tricrypto-NG 21789) " +
      "— 다른 implementation. 추측 emit 회피.",
  },
  {
    chainId: 8453,
    address: "0x11c1fbd4b3de66bc0565779b35171a6cf3e71f59",
    name: "cbETH/WETH cbeth-f (Base factory-crypto, $6M)",
    reason:
      "factory-crypto = 2-coin cryptoswap(twocrypto). registry 의 cryptoswap " +
      "template(tricrypto2·tricryptousdc) 은 둘 다 3-coin — 2-coin cryptoswap " +
      "template 부재.",
  },
  {
    chainId: 8453,
    address: "0x302a94e3c28c290eaf2a4605fc52e11eb915f378",
    name: "superOETHb/WETH sOETH/WETH (Base factory-stable-ng, $17M)",
    reason:
      "factory-stable-ng = 진짜 stableswap-NG(plainstableng impl). NG 의 add_liquidity " +
      "는 3-arg(uint256[],uint256,address) 라 selector 가 registry 의 factory-crvusd " +
      "plain template(2-arg add_liquidity, dir 명 'stableswap-ng' 은 misnomer)과 다름 " +
      "— stableswap-NG template 부재.",
  },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function fail(msg: string): never {
  console.error(`[gen-curve] ERROR: ${msg}`);
  process.exit(1);
}

function exists(p: string): boolean {
  try {
    statSync(p);
    return true;
  } catch {
    return false;
  }
}

/** template manifest 의 모든 old 주소를 new 주소로 단일-패스 치환. */
function remapAddresses(raw: string, addrMap: Map<string, string>): string {
  const olds = [...addrMap.keys()];
  // 주소는 고정 40-hex — prefix 충돌 없음. case-insensitive 매칭, single-pass.
  const re = new RegExp(olds.join("|"), "gi");
  return raw.replace(re, (m) => {
    const v = addrMap.get(m.toLowerCase());
    if (v === undefined) fail(`치환 맵 누락: ${m}`);
    return v;
  });
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

function main(): void {
  // --- CURATED sanity ---
  const seenName = new Set<string>();
  for (const p of CURATED) {
    const tmpl = TEMPLATES[p.template];
    if (!tmpl) fail(`pool ${p.poolName}: 알 수 없는 template '${p.template}'`);
    if (!ADDRESS_RE.test(p.address)) fail(`pool ${p.poolName}: bad address ${p.address}`);
    if (p.address !== p.address.toLowerCase()) {
      fail(`pool ${p.poolName}: address 는 소문자여야 함 (${p.address})`);
    }
    if (p.coins.length !== tmpl.oldCoins.length) {
      fail(
        `pool ${p.poolName}: coins ${p.coins.length}개 — template '${p.template}' 은 ` +
          `${tmpl.oldCoins.length}-coin`,
      );
    }
    for (const c of p.coins) {
      if (!ADDRESS_RE.test(c) || c !== c.toLowerCase()) {
        fail(`pool ${p.poolName}: bad/non-lowercase coin ${c}`);
      }
    }
    const nameKey = `${tmpl.outSubdir}/${p.poolName}`;
    if (seenName.has(nameKey)) fail(`pool name 중복: ${nameKey}`);
    seenName.add(nameKey);
  }

  // --- 생성 ---
  let poolCount = 0;
  let manifestCount = 0;
  const byTemplate: Record<string, number> = {};

  for (const p of CURATED) {
    const tmpl = TEMPLATES[p.template];
    const templateDirAbs = join(CURVE_DIR, tmpl.templateDir);
    const outDirAbs = join(CURVE_DIR, tmpl.outSubdir, p.poolName);

    // old→new 주소 맵: pool/LP(NG·crvusd 는 한 주소) + coin[k].
    const addrMap = new Map<string, string>();
    addrMap.set(tmpl.oldPool, p.address);
    for (let k = 0; k < tmpl.oldCoins.length; k++) {
      addrMap.set(tmpl.oldCoins[k], p.coins[k]);
    }
    if (addrMap.size !== tmpl.oldCoins.length + 1) {
      fail(
        `pool ${p.poolName}: template '${p.template}' 의 old 주소 set 에 중복 — ` +
          `oldPool 이 oldCoins 와 겹침 (1-pass replace 모호).`,
      );
    }

    mkdirSync(outDirAbs, { recursive: true });

    const oldIdPath = `${tmpl.idPrefix}/${tmpl.idPoolSeg}/`;
    const newIdPath = `${tmpl.idPrefix}/${p.poolName}/`;

    for (const fname of tmpl.files) {
      const srcPath = join(templateDirAbs, fname);
      if (!exists(srcPath)) fail(`template manifest 없음: ${srcPath}`);
      const srcRaw = readFileSync(srcPath, "utf8");

      // 1) 주소 literal 치환 (emit rule 트리 무변경).
      let outRaw = remapAddresses(srcRaw, addrMap);

      // 2) parse 후 id / match 만 명시적 재set.
      const manifest = JSON.parse(outRaw) as {
        id: string;
        match: { chain_ids: number[]; to: string[]; selector: string };
        [k: string]: unknown;
      };

      if (!manifest.id.startsWith(oldIdPath)) {
        fail(
          `${tmpl.templateDir}/${fname}: id '${manifest.id}' 가 예상 prefix ` +
            `'${oldIdPath}' 로 시작하지 않음 — template 구조 변경 의심.`,
        );
      }
      manifest.id = newIdPath + manifest.id.slice(oldIdPath.length);
      manifest.match = {
        chain_ids: [p.chainId],
        to: [p.address],
        selector: manifest.match.selector,
      };

      writeFileSync(
        join(outDirAbs, fname),
        JSON.stringify(manifest, null, 2) + "\n",
        "utf8",
      );
      manifestCount++;
    }

    poolCount++;
    byTemplate[p.template] = (byTemplate[p.template] ?? 0) + 1;
    console.error(
      `[gen-curve] ${p.template}  ${tmpl.outSubdir}/${p.poolName}  ` +
        `(chain ${p.chainId}, ${tmpl.files.length} manifest)`,
    );
  }

  console.error("");
  console.error(
    `[gen-curve] done — ${poolCount} pool → ${manifestCount} manifest`,
  );
  for (const [t, n] of Object.entries(byTemplate)) {
    console.error(`[gen-curve]   ${t}: ${n} pool × 6 = ${n * 6} manifest`);
  }
  console.error(`[gen-curve] skipped(미생성) ${SKIPPED.length} pool:`);
  for (const s of SKIPPED) {
    console.error(`[gen-curve]   chain ${s.chainId} ${s.name}\n              ${s.reason}`);
  }
}

main();
