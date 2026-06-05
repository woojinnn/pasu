/**
 * Curve protocol source resolvers.
 *
 * Curve sources are intentionally file-backed by checked-in P0 universe
 * artifacts. The public Curve API already feeds those universes during P0;
 * build-index should consume the reviewed artifacts instead of performing a
 * second live fetch with a different boundary.
 */

import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

import type {
  Hex,
  ProtocolResolvedAddress,
  ProtocolResolver,
  ProtocolSourceKind,
  ResolverOpts,
} from "./types.ts";

const REGISTRY_ROOT = process.env.BUILD_INDEX_REGISTRY_ROOT
  ? resolve(process.env.BUILD_INDEX_REGISTRY_ROOT)
  : resolve(new URL("../..", import.meta.url).pathname);
const POOL_UNIVERSE_PATH = join(REGISTRY_ROOT, "surface", "curve", "_pool_universe.json");
const GAUGE_UNIVERSE_PATH = join(REGISTRY_ROOT, "surface", "curve", "_gauge_universe.json");
const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;
const ZERO_ADDRESS_RE = /^0x0{40}$/i;
const STABLE_NG_COIN_COUNTS = [2, 3, 4, 5, 6, 7, 8] as const;
const BASE_STABLE_NG_COIN_COUNTS = [2, 3, 4, 5, 6, 8] as const;
type StableNgCoinCount = (typeof STABLE_NG_COIN_COUNTS)[number];
type StableNgChain = "mainnet" | "base";

interface StableNgSourceSpec {
  readonly source: ProtocolSourceKind;
  readonly expectedChainId: 1 | 8453;
  readonly coinCount: StableNgCoinCount;
}

interface CurvePoolSourceSpec {
  readonly source: ProtocolSourceKind;
  readonly expectedChainId: 1 | 8453;
  readonly family: string;
  readonly curveIdPrefix?: string;
  readonly coinCount: number;
}

interface CurvePoolCandidate {
  chainId?: number;
  chain_id?: number;
  address?: string;
  curve_id?: string;
  name?: string;
  symbol?: string;
  lpTokenAddress?: string;
  lp_token_address?: string;
  gaugeAddress?: string;
  gauge_address?: string;
  coins?: string[];
  families?: string[];
  decision?: string;
}

interface CurvePoolUniverse {
  candidates?: CurvePoolCandidate[];
}

interface CurveGaugeCandidate {
  chainId?: number;
  chain_id?: number;
  address?: string;
  decision?: string;
}

interface CurveGaugeUniverse {
  candidates?: CurveGaugeCandidate[];
}

function loadJson<T>(path: string): T {
  return JSON.parse(readFileSync(path, "utf8")) as T;
}

function loadPoolUniverse(): CurvePoolCandidate[] {
  const parsed = loadJson<CurvePoolUniverse | CurvePoolCandidate[]>(POOL_UNIVERSE_PATH);
  if (Array.isArray(parsed)) return parsed;
  if (Array.isArray(parsed.candidates)) return parsed.candidates;
  throw new Error(`curve: ${POOL_UNIVERSE_PATH} has no candidates[]`);
}

function loadGaugeUniverse(): CurveGaugeCandidate[] {
  if (existsSync(GAUGE_UNIVERSE_PATH)) {
    const parsed = loadJson<CurveGaugeUniverse | CurveGaugeCandidate[]>(GAUGE_UNIVERSE_PATH);
    if (Array.isArray(parsed)) return parsed;
    if (Array.isArray(parsed.candidates)) return parsed.candidates;
    throw new Error(`curve:gauges: ${GAUGE_UNIVERSE_PATH} has no candidates[]`);
  }

  const candidates = loadPoolUniverse();
  return candidates
    .map((candidate) => ({
      chainId: candidate.chainId ?? candidate.chain_id,
      address: candidate.gaugeAddress ?? candidate.gauge_address,
      decision: candidate.decision,
    }))
    .filter((candidate) => candidate.address);
}

function gaugeAddressesFor(chainId: number): Hex[] {
  const out = new Set<Hex>();
  for (const candidate of loadGaugeUniverse()) {
    const candidateChain = candidate.chainId ?? candidate.chain_id;
    if (candidateChain !== chainId) continue;
    if (candidate.decision && candidate.decision !== "cover") continue;
    const gauge = candidate.address;
    if (!gauge || !ADDRESS_RE.test(gauge) || ZERO_ADDRESS_RE.test(gauge)) continue;
    out.add(gauge.toLowerCase() as Hex);
  }
  return [...out].sort();
}

function candidateAddress(candidate: CurvePoolCandidate): Hex | undefined {
  const address = candidate.address?.toLowerCase();
  if (!address || !ADDRESS_RE.test(address) || ZERO_ADDRESS_RE.test(address)) return undefined;
  return address as Hex;
}

function activeCoins(candidate: CurvePoolCandidate): Hex[] {
  return (candidate.coins ?? [])
    .map((coin) => coin.toLowerCase())
    .filter((coin) => ADDRESS_RE.test(coin) && !ZERO_ADDRESS_RE.test(coin)) as Hex[];
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}

function stableNgSourceKind(coinCount: StableNgCoinCount, chain: StableNgChain): ProtocolSourceKind {
  return `curve:factory_stable_ng_${coinCount}coin_${chain}` as ProtocolSourceKind;
}

