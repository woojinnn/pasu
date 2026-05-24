import {
  RpcMethodError,
  type JsonObject,
  type NowMs,
} from "../types.js";
import { isRecord } from "../validation.js";
import type { MethodCatalogEntry } from "./catalog.js";

const MAX_UINT256 =
  "115792089237316195423570985008687907853269984665640564039457584007913129639935";

// ── Catalog entries ────────────────────────────────────────────────
//
// One `MethodCatalogEntry` per public method, exported next to its
// factory. The registry collects them; the dashboard's manifest editor
// consumes them via `GET /v1/methods` to drive its dropdowns.
//
// `defaultSelector` values track the canonical "swap" wiring because
// that's the bundled starter pack's reference shape. Authors targeting
// other actions (transfer, approve) edit after picking the method —
// the catalog is best-effort here, not strict.

export const clockNowCatalog: MethodCatalogEntry = {
  name: "clock.now",
  description: "Current Unix timestamp (seconds) from the daemon's clock.",
  params: {},
  returns: { kind: "scalar", type: "Long", from: "$.result.nowTs" },
  origin: "bundled",
};

export const approvalAllowanceCatalog: MethodCatalogEntry = {
  name: "approval.allowance",
  description:
    "Inspect a token allowance: surfaces raw value, unlimited-allowance flag, and coverage of a requested amount.",
  params: {
    allowance: {
      type: "String",
      required: false,
      description: "Current allowance (uint256 string). Defaults to 0 when omitted.",
    },
    requested_amount: {
      type: "String",
      required: false,
      description: "Amount being requested by the transaction (uint256 string).",
    },
  },
  // Returns a multi-field record. Output editor pulls out the scalar
  // leaves explicitly via $.result.<field>; there's no Cedar alias
  // for the whole shape so we mark it `scalar` from the umbrella
  // result with the most policy-relevant field as default.
  returns: { kind: "scalar", type: "Bool", from: "$.result.coversRequestedAmount" },
  origin: "bundled",
};

export const approvalCoverInputsCatalog: MethodCatalogEntry = {
  name: "approval.cover_inputs",
  description: "Whether the token allowance covers the requested input amount.",
  params: {
    allowances_cover_inputs: {
      type: "Bool",
      required: false,
      description: "Caller-supplied override; takes precedence when present.",
    },
    allowance: {
      type: "String",
      required: false,
      description: "Current allowance (uint256 string).",
    },
    requested_amount: {
      type: "String",
      required: false,
      description: "Amount being requested by the transaction.",
    },
  },
  returns: { kind: "scalar", type: "Bool", from: "$.result.allowancesCoverInputs" },
  origin: "bundled",
};

export const oracleEffectiveRateBpsCatalog: MethodCatalogEntry = {
  name: "oracle.effective_rate_bps",
  description:
    "Effective rate of a swap vs the oracle's mid-market price, expressed in basis points (negative = unfavourable).",
  params: {
    chain_id: {
      type: "Long",
      required: true,
      defaultSelector: "$.root.chain_id",
    },
    token_in: {
      type: "AssetRef",
      required: true,
      defaultSelector: "$.action.inputToken.asset",
    },
    amount_in: {
      type: "String",
      required: true,
      defaultSelector: "$.action.inputToken.amount.value",
    },
    token_out: {
      type: "AssetRef",
      required: true,
      defaultSelector: "$.action.outputToken.asset",
    },
    amount_out: {
      type: "String",
      required: true,
      defaultSelector: "$.action.outputToken.amount.value",
    },
  },
  returns: { kind: "scalar", type: "Long", from: "$.result.bps" },
  origin: "bundled",
};

export const portfolioBalanceCatalog: MethodCatalogEntry = {
  name: "portfolio.balance",
  description: "Wallet balance of a specific token.",
  params: {
    chain_id: {
      type: "Long",
      required: true,
      defaultSelector: "$.root.chain_id",
    },
    owner: {
      type: "String",
      required: true,
      defaultSelector: "$.root.from",
    },
    asset: {
      type: "AssetRef",
      required: true,
      defaultSelector: "$.action.inputToken.asset",
    },
  },
  returns: { kind: "scalar", type: "String", from: "$.result.balance" },
  origin: "bundled",
};

export const portfolioInputFractionBpsCatalog: MethodCatalogEntry = {
  name: "portfolio.input_fraction_bps",
  description:
    "Fraction of the wallet's portfolio (denominated in the input asset) the transaction is spending, in basis points.",
  params: {
    chain_id: {
      type: "Long",
      required: true,
      defaultSelector: "$.root.chain_id",
    },
    owner: {
      type: "String",
      required: true,
      defaultSelector: "$.root.from",
    },
    asset: {
      type: "AssetRef",
      required: true,
      defaultSelector: "$.action.inputToken.asset",
    },
    amount: {
      type: "String",
      required: true,
      defaultSelector: "$.action.inputToken.amount.value",
    },
  },
  returns: { kind: "scalar", type: "Long", from: "$.result.bps" },
  origin: "bundled",
};

export const statWindowSnapshotCatalog: MethodCatalogEntry = {
  name: "stat_window.snapshot",
  description: "Rolling-window snapshot of recent on-chain activity for the wallet.",
  params: {
    owner: {
      type: "String",
      required: true,
      defaultSelector: "$.root.from",
    },
  },
  returns: { kind: "record", type: "WindowStats" },
  origin: "bundled",
};

