// Chainlink price source.
//
// Reads on-chain Chainlink AggregatorV3 contracts via `eth_call` over
// any operator-provided EVM RPC endpoint. Implements the same
// `PriceSourceClient` shape `oracle.usd_value` consumes for CoinGecko
// so it slots into the existing dispatch table.
//
// Single-file by design — mirrors `coingecko-client.ts`. Three
// concerns live here:
//
//   1. `evmRpcUrlForChain` / env loader — operators set
//      `POLICY_RPC_CHAIN_RPCS={"1":"https://…","10":"…"}` to point at
//      their RPC provider per chain. Other on-chain oracles (Pyth,
//      1inch spot) can pull from the same loader if/when they ship.
//
//   2. `CHAINLINK_FEEDS` — curated `(chainId, tokenAddress) →
//      {feedAddress, feedDecimals}` table. Bootstraps with the
//      handful of feeds policy authors realistically reach for
//      (ETH/BTC + major stables on Ethereum mainnet, plus L2
//      wrapped natives that share the same Chainlink feed). Extend
//      by editing this table; full directory at data.chain.link.
//
//   3. `ChainlinkClient` — implements `tokenUsdPrice`. Encodes
//      `latestRoundData()` (selector `0xfeaf968c`), POSTs the
//      `eth_jsonrpc` envelope, decodes the 5-tuple return, and
//      reports `{priceUsd, asOfTs}`.
//
// Zero external deps — raw `fetch` + manual ABI codec. The codec is
// trivial because `latestRoundData()` is a fixed-shape return
// (uint80, int256, uint256, uint256, uint80 — 5 × 32-byte words).

import { RpcMethodError, type FetchLike } from "./types.js";

// ── RPC URL configuration ─────────────────────────────────────────

/**
 * Bundled public RPC endpoints. Used when an operator hasn't set
 * `POLICY_RPC_CHAIN_RPCS` for a given chain — makes Chainlink "just
 * work" out of the box, same UX as CoinGecko.
 *
 * Multiple endpoints per chain so the client can fail over: when the
 * first 429s or 5xxs, it tries the next. Public RPCs go down or rate-
 * limit individually fairly often; redundancy across operators
 * (llamarpc, publicnode, ankr, chain-foundation) recovers cleanly.
 *
 * Production deployments should still configure `POLICY_RPC_CHAIN_RPCS`
 * with their own provider (Alchemy / Infura / self-hosted) — public
 * endpoints have no SLA and aggressive rate limits. Operators who
 * MUST stay within their own infra can set
 * `POLICY_RPC_DISABLE_PUBLIC_RPCS=1` to skip these fallbacks entirely.
 */
const DEFAULT_PUBLIC_RPCS: Readonly<Record<string, readonly string[]>> = {
  // Ethereum mainnet
  "1": [
    "https://eth.llamarpc.com",
    "https://ethereum-rpc.publicnode.com",
    "https://rpc.ankr.com/eth",
    "https://cloudflare-eth.com",
  ],
  // Optimism
  "10": [
    "https://optimism.llamarpc.com",
    "https://optimism-rpc.publicnode.com",
    "https://rpc.ankr.com/optimism",
    "https://mainnet.optimism.io",
  ],
  // BNB Smart Chain
  "56": [
    "https://bsc-rpc.publicnode.com",
    "https://rpc.ankr.com/bsc",
    "https://bsc-dataseed.bnbchain.org",
  ],
  // Polygon PoS
  "137": [
    "https://polygon.llamarpc.com",
    "https://polygon-bor-rpc.publicnode.com",
    "https://rpc.ankr.com/polygon",
    "https://polygon-rpc.com",
  ],
  // Base
  "8453": [
    "https://base.llamarpc.com",
    "https://base-rpc.publicnode.com",
    "https://mainnet.base.org",
  ],
  // Arbitrum One
  "42161": [
    "https://arbitrum.llamarpc.com",
    "https://arbitrum-one-rpc.publicnode.com",
    "https://rpc.ankr.com/arbitrum",
    "https://arb1.arbitrum.io/rpc",
  ],
};

