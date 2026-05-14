import { coinGeckoPlatformForChain } from "./chain-config";
import { RpcMethodError, type FetchLike, type NowMs } from "./types";
import { isRecord } from "./validation";

export interface CoinGeckoTokenPrice {
  priceUsd: string;
  asOfTs: number;
}

export interface CoinGeckoClientOptions {
  fetch?: FetchLike;
  nowMs?: NowMs;
  baseUrl?: string;
}

export class CoinGeckoClient {
  private readonly fetchImpl: FetchLike;
  private readonly nowMs: NowMs;
  private readonly baseUrl: string;

  constructor(options: CoinGeckoClientOptions = {}) {
    this.fetchImpl = options.fetch ?? globalThis.fetch.bind(globalThis);
    this.nowMs = options.nowMs ?? Date.now;
    this.baseUrl = options.baseUrl ?? "https://api.coingecko.com/";
  }

  async tokenUsdPrice(chainId: number, address: string): Promise<CoinGeckoTokenPrice> {
    const normalizedAddress = address.toLowerCase();
    const url = this.tokenPriceUrl(chainId, normalizedAddress);
    const response = await this.fetchImpl(url, {
      method: "GET",
      headers: { accept: "application/json" },
    });

    if (!response.ok) {
      throw new RpcMethodError(
        "upstream_error",
        `CoinGecko returned HTTP ${response.status}`,
      );
    }

    const body = await response.json();
    const entry = findCoinGeckoEntry(body, normalizedAddress);

    if (!entry || entry.usd === undefined || entry.usd === null) {
      throw new RpcMethodError("not_found", "CoinGecko returned no USD price");
    }

    return {
      priceUsd: decimalInputToString(entry.usd),
      asOfTs: safeTimestamp(entry.last_updated_at, this.nowMs),
    };
  }

  private tokenPriceUrl(chainId: number, address: string): string {
    const platform = coinGeckoPlatformForChain(chainId);
    const root = this.baseUrl.endsWith("/") ? this.baseUrl : `${this.baseUrl}/`;
    const url = new URL(`api/v3/simple/token_price/${platform}`, root);
    url.searchParams.set("contract_addresses", address);
    url.searchParams.set("vs_currencies", "usd");
    url.searchParams.set("include_last_updated_at", "true");

    return url.toString();
  }
}

function findCoinGeckoEntry(
  body: unknown,
  address: string,
): Record<string, unknown> | undefined {
  if (!isRecord(body)) {
    throw new RpcMethodError("upstream_error", "CoinGecko returned a non-object response");
  }

  const direct = body[address];
  if (isRecord(direct)) {
    return direct;
  }

  const matchingKey = Object.keys(body).find((key) => key.toLowerCase() === address);
  const matchingEntry = matchingKey ? body[matchingKey] : undefined;

  return isRecord(matchingEntry) ? matchingEntry : undefined;
}

function decimalInputToString(value: unknown): string {
  if (typeof value === "string" && value.trim() !== "") {
    return value.trim();
  }

  if (typeof value === "number" && Number.isFinite(value)) {
    return String(value);
  }

  throw new RpcMethodError("upstream_error", "CoinGecko returned an invalid USD price");
}

function safeTimestamp(value: unknown, nowMs: NowMs): number {
  if (typeof value === "number" && Number.isSafeInteger(value) && value > 0) {
    return value;
  }

  return Math.floor(nowMs() / 1000);
}
