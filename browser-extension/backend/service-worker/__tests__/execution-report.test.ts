import { beforeEach, describe, expect, it, vi } from "vitest";
import { RequestType, type ExecutionReportPayload } from "@lib/types";

const mocks = vi.hoisted(() => ({
  localStore: new Map<string, unknown>(),
  browser: {
    storage: {
      local: {
        get: vi.fn(async (key: string) => ({ [key]: mocks.localStore.get(key) })),
        set: vi.fn(async (entries: Record<string, unknown>) => {
          for (const [key, value] of Object.entries(entries)) {
            mocks.localStore.set(key, value);
          }
        }),
        remove: vi.fn(async (key: string) => {
          mocks.localStore.delete(key);
        }),
      },
    },
  },
}));

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import { reportExecutionOutcome } from "../execution-report";

function report(): ExecutionReportPayload {
  return {
    type: RequestType.EXECUTION_REPORT,
    hostname: "app.hyperliquid.xyz",
    wallet_id: {
      address: "0x362E7e9e630481631D7C804dfe50e24b53250925",
      chains: ["hyperliquid"],
    },
    outcome: {
      kind: "venue_accepted",
      venue: "hyperliquid",
      venue_order_id: "123",
    },
    metadata: { source: "test" },
  };
}

describe("reportExecutionOutcome", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
  });

  it("appends the full execution report to chrome.storage.local", async () => {
    await reportExecutionOutcome(report());

    const rows = mocks.localStore.get("execution-reports:log") as
      | Array<ExecutionReportPayload & { id: string; ts: number }>
      | undefined;
    expect(rows).toHaveLength(1);
    expect(rows?.[0]).toMatchObject(report());
    expect(rows?.[0]?.id).toEqual(expect.any(String));
    expect(rows?.[0]?.ts).toEqual(expect.any(Number));
  });

  it("ignores non-execution-report payloads", async () => {
    await reportExecutionOutcome({
      ...report(),
      type: RequestType.TRANSACTION,
    } as unknown as ExecutionReportPayload);

    expect(mocks.browser.storage.local.set).not.toHaveBeenCalled();
  });
});