export interface ChainRpcUrls {
  /**
   * Resolve chain → ordered list of RPC URLs. The client tries them
   * in order, falling over to the next on transport failure. Throws
   * `RpcMethodError("unsupported_chain", …)` when no endpoints are
   * configured (and no defaults apply) so callers treat the error as
   * a fail-soft "this oracle has no coverage here" signal.
   */
  forChain(chainId: number): readonly string[];
}

/**
 * Build a ChainRpcUrls from a plain `{chainId: url}` map. Accepts a
 * single URL string OR a `string[]` per chain — the daemon stores
 * everything as `string[]` internally so the failover code path is
 * the only branch even for single-endpoint configs.
 *
 * Throws at construction time for malformed entries — better to fail
 * at startup than at first request with a confusing error.
 */
export function chainRpcUrlsFromMap(
  rpcs: Readonly<Record<string, string | readonly string[]>>,
): ChainRpcUrls {
  const normalized = new Map<number, readonly string[]>();
  for (const [key, raw] of Object.entries(rpcs)) {
    const chainId = Number(key);
    if (!Number.isInteger(chainId) || chainId <= 0) {
      throw new Error(`[policy-rpc] chain RPC config: invalid chain id "${key}"`);
    }
    const urls = normalizeRpcEntry(raw, chainId);
    normalized.set(chainId, urls);
  }
  return {
    forChain(chainId: number): readonly string[] {
      const urls = normalized.get(chainId);
      if (!urls || urls.length === 0) {
        throw new RpcMethodError(
          "unsupported_chain",
          `No RPC URL configured for chain ${chainId}. Set POLICY_RPC_CHAIN_RPCS to a JSON map, e.g. {"1":"https://..."}, or unset POLICY_RPC_DISABLE_PUBLIC_RPCS to enable bundled public endpoints.`,
        );
      }
      return urls;
    },
  };
}

function normalizeRpcEntry(
  raw: string | readonly string[],
  chainId: number,
): readonly string[] {
  const list = Array.isArray(raw) ? raw : [raw as string];
  if (list.length === 0) {
    throw new Error(
      `[policy-rpc] chain RPC config for chain ${chainId}: url list must not be empty`,
    );
  }
  for (const url of list) {
    if (typeof url !== "string" || url.trim() === "") {
      throw new Error(
        `[policy-rpc] chain RPC config for chain ${chainId}: every url must be a non-empty string`,
      );
    }
  }
  return list;
}

/** Options consumed by `chainRpcUrlsFromEnv`. */
export interface ChainRpcUrlsEnvOptions {
  /**
   * Skip the bundled `DEFAULT_PUBLIC_RPCS` fallback. Set via
   * `POLICY_RPC_DISABLE_PUBLIC_RPCS=1` in env. Production daemons
   * that must stay within a single RPC provider use this to make
   * sure no traffic leaks to public endpoints.
   */
  disablePublicRpcs?: boolean;
  /** Override the env source (tests). */
  env?: NodeJS.ProcessEnv;
}

/**
 * Read `POLICY_RPC_CHAIN_RPCS` and merge over the bundled public-RPC
 * defaults to produce a ChainRpcUrls. With nothing set, daemons boot
 * with the public-RPC table active for the chains it ships — Chainlink
 * "just works" the same way CoinGecko does.
 *
 * Merge rule per chain (when defaults are enabled):
 *   user URLs (in env order) → default public URLs (in shipped order)
 *
 * User URLs are tried first, public URLs catch outages. Setting
 * `POLICY_RPC_DISABLE_PUBLIC_RPCS=1` switches to strict mode: only
 * user URLs are used, unconfigured chains return `unsupported_chain`.
 *
 * Malformed JSON throws — silently disabling Chainlink would surface
 * later as opaque errors.
 */
