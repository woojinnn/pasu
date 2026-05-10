import { beforeEach, describe, expect, it, vi } from "vitest";
import { RequestType, type Message } from "@lib/types";
import { WasmDecodeError } from "../wasm-bridge.types";
import type { VerdictDto } from "../wasm-bridge.types";

const OWNER = "0x1111111111111111111111111111111111111111";
const ROUTER = "0x2222222222222222222222222222222222222222";
const WETH = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const USDT = "0xdac17f958d2ee523a2206206994597c13d831ec7";

const mocks = vi.hoisted(() => {
  class MockEngineError extends Error {
    constructor(
      readonly kind: string,
      message: string,
    ) {
      super(message);
    }
  }

  const sessionStore = new Map<string, unknown>();
  const localStore = new Map<string, unknown>();
  const runtimeMessageListeners: Array<(message: unknown) => void> = [];
  const windowRemovedListeners: Array<(windowId: number) => void> = [];

  const readStore = async (
    store: Map<string, unknown>,
    keys?: string | string[] | Record<string, unknown>,
  ): Promise<Record<string, unknown>> => {
    if (keys === undefined || keys === null)
      return Object.fromEntries(store.entries());
    const out: Record<string, unknown> = {};
    if (typeof keys === "string") {
      out[keys] = store.get(keys);
      return out;
    }
    if (Array.isArray(keys)) {
      for (const key of keys) out[key] = store.get(key);
      return out;
    }
    for (const [key, fallback] of Object.entries(keys)) {
      out[key] = store.has(key) ? store.get(key) : fallback;
    }
    return out;
  };

  return {
    MockEngineError,
    sessionStore,
    localStore,
    runtimeMessageListeners,
    windowRemovedListeners,
    ensureDefaultPoliciesInstalled: vi.fn(async () => undefined),
    fetchTier1: vi.fn(),
    intoHostSnapshot: vi.fn((tier1: unknown, windows: unknown[]) => ({
      ...(tier1 as object),
      windows,
    })),
    committedForActor: vi.fn(async () => []),
    pendingForActor: vi.fn(async () => []),
    reservePending: vi.fn(async () => undefined),
    setTxHash: vi.fn(async () => undefined),
    pendingPut: vi.fn(async () => undefined),
    pendingDelete: vi.fn(async () => undefined),
    auditAppend: vi.fn(async () => undefined),
    buildAction: vi.fn(),
    tier1FactPlan: vi.fn(),
    tier2WindowKeys: vi.fn(),
    evaluate: vi.fn(),
    browser: {
      storage: {
        session: {
          get: vi.fn((keys?: string | string[] | Record<string, unknown>) =>
            readStore(sessionStore, keys),
          ),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [key, value] of Object.entries(entries))
              sessionStore.set(key, value);
          }),
        },
        local: {
          get: vi.fn((keys?: string | string[] | Record<string, unknown>) =>
            readStore(localStore, keys),
          ),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [key, value] of Object.entries(entries))
              localStore.set(key, value);
          }),
        },
      },
      windows: {
        create: vi.fn(async () => ({ id: 99 })),
        remove: vi.fn(async () => undefined),
        onRemoved: {
          addListener: vi.fn((listener: (windowId: number) => void) => {
            windowRemovedListeners.push(listener);
          }),
          removeListener: vi.fn((listener: (windowId: number) => void) => {
            const index = windowRemovedListeners.indexOf(listener);
            if (index >= 0) windowRemovedListeners.splice(index, 1);
          }),
        },
      },
      runtime: {
        getURL: vi.fn((path: string) => `chrome-extension://scopeball/${path}`),
        sendMessage: vi.fn(async () => undefined),
        onMessage: {
          addListener: vi.fn((listener: (message: unknown) => void) => {
            runtimeMessageListeners.push(listener);
          }),
          removeListener: vi.fn((listener: (message: unknown) => void) => {
            const index = runtimeMessageListeners.indexOf(listener);
            if (index >= 0) runtimeMessageListeners.splice(index, 1);
          }),
        },
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));
vi.mock("../policies-loader", () => ({
  ensureDefaultPoliciesInstalled: mocks.ensureDefaultPoliciesInstalled,
}));
vi.mock("../facts/tier1-fetcher", () => ({
  fetchTier1: mocks.fetchTier1,
  intoHostSnapshot: mocks.intoHostSnapshot,
}));
vi.mock("../pending-deltas", () => ({
  committedForActor: mocks.committedForActor,
  pendingForActor: mocks.pendingForActor,
  reservePending: mocks.reservePending,
  setTxHash: mocks.setTxHash,
}));
vi.mock("../storage", () => ({
  pendingPut: mocks.pendingPut,
  pendingDelete: mocks.pendingDelete,
  auditAppend: mocks.auditAppend,
}));
vi.mock("../wasm-bridge", () => ({
  buildAction: mocks.buildAction,
  evaluate: mocks.evaluate,
  EngineError: mocks.MockEngineError,
  tier1FactPlan: mocks.tier1FactPlan,
  tier2WindowKeys: mocks.tier2WindowKeys,
}));

