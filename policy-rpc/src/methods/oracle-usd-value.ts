import type { CoinGeckoClientOptions } from "../coingecko-client.js";
import {
  AggregatorError,
  OracleAggregator,
  type OracleAggregatorOptions,
} from "../oracle/aggregator.js";
import { ChainlinkSource } from "../oracle/sources/chainlink.js";
import { CoinGeckoSource } from "../oracle/sources/coingecko.js";
import { UniswapV3TwapSource } from "../oracle/sources/uniswap-v3-twap.js";
import type { OracleSource } from "../oracle/source.js";
import {
  RpcMethodError,
  type JsonValue,
  type NowMs,
  type OracleUsdValueParams,
  type UsdValuation,
} from "../types.js";
import { parseOracleUsdValueParams } from "../validation.js";

const USD_DECIMAL_PLACES = 4;

export interface OracleUsdValueMethodOptions extends CoinGeckoClientOptions {
  /** Pre-constructed aggregator (tests inject a fully mocked instance). */
  aggregator?: OracleAggregator;
  /** Override the source list when constructing the default aggregator. */
  sources?: OracleSource[];
  /** Aggregator tuning when sources are supplied (otherwise built fresh). */
  aggregatorOptions?: Omit<OracleAggregatorOptions, "sources">;
}

export type OracleUsdValueMethod = (params: unknown) => Promise<UsdValuation>;

export function createOracleUsdValueMethod(
  options: OracleUsdValueMethodOptions = {},
): OracleUsdValueMethod {
  const aggregator = options.aggregator ?? buildDefaultAggregator(options);
  const nowMs = options.nowMs ?? Date.now;

  return async (rawParams: unknown): Promise<UsdValuation> => {
    const params = parseOracleUsdValueParams(rawParams);

    let valuation: UsdValuation;
    try {
      valuation = await aggregator.aggregate(params.chain_id, {
        address: params.address,
        decimals: params.decimals,
      });
    } catch (error) {
      throw mapAggregatorError(error);
    }

    return scaleValuationByAmount(valuation, params, nowMs);
  };
}

function buildDefaultAggregator(
  options: OracleUsdValueMethodOptions,
): OracleAggregator {
  const sources: OracleSource[] = options.sources ?? [
    new ChainlinkSource(),
    new UniswapV3TwapSource(),
    new CoinGeckoSource(options),
  ];

  return new OracleAggregator({
    sources,
    outputDecimals: USD_DECIMAL_PLACES,
    ...(options.aggregatorOptions ?? {}),
    ...(options.nowMs ? { nowMs: options.nowMs } : {}),
  });
}

/**
 * The aggregator returns the USD value of a single token unit (e.g. "1 WETH
 * costs $X"). The RPC method must scale that by the requested raw amount.
 * We do this in bigint by combining the unit price (scaled by
 * `USD_DECIMAL_PLACES`) with the raw amount divided by `10^token.decimals`.
 */
function scaleValuationByAmount(
  unitValuation: UsdValuation,
  params: OracleUsdValueParams,
  nowMs: NowMs,
): UsdValuation {
  const unitPriceScaled = decimalToScaledBigInt(
    unitValuation.value,
    USD_DECIMAL_PLACES,
  );
  const amount = BigInt(params.amount);
  const tokenScale = 10n ** BigInt(params.decimals);
  const scaledUsd = (amount * unitPriceScaled) / tokenScale;
  const nowSec = Math.floor(nowMs() / 1000);

  return {
    ...unitValuation,
    value: formatScaledDecimal(scaledUsd, USD_DECIMAL_PLACES),
    staleSec: Math.max(0, nowSec - unitValuation.asOfTs),
  };
}

function mapAggregatorError(error: unknown): RpcMethodError {
  if (error instanceof RpcMethodError) {
    return error;
  }
  if (error instanceof AggregatorError) {
    switch (error.code) {
      case "all_sources_stale":
        return new RpcMethodError("stale_data", error.message);
      case "oracle_disagreement":
        return new RpcMethodError("oracle_disagreement", error.message);
      case "no_sources_configured":
        return new RpcMethodError("internal_error", error.message);
      case "all_sources_failed":
      default:
        return new RpcMethodError("upstream_error", error.message);
    }
  }
  if (error instanceof Error) {
    return new RpcMethodError("internal_error", error.message);
  }
  return new RpcMethodError("internal_error", "Unknown aggregator failure");
}

export function decimalToScaledBigInt(input: string, scale: number): bigint {
  const normalized = expandExponentialDecimal(input.trim());
  const matched = /^([+-]?)([0-9]+)(?:\.([0-9]+))?$/.exec(normalized);

  if (!matched) {
    throw new RpcMethodError(
      "internal_error",
      "Aggregator returned a malformed USD value",
    );
  }

  const [, sign, whole, fraction = ""] = matched;
  const scaledFraction = fraction.padEnd(scale, "0").slice(0, scale);
  const digits = `${whole}${scaledFraction}`.replace(/^0+(?=\d)/, "");
  const scaled = BigInt(digits === "" ? "0" : digits);

  return sign === "-" ? -scaled : scaled;
}

export function formatScaledDecimal(value: bigint, scale: number): string {
  const sign = value < 0n ? "-" : "";
  const absolute = value < 0n ? -value : value;

  if (scale === 0) {
    return `${sign}${absolute.toString()}`;
  }

  const digits = absolute.toString().padStart(scale + 1, "0");
  const whole = digits.slice(0, -scale);
  const fraction = digits.slice(-scale);

  return `${sign}${whole}.${fraction}`;
}

function expandExponentialDecimal(input: string): string {
  const matched = /^([+-]?)([0-9]+)(?:\.([0-9]+))?[eE]([+-]?[0-9]+)$/.exec(input);

  if (!matched) {
    return input;
  }

  const [, sign, whole, fraction = "", exponentString] = matched;
  const exponent = Number(exponentString);
  const digits = `${whole}${fraction}`;
  const decimalIndex = whole.length + exponent;

  if (decimalIndex <= 0) {
    return `${sign}0.${"0".repeat(Math.abs(decimalIndex))}${digits}`;
  }

  if (decimalIndex >= digits.length) {
    return `${sign}${digits}${"0".repeat(decimalIndex - digits.length)}`;
  }

  return `${sign}${digits.slice(0, decimalIndex)}.${digits.slice(decimalIndex)}`;
}

export function isJsonValue(value: unknown): value is JsonValue {
  if (
    value === null ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return true;
  }

  if (Array.isArray(value)) {
    return value.every(isJsonValue);
  }

  if (typeof value === "object") {
    return Object.values(value as Record<string, unknown>).every(isJsonValue);
  }

  return false;
}
