import type { Abi, PublicClient } from "viem";

import { getPublicClient } from "../../eth-provider.js";
import type { NowMs } from "../../types.js";
import {
  OracleSourceError,
  ORACLE_USD_DECIMALS,
  ORACLE_USD_SCALE,
  type AssetRef,
  type OracleSample,
  type OracleSource,
} from "../source.js";

const SOURCE_ID = "chainlink";

/** Reject Chainlink rounds whose updatedAt is older than 1 hour. */
const DEFAULT_MAX_AGE_SEC = 60 * 60;

/**
 * Chainlink AggregatorV3Interface (subset used here). Each feed quotes a
 * single asset against USD with `decimals()` precision (almost always 8).
 */
export const CHAINLINK_AGGREGATOR_ABI = [
  {
    type: "function",
    stateMutability: "view",
    name: "decimals",
    inputs: [],
    outputs: [{ type: "uint8", name: "" }],
  },
  {
    type: "function",
    stateMutability: "view",
    name: "latestRoundData",
    inputs: [],
    outputs: [
      { type: "uint80", name: "roundId" },
      { type: "int256", name: "answer" },
      { type: "uint256", name: "startedAt" },
      { type: "uint256", name: "updatedAt" },
      { type: "uint80", name: "answeredInRound" },
    ],
  },
] as const satisfies Abi;

/**
 * Registry of `(chainId, tokenAddress) → aggregatorAddress` for the most
 * common assets we price. Addresses are sourced from Chainlink's official
 * data feeds documentation. Token addresses are lowercased.
 *
 * NOTE: All seeded feeds quote USD with 8 decimals which matches
 * `ORACLE_USD_DECIMALS`.
 */
const CHAINLINK_FEEDS: Record<number, Record<string, string>> = {
  1: {
    // USDC / USD
    "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48":
      "0x8fffffd4afb6115b954bd326cbe7b4ba576818f6",
    // USDT / USD
    "0xdac17f958d2ee523a2206206994597c13d831ec7":
      "0x3e7d1eab13ad0104d2750b8863b489d65364e32d",
    // WETH (ETH) / USD
    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2":
      "0x5f4ec3df9cbd43714fe2740f5e3616155c5b8419",
    // WBTC / USD - using BTC / USD feed
    "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599":
      "0xf4030086522a5beea4988f8ca5b36dbc97bee88c",
    // DAI / USD
    "0x6b175474e89094c44da98b954eedeac495271d0f":
      "0xaed0c38402a5d19df6e4c03f4e2dced6e29c1ee9",
  },
};

export interface ChainlinkSourceOptions {
  /** Override the registry (allows tests to add fake feeds). */
  feeds?: Record<number, Record<string, string>>;
  /** Inject a viem PublicClient or factory (per-chain). */
  publicClient?: PublicClient;
  getPublicClient?: (chainId: number) => PublicClient;
  /** Override staleness budget in seconds (default 1h). */
  maxAgeSec?: number;
  /** Override `Date.now`. */
  nowMs?: NowMs;
}

interface LatestRoundData {
  answer: bigint;
  updatedAt: bigint;
  decimals: number;
}

export class ChainlinkSource implements OracleSource {
  readonly id = SOURCE_ID;
  private readonly feeds: Record<number, Record<string, string>>;
  private readonly publicClient: PublicClient | undefined;
  private readonly clientFactory: (chainId: number) => PublicClient;
  private readonly maxAgeSec: number;
  private readonly nowMs: NowMs;
  /**
   * `(chainId, feedAddress) -> decimals()` cache. Feed decimals are immutable
   * per Chainlink AggregatorV3Interface contract, so once we read them we can
   * skip the RPC call on subsequent price lookups. The cache is instance-
   * scoped so test cases that swap mock clients don't leak state.
   */
  private readonly decimalsCache = new Map<string, number>();

  constructor(options: ChainlinkSourceOptions = {}) {
    this.feeds = options.feeds ?? CHAINLINK_FEEDS;
    this.publicClient = options.publicClient;
    this.clientFactory =
      options.getPublicClient ??
      ((chainId: number) => getPublicClient(chainId));
    this.maxAgeSec = options.maxAgeSec ?? DEFAULT_MAX_AGE_SEC;
    this.nowMs = options.nowMs ?? Date.now;
  }

