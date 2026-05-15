import type { NowMs, UsdValuation, UsdValuationSource } from "../types.js";

import {
  OracleSourceError,
  ORACLE_USD_SCALE,
  type AssetRef,
  type OracleSample,
  type OracleSource,
} from "./source.js";

export type AggregatorErrorCode =
  | "all_sources_failed"
  | "all_sources_stale"
  | "oracle_disagreement"
  | "no_sources_configured";

export class AggregatorError extends Error {
  readonly code: AggregatorErrorCode;
  readonly breakdown: UsdValuationSource[];

  constructor(
    code: AggregatorErrorCode,
    message: string,
    breakdown: UsdValuationSource[] = [],
  ) {
    super(message);
    this.name = "AggregatorError";
    this.code = code;
    this.breakdown = breakdown;
  }
}

export interface OracleAggregatorOptions {
  /** Sources to query in parallel. */
  sources: OracleSource[];
  /** Max deviation (basis points) before a source is treated as an outlier. */
  outlierBps?: number;
  /** Wall-clock function (mostly for tests). */
  nowMs?: NowMs;
  /** USD precision returned to callers. The internal scale is always 1e8. */
  outputDecimals?: number;
}

const DEFAULT_OUTLIER_BPS = 300; // 3 %
const DEFAULT_OUTPUT_DECIMALS = 4;

interface CollectedSample {
  sample: OracleSample;
}

interface CollectedFailure {
  sourceId: string;
  reason: string;
  code: OracleErrorCode;
}

type OracleErrorCode = OracleSourceError["code"] | "unknown";

/**
 * Aggregates `OracleSource` quotes with a "median + deviation + staleness"
 * rule:
 *   1. Fire all sources in parallel via `Promise.allSettled`.
 *   2. Drop any source that errored.
 *   3. From the survivors compute the median USD value.
 *   4. Flag and drop any survivor more than `outlierBps` from the median;
 *      recompute the median over the remaining set.
 *   5. Promote `confidence: "low"` when only one survivor remains.
 *   6. Throw `AggregatorError` when nothing survives.
 */
export class OracleAggregator {
  private readonly sources: OracleSource[];
  private readonly outlierBps: number;
  private readonly nowMs: NowMs;
  private readonly outputDecimals: number;

  constructor(options: OracleAggregatorOptions) {
    if (!options.sources.length) {
      throw new AggregatorError(
        "no_sources_configured",
        "OracleAggregator requires at least one source",
      );
    }
    this.sources = options.sources;
    this.outlierBps = options.outlierBps ?? DEFAULT_OUTLIER_BPS;
    this.nowMs = options.nowMs ?? Date.now;
    this.outputDecimals = options.outputDecimals ?? DEFAULT_OUTPUT_DECIMALS;
  }

  async aggregate(chainId: number, token: AssetRef): Promise<UsdValuation> {
    const results = await Promise.allSettled(
      this.sources.map((source) => this.fetchOne(source, chainId, token)),
    );

    const failures: CollectedFailure[] = [];
    const samples: CollectedSample[] = [];
    for (let i = 0; i < results.length; i += 1) {
      const result = results[i];
      const source = this.sources[i];
      if (!source) continue;
      if (result.status === "fulfilled") {
        samples.push({ sample: result.value });
      } else {
        failures.push(failureFromReason(source.id, result.reason));
      }
    }

    if (samples.length === 0) {
      const breakdown = failures.map(failureToBreakdown);
      const stale = failures.length > 0 && failures.every((f) => f.code === "stale");
      throw new AggregatorError(
        stale ? "all_sources_stale" : "all_sources_failed",
        stale
          ? "All oracle sources returned stale data"
          : `No oracle source produced a usable quote: ${failures.map((f) => `${f.sourceId}=${f.code}`).join(", ")}`,
        breakdown,
      );
    }

    if (samples.length === 1) {
      const only = samples[0]!.sample;
      const breakdown: UsdValuationSource[] = [
        sampleToBreakdown(only, true, undefined),
        ...failures.map(failureToBreakdown),
      ];
      return this.buildValuation([only], breakdown, "low");
    }

    const initialMedian = median(samples.map(({ sample }) => sample.usd));
    const tolerance = (initialMedian * BigInt(this.outlierBps)) / 10_000n;

    const surviving: OracleSample[] = [];
    const droppedAsOutliers: OracleSample[] = [];
    for (const { sample } of samples) {
      if (deviation(sample.usd, initialMedian) <= tolerance) {
        surviving.push(sample);
      } else {
        droppedAsOutliers.push(sample);
      }
    }

    const breakdown: UsdValuationSource[] = [
      ...surviving.map((sample) => sampleToBreakdown(sample, true, undefined)),
      ...droppedAsOutliers.map((sample) =>
        sampleToBreakdown(sample, false, "outlier"),
      ),
      ...failures.map(failureToBreakdown),
    ];

    if (surviving.length === 0) {
      throw new AggregatorError(
        "oracle_disagreement",
        `Oracle sources disagree beyond ${this.outlierBps}bps; dropped ${droppedAsOutliers.length} outliers`,
        breakdown,
      );
    }

    const confidence: UsdValuation["confidence"] = surviving.length === 1 ? "low" : "high";

    return this.buildValuation(surviving, breakdown, confidence);
  }