export const statWindowSwapStatsCatalog: MethodCatalogEntry = {
  name: "stat_window.swap_stats",
  description: "Per-action stats over a sliding 24h window (volume + count).",
  params: {
    owner: {
      type: "String",
      required: true,
      defaultSelector: "$.root.from",
    },
    action: {
      type: "String",
      required: true,
      description: "Action keyword (e.g. \"swap\").",
      defaultSelector: "swap",
    },
  },
  returns: { kind: "record", type: "WindowStats" },
  origin: "bundled",
};

export function createClockNowMethod(nowMs: NowMs = Date.now) {
  return async (rawParams: unknown): Promise<JsonObject> => {
    expectParamsObject(rawParams, "clock.now");

    return { nowTs: Math.floor(nowMs() / 1000) };
  };
}

export function createApprovalAllowanceMethod() {
  return async (rawParams: unknown): Promise<JsonObject> => {
    const params = expectParamsObject(rawParams, "approval.allowance");
    const allowance = optionalUnsignedIntegerString(params.allowance, "allowance") ?? "0";
    const requested = optionalUnsignedIntegerString(
      params.requested_amount,
      "requested_amount",
    );

    return {
      allowance,
      coversRequestedAmount: requested === undefined ? false : bigintGte(allowance, requested),
      hasUnlimitedAllowance: allowance === MAX_UINT256,
    };
  };
}

export function createApprovalCoverInputsMethod() {
  return async (rawParams: unknown): Promise<JsonObject> => {
    const params = expectParamsObject(rawParams, "approval.cover_inputs");
    const override = optionalBoolean(params.allowances_cover_inputs, "allowances_cover_inputs");
    const allowance = optionalUnsignedIntegerString(params.allowance, "allowance");
    const requested = optionalUnsignedIntegerString(
      params.requested_amount,
      "requested_amount",
    );
    const hasUnlimitedAllowance = allowance === MAX_UINT256;
    const allowancesCoverInputs =
      override ?? (allowance !== undefined && requested !== undefined
        ? bigintGte(allowance, requested)
        : true);

    return {
      allowancesCoverInputs,
      hasUnlimitedAllowance,
    };
  };
}

export function createPortfolioBalanceMethod() {
  return async (rawParams: unknown): Promise<JsonObject> => {
    const params = expectParamsObject(rawParams, "portfolio.balance");
    const balance = optionalUnsignedIntegerString(params.balance, "balance") ?? "0";

    return { balance };
  };
}

export function createPortfolioInputFractionBpsMethod() {
  return async (rawParams: unknown): Promise<JsonObject> => {
    const params = expectParamsObject(rawParams, "portfolio.input_fraction_bps");
    const bps = optionalSafeInteger(params.bps, "bps") ?? 0;

    return { bps };
  };
}

export function createOracleEffectiveRateBpsMethod() {
  return async (rawParams: unknown): Promise<JsonObject> => {
    const params = expectParamsObject(rawParams, "oracle.effective_rate_bps");
    const bps = optionalSafeInteger(params.bps, "bps") ?? 10_000;

    return { bps };
  };
}

export function createStatWindowSnapshotMethod() {
  return async (rawParams: unknown): Promise<JsonObject> => {
    const params = expectParamsObject(rawParams, "stat_window.snapshot");
    const values = isRecord(params.values) ? (params.values as JsonObject) : {};

    return { values };
  };
}

export function createStatWindowSwapStatsMethod() {
  return async (rawParams: unknown): Promise<JsonObject> => {
    const params = expectParamsObject(rawParams, "stat_window.swap_stats");
    const swapVolumeUsd24h =
      optionalDecimalString(params.swap_volume_usd_24h, "swap_volume_usd_24h") ?? "0.0000";
    const swapCount24h = optionalSafeInteger(params.swap_count_24h, "swap_count_24h") ?? 0;

    return {
      swapVolumeUsd24h,
      swapCount24h,
    };
  };
}

function expectParamsObject(value: unknown, method: string): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new RpcMethodError("invalid_params", `${method} params must be an object`);
  }

  return value;
}

function optionalBoolean(value: unknown, label: string): boolean | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "boolean") {
    throw new RpcMethodError("invalid_params", `${label} must be a boolean`);
  }

  return value;
}

function optionalSafeInteger(value: unknown, label: string): number | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "number" || !Number.isSafeInteger(value)) {
    throw new RpcMethodError("invalid_params", `${label} must be a safe integer`);
  }

  return value;
}

function optionalUnsignedIntegerString(value: unknown, label: string): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "string" || !/^(0|[1-9][0-9]*)$/.test(value)) {
    throw new RpcMethodError(
      "invalid_params",
      `${label} must be an unsigned integer string`,
    );
  }

  return value;
}

function optionalDecimalString(value: unknown, label: string): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "string" || !/^(0|[1-9][0-9]*)(\.[0-9]+)?$/.test(value)) {
    throw new RpcMethodError("invalid_params", `${label} must be a decimal string`);
  }

  return value;
}

function bigintGte(left: string, right: string): boolean {
  return BigInt(left) >= BigInt(right);
}