export function chainRpcUrlsFromEnv(
  options: ChainRpcUrlsEnvOptions = {},
): ChainRpcUrls {
  const env = options.env ?? process.env;
  const disablePublicRpcs =
    options.disablePublicRpcs ?? env.POLICY_RPC_DISABLE_PUBLIC_RPCS === "1";

  const userMap = parseUserRpcEnv(env.POLICY_RPC_CHAIN_RPCS);
  const defaults = disablePublicRpcs ? {} : DEFAULT_PUBLIC_RPCS;

  const merged: Record<string, string[]> = {};
  const chainKeys = new Set([
    ...Object.keys(userMap),
    ...Object.keys(defaults),
  ]);
  for (const key of chainKeys) {
    const userUrls = userMap[key] ?? [];
    const defaultUrls = defaults[key] ?? [];
    // Dedup: a user who repeats a default URL doesn't get it tried
    // twice. Order: user URLs first, defaults after.
    const seen = new Set<string>();
    const ordered: string[] = [];
    for (const url of [...userUrls, ...defaultUrls]) {
      if (seen.has(url)) continue;
      seen.add(url);
      ordered.push(url);
    }
    merged[key] = ordered;
  }
  return chainRpcUrlsFromMap(merged);
}

function parseUserRpcEnv(
  raw: string | undefined,
): Record<string, readonly string[]> {
  if (!raw || raw.trim() === "") return {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    throw new Error(
      `[policy-rpc] POLICY_RPC_CHAIN_RPCS is not valid JSON: ${
        error instanceof Error ? error.message : String(error)
      }`,
    );
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(
      `[policy-rpc] POLICY_RPC_CHAIN_RPCS must be a JSON object of {chainId: url | url[]}`,
    );
  }
  const out: Record<string, readonly string[]> = {};
  for (const [key, value] of Object.entries(parsed)) {
    if (typeof value === "string") {
      out[key] = [value];
    } else if (
      Array.isArray(value) &&
      value.every((v) => typeof v === "string")
    ) {
      out[key] = value as readonly string[];
    } else {
      throw new Error(
        `[policy-rpc] POLICY_RPC_CHAIN_RPCS["${key}"] must be a string or array of strings`,
      );
    }
  }
  return out;
}

// ── Chainlink feed directory ──────────────────────────────────────

interface ChainlinkFeed {
  /** AggregatorV3 contract address (lowercase). */
  feedAddress: string;
  /** Number of decimal places the feed's `answer` carries (almost always 8 for USD pairs). */
  feedDecimals: number;
}

/**
 * Curated lookup table: `<chainId>:<tokenAddress(lowercase)>` →
 * Chainlink feed metadata. Extend by adding rows; the key MUST match
 * the lowercased token address the daemon receives (CoinGecko client
 * does the same lowercase normalization on its side).
 *
 * The token side is the asset being priced (e.g. WETH on mainnet
 * routes to the ETH / USD feed because they're 1:1 in price).
 * Native-ETH callers already get rewritten to wrapped-ETH upstream
 * by `wrappedNativeAddressForChain`, so we only need wrapped
 * entries here.
 *
 * Feed addresses from https://data.chain.link/ (USD pairs page,
 * picked the 8-decimal proxy contracts so consumers don't need a
 * `decimals()` follow-up call). Last refreshed: bootstrap; verify
 * against the directory before relying on production trades.
 */
const CHAINLINK_FEEDS = new Map<string, ChainlinkFeed>([
  // ── Ethereum mainnet (chain 1) ──
  // WETH → ETH/USD
  [
    "1:0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
    { feedAddress: "0x5f4ec3df9cbd43714fe2740f5e3616155c5b8419", feedDecimals: 8 },
  ],
  // WBTC → BTC/USD
  [
    "1:0x2260fac5e5542a773aa44fbcfedf7c193bc2c599",
    { feedAddress: "0xf4030086522a5beea4988f8ca5b36dbc97bee88c", feedDecimals: 8 },
  ],
  // USDC → USDC/USD
  [
    "1:0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
    { feedAddress: "0x8fffffd4afb6115b954bd326cbe7b4ba576818f6", feedDecimals: 8 },
  ],
  // USDT → USDT/USD
  [
    "1:0xdac17f958d2ee523a2206206994597c13d831ec7",
    { feedAddress: "0x3e7d1eab13ad0104d2750b8863b489d65364e32d", feedDecimals: 8 },
  ],
  // DAI → DAI/USD
  [
    "1:0x6b175474e89094c44da98b954eedeac495271d0f",
    { feedAddress: "0xaed0c38402a5d19df6e4c03f4e2dced6e29c1ee9", feedDecimals: 8 },
  ],

  // ── Optimism (chain 10) — WETH → ETH/USD ──
  [
    "10:0x4200000000000000000000000000000000000006",
    { feedAddress: "0x13e3ee699d1909e989722e753853ae30b17e08c5", feedDecimals: 8 },
  ],

  // ── Arbitrum One (chain 42161) — WETH → ETH/USD ──
  [
    "42161:0x82af49447d8a07e3bd95bd0d56f35241523fbab1",
    { feedAddress: "0x639fe6ab55c921f74e7fac1ee960c0b6293ba612", feedDecimals: 8 },
  ],

  // ── Base (chain 8453) — WETH → ETH/USD ──
  [
    "8453:0x4200000000000000000000000000000000000006",
    { feedAddress: "0x71041dddad3595f9ced3dccfbe3d1f4b0a16bb70", feedDecimals: 8 },
  ],

  // ── Polygon PoS (chain 137) — WETH → ETH/USD ──
  [
    "137:0x7ceb23fd6bc0add59e62ac25578270cff1b9f619",
    { feedAddress: "0xf9680d99d6c9589e2a93a78a04a279e509205945", feedDecimals: 8 },
  ],
]);

