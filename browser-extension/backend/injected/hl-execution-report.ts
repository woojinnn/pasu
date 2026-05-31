import { RequestType } from "@lib/types";
import type {
  ExecutionReportOutcome,
  ExecutionReportPayload,
  VenueOrderPayload,
} from "@lib/types";

interface HyperliquidObservation {
  httpStatus: number;
  responseJson?: unknown;
  responseText?: string;
  statusIndex?: number;
}

export function classifyHyperliquidExchangeResponse(
  httpStatus: number,
  body: unknown,
  statusIndex = 0,
): ExecutionReportOutcome {
  if (httpStatus < 200 || httpStatus >= 300) {
    return {
      kind: "venue_rejected",
      venue: "hyperliquid",
      reason: `http ${httpStatus}`,
    };
  }

  const root = asRecord(body);
  if (!root) {
    return {
      kind: "venue_submitted",
      venue: "hyperliquid",
    };
  }
  const status = typeof root?.status === "string" ? root.status : undefined;
  if (status === "err") {
    return {
      kind: "venue_rejected",
      venue: "hyperliquid",
      reason:
        extractReason(root.response) ??
        extractReason(root.error) ??
        "venue rejected",
    };
  }

  if (status === "ok") {
    const statusEntry = statusAt(root, statusIndex);
    const statusError = extractReason(asRecord(statusEntry)?.error);
    if (statusError) {
      return {
        kind: "venue_rejected",
        venue: "hyperliquid",
        reason: statusError,
      };
    }
    const venueOrderId = extractOrderId(root, statusIndex);
    const outcome: ExecutionReportOutcome = {
      kind: "venue_accepted",
      venue: "hyperliquid",
    };
    if (venueOrderId) outcome.venue_order_id = venueOrderId;
    return outcome;
  }

  return {
    kind: "venue_submitted",
    venue: "hyperliquid",
  };
}

export function buildHyperliquidExecutionReport(
  payload: VenueOrderPayload,
  observation: HyperliquidObservation,
): ExecutionReportPayload {
  const body = observation.responseJson ?? observation.responseText;
  const outcome = withClientOrderId(
    classifyHyperliquidExchangeResponse(
      observation.httpStatus,
      body,
      observation.statusIndex ?? 0,
    ),
    payload,
  );
  const metadata: Record<string, unknown> = {
    source: "hyperliquid-fetch-hook",
    endpoint: payload.endpoint,
    hostname: payload.hostname,
    symbol: payload.symbol,
    action_kind: payload.hlAction.kind,
    http_status: observation.httpStatus,
  };
  if (observation.responseJson !== undefined) {
    metadata.response = observation.responseJson;
  } else if (observation.responseText !== undefined) {
    metadata.response = observation.responseText;
  }

  const report: ExecutionReportPayload = {
    type: RequestType.EXECUTION_REPORT,
    hostname: payload.hostname,
    outcome,
    metadata,
  };
  if (payload.wallet_id) report.wallet_id = payload.wallet_id;
  return report;
}

function withClientOrderId(
  outcome: ExecutionReportOutcome,
  payload: VenueOrderPayload,
): ExecutionReportOutcome {
  const clientOrderId =
    payload.hlAction.kind === "order" ? payload.hlAction.order.c : undefined;
  if (!clientOrderId) return outcome;
  if (outcome.kind === "venue_accepted") {
    return { ...outcome, client_order_id: clientOrderId };
  }
  if (outcome.kind === "venue_submitted") {
    return { ...outcome, client_order_id: clientOrderId };
  }
  return outcome;
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null
    ? (value as Record<string, unknown>)
    : undefined;
}

function statusAt(
  root: Record<string, unknown>,
  statusIndex: number,
): unknown | undefined {
  const response = asRecord(root.response);
  const data = asRecord(response?.data);
  const statuses = Array.isArray(data?.statuses) ? data.statuses : [];
  return statuses[statusIndex];
}

function extractReason(value: unknown): string | undefined {
  if (typeof value === "string") return value;
  const record = asRecord(value);
  if (!record) return undefined;
  if (typeof record.error === "string") return record.error;
  if (typeof record.message === "string") return record.message;
  return undefined;
}

function extractOrderId(
  root: Record<string, unknown>,
  statusIndex: number,
): string | undefined {
  const statuses = [statusAt(root, statusIndex)];
  for (const status of statuses) {
    const record = asRecord(status);
    const resting = asRecord(record?.resting);
    const filled = asRecord(record?.filled);
    const oid = resting?.oid ?? filled?.oid;
    if (typeof oid === "string" || typeof oid === "number") {
      return String(oid);
    }
  }
  return undefined;
}
