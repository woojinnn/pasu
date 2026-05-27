/**
 * gen-curve-pools.ts — ScopeBall Adapter Registry v2
 *
 * Curve per-pool manifest generator. Reads `_template/<func>@<v>.json`
 * placeholders + a curated pool list and string-replaces address/chainId
 * literals to produce per-pool manifest sets (1 pool → N manifest files).
 *
 * 디자인 원칙 (legacy gen-curve-pools.ts F7 audit 처치의 v2 재작성):
 *   1. emit rule 발명 금지 — `_template/` manifest 의 트리는 1 byte 도
 *      건드리지 않는다. placeholder 문자열만 단일-패스 string replace.
 *   2. ABI 동일성 — 각 curated pool 은 해당 poolType 의 template ABI 와
 *      byte-identical 한 ABI 여야 한다. 사전 on-chain 검증을 통과한 것만
 *      CURATED entry 로 채택.
 *   3. coin 수 일치 — template 의 coin slot 수 (`coinCount`) 와 pool 의
 *      coin 수가 일치해야 한다. mismatch = fail-loud, 생성 안 함.
 *   4. 멱등 — 한 pool 의 출력 dir N 파일을 매 실행 덮어쓴다. 재실행 안전.
 *   5. CURATED 외 pool 은 만들지 않는다 — pool 추가 의도가 명확히 등재된
 *      것만 작성. 검토 후 제외한 pool 은 SKIPPED 에 사유와 함께 기록.
 *
 * placeholder spec:
 *   __CHAIN_ID__     — string chainId (1, 8453)
 *   __POOL_ADDRESS__ — pool 주소 (소문자 hex). pool.address / `match.to` /
 *                      inputLp.asset.address / outputLp.asset.address
 *                      (modern NG 의 LP token == pool address 인 케이스)
 *                      및 emit rule 의 literal 모두 일치
 *   __LP_TOKEN__     — LP token 주소 (소문자 hex). 보통 == __POOL_ADDRESS__,
 *                      단 frxETH 등 일부 pool 은 다른 주소 (P1-6 audit fix)
 *   __COIN0__        — pool.coins(0)
 *   __COIN1__        — pool.coins(1)
 *   __COIN2__        — pool.coins(2)  (3-coin pool 만)
 *
 * 실행:
 *   $ cd registry
 *   $ npx tsx scripts/gen-curve-pools.ts
 *   $ npm run build      (index 재생성 + sha256 + JCS canonicalize)
 *
 * 1차 출처 (CURATED 의 사전 검증 근거):
 *   - Curve API   https://api.curve.finance/v1/getPools/big/{ethereum,base}
 *                  — registryId / coins / implementation
 *   - on-chain    coins(uint256) selector 0xc6610657 via
 *                  https://ethereum-rpc.publicnode.com /
 *                  https://base-rpc.publicnode.com — coin 주소 cross-check
 *   - on-chain    eth_getCode + EIP-1167 follow — template 의 selector 가
 *                  bytecode 에 존재하는지 ABI 동일성 실측
 *   - Etherscan / BaseScan verified label — pool 주소 cross-check
 *
 * SKIPPED 사유 catalog 는 본 파일 하단 `SKIPPED` 배열 참조.
 */