  private async fetchOne(
    source: OracleSource,
    chainId: number,
    token: AssetRef,
  ): Promise<OracleSample> {
    const sample = await source.fetch(chainId, token);
    if (sample.usd <= 0n) {
      throw new OracleSourceError(
        "invalid_response",
        source.id,
        `${source.id} returned non-positive USD value`,
      );
    }
    return sample;
  }

  private buildValuation(
    surviving: OracleSample[],
    breakdown: UsdValuationSource[],
    confidence: UsdValuation["confidence"],
  ): UsdValuation {
    const medianUsd = median(surviving.map((s) => s.usd));
    const newestObservedAt = surviving.reduce(
      (acc, s) => (s.observedAt > acc ? s.observedAt : acc),
      0,
    );
    const asOfTs = Math.floor(newestObservedAt / 1000);
    const nowSec = Math.floor(this.nowMs() / 1000);
    const staleSec = Math.max(0, nowSec - asOfTs);
    const sources = surviving.map((s) => s.sourceId);

    return {
      value: formatScaledUsd(medianUsd, this.outputDecimals),
      asOfTs,
      staleSec,
      sources,
      sourceBreakdown: breakdown,
      ...(confidence ? { confidence } : {}),
    };
  }
}

function deviation(a: bigint, b: bigint): bigint {
  return a > b ? a - b : b - a;
}

/**
 * Median over a non-empty bigint array. For even sized arrays we return the
 * arithmetic floor of the two middle values - deterministic and avoids
 * pulling in JS number arithmetic.
 */
export function median(values: bigint[]): bigint {
  if (values.length === 0) {
    throw new Error("median requires at least one value");
  }
  const sorted = [...values].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  const mid = sorted.length >> 1;
  if (sorted.length % 2 === 1) {
    return sorted[mid]!;
  }
  return (sorted[mid - 1]! + sorted[mid]!) / 2n;
}

/** Convert an integer scaled by `ORACLE_USD_SCALE` (1e8) into a decimal string. */
export function formatScaledUsd(value: bigint, outputDecimals: number): string {
  if (outputDecimals < 0 || outputDecimals > 18) {
    throw new RangeError(`outputDecimals out of range: ${outputDecimals}`);
  }
  // ORACLE_USD_SCALE is 1e8; resize to the desired precision.
  let scaled: bigint;
  if (outputDecimals === 8) {
    scaled = value;
  } else if (outputDecimals < 8) {
    const diff = 8 - outputDecimals;
    const divisor = 10n ** BigInt(diff);
    scaled = value / divisor; // floor truncation - acceptable for display
  } else {
    const diff = outputDecimals - 8;
    scaled = value * 10n ** BigInt(diff);
  }

  const sign = scaled < 0n ? "-" : "";
  const absolute = scaled < 0n ? -scaled : scaled;
  if (outputDecimals === 0) {
    return `${sign}${absolute.toString()}`;
  }
  const padded = absolute.toString().padStart(outputDecimals + 1, "0");
  const whole = padded.slice(0, -outputDecimals);
  const fraction = padded.slice(-outputDecimals);
  return `${sign}${whole}.${fraction}`;
}

function sampleToBreakdown(
  sample: OracleSample,
  included: boolean,
  reason: string | undefined,
): UsdValuationSource {
  return {
    sourceId: sample.sourceId,
    value: formatScaledUsd(sample.usd, DEFAULT_OUTPUT_DECIMALS),
    asOfTs: Math.floor(sample.observedAt / 1000),
    included,
    ...(reason !== undefined ? { reason } : {}),
  };
}

function failureToBreakdown(failure: CollectedFailure): UsdValuationSource {
  return {
    sourceId: failure.sourceId,
    value: "0.0000",
    asOfTs: 0,
    included: false,
    reason: failure.code === "unknown" ? failure.reason : failure.code,
  };
}

function failureFromReason(sourceId: string, reason: unknown): CollectedFailure {
  if (reason instanceof OracleSourceError) {
    return { sourceId: reason.sourceId, code: reason.code, reason: reason.message };
  }
  const message = reason instanceof Error ? reason.message : String(reason);
  return { sourceId, code: "unknown", reason: message };
}

/** Returns the canonical 1e8 scale - exported for tests / callers that need it. */
export const AGGREGATOR_USD_SCALE = ORACLE_USD_SCALE;
