/**
 * Uniswap protocol source resolvers.
 *
 * Uniswap factory-child universes are reviewed in
 * `surface/uniswap/_address_universe.json`. Build-index consumes only entries
 * explicitly dispositioned as `cover`; large deferred batches stay counted in
 * the universe artifact without silently becoming routed callkeys.
 */

import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

import type {
  Hex,
  ProtocolResolvedAddress,
  ProtocolResolver,
  ResolverOpts,
} from "./types.ts";

const REGISTRY_ROOT = process.env.BUILD_INDEX_REGISTRY_ROOT
  ? resolve(process.env.BUILD_INDEX_REGISTRY_ROOT)
  : resolve(new URL("../..", import.meta.url).pathname);
const ADDRESS_UNIVERSE_PATH = join(REGISTRY_ROOT, "surface", "uniswap", "_address_universe.json");
const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;
const ZERO_ADDRESS_RE = /^0x0{40}$/i;
const V2_PAIR_BATCH = "uniswap-v2-child-universe-deferred";
const V3_POOL_BATCH = "uniswap-v3-child-universe-deferred";

interface UniswapAddressCandidate {
  chainId?: number;
  chain_id?: number;
  address?: string;
  decision?: string;
  reason?: string;
  batch?: string;
  lp_token_inventory?: {
    token_file?: string;
    status?: string;
    source?: string;
  };
  pool_inventory?: {
    token0?: string;
    token1?: string;
    fee?: number;
    tick_spacing?: number;
    status?: string;
    source?: string;
    query?: string;
  };
}

interface UniswapAddressUniverse {
  candidates?: UniswapAddressCandidate[];
}

function loadUniverse(): UniswapAddressCandidate[] {
  const parsed = JSON.parse(readFileSync(ADDRESS_UNIVERSE_PATH, "utf8")) as
    | UniswapAddressUniverse
    | UniswapAddressCandidate[];
  if (Array.isArray(parsed)) return parsed;
  if (Array.isArray(parsed.candidates)) return parsed.candidates;
  throw new Error(`uniswap: ${ADDRESS_UNIVERSE_PATH} has no candidates[]`);
}

function candidateAddress(candidate: UniswapAddressCandidate): Hex | undefined {
  const address = candidate.address?.toLowerCase();
  if (!address || !ADDRESS_RE.test(address) || ZERO_ADDRESS_RE.test(address)) return undefined;
  return address as Hex;
}

function slugifyAddress(address: Hex): string {
  return address.slice(2, 10);
}

function v2PairEntries(chainId: number): ProtocolResolvedAddress[] {
  const out: ProtocolResolvedAddress[] = [];
  for (const candidate of loadUniverse()) {
    const candidateChain = candidate.chainId ?? candidate.chain_id;
    if (candidateChain !== chainId) continue;
    if (candidate.decision !== "cover") continue;
    if (candidate.batch !== V2_PAIR_BATCH) continue;
    const address = candidateAddress(candidate);
    if (!address) continue;
    out.push({
      address,
      id_suffix: `${chainId}-v2-pair-${slugifyAddress(address)}`,
      context: {
        batch: V2_PAIR_BATCH,
        disposition: candidate.decision,
        reason: candidate.reason,
        lp_token_file: candidate.lp_token_inventory?.token_file,
        lp_token_inventory_status: candidate.lp_token_inventory?.status,
        source: candidate.lp_token_inventory?.source ?? "surface/uniswap/_address_universe.json",
      },
    });
  }
  return out.sort((a, b) => a.address.localeCompare(b.address));
}

function v3PoolEntries(chainId: number): ProtocolResolvedAddress[] {
  const out: ProtocolResolvedAddress[] = [];
  for (const candidate of loadUniverse()) {
    const candidateChain = candidate.chainId ?? candidate.chain_id;
    if (candidateChain !== chainId) continue;
    if (candidate.decision !== "cover") continue;
    if (candidate.batch !== V3_POOL_BATCH) continue;
    const address = candidateAddress(candidate);
    const inventory = candidate.pool_inventory;
    if (!address || !inventory) continue;
    const token0 = inventory.token0?.toLowerCase();
    const token1 = inventory.token1?.toLowerCase();
    if (!token0 || !token1 || !ADDRESS_RE.test(token0) || !ADDRESS_RE.test(token1)) continue;
    if (typeof inventory.fee !== "number" || !Number.isInteger(inventory.fee)) continue;
    out.push({
      address,
      id_suffix: `${chainId}-v3-pool-${slugifyAddress(address)}`,
      context: {
        batch: V3_POOL_BATCH,
        disposition: candidate.decision,
        reason: candidate.reason,
        token0,
        token1,
        fee_tier_bp: inventory.fee,
        tick_spacing: inventory.tick_spacing,
        pool_inventory_status: inventory.status,
        source: inventory.source ?? "surface/uniswap/_address_universe.json",
        source_query: inventory.query,
      },
    });
  }
  return out.sort((a, b) => a.address.localeCompare(b.address));
}

export const v2PairCandidatesResolver: ProtocolResolver = {
  source: "uniswap:v2_pair_candidates",
  async resolve(chainId: number, _opts: ResolverOpts): Promise<Hex[]> {
    return v2PairEntries(chainId).map((entry) => entry.address);
  },
  async resolveWithContext(
    chainId: number,
    _opts: ResolverOpts,
  ): Promise<ProtocolResolvedAddress[]> {
    return v2PairEntries(chainId);
  },
};

export const v3PoolCandidatesResolver: ProtocolResolver = {
  source: "uniswap:v3_pool_candidates",
  async resolve(chainId: number, _opts: ResolverOpts): Promise<Hex[]> {
    return v3PoolEntries(chainId).map((entry) => entry.address);
  },
  async resolveWithContext(
    chainId: number,
    _opts: ResolverOpts,
  ): Promise<ProtocolResolvedAddress[]> {
    return v3PoolEntries(chainId);
  },
};