import {
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REGISTRY_ROOT = resolve(HERE, "..");
const CURVE_DIR = join(REGISTRY_ROOT, "manifests", "curve");
const CURATED_JSON = join(REGISTRY_ROOT, "curve-pools-curated.json");

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type PoolType =
  | "stableswap-ng-factory"
  | "cryptoswap"
  | "twocrypto"
  | "factory-crypto"
  | "crvusd-controller";

interface CuratedPool {
  /** EVM chainId (1 = Ethereum mainnet, 8453 = Base). */
  chainId: 1 | 8453;
  /** Output dir name & manifest id segment (lowercase, [a-z0-9-]). */
  poolName: string;
  /** Pool contract address (lowercase hex). For crvusd-controller this is the
   *  per-collateral Controller address. */
  poolAddress: `0x${string}`;
  /** LP token address (lowercase hex). For modern NG pools LP == pool.
   *  null = crvusd-controller (LLAMMA AMM band state, ERC20 LP 부재). */
  lpToken: `0x${string}` | null;
  /** Which `_template/` to expand. */
  poolType: PoolType;
  /** coins(0..N-1) order from on-chain coins() selector 0xc6610657.
   *  crvusd-controller 의 경우 [collateral] 1 element. */
  coins: `0x${string}`[];
  /** crvusd-controller 의 collateral token address (lowercase hex). 다른 poolType
   *  에서는 미사용. applyPlaceholders 가 `__COLLATERAL_ADDRESS__` 로 substitution. */
  collateral?: `0x${string}`;
  /** TVL USD from Curve API (informational; not validated). */
  tvlUsd?: number;
  /** 1차 출처 cross-check note (Curve API / Etherscan label / verified). */
  source: string;
}

interface SkippedPool {
  chainId: 1 | 8453;
  address: string;
  name: string;
  reason: string;
}

interface TemplateDef {
  /** Relative subdir under `manifests/curve/`. */
  templateDir: string;
  /** Output subdir under `manifests/curve/<outSubdir>/<poolName>/`. */
  outSubdir: string;
  /** N = expected coin count. mismatch → fail-loud.
   *  crvusd-controller 은 1 (collateral 1 element). */
  coinCount: 1 | 2 | 3;
  /** Manifest file basenames inside `_template/`. */
  files: string[];
  /** crvusd-controller 처럼 LP token 부재 + collateral 사용하는 entry 인지. */
  hasCollateral?: boolean;
}

interface CuratedDoc {
  $schema: string;
  version: number;
  generatedAt: string;
  pools: CuratedPool[];
  skipped: SkippedPool[];
}

// ---------------------------------------------------------------------------
// Template registry
// ---------------------------------------------------------------------------

const TEMPLATES: Record<PoolType, TemplateDef> = {
  "stableswap-ng-factory": {
    templateDir: "stableswap-ng-factory/_template",
    outSubdir: "stableswap-ng-factory",
    coinCount: 2,
    files: [
      "exchange@1.0.0.json",
      "exchangeUnderlying@1.0.0.json",
      "addLiquidity@1.0.0.json",
      "removeLiquidity@1.0.0.json",
      "removeLiquidityOneCoin@1.0.0.json",
      "removeLiquidityImbalance@1.0.0.json",
    ],
  },
  cryptoswap: {
    templateDir: "cryptoswap/_template",
    outSubdir: "cryptoswap",
    coinCount: 3,
    files: [
      "exchange@1.0.0.json",
      "exchangeUseEth@1.0.0.json",
      "exchangeReceiver@1.0.0.json",
      "addLiquidity@1.0.0.json",
      "removeLiquidity@1.0.0.json",
      "removeLiquidityOneCoin@1.0.0.json",
    ],
  },
  twocrypto: {
    templateDir: "twocrypto/_template",
    outSubdir: "twocrypto",
    coinCount: 2,
    files: [
      "exchange@1.0.0.json",
      "exchangeReceived@1.0.0.json",
      "addLiquidity@1.0.0.json",
      "removeLiquidity@1.0.0.json",
      "removeLiquidityOneCoin@1.0.0.json",
      "removeLiquidityFixedOut@1.0.0.json",
    ],
  },
  "factory-crypto": {
    templateDir: "factory-crypto/_template",
    outSubdir: "factory-crypto",
    coinCount: 2,
    files: [
      "exchange@1.0.0.json",
      "exchangeUseEth@1.0.0.json",
      "exchangeReceiver@1.0.0.json",
      "addLiquidity@1.0.0.json",
      "removeLiquidity@1.0.0.json",
      "removeLiquidityOneCoin@1.0.0.json",
    ],
  },
  "crvusd-controller": {
    templateDir: "crvusd-controller/_template",
    outSubdir: "crvusd-controller",
    coinCount: 1,
    files: [
      "createLoan@1.0.0.json",
      "borrowMore@1.0.0.json",
      "repay@1.0.0.json",
      "liquidate@1.0.0.json",
      "addCollateral@1.0.0.json",
      "removeCollateral@1.0.0.json",
    ],
    hasCollateral: true,
  },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const ADDRESS_RE = /^0x[0-9a-f]{40}$/;

function fail(msg: string): never {
  console.error(`[gen-curve] ERROR: ${msg}`);
  process.exit(1);
}

function safeExists(p: string): boolean {
  try {
    statSync(p);
    return true;
  } catch {
    return false;
  }
}

function ensureLowercaseAddress(label: string, value: string): void {
  if (!ADDRESS_RE.test(value)) {
    fail(`${label}: bad address — must be 0x + 40 lowercase hex (${value})`);
  }
}

/**
 * Single-pass placeholder replacement. Operates on the raw JSON string to
 * preserve formatting precisely (no JSON parse/re-serialize churn until the
 * id rewrite step).
 */
function applyPlaceholders(raw: string, pool: CuratedPool): string {
  let out = raw;

  out = out.replace(/__CHAIN_ID__/g, String(pool.chainId));
  out = out.replace(/__POOL_ADDRESS__/g, pool.poolAddress);
  // crvusd-controller template 이 `__CONTROLLER_ADDRESS__` placeholder 사용 — controller
  // 가 곧 pool.poolAddress 위치 (per-collateral Controller).
  out = out.replace(/__CONTROLLER_ADDRESS__/g, pool.poolAddress);
  if (pool.lpToken !== null) {
    out = out.replace(/__LP_TOKEN__/g, pool.lpToken);
  }
  if (pool.collateral !== undefined) {
    out = out.replace(/__COLLATERAL_ADDRESS__/g, pool.collateral);
  }

  pool.coins.forEach((coin, i) => {
    const placeholder = new RegExp(`__COIN${i}__`, "g");
    out = out.replace(placeholder, coin);
  });

  return out;
}

/**
 * After placeholder substitution, rewrite `id` from
 *   `curve/<outSubdir>/_template/<file>@<v>`
 * to
 *   `curve/<outSubdir>/<poolName>/<file>@<v>`
 * Done via parse/stringify to avoid touching the rest of the document.
 */
function rewriteIdAndMatchTo(
  raw: string,
  pool: CuratedPool,
  template: TemplateDef,
  fileName: string,
): string {
  let parsed: Record<string, unknown>;
  try {
    parsed = JSON.parse(raw) as Record<string, unknown>;
  } catch (e) {
    fail(
      `${template.outSubdir}/${pool.poolName}/${fileName}: JSON.parse failed after placeholder substitution — ${(e as Error).message}`,
    );
  }

  const id = parsed.id;
  if (typeof id !== "string") {
    fail(
      `${template.outSubdir}/${pool.poolName}/${fileName}: id missing or non-string`,
    );
  }
  const oldPrefix = `curve/${template.outSubdir}/_template/`;
  if (!id.startsWith(oldPrefix)) {
    fail(
      `${template.outSubdir}/${pool.poolName}/${fileName}: id '${id}' lacks expected prefix '${oldPrefix}' — template tampered`,
    );
  }
  parsed.id = `curve/${template.outSubdir}/${pool.poolName}/` + id.slice(oldPrefix.length);

  // Sanity: match.to lowercased — template uses lowercase placeholder, but
  // we re-verify just in case a future hand-edit slipped in mixed case.
  const match = parsed.match as { chain_to_addresses?: Record<string, string[]> } | undefined;
  if (
    !match ||
    !match.chain_to_addresses ||
    !Array.isArray(match.chain_to_addresses[String(pool.chainId)])
  ) {
    fail(
      `${template.outSubdir}/${pool.poolName}/${fileName}: match.chain_to_addresses['${pool.chainId}'] missing after substitution`,
    );
  }
  const addrList = match.chain_to_addresses[String(pool.chainId)];
  for (let i = 0; i < addrList.length; i++) {
    addrList[i] = addrList[i].toLowerCase();
  }

  return JSON.stringify(parsed, null, 2) + "\n";
}

// ---------------------------------------------------------------------------
// Pre-validate — fail-loud on structural issues before any write
// ---------------------------------------------------------------------------

function preValidate(curated: CuratedDoc): void {
  if (!Array.isArray(curated.pools)) {
    fail("curve-pools-curated.json: .pools must be an array");
  }
  const seenName = new Set<string>();

  for (const pool of curated.pools) {
    if (pool.chainId !== 1 && pool.chainId !== 8453) {
      fail(
        `pool ${pool.poolName}: chainId ${pool.chainId} unsupported (mainnet 1 + Base 8453 only)`,
      );
    }
    if (!/^[a-z0-9-]+$/.test(pool.poolName)) {
      fail(
        `pool ${pool.poolName}: poolName must match /^[a-z0-9-]+$/ (got '${pool.poolName}')`,
      );
    }
    ensureLowercaseAddress(`pool ${pool.poolName}.poolAddress`, pool.poolAddress);

    const tmpl = TEMPLATES[pool.poolType];
    if (!tmpl) {
      fail(`pool ${pool.poolName}: unknown poolType '${pool.poolType}'`);
    }

    // lpToken 검증 — crvusd-controller (hasCollateral) 는 null 허용, 그 외는 address 필수
    if (tmpl.hasCollateral) {
      if (pool.lpToken !== null) {
        fail(
          `pool ${pool.poolName}: poolType '${pool.poolType}' requires lpToken=null (LLAMMA AMM, ERC20 LP 부재)`,
        );
      }
      if (pool.collateral === undefined) {
        fail(
          `pool ${pool.poolName}: poolType '${pool.poolType}' requires .collateral field`,
        );
      }
      ensureLowercaseAddress(`pool ${pool.poolName}.collateral`, pool.collateral);
    } else {
      if (pool.lpToken === null) {
        fail(
          `pool ${pool.poolName}: poolType '${pool.poolType}' requires non-null lpToken`,
        );
      }
      ensureLowercaseAddress(`pool ${pool.poolName}.lpToken`, pool.lpToken);
    }

    if (pool.coins.length !== tmpl.coinCount) {
      fail(
        `pool ${pool.poolName}: coins[${pool.coins.length}] != template '${pool.poolType}' coinCount=${tmpl.coinCount}`,
      );
    }
    for (let i = 0; i < pool.coins.length; i++) {
      ensureLowercaseAddress(`pool ${pool.poolName}.coins[${i}]`, pool.coins[i]);
    }

    const nameKey = `${tmpl.outSubdir}/${pool.poolName}`;
    if (seenName.has(nameKey)) {
      fail(`duplicate output dir: ${nameKey}`);
    }
    seenName.add(nameKey);

    // Cross-check templates exist (fail before generating anything)
    const templateDirAbs = join(CURVE_DIR, tmpl.templateDir);
    for (const f of tmpl.files) {
      const p = join(templateDirAbs, f);
      if (!safeExists(p)) {
        fail(`template file missing: ${p}`);
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Generate
// ---------------------------------------------------------------------------

function generatePool(pool: CuratedPool): number {
  const tmpl = TEMPLATES[pool.poolType];
  const templateDirAbs = join(CURVE_DIR, tmpl.templateDir);
  const outDirAbs = join(CURVE_DIR, tmpl.outSubdir, pool.poolName);
  mkdirSync(outDirAbs, { recursive: true });

  let count = 0;
  for (const fname of tmpl.files) {
    const srcPath = join(templateDirAbs, fname);
    const raw = readFileSync(srcPath, "utf8");

    // 1) Single-pass placeholder substitution.
    const substituted = applyPlaceholders(raw, pool);

    // 2) Sanity — no placeholders left unreplaced.
    const stray = substituted.match(/__[A-Z0-9_]+__/g);
    if (stray !== null) {
      fail(
        `${tmpl.outSubdir}/${pool.poolName}/${fname}: unreplaced placeholders ${JSON.stringify(stray)}`,
      );
    }

    // 3) Rewrite id + lowercase to-addresses.
    const final = rewriteIdAndMatchTo(substituted, pool, tmpl, fname);

    writeFileSync(join(outDirAbs, fname), final, "utf8");
    count++;
  }
  return count;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

function main(): void {
  const raw = readFileSync(CURATED_JSON, "utf8");
  const curated = JSON.parse(raw) as CuratedDoc;

  preValidate(curated);

  let poolCount = 0;
  let manifestCount = 0;
  const byPoolType: Record<string, number> = {};

  for (const pool of curated.pools) {
    const tmpl = TEMPLATES[pool.poolType];
    const written = generatePool(pool);

    poolCount++;
    manifestCount += written;
    byPoolType[pool.poolType] = (byPoolType[pool.poolType] ?? 0) + 1;
    console.error(
      `[gen-curve] ${pool.poolType.padEnd(22)} chain ${pool.chainId}  ` +
        `${tmpl.outSubdir}/${pool.poolName}  (${written} manifest)`,
    );
  }

  console.error("");
  console.error(`[gen-curve] done — ${poolCount} pool → ${manifestCount} manifest`);
  for (const [pt, n] of Object.entries(byPoolType)) {
    const t = TEMPLATES[pt as PoolType];
    console.error(`[gen-curve]   ${pt}: ${n} pool × ${t.files.length} = ${n * t.files.length} manifest`);
  }

  if (curated.skipped.length > 0) {
    console.error(`[gen-curve] skipped (not generated) ${curated.skipped.length} pool:`);
    for (const s of curated.skipped) {
      console.error(`[gen-curve]   chain ${s.chainId} ${s.name}`);
      console.error(`[gen-curve]     ${s.reason}`);
    }
  }
}

main();
