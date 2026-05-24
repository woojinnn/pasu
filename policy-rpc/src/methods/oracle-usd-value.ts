import {
  CoinGeckoClient,
  type CoinGeckoClientOptions,
} from "../coingecko-client.js";
import {
  ChainlinkClient,
  type ChainlinkClientOptions,
} from "../chainlink-client.js";
import {
  RpcMethodError,
  type JsonValue,
  type NowMs,
  type OracleUsdValueParams,
  type UsdValuation,
} from "../types.js";
import { parseOracleUsdValueParams } from "../validation.js";
import type { MethodCatalogEntry } from "./catalog.js";

const USD_DECIMAL_PLACES = 4;

/**
 * Catalog entry for `oracle.usd_value`. The `source` enum param is the
 * Phase 8.5 extension point — adding a new price source means dropping
 * a new client in here and an enum value here (and inside the method
 * body's `sources` lookup). No client today besides CoinGecko is
 * wired, but the param surface is stable so manifests written today
 * keep working as we add Chainlink / Pyth.
 *
 * `defaultSelector` values cover the canonical "input token of a
 * swap" wiring — that's what the bundled starter pack uses. Manifest
 * authors targeting outputToken or another action shape edit them
 * after picking the method.
 */
export const oracleUsdValueCatalog: MethodCatalogEntry = {
  name: "oracle.usd_value",
  description: "Convert a token amount to its USD valuation via a price oracle.",
  params: {
    chain_id: {
      type: "Long",
      required: true,
      description: "EIP-155 chain ID.",
      defaultSelector: "$.root.chain_id",
    },
    asset: {
      type: "AssetRef",
      required: true,
      description: "Token to price (address, symbol, decimals, …).",
      defaultSelector: "$.action.inputToken.asset",
    },
    amount: {
      type: "String",
      required: true,
      description: "On-chain amount (uint256 wei-form string).",
      defaultSelector: "$.action.inputToken.amount.value",
    },
    source: {
      type: "String",
      required: false,
      description:
        "Which price source to query. `coingecko` is HTTP-based and works out of the box; `chainlink` reads on-chain feeds and needs POLICY_RPC_CHAIN_RPCS configured per chain.",
      enum_: ["coingecko", "chainlink"],
      default: "coingecko",
    },
  },
  returns: { kind: "record", type: "UsdValuation" },
  origin: "bundled",
};

/**
 * Common shape every price source exposes. Lets the dispatcher pick a
 * source by string name without baking client-specific code into the
 * method body — `oracle.usd_value` stays one entrypoint and the
 * "which source" decision moves into a tiny lookup. Chainlink/Pyth
 * impls plug in here with zero change to the method's call surface.
 */
interface PriceSourceClient {
  tokenUsdPrice(
    chainId: number,
    address: string,
  ): Promise<{ priceUsd: string; asOfTs: number }>;
}

export interface OracleUsdValueMethodOptions extends CoinGeckoClientOptions {
  client?: CoinGeckoClient;
  /**
   * Programmatic Chainlink client override (tests + embedders). When
   * absent we construct one with env-var-derived RPC URL config, so
   * production daemons just set `POLICY_RPC_CHAIN_RPCS` and Chainlink
   * routes through automatically.
   */
  chainlinkClient?: ChainlinkClient;
  /**
   * Forwarded to the auto-constructed `ChainlinkClient` when
   * `chainlinkClient` isn't supplied. Tests use this to inject a fake
   * `fetch` or a programmatic RPC URL map without first wiring env.
   */
  chainlinkOptions?: ChainlinkClientOptions;
  /**
   * Inject custom source clients on top of the bundled
   * `{ coingecko, chainlink }` map. Used by tests; also the seam
   * sidecar / plugin extensions would target if they wanted to add a
   * new source to this method (vs. shipping a sibling method).
   */
  sources?: Record<string, PriceSourceClient>;
}

export type OracleUsdValueMethod = (params: unknown) => Promise<UsdValuation>;

export function createOracleUsdValueMethod(
  options: OracleUsdValueMethodOptions = {},
): OracleUsdValueMethod {
  const coingecko = options.client ?? new CoinGeckoClient(options);
  const chainlink =
    options.chainlinkClient ?? new ChainlinkClient(options.chainlinkOptions);
  // `sources` is the dispatch table. Default-includes CoinGecko +
  // Chainlink so manifests choosing either work out of the box (subject
  // to per-chain config for Chainlink). `options.sources` extends or
  // overrides for tests / sidecar-style additions.
  const sources: Record<string, PriceSourceClient> = {
    coingecko,
    chainlink,
    ...(options.sources ?? {}),
  };
  const nowMs = options.nowMs ?? Date.now;

  return async (rawParams: unknown): Promise<UsdValuation> => {
    const params = parseOracleUsdValueParams(rawParams);
    const client = sources[params.source];
    if (!client) {
      // Validation accepted the enum value but the dispatch table
      // doesn't have a runtime client for it — this means the
      // catalog/validation declared a source the daemon isn't
      // actually shipping yet. Surface as `upstream_unavailable`
      // rather than `invalid_params` so the caller sees it as a
      // server-side gap, not their typo.
      throw new RpcMethodError(
        "upstream_unavailable",
        `oracle.usd_value: no client registered for source "${params.source}"`,
      );
    }
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
    // Echo back the source the caller selected. When we add multi-
    // source aggregation later this stays an array (cedarschema's
    // `Set<String>`) so the field shape is forward-compatible.
    sources: [params.source],
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