/**
 * Read-only access to the feed table. Tests and admin tooling use
 * this to assert coverage without re-exporting the underlying Map.
 */
export function chainlinkFeedFor(
  chainId: number,
  tokenAddress: string,
): ChainlinkFeed | undefined {
  return CHAINLINK_FEEDS.get(`${chainId}:${tokenAddress.toLowerCase()}`);
}

// ── ABI codec for `latestRoundData()` ─────────────────────────────

/** Function selector for `latestRoundData()` — first 4 bytes of `keccak256("latestRoundData()")`. */
const LATEST_ROUND_DATA_SELECTOR = "0xfeaf968c";

interface LatestRoundData {
  /** Signed answer carried by the feed (scaled by `feedDecimals`). */
  answer: bigint;
  /** Unix seconds when the answer was last written on-chain. */
  updatedAt: number;
}

/**
 * Decode the 5 × 32-byte return tuple of `AggregatorV3.latestRoundData()`.
 *
 * Tuple shape (in 32-byte words from the start of the hex payload):
 *   word 0: roundId          (uint80,  right-padded inside its slot)
 *   word 1: answer           (int256,  two's complement)
 *   word 2: startedAt        (uint256)
 *   word 3: updatedAt        (uint256)  ← we use this
 *   word 4: answeredInRound  (uint80)
 *
 * We only consume `answer` and `updatedAt`; the rest are skipped so
 * the function can ignore unrelated extensions some adapters return.
 */
function decodeLatestRoundData(hexPayload: string): LatestRoundData {
  const body = hexPayload.startsWith("0x") ? hexPayload.slice(2) : hexPayload;
  if (body.length < 5 * 64) {
    throw new RpcMethodError(
      "upstream_error",
      "Chainlink AggregatorV3 returned a short payload",
    );
  }
  const answerHex = body.slice(64, 128);
  const updatedAtHex = body.slice(192, 256);

  return {
    answer: signedFromTwosComplementHex(answerHex),
    updatedAt: Number(BigInt(`0x${updatedAtHex}`)),
  };
}

/**
 * Read a 32-byte hex word as a signed int256 in two's complement.
 * BigInt's native `BigInt("0x…")` treats the hex as unsigned, so we
 * subtract 2²⁵⁶ when the top bit is set.
 */
function signedFromTwosComplementHex(hexWord: string): bigint {
  const unsigned = BigInt(`0x${hexWord}`);
  const sign = BigInt(`0x${hexWord.slice(0, 1)}`);
  // Top hex digit ≥ 8 means the sign bit is set.
  if (sign >= 8n) {
    return unsigned - (1n << 256n);
  }
  return unsigned;
}

// ── Client ────────────────────────────────────────────────────────

export interface ChainlinkClientOptions {
  fetch?: FetchLike;
  /** Override the RPC URL config. Defaults to env-var-derived. */
  rpcs?: ChainRpcUrls;
}

export class ChainlinkClient {
  private readonly fetchImpl: FetchLike;
  private readonly rpcs: ChainRpcUrls;

  constructor(options: ChainlinkClientOptions = {}) {
    this.fetchImpl = options.fetch ?? globalThis.fetch.bind(globalThis);
    this.rpcs = options.rpcs ?? chainRpcUrlsFromEnv();
  }