import { decideMessage } from "../orchestrator";

function txMessage(requestId = "req-1"): Message {
  return {
    requestId,
    data: {
      type: RequestType.TRANSACTION,
      chainId: 1,
      hostname: "app.example",
      transaction: {
        from: OWNER,
        to: ROUTER,
        value: "0xde0b6b3a7640000",
        data: "0x",
      },
    },
  } as Message;
}

function untypedMessage(requestId = "sig-1"): Message {
  return {
    requestId,
    data: {
      type: RequestType.UNTYPED_SIGNATURE,
      hostname: "app.example",
      message: "sign this opaque payload",
    },
  };
}

function dexAction(): Record<string, unknown> {
  return {
    dex: {
      actor: OWNER,
      target: ROUTER,
      value_wei: "0",
      facts: {
        protocol_ids: ["uniswap_v3"],
        input_tokens: [],
        output_tokens: [],
        total_input_usd: null,
        total_min_output_usd: null,
        max_fee_bps: null,
        has_zero_min_output: false,
        has_external_recipient: false,
        total_input_fraction_of_portfolio_bps: null,
        allowances_cover_inputs: null,
        window_stats: null,
      },
      oracle_requirements: [
        {
          kind: "input",
          token: {
            chain_id: 1,
            address: WETH,
            symbol: "WETH",
            decimals: 18,
            is_native: false,
          },
          raw_amount: "1000000000000000000",
        },
      ],
      trace: { steps: [] },
    },
  };
}

function tokenLite(address: string, symbol: string, decimals = 18) {
  return {
    chain_id: 1,
    address,
    symbol,
    decimals,
    is_native: false,
  };
}

function setupDexPass(
  verdict: VerdictDto = { kind: "pass" },
) {
  mocks.buildAction.mockResolvedValue(dexAction());
  mocks.tier1FactPlan.mockResolvedValue({
    tokens_for_oracle: [],
    balances: [],
    allowances: [],
    clock_required: false,
    sig_oracle_requirements: [],
  });
  mocks.fetchTier1.mockResolvedValue({
    oracle: [
      {
        token_key: `1:${WETH}`,
        usd_price: 3500,
        usd_per_unit: "3500",
        as_of_ts: 1,
        stale_sec: 0,
      },
    ],
    balances: [],
    allowances: [],
    now_ts: 1,
  });
  mocks.tier2WindowKeys.mockResolvedValue({ keys: [] });
  mocks.evaluate.mockResolvedValue(verdict);
}

function approve(requestId: string, ok: boolean): void {
  for (const listener of [...mocks.runtimeMessageListeners]) {
    listener({ type: "scopeball:verdict-decision", requestId, ok });
  }
}