function poolEntries(
  chainId: number,
  family: string,
  coinCount: number,
  curveIdPrefix?: string,
): ProtocolResolvedAddress[] {
  const out: ProtocolResolvedAddress[] = [];
  for (const candidate of loadPoolUniverse()) {
    const candidateChain = candidate.chainId ?? candidate.chain_id;
    if (candidateChain !== chainId) continue;
    if (candidate.decision && candidate.decision !== "cover") continue;
    const curveId = candidate.curve_id ?? candidateAddress(candidate);
    const familyMatches = (candidate.families ?? []).includes(family);
    const prefixMatches = curveIdPrefix ? String(curveId).startsWith(curveIdPrefix) : false;
    if (curveIdPrefix ? !prefixMatches : !familyMatches) continue;
    const address = candidateAddress(candidate);
    if (!address) continue;
    const coins = activeCoins(candidate);
    if (coins.length !== coinCount) continue;
    const resolvedCurveId = candidate.curve_id ?? address;
    const name = candidate.name ?? resolvedCurveId;
    const symbol = candidate.symbol ?? resolvedCurveId;
    const lpToken = (candidate.lpTokenAddress ?? candidate.lp_token_address ?? address).toLowerCase();
    if (!ADDRESS_RE.test(lpToken) || ZERO_ADDRESS_RE.test(lpToken)) continue;
    const suffix = `${chainId}-${slugify(resolvedCurveId)}-${address.slice(2, 10)}`;
    out.push({
      address,
      id_suffix: suffix,
      context: {
        curve_id: resolvedCurveId,
        pool_name: name,
        symbol,
        lp_token: lpToken,
        coins,
        n_coins: coinCount,
      },
    });
  }
  return out.sort((a, b) => a.address.localeCompare(b.address));
}

export const gaugesResolver: ProtocolResolver = {
  source: "curve:gauges",
  async resolve(chainId: number, _opts: ResolverOpts): Promise<Hex[]> {
    return gaugeAddressesFor(chainId);
  },
};

function makeStableNgResolver(spec: StableNgSourceSpec): ProtocolResolver {
  return makePoolResolver({
    source: spec.source,
    expectedChainId: spec.expectedChainId,
    family: "factory-stable-ng",
    coinCount: spec.coinCount,
  });
}

function makePoolResolver(spec: CurvePoolSourceSpec): ProtocolResolver {
  return {
    source: spec.source,
    async resolve(chainId: number, _opts: ResolverOpts): Promise<Hex[]> {
      if (chainId !== spec.expectedChainId) return [];
      return poolEntries(chainId, spec.family, spec.coinCount, spec.curveIdPrefix).map(
        (entry) => entry.address,
      );
    },
    async resolveWithContext(
      chainId: number,
      _opts: ResolverOpts,
    ): Promise<ProtocolResolvedAddress[]> {
      if (chainId !== spec.expectedChainId) return [];
      return poolEntries(chainId, spec.family, spec.coinCount, spec.curveIdPrefix);
    },
  };
}

export const stableNgResolvers: ProtocolResolver[] = [
  ...STABLE_NG_COIN_COUNTS.map((coinCount) =>
    makeStableNgResolver({
      source: stableNgSourceKind(coinCount, "mainnet"),
      expectedChainId: 1,
      coinCount,
    }),
  ),
  ...BASE_STABLE_NG_COIN_COUNTS.map((coinCount) =>
    makeStableNgResolver({
      source: stableNgSourceKind(coinCount, "base"),
      expectedChainId: 8453,
      coinCount,
    }),
  ),
];

export const factoryV2Resolvers: ProtocolResolver[] = [
  ...([2, 3, 4] as const).flatMap((coinCount) => [
    makePoolResolver({
      source: `curve:factory_v2_${coinCount}coin_mainnet` as ProtocolSourceKind,
      expectedChainId: 1,
      family: "factory",
      curveIdPrefix: "factory-v2-",
      coinCount,
    }),
    makePoolResolver({
      source: `curve:factory_v2_${coinCount}coin_base` as ProtocolSourceKind,
      expectedChainId: 8453,
      family: "factory",
      curveIdPrefix: "factory-v2-",
      coinCount,
    }),
  ]),
];

export const factoryCrvusdResolver = makePoolResolver({
  source: "curve:factory_crvusd_2coin_mainnet",
  expectedChainId: 1,
  family: "factory-crvusd",
  curveIdPrefix: "factory-crvusd-",
  coinCount: 2,
});

export const factoryCryptoResolver = makePoolResolver({
  source: "curve:factory_crypto_mainnet",
  expectedChainId: 1,
  family: "factory-crypto",
  curveIdPrefix: "factory-crypto-",
  coinCount: 2,
});

export const factoryCryptoBaseResolver = makePoolResolver({
  source: "curve:factory_crypto_base",
  expectedChainId: 8453,
  family: "factory-crypto",
  curveIdPrefix: "factory-crypto-",
  coinCount: 2,
});

export const factoryTricryptoResolver = makePoolResolver({
  source: "curve:factory_tricrypto_mainnet",
  expectedChainId: 1,
  family: "factory-tricrypto",
  curveIdPrefix: "factory-tricrypto-",
  coinCount: 3,
});

export const factoryTricryptoBaseResolver = makePoolResolver({
  source: "curve:factory_tricrypto_base",
  expectedChainId: 8453,
  family: "factory-tricrypto",
  curveIdPrefix: "factory-tricrypto-",
  coinCount: 3,
});

export const factoryTwocryptoResolver = makePoolResolver({
  source: "curve:factory_twocrypto_mainnet",
  expectedChainId: 1,
  family: "factory-twocrypto",
  curveIdPrefix: "factory-twocrypto-",
  coinCount: 2,
});

export const factoryTwocryptoBaseResolver = makePoolResolver({
  source: "curve:factory_twocrypto_base",
  expectedChainId: 8453,
  family: "factory-twocrypto",
  curveIdPrefix: "factory-twocrypto-",
  coinCount: 2,
});
