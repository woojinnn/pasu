import { beforeEach, describe, expect, it, vi } from "vitest";
import { RequestType, type Message } from "@lib/types";

const OWNER = "0x1111111111111111111111111111111111111111";
const ROUTER = "0x2222222222222222222222222222222222222222";

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
    getActivePolicyRpcManifests: vi.fn(() => [{ id: "manifest-a" }]),
    pendingPut: vi.fn(async () => undefined),
    pendingDelete: vi.fn(async () => undefined),
    auditAppend: vi.fn(async () => undefined),
    evaluateWithPolicyRpc: vi.fn(),
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
  getActivePolicyRpcManifests: mocks.getActivePolicyRpcManifests,
}));
vi.mock("../storage", () => ({
  pendingPut: mocks.pendingPut,
  pendingDelete: mocks.pendingDelete,
  auditAppend: mocks.auditAppend,
}));
vi.mock("../wasm-bridge", () => ({
  EngineError: mocks.MockEngineError,
}));
vi.mock("../policy-rpc", () => ({
  evaluateWithPolicyRpc: mocks.evaluateWithPolicyRpc,
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
    mocks.evaluateWithPolicyRpc.mockResolvedValue({
      verdict: { kind: "pass" },
      audit: {
        request_id: "stubbed-tx-1",
        manifest_set_hash: "sha256:manifest",
        schema_hash: "sha256:schema",
        call_ids: [],
        methods: [],
      },
    });
  });

  it("evaluates transactions through policy-rpc coordinator", async () => {
    const result = await decideMessage(txMessage("stubbed-tx-1"));

    expect(result.ok).toBe(true);
    expect(result.verdict.kind).toBe("pass");
    expect(mocks.evaluateWithPolicyRpc).toHaveBeenCalledWith(
      txMessage("stubbed-tx-1"),
      { manifests: [{ id: "manifest-a" }] },
    );
    expect(mocks.pendingPut).toHaveBeenCalledOnce();
    expect(mocks.pendingDelete).toHaveBeenCalledWith("stubbed-tx-1");
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        policyRpc: expect.objectContaining({
          manifest_set_hash: "sha256:manifest",
          schema_hash: "sha256:schema",
        }),
      }),
    );
  });

  it("lets the user explicitly approve unsupported untyped signatures", async () => {
    const result = decideMessage(untypedMessage(), { onAwaitingUser: vi.fn() });
    await vi.waitFor(() =>
      expect(mocks.browser.windows.create).toHaveBeenCalledTimes(1),
    );

    approve("sig-1", true);

    await expect(result).resolves.toMatchObject({
      ok: true,
      verdict: { kind: "warn" },
    });
  });
});