describe("orchestrator", () => {
  beforeEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
    mocks.sessionStore.clear();
    mocks.localStore.clear();
    mocks.runtimeMessageListeners.length = 0;
    mocks.windowRemovedListeners.length = 0;
    mocks.committedForActor.mockResolvedValue([]);
    mocks.pendingForActor.mockResolvedValue([]);
  });

  it("normalizes hex transaction value before asking the engine to build an action", async () => {
    setupDexPass();

    await expect(decideMessage(txMessage())).resolves.toMatchObject({
      ok: true,
    });

    expect(mocks.buildAction).toHaveBeenCalledWith(
      expect.objectContaining({
        Tx: expect.objectContaining({
          value_wei: "1000000000000000000",
        }),
      }),
    );
  });

  it("reserves pending DEX window deltas after a passing decision", async () => {
    setupDexPass();

    await expect(decideMessage(txMessage())).resolves.toMatchObject({
      ok: true,
    });

    expect(mocks.reservePending).toHaveBeenCalledWith(
      expect.objectContaining({
        requestId: "req-1",
        actor: OWNER,
        chainId: 1,
        windowEntries: [
          { name: "swapVolumeUsd24h", value: "3500.0000" },
          { name: "swapCount24h", value: "1" },
        ],
      }),
    );
  });

  it("still reserves the DEX count delta when oracle input USD is unavailable", async () => {
    setupDexPass();
    mocks.fetchTier1.mockResolvedValue({
      oracle: [],
      balances: [],
      allowances: [],
      now_ts: 1,
    });

    await expect(decideMessage(txMessage())).resolves.toMatchObject({
      ok: true,
    });

    expect(mocks.reservePending).toHaveBeenCalledWith(
      expect.objectContaining({
        requestId: "req-1",
        windowEntries: [{ name: "swapCount24h", value: "1" }],
      }),
    );
  });

  it("warns when planned oracle requirements are missing from the tier1 snapshot", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    setupDexPass();
    mocks.tier1FactPlan.mockResolvedValue({
      tokens_for_oracle: [tokenLite(WETH, "WETH"), tokenLite(USDT, "USDT", 6)],
      balances: [],
      allowances: [],
      clock_required: false,
      sig_oracle_requirements: [],
    });
    mocks.fetchTier1.mockResolvedValue({
      oracle: [],
      balances: [],
      allowances: [],
    });

    await expect(decideMessage(txMessage("oracle-gap-1"))).resolves.toMatchObject(
      {
        ok: true,
        verdict: { kind: "pass" },
      },
    );

    expect(warnSpy).toHaveBeenCalledWith(
      "[Scopeball SW] oracle_requirements declared but no entries returned — dex/USD policies will silently miss",
      {
        requestId: "oracle-gap-1",
        hostname: "app.example",
        requested: [`1:${WETH}`, `1:${USDT}`],
        missing: [`1:${WETH}`, `1:${USDT}`],
      },
    );

    warnSpy.mockRestore();
  });

  it("warns only for unresolved oracle requirements when part of the tier1 snapshot resolves", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    setupDexPass();
    mocks.tier1FactPlan.mockResolvedValue({
      tokens_for_oracle: [tokenLite(WETH, "WETH"), tokenLite(USDT, "USDT", 6)],
      balances: [],
      allowances: [],
      clock_required: false,
      sig_oracle_requirements: [],
    });
    mocks.fetchTier1.mockResolvedValue({
      oracle: [
        {
          token_key: `1:${WETH}`,
          usd_price: 3500,
          usd_per_unit: "3500",
          as_of_ts: 1,
          stale_sec: 0,
        },
      ],
      balances: [],
      allowances: [],
    });

    await expect(decideMessage(txMessage("oracle-gap-2"))).resolves.toMatchObject(
      {
        ok: true,
        verdict: { kind: "pass" },
      },
    );

    expect(warnSpy).toHaveBeenCalledTimes(1);
    expect(warnSpy).toHaveBeenCalledWith(
      "[Scopeball SW] oracle_requirements declared but no entries returned — dex/USD policies will silently miss",
      {
        requestId: "oracle-gap-2",
        hostname: "app.example",
        requested: [`1:${WETH}`, `1:${USDT}`],
        missing: [`1:${USDT}`],
      },
    );

    warnSpy.mockRestore();
  });

  it("opens a warning decision window and reserves only after user approval", async () => {
    setupDexPass({
      kind: "warn",
      matched: [
        {
          policy_id: "policy::warn",
          reason: null,
          severity: "warn",
          origin: "tx",
        },
      ],
    });
    const awaitingUser = vi.fn();

    const result = decideMessage(txMessage(), { onAwaitingUser: awaitingUser });
    await vi.waitFor(() =>
      expect(mocks.browser.windows.create).toHaveBeenCalledTimes(1),
    );

    expect(awaitingUser).toHaveBeenCalledTimes(1);
    expect(mocks.reservePending).not.toHaveBeenCalled();
    approve("req-1", true);

    await expect(result).resolves.toMatchObject({ ok: true });
    expect(mocks.reservePending).toHaveBeenCalledTimes(1);
  });

  it("turns engine timeout into a user-confirmable warning", async () => {
    vi.useFakeTimers();
    mocks.buildAction.mockReturnValue(new Promise(() => undefined));
    const awaitingUser = vi.fn();

    const result = decideMessage(txMessage("timeout-1"), {
      onAwaitingUser: awaitingUser,
    });
    // HARD_TIMEOUT_MS is 8 s — see orchestrator.ts. Bumped from 3 s so
    // legitimate cold-cache lifecycle work (per-dimension oracle fetch +
    // storage round-trips) doesn't trip a timeout-warn for routine
    // wallet_sendCalls passes.
    await vi.advanceTimersByTimeAsync(8_000);
    await vi.waitFor(() =>
      expect(mocks.browser.windows.create).toHaveBeenCalledTimes(1),
    );

    expect(awaitingUser).toHaveBeenCalledTimes(1);
    approve("timeout-1", true);

    await expect(result).resolves.toMatchObject({
      ok: true,
      verdict: { kind: "warn" },
    });
    expect(mocks.reservePending).not.toHaveBeenCalled();
  });

  it("surfaces malformed tier1 plans as engine-error failures", async () => {
    setupDexPass();
    mocks.tier1FactPlan.mockRejectedValue(
      new WasmDecodeError(
        "$: expected tier1 plan",
        "tier1FactPlan",
        { wrong: "shape" },
      ),
    );

    await expect(decideMessage(txMessage("decode-error-1"))).resolves.toEqual({
      ok: false,
      verdict: {
        kind: "fail",
        matched: [
          expect.objectContaining({
            policy_id: expect.stringMatching(/^__engine::/),
            origin: "engine_error",
          }),
        ],
      },
    });
  });

  it("lets the user explicitly approve unsupported untyped signatures", async () => {
    const result = decideMessage(untypedMessage(), { onAwaitingUser: vi.fn() });
    await vi.waitFor(() =>
      expect(mocks.browser.windows.create).toHaveBeenCalledTimes(1),
    );

    expect(mocks.buildAction).not.toHaveBeenCalled();
    approve("sig-1", true);

    await expect(result).resolves.toMatchObject({
      ok: true,
      verdict: { kind: "warn" },
    });
  });
});
