/**
 * Balancer V3 protocol source resolver.
 *
 * Unlike Curve (one callkey per pool address), Balancer V3 routes every pool's
 * liquidity through ONE Router. `addLiquidityProportional` / `addLiquidityUnbalanced`
 * / `removeLiquidityProportional` carry the per-token `amounts[]` array but NOT the
 * token addresses (those live on the V3 Vault via `getPoolTokens`). Since Dambi
 * is static (no sim), this resolver bakes a `pool -> [token addresses]` map from the
 * reviewed `surface/balancer/_pool_universe.json` artifact (sourced from the Balancer
 * official API during P0) and surfaces it to the Router liquidity manifests as a
 * single `$source.pool_tokens` context object.
 *
 * One source kind, `balancer_v3:pool_tokens`, materializing exactly one bundle (the
 * mainnet V3 Router) that carries the full pool->tokens map. The map is consumed at
 * decode time by the `balancer_v3_zip_pool_tokens` $fn, which looks up `$args.pool`
 * and zips its token list with the calldata `amounts[]`.
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
const POOL_UNIVERSE_PATH = join(REGISTRY_ROOT, "surface", "balancer", "_pool_universe.json");
const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;

/** Canonical mainnet Balancer V3 Router v2 (balancer-deployments v3 20250307-v3-router-v2). */
const V3_ROUTER_MAINNET = "0xae563e3f8219521950555f5962419c8919758ea2";
/** Mainnet Balancer V3 CompositeLiquidityRouter v2 (balancer-deployments v3 20250307). ERC4626/boosted pool liquidity; same pool->tokens resolution as the Router. */
const V3_COMPOSITE_MAINNET = "0xb21a277466e7db6934556a1ce12eb3f032815c8a";

interface PoolCandidate {
  chainId?: number;
  chain_id?: number;
  address?: string;
  tokens?: string[];
}
interface PoolUniverse {
  candidates?: PoolCandidate[];
}

/** Build the `pool -> [token]` map (lowercased) for `chainId` from the reviewed universe artifact. */
function loadPoolTokenMap(chainId: number): Record<string, string[]> {
  if (!existsSync(POOL_UNIVERSE_PATH)) return {};
  const data = JSON.parse(readFileSync(POOL_UNIVERSE_PATH, "utf8")) as PoolUniverse;
  const map: Record<string, string[]> = {};
  for (const c of data.candidates ?? []) {
    if ((c.chainId ?? c.chain_id ?? 1) !== chainId) continue;
    const addr = (c.address ?? "").toLowerCase();
    if (!ADDRESS_RE.test(addr)) continue;
    const tokens = (c.tokens ?? [])
      .map((t) => String(t).toLowerCase())
      .filter((t) => ADDRESS_RE.test(t));
    if (tokens.length < 2) continue; // proportional liquidity needs >= 2 tokens
    map[addr] = tokens;
  }
  return map;
}

/**
 * Build a resolver that materializes ONE router address carrying the full mainnet
 * V3 `pool -> tokens` map under `$source.pool_tokens`. The V3 Router (proportional/
 * unbalanced liquidity) and the CompositeLiquidityRouter v2 (ERC4626/boosted pool
 * liquidity) both resolve a calldata `pool` against the SAME baked map; they only
 * differ in the callkey address, so this factory parameterizes by `(source, address)`.
 */
function makePoolTokensResolver(
  source: ProtocolSourceKind,
  routerAddress: Hex,
): ProtocolResolver {
  return {
    source,
    async resolve(chainId: number, _opts: ResolverOpts): Promise<Hex[]> {
      if (chainId !== 1) return [];
      return [routerAddress];
    },
    async resolveWithContext(
      chainId: number,
      _opts: ResolverOpts,
    ): Promise<ProtocolResolvedAddress[]> {
      if (chainId !== 1) return [];
      const poolTokens = loadPoolTokenMap(chainId);
      if (Object.keys(poolTokens).length === 0) return [];
      return [{ address: routerAddress, id_suffix: "v2-mainnet", context: { pool_tokens: poolTokens } }];
    },
  };
}

/** `balancer_v3:pool_tokens` — V3 Router liquidity (proportional/unbalanced). */
export const balancerV3PoolTokensResolver: ProtocolResolver = makePoolTokensResolver(
  "balancer_v3:pool_tokens",
  V3_ROUTER_MAINNET,
);

/** `balancer_v3:composite_pool_tokens` — CompositeLiquidityRouter v2 ERC4626/boosted liquidity. */
export const balancerV3CompositePoolTokensResolver: ProtocolResolver = makePoolTokensResolver(
  "balancer_v3:composite_pool_tokens",
  V3_COMPOSITE_MAINNET,
);
