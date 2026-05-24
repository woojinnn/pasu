/**
 * gen-from-sourcify.ts
 *
 * ScopeBall Curve registry — pool-coverage extension generator R1.1 (감사 처치 F7).
 *
 * 본 generator 는 `gen-curve-pools.ts` 의 후속 — 추가 3 pool-type 을 다룬다:
 *   - stableswap-ng-factory (factory-stable-ng, true stableswap-NG)
 *   - twocrypto (factory-twocrypto, 2-coin cryptoswap-NG)
 *   - factory-crypto (old 2-coin cryptoswap pre-NG)
 *
 * 동일한 misdecode-방지 원칙:
 *
 *   1. _template/ 디렉토리의 6개 manifest 가 ABI + emit rule 의 단일 진실.
 *      placeholder 주소 (`0x1111…1111` for pool, `0xaaaa…aaa1`/`aaa2` for coins)
 *      만 per-pool 값으로 단일-패스 string replace 한다. emit rule 트리 구조는
 *      1 byte 도 건드리지 않는다.
 *   2. 각 pool 은 chain 의 RPC `eth_getCode` 로 byte 검증된 것 — 그 bytecode
 *      안에 6 selector 가 전부 존재해야 한다. 부재 selector 가 있는 pool 은
 *      `skip_selectors` 에 명시 (예: lcap-eusd 가 0xb2f9173e missing).
 *   3. 새 pool-type 의 _template 은 manual 작성 — 그 ABI 가 Curve 공식 Vyper
 *      repo (curvefi/{stableswap-ng,twocrypto-ng,curve-crypto-contract}) 의
 *      contract 와 byte-identical 함을 확인한 것.
 *
 * 출처:
 *   - Curve API:  https://api.curve.finance/v1/getPools/{ethereum,base}/{registry_id}
 *   - on-chain `coins(uint256)` + `eth_getCode` cross-check
 *   - selector keccak: `cast keccak '<signature>'` on canonical Vyper sigs
 *
 * 입력:  scripts/curve-pool-targets.json
 * 출력:  manifests/curve/{pool-type}/{pool-name}/*.json (per-pool dir)
 *
 * 멱등 — 매 실행 동일 출력 (byte-identical). _template/ 자체는 skip 대상으로
 * build-index.ts + audit-addresses.ts 가 처리.
 *
 * 실행:  cd registry && npx tsx scripts/gen-from-sourcify.ts
 *        이어서  npm run build  (index 재생성)
 *               npm run verify-addresses  (on-chain 존재검증)
 */

import { mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const REGISTRY_ROOT = resolve(HERE, "..");
const MANIFESTS_DIR = join(REGISTRY_ROOT, "manifests");

const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;
const SELECTOR_RE = /^0x[0-9a-fA-F]{8}$/;

// Template 의 placeholder 주소 — 본 generator 는 이 정확한 lowercase 주소를
// per-pool 값으로 string-replace 한다. _template/*.json 의 모든 erc-asset 주소·
// match.to 가 이 set 안의 것이어야 한다.
const PLACEHOLDER_POOL = "0x1111111111111111111111111111111111111111";
const PLACEHOLDER_COIN: Record<number, string> = {
  0: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa1",
  1: "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa2",
};

// ---------------------------------------------------------------------------
// curve-pool-targets.json types
// ---------------------------------------------------------------------------

interface PoolTypeConfig {
  registry_id: string;
  vyper_repo: string;
  coin_count_fixed: boolean;
  coin_count_used: number;
  _template_dir: string;
  selectors_full: string[];
  out_subdir: string;
}

interface PoolEntry {
  chain: number;
  pool_type: string;
  pool_name: string;
  address: string;
  coins: string[];
  label: string;
  tvl_usd: number;
  skip_selectors?: string[];
  _skip_note?: string;
  _note?: string;
}

interface PoolTargetsFile {
  _meta?: unknown;
  pool_types: Record<string, PoolTypeConfig>;
  pools: PoolEntry[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function fail(msg: string): never {
  console.error(`[gen-from-sourcify] ERROR: ${msg}`);
  process.exit(1);
}

function loadJson<T>(path: string): T {
  return JSON.parse(readFileSync(path, "utf8")) as T;
}

function listTemplateFiles(dir: string): string[] {
  const abs = join(REGISTRY_ROOT, dir);
  try {
    return readdirSync(abs)
      .filter((f) => f.endsWith(".json"))
      .sort();
  } catch (e) {
    fail(`template dir read 실패 ${abs}: ${(e as Error).message}`);
  }
}

interface TemplateBundle {
  type: string;
  id: string;
  match: { chain_ids: number[]; to: string[]; selector: string };
  [k: string]: unknown;
}

function loadTemplate(dir: string, fname: string): TemplateBundle {
  const abs = join(REGISTRY_ROOT, dir, fname);
  const raw = readFileSync(abs, "utf8");
  const t = JSON.parse(raw) as TemplateBundle;
  if (t.type !== "adapter_function") {
    fail(`${fname}: type !== "adapter_function"`);
  }
  if (!SELECTOR_RE.test(t.match.selector)) {
    fail(`${fname}: bad selector "${t.match.selector}"`);
  }
  if (t.match.to.length !== 1 || t.match.to[0].toLowerCase() !== PLACEHOLDER_POOL) {
    fail(
      `${fname}: match.to must be exactly ["${PLACEHOLDER_POOL}"], got ${JSON.stringify(t.match.to)}`,
    );
  }
  if (t.match.chain_ids.length !== 1 || t.match.chain_ids[0] !== 1) {
    fail(`${fname}: match.chain_ids must be [1] in template, got ${JSON.stringify(t.match.chain_ids)}`);
  }
  // id segment check — must contain "_template/" so we can replace it
  if (!t.id.includes("/_template/")) {
    fail(`${fname}: id "${t.id}" must contain "/_template/" segment`);
  }
  return t;
}

/**
 * Per-pool address remap on raw bundle JSON string. case-insensitive single-pass.
 * Generator builds map: {placeholder-pool → real-pool, placeholder-coin[k] → real-coin[k]}.
 * Returns new JSON string. emit rule tree structure preserved 1 byte.
 */
function remapAddresses(raw: string, addrMap: Map<string, string>): string {
  const olds = [...addrMap.keys()];
  // 모든 placeholder 주소는 40-hex, prefix 충돌 없음. case-insensitive 매칭.
  const re = new RegExp(olds.join("|"), "gi");
  return raw.replace(re, (m) => {
    const v = addrMap.get(m.toLowerCase());
    if (v === undefined) fail(`치환 맵 누락: ${m}`);
    return v;
  });
}

function chainName(c: number): string {
  return { 1: "Ethereum", 8453: "Base" }[c] ?? `chain-${c}`;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

function main(): void {
  const targetsPath = join(HERE, "curve-pool-targets.json");
  const targets = loadJson<PoolTargetsFile>(targetsPath);

  if (!targets.pool_types || !targets.pools) {
    fail(`curve-pool-targets.json: missing pool_types or pools`);
  }

  // --- Targets sanity ---
  const seenAddr = new Set<string>();
  for (const p of targets.pools) {
    const cfg = targets.pool_types[p.pool_type];
    if (!cfg) fail(`pool ${p.pool_name}: unknown pool_type "${p.pool_type}"`);
    if (!ADDRESS_RE.test(p.address) || p.address !== p.address.toLowerCase()) {
      fail(`pool ${p.pool_name}: bad/non-lowercase address ${p.address}`);
    }
    const key = `${p.chain}:${p.address}`;
    if (seenAddr.has(key)) fail(`duplicate (chain, address): ${key}`);
    seenAddr.add(key);
    if (p.coins.length !== cfg.coin_count_used) {
      fail(
        `pool ${p.pool_name}: ${p.coins.length} coin(s) but pool_type "${p.pool_type}" uses ${cfg.coin_count_used}-coin template`,
      );
    }
    for (const c of p.coins) {
      if (!ADDRESS_RE.test(c) || c !== c.toLowerCase()) {
        fail(`pool ${p.pool_name}: bad/non-lowercase coin ${c}`);
      }
    }
    if (p.skip_selectors) {
      for (const s of p.skip_selectors) {
        if (!SELECTOR_RE.test(s)) fail(`pool ${p.pool_name}: bad skip_selector ${s}`);
        if (!cfg.selectors_full.includes(s)) {
          fail(`pool ${p.pool_name}: skip_selector ${s} not in pool_type selectors_full`);
        }
      }
    }
  }

  // --- Generate per pool ---
  const byPoolType: Record<string, number> = {};
  const byChain: Record<number, number> = {};
  let poolCount = 0;
  let manifestCount = 0;
  let skipCount = 0;

  for (const p of targets.pools) {
    const cfg = targets.pool_types[p.pool_type];
    const templateFiles = listTemplateFiles(cfg._template_dir);
    if (templateFiles.length !== 6) {
      fail(
        `pool_type ${p.pool_type}: template dir has ${templateFiles.length} files, expected 6`,
      );
    }

    const outDirAbs = join(MANIFESTS_DIR, "curve", cfg.out_subdir, p.pool_name);
    mkdirSync(outDirAbs, { recursive: true });

    // address remap: 1 pool placeholder + N coin placeholders.
    const addrMap = new Map<string, string>();
    addrMap.set(PLACEHOLDER_POOL, p.address);
    for (let k = 0; k < p.coins.length; k++) {
      const ph = PLACEHOLDER_COIN[k];
      if (!ph) fail(`pool ${p.pool_name}: no placeholder defined for coin index ${k}`);
      addrMap.set(ph, p.coins[k]);
    }
    if (addrMap.size !== 1 + p.coins.length) {
      fail(`pool ${p.pool_name}: addrMap size mismatch — placeholder overlap?`);
    }

    let manifestsForPool = 0;
    let skipsForPool = 0;
    for (const fname of templateFiles) {
      const t = loadTemplate(cfg._template_dir, fname);

      if (p.skip_selectors?.includes(t.match.selector)) {
        skipCount++;
        skipsForPool++;
        console.error(
          `[gen-from-sourcify] SKIP ${p.pool_type}/${p.pool_name}/${fname}  ` +
            `selector ${t.match.selector} on bytecode-missing pool (note: ${p._skip_note ?? "n/a"})`,
        );
        continue;
      }

      // template raw JSON → address remap.
      const rawTemplate = readFileSync(
        join(REGISTRY_ROOT, cfg._template_dir, fname),
        "utf8",
      );
      let outRaw = remapAddresses(rawTemplate, addrMap);

      // parse → set id segment + chain_id, then re-stringify.
      const bundle = JSON.parse(outRaw) as TemplateBundle;
      const newId = bundle.id.replace("/_template/", `/${p.pool_name}/`);
      if (newId === bundle.id) {
        fail(`${fname}: id replace produced unchanged id "${bundle.id}"`);
      }
      bundle.id = newId;
      bundle.match = {
        chain_ids: [p.chain],
        to: [p.address],
        selector: bundle.match.selector,
      };

      writeFileSync(
        join(outDirAbs, fname),
        JSON.stringify(bundle, null, 2) + "\n",
        "utf8",
      );
      manifestCount++;
      manifestsForPool++;
    }

    poolCount++;
    byPoolType[p.pool_type] = (byPoolType[p.pool_type] ?? 0) + 1;
    byChain[p.chain] = (byChain[p.chain] ?? 0) + 1;
    const skipMarker = skipsForPool > 0 ? ` (skipped ${skipsForPool} selector)` : "";
    console.error(
      `[gen-from-sourcify] ${p.pool_type.padEnd(22)} ${chainName(p.chain).padEnd(8)} ${p.pool_name.padEnd(20)} ${p.address}  ${manifestsForPool} manifest${skipMarker}`,
    );
  }

  console.error("");
  console.error(`[gen-from-sourcify] done — ${poolCount} pool → ${manifestCount} manifest written${skipCount > 0 ? ` (${skipCount} selector skipped)` : ""}`);
  console.error(`[gen-from-sourcify] by pool_type:`);
  for (const [t, n] of Object.entries(byPoolType)) {
    const cfg = targets.pool_types[t];
    const expectedManifests = n * cfg.selectors_full.length;
    console.error(`[gen-from-sourcify]   ${t.padEnd(22)}: ${n} pool × ${cfg.selectors_full.length} = ${expectedManifests} (target before skips)`);
  }
  console.error(`[gen-from-sourcify] by chain:`);
  for (const [c, n] of Object.entries(byChain).sort()) {
    console.error(`[gen-from-sourcify]   ${chainName(Number(c)).padEnd(10)} (${c}): ${n} pool`);
  }
}

main();