  async fetch(chainId: number, token: AssetRef): Promise<OracleSample> {
    const feed = this.lookupFeed(chainId, token.address);

    const client = this.publicClient ?? this.clientFactory(chainId);

    let round: LatestRoundData;
    try {
      round = await this.readLatestRoundData(client, chainId, feed);
    } catch (error) {
      if (error instanceof OracleSourceError) {
        throw error;
      }
      throw new OracleSourceError(
        "unavailable",
        SOURCE_ID,
        error instanceof Error ? error.message : "Chainlink call failed",
      );
    }

    if (round.answer <= 0n) {
      throw new OracleSourceError(
        "invalid_response",
        SOURCE_ID,
        `Chainlink feed ${feed} returned non-positive answer ${round.answer}`,
      );
    }

    const updatedAtSec = Number(round.updatedAt);
    if (!Number.isFinite(updatedAtSec) || updatedAtSec <= 0) {
      throw new OracleSourceError(
        "invalid_response",
        SOURCE_ID,
        `Chainlink feed ${feed} returned invalid updatedAt ${round.updatedAt}`,
      );
    }

    const nowSec = Math.floor(this.nowMs() / 1000);
    const ageSec = Math.max(0, nowSec - updatedAtSec);

    if (ageSec > this.maxAgeSec) {
      throw new OracleSourceError(
        "stale",
        SOURCE_ID,
        `Chainlink feed ${feed} is ${ageSec}s old (> ${this.maxAgeSec}s budget)`,
      );
    }

    const usd = rescaleToUsdDecimals(round.answer, round.decimals);

    return {
      usd,
      decimals: ORACLE_USD_DECIMALS,
      observedAt: updatedAtSec * 1000,
      sourceId: SOURCE_ID,
    };
  }

  private lookupFeed(chainId: number, tokenAddress: string): `0x${string}` {
    const chain = this.feeds[chainId];
    const lower = tokenAddress.toLowerCase();
    const feed = chain ? chain[lower] : undefined;

    if (!feed) {
      throw new OracleSourceError(
        "unsupported_token",
        SOURCE_ID,
        `No Chainlink feed registered for token ${tokenAddress} on chain ${chainId}`,
      );
    }

    return feed as `0x${string}`;
  }

  private async readLatestRoundData(
    client: PublicClient,
    chainId: number,
    feedAddress: `0x${string}`,
  ): Promise<LatestRoundData> {
    const cacheKey = `${chainId}:${feedAddress.toLowerCase()}`;
    const cachedDecimals = this.decimalsCache.get(cacheKey);

    // Issue `decimals()` only on first read per (chainId, feedAddress). On
    // cache hits the latest round is the sole RPC call.
    const latestPromise = client.readContract({
      address: feedAddress,
      abi: CHAINLINK_AGGREGATOR_ABI,
      functionName: "latestRoundData",
      args: [],
    });

    let decimals: number;
    if (cachedDecimals !== undefined) {
      decimals = cachedDecimals;
    } else {
      const decimalsRaw = await client.readContract({
        address: feedAddress,
        abi: CHAINLINK_AGGREGATOR_ABI,
        functionName: "decimals",
        args: [],
      });
      decimals = Number(decimalsRaw);
      this.decimalsCache.set(cacheKey, decimals);
    }

    const latest = await latestPromise;
    const [, answer, , updatedAt] = latest as readonly [
      bigint,
      bigint,
      bigint,
      bigint,
      bigint,
    ];

    return {
      answer,
      updatedAt,
      decimals,
    };
  }
}

/**
 * Convert a Chainlink answer (scaled by `feedDecimals`) into the canonical
 * `ORACLE_USD_DECIMALS` (1e8) representation. Uses pure bigint math.
 */
function rescaleToUsdDecimals(answer: bigint, feedDecimals: number): bigint {
  if (feedDecimals === ORACLE_USD_DECIMALS) {
    return answer;
  }
  if (feedDecimals < ORACLE_USD_DECIMALS) {
    return answer * 10n ** BigInt(ORACLE_USD_DECIMALS - feedDecimals);
  }
  return (answer * ORACLE_USD_SCALE) / 10n ** BigInt(feedDecimals);
}
