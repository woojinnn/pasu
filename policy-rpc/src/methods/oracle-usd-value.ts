import {
  CoinGeckoClient,
  type CoinGeckoClientOptions,
} from "../coingecko-client.js";
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
  client?: CoinGeckoClient;
}

export type OracleUsdValueMethod = (params: unknown) => Promise<UsdValuation>;

export function createOracleUsdValueMethod(
  options: OracleUsdValueMethodOptions = {},
): OracleUsdValueMethod {
  const client = options.client ?? new CoinGeckoClient(options);
  const nowMs = options.nowMs ?? Date.now;

  return async (rawParams: unknown): Promise<UsdValuation> => {
    const params = parseOracleUsdValueParams(rawParams);
    const tokenPrice = await client.tokenUsdPrice(params.chain_id, params.address);

    return valueFromPrice(params, tokenPrice.priceUsd, tokenPrice.asOfTs, nowMs);
  };
}

function valueFromPrice(
  params: OracleUsdValueParams,
  unitPriceUsd: string,
  asOfTs: number,
  nowMs: NowMs,
): UsdValuation {
  const priceScaled = decimalToScaledBigInt(unitPriceUsd, USD_DECIMAL_PLACES);
  const amount = BigInt(params.amount);
  const tokenScale = 10n ** BigInt(params.decimals);
  const scaledUsd = (amount * priceScaled) / tokenScale;
  const nowSec = Math.floor(nowMs() / 1000);

  return {
    value: formatScaledDecimal(scaledUsd, USD_DECIMAL_PLACES),
    asOfTs,
    staleSec: Math.max(0, nowSec - asOfTs),
    sources: ["coingecko"],
  };
}

export function decimalToScaledBigInt(input: string, scale: number): bigint {
  const normalized = expandExponentialDecimal(input.trim());
  const match = /^([+-]?)([0-9]+)(?:\.([0-9]+))?$/.exec(normalized);

  if (!match) {
    throw new RpcMethodError("upstream_error", "CoinGecko returned an invalid USD price");
  }

  const [, sign, whole, fraction = ""] = match;
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
  const match = /^([+-]?)([0-9]+)(?:\.([0-9]+))?[eE]([+-]?[0-9]+)$/.exec(input);

  if (!match) {
    return input;
  }

  const [, sign, whole, fraction = "", exponentText] = match;
  const exponent = Number(exponentText);
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