  async tokenUsdPrice(
    chainId: number,
    address: string,
  ): Promise<{ priceUsd: string; asOfTs: number }> {
    const feed = chainlinkFeedFor(chainId, address);
    if (!feed) {
      throw new RpcMethodError(
        "not_found",
        `Chainlink has no feed configured for chain ${chainId} token ${address}`,
      );
    }
    const rpcUrls = this.rpcs.forChain(chainId);
    const payload = await this.ethCallWithFailover(
      rpcUrls,
      feed.feedAddress,
      LATEST_ROUND_DATA_SELECTOR,
    );
    const { answer, updatedAt } = decodeLatestRoundData(payload);

    // answer is signed (Chainlink feeds CAN go negative for some
    // exotic pairs; USD prices in our curated set never do, but we
    // still reject negatives loudly instead of silently flipping
    // sign — a negative USD price would point at a misconfigured
    // feed, not real market data).
    if (answer <= 0n) {
      throw new RpcMethodError(
        "upstream_error",
        `Chainlink feed ${feed.feedAddress} returned a non-positive answer`,
      );
    }

    return {
      priceUsd: formatScaledBigInt(answer, feed.feedDecimals),
      asOfTs: updatedAt,
    };
  }

  /**
   * Try each RPC URL in order, returning the first successful
   * `eth_call` result. Public endpoints rate-limit / fail
   * independently, so a fresh URL almost always succeeds when the
   * previous one returned 429 / 5xx / hung.
   *
   * `eth_call` is idempotent (read-only), so retrying across providers
   * is safe — no risk of double-spend or state mutation. We stop only
   * on success or after exhausting the list, in which case we throw
   * with the last error's message attached.
   */
  private async ethCallWithFailover(
    rpcUrls: readonly string[],
    to: string,
    data: string,
  ): Promise<string> {
    let lastError: unknown;
    for (const rpcUrl of rpcUrls) {
      try {
        return await this.ethCallOne(rpcUrl, to, data);
      } catch (error) {
        lastError = error;
        // Keep going; the next URL might succeed.
      }
    }
    const message =
      lastError instanceof Error ? lastError.message : String(lastError);
    throw new RpcMethodError(
      "upstream_error",
      `All ${rpcUrls.length} Chainlink RPC endpoint(s) failed; last error: ${message}`,
    );
  }

  private async ethCallOne(
    rpcUrl: string,
    to: string,
    data: string,
  ): Promise<string> {
    // Per-request timeout — public endpoints occasionally hang for
    // tens of seconds, which would stall the whole policy evaluation
    // before failover kicks in. 10s gives a slow-but-alive node
    // plenty of room while keeping the failover responsive.
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
    let response: Response;
    try {
      response = await this.fetchImpl(rpcUrl, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          method: "eth_call",
          params: [{ to, data }, "latest"],
        }),
        signal: controller.signal,
      });
    } finally {
      clearTimeout(timer);
    }
    if (!response.ok) {
      throw new RpcMethodError(
        "upstream_error",
        `Chainlink RPC returned HTTP ${response.status}`,
      );
    }
    const body = (await response.json()) as {
      result?: string;
      error?: { code?: number; message?: string };
    };
    if (body.error) {
      throw new RpcMethodError(
        "upstream_error",
        `Chainlink RPC error: ${body.error.message ?? "unknown"}`,
      );
    }
    if (typeof body.result !== "string") {
      throw new RpcMethodError(
        "upstream_error",
        "Chainlink RPC returned no result field",
      );
    }
    return body.result;
  }
}

const REQUEST_TIMEOUT_MS = 10_000;

/**
 * Format a `value × 10^decimals` integer as a decimal string. Pure
 * BigInt to keep precision; mirrors what `coingecko-client.ts`'s
 * `decimalToScaledBigInt` does in reverse.
 */
function formatScaledBigInt(value: bigint, decimals: number): string {
  if (decimals === 0) return value.toString();
  const digits = value.toString().padStart(decimals + 1, "0");
  const whole = digits.slice(0, -decimals);
  const fraction = digits.slice(-decimals);
  return `${whole}.${fraction}`;
}
