import { beforeEach, describe, expect, it, vi } from "vitest";

import { Identifier } from "@lib/identifier";
import { generateRequestId } from "@lib/messages";
import { RequestType } from "@lib/types";

const streamState = vi.hoisted(() => ({
  instances: [] as MockStream[],
  responses: [] as boolean[],
  // C1: verdicts now travel over an authenticated MessageChannel, not the
  // stream. The harness simulates the ISOLATED bridge: it transfers a port to
  // the MAIN-world receiver (created at module import) and posts verdicts over
  // the writer port it keeps here.
  verdictPort: null as MessagePort | null,
}));

/** Simulate the ISOLATED bridge transferring its verdict reader port to the
 *  MAIN-world receiver, and keep the writer port to post verdicts over. */
function ensureVerdictPort(): MessagePort {
  if (!streamState.verdictPort) {
    const channel = new MessageChannel();
    window.dispatchEvent(
      new MessageEvent("message", {
        data: { [Identifier.VERDICT_PORT_INIT]: true },
        ports: [channel.port2],
        source: window,
      } as MessageEventInit),
    );
    streamState.verdictPort = channel.port1;
  }
  return streamState.verdictPort;
}

class MockStream {
  listeners: Array<(message: { requestId: string; data: boolean }) => void> =
    [];
  writes: Array<{ requestId: string; data: unknown }> = [];

  on(
    event: "data",
    callback: (message: { requestId: string; data: boolean }) => void,
  ): void {
    if (event === "data") this.listeners.push(callback);
  }

  removeListener(
    event: "data",
    callback: (message: { requestId: string; data: boolean }) => void,
  ): void {
    if (event !== "data") return;
    this.listeners = this.listeners.filter((listener) => listener !== callback);
  }

  write(message: { requestId: string; data: unknown }): boolean {
    this.writes.push(message);
    // Execution-report writes are fire-and-forget (no verdict). Only policy
    // requests receive a verdict — delivered over the authenticated port (C1).
    if ((message.data as { type?: string } | undefined)?.type === "execution-report") {
      return true;
    }
    const data = streamState.responses.shift() ?? true;
    const port = ensureVerdictPort();
    queueMicrotask(() => {
      port.postMessage({ requestId: message.requestId, data });
    });
    return true;
  }
}

function streamWrites(): Array<{ requestId: string; data: unknown }> {
  return streamState.instances[0]?.writes ?? [];
}

function policyWrites(): Array<{ requestId: string; data: unknown }> {
  return streamWrites().filter(
    (write) => (write.data as { type?: string }).type !== "execution-report",
  );
}

function executionReportWrites(): Array<{ requestId: string; data: unknown }> {
  return streamWrites().filter(
    (write) => (write.data as { type?: string }).type === "execution-report",
  );
}

vi.mock("@metamask/post-message-stream", () => ({
  WindowPostMessageStream: class extends MockStream {
    constructor() {
      super();
      streamState.instances.push(this);
    }
  },
}));

describe("inpage provider proxy", () => {
  beforeEach(() => {
    vi.resetModules();
    streamState.instances.length = 0;
    streamState.responses.length = 0;
    streamState.verdictPort = null;
    vi.stubGlobal("location", { hostname: "app.example" });
    delete (window as any).ethereum;
  });

  it("wraps a provider assigned to window.ethereum before the first request", async () => {
    streamState.responses.push(true);

    await import("../proxy-injected-providers");

    const originalRequest = vi.fn(async (request: { method?: string }) => {
      if (request.method === "eth_chainId") return "0x1";
      return "sent";
    });
    (window as any).ethereum = {
      request: originalRequest,
    };

    const tx = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x",
    };

    await expect(
      (window as any).ethereum.request({
        method: "eth_sendTransaction",
        params: [tx],
      }),
    ).resolves.toBe("sent");

    expect(policyWrites()).toHaveLength(1);
    expect(
      originalRequest.mock.calls.map(([request]) => request.method),
    ).toEqual(["eth_chainId", "eth_sendTransaction"]);
  });

  it("ignores a page-forged window-bus verdict — only the authenticated port decides (C1)", async () => {
    // The genuine SW verdict is DENY. The page (same MAIN realm) forges an
    // `allow` over `window.postMessage` for the exact (deterministic) requestId —
    // the original C1 exploit. The proxy must read ONLY the authenticated port,
    // so the forgery is ignored and the genuine deny blocks the tx.
    streamState.responses.push(false);

    await import("../proxy-injected-providers");

    const originalRequest = vi.fn(async (request: { method?: string }) => {
      if (request.method === "eth_chainId") return "0x1";
      return "sent";
    });
    (window as any).ethereum = { request: originalRequest };

    const tx = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x",
    };
    // The page can recompute the deterministic requestId (objectHash of the tx).
    const forgedRequestId = generateRequestId({
      type: RequestType.TRANSACTION,
      transaction: tx,
    } as Parameters<typeof generateRequestId>[0]);

    const pending = (window as any).ethereum.request({
      method: "eth_sendTransaction",
      params: [tx],
    });

    // Forge `allow` on the window bus a few times (plain message + a fake
    // port-init shape) — all must be ignored by the receiver.
    for (let i = 0; i < 3; i++) {
      window.dispatchEvent(
        new MessageEvent("message", {
          data: { requestId: forgedRequestId, data: true },
          source: window,
        } as MessageEventInit),
      );
      window.dispatchEvent(
        new MessageEvent("message", {
          data: {
            [Identifier.VERDICT_PORT_INIT]: true,
            requestId: forgedRequestId,
            data: true,
          },
          source: window,
        } as MessageEventInit),
      );
    }

    // Genuine DENY (over the authenticated port) wins → tx rejected, never forwarded.
    await expect(pending).rejects.toThrow();
    expect(
      originalRequest.mock.calls.map(([request]) => request.method),
    ).toEqual(["eth_chainId"]);
  });

  it("reports an onchain submission when the wallet confirms eth_sendTransaction", async () => {
    streamState.responses.push(true);

    await import("../proxy-injected-providers");

    const originalRequest = vi.fn(async (request: { method?: string }) => {
      if (request.method === "eth_chainId") return "0x1";
      return "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    });
    (window as any).ethereum = {
      request: originalRequest,
    };

    const tx = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x",
    };

    await expect(
      (window as any).ethereum.request({
        method: "eth_sendTransaction",
        params: [tx],
      }),
    ).resolves.toBe(
      "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );

    expect(executionReportWrites()).toHaveLength(1);
    expect(executionReportWrites()[0].data).toEqual(
      expect.objectContaining({
        type: "execution-report",
        wallet_id: {
          address: tx.from,
          chains: ["eip155:1"],
        },
        outcome: {
          kind: "onchain_submitted",
          chain: "eip155:1",
          tx_hash:
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        },
        metadata: expect.objectContaining({
          source: "provider-proxy",
          method: "eth_sendTransaction",
        }),
      }),
    );
  });

  it("reports wallet rejection when the wallet rejects eth_sendTransaction", async () => {
    streamState.responses.push(true);

    await import("../proxy-injected-providers");

    const walletError = Object.assign(new Error("User rejected the request."), {
      code: 4001,
    });
    const originalRequest = vi.fn(async (request: { method?: string }) => {
      if (request.method === "eth_chainId") return "0x1";
      throw walletError;
    });
    (window as any).ethereum = {
      request: originalRequest,
    };

    const tx = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x",
    };

    await expect(
      (window as any).ethereum.request({
        method: "eth_sendTransaction",
        params: [tx],
      }),
    ).rejects.toMatchObject({ code: 4001 });

    expect(executionReportWrites()).toHaveLength(1);
    expect(executionReportWrites()[0].data).toEqual(
      expect.objectContaining({
        type: "execution-report",
        wallet_id: {
          address: tx.from,
          chains: ["eip155:1"],
        },
        outcome: {
          kind: "wallet_rejected",
          reason: "User rejected the request.",
        },
        metadata: expect.objectContaining({
          source: "provider-proxy",
          method: "eth_sendTransaction",
        }),
      }),
    );
  });

  it("reports a wallet signature when the wallet confirms typed data", async () => {
    streamState.responses.push(true);

    await import("../proxy-injected-providers");

    const originalRequest = vi.fn(async (request: { method?: string }) => {
      if (request.method === "eth_chainId") return "0x1";
      return "0xsigned";
    });
    (window as any).ethereum = {
      request: originalRequest,
    };

    const address = "0x1111111111111111111111111111111111111111";

    await expect(
      (window as any).ethereum.request({
        method: "eth_signTypedData_v4",
        params: [address, { domain: { chainId: 1 }, message: { ok: true } }],
      }),
    ).resolves.toBe("0xsigned");

    expect(executionReportWrites()).toHaveLength(1);
    expect(executionReportWrites()[0].data).toEqual(
      expect.objectContaining({
        type: "execution-report",
        wallet_id: {
          address,
          chains: ["eip155:1"],
        },
        outcome: {
          kind: "wallet_signed",
          signature: "0xsigned",
        },
        metadata: expect.objectContaining({
          source: "provider-proxy",
          method: "eth_signTypedData_v4",
        }),
      }),
    );
  });

  it("gates payload-only send(payload) transaction calls before forwarding", async () => {
    const originalSend = vi.fn(() => "sent");
    const provider = {
      request: vi.fn(async (request: { method: string }) => {
        if (request.method === "eth_chainId") return "0x1";
        return "request-result";
      }),
      send: originalSend,
    };
    (window as any).ethereum = provider;

    await import("../proxy-injected-providers");

    const tx = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x",
    };
    const result = (window as any).ethereum.send({
      method: "eth_sendTransaction",
      params: [tx],
    });

    expect(originalSend).not.toHaveBeenCalled();
    await expect(result).resolves.toBe("sent");
    expect(policyWrites()).toHaveLength(1);
    expect(originalSend).toHaveBeenCalledTimes(1);
  });

  it("gates payload-only sendAsync(payload) transaction calls before forwarding", async () => {
    const originalSendAsync = vi.fn(() => "sent");
    const provider = {
      request: vi.fn(async (request: { method: string }) => {
        if (request.method === "eth_chainId") return "0x1";
        return "request-result";
      }),
      sendAsync: originalSendAsync,
    };
    (window as any).ethereum = provider;

    await import("../proxy-injected-providers");

    const tx = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x",
    };
    const result = (window as any).ethereum.sendAsync({
      method: "eth_sendTransaction",
      params: [tx],
    });

    expect(originalSendAsync).not.toHaveBeenCalled();
    await expect(result).resolves.toBe("sent");
    expect(policyWrites()).toHaveLength(1);
    expect(originalSendAsync).toHaveBeenCalledTimes(1);
  });

  it("N8: gates eth_signTransaction like a transaction (deny throws 4001, never forwards)", async () => {
    streamState.responses.push(false); // SW denies
    const originalRequest = vi.fn(async (request: { method: string }) => {
      if (request.method === "eth_chainId") return "0x1";
      return "signed-tx-hex";
    });
    (window as any).ethereum = { request: originalRequest };

    await import("../proxy-injected-providers");

    const tx = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x",
    };
    await expect(
      (window as any).ethereum.request({
        method: "eth_signTransaction",
        params: [tx],
      }),
    ).rejects.toMatchObject({ code: 4001 });

    // It was gated as a transaction (one policy write of type "transaction") and
    // the underlying provider was NEVER asked to sign (blocked before forward).
    expect(policyWrites()).toHaveLength(1);
    expect((policyWrites()[0].data as { type?: string }).type).toBe(
      "transaction",
    );
    expect(
      originalRequest.mock.calls.map(([r]) => r.method),
    ).not.toContain("eth_signTransaction");
  });

  it("N4: gates each eth_sendTransaction in a send([...]) JSON-RPC batch array (any deny blocks)", async () => {
    streamState.responses.push(true, false); // leg1 allow, leg2 deny
    const originalSend = vi.fn(() => "batch-sent");
    const provider = {
      request: vi.fn(async (request: { method: string }) => {
        if (request.method === "eth_chainId") return "0x1";
        return "request-result";
      }),
      send: originalSend,
    };
    (window as any).ethereum = provider;

    await import("../proxy-injected-providers");

    const tx1 = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x",
    };
    const tx2 = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x3333333333333333333333333333333333333333",
      value: "0x0",
      data: "0x",
    };

    // A JSON-RPC 2.0 batch ARRAY has no top-level `.method`, so the per-method
    // gate used to see `undefined` and forward the whole array ungated (N4).
    await expect(
      (window as any).ethereum.send([
        { method: "eth_sendTransaction", params: [tx1] },
        { method: "eth_sendTransaction", params: [tx2] },
      ]),
    ).rejects.toMatchObject({ code: 4001 });

    // Both legs were gated (2 transaction writes); the denied leg blocked the
    // batch before it reached the native provider.
    expect(policyWrites()).toHaveLength(2);
    expect(originalSend).not.toHaveBeenCalled();
  });

  it("N4: gates each eth_sendTransaction in a request([...]) batch array (any deny blocks)", async () => {
    streamState.responses.push(true, false); // leg1 allow, leg2 deny
    const originalRequest = vi.fn(async (request: unknown) => {
      if ((request as { method?: string }).method === "eth_chainId") return "0x1";
      return "request-result";
    });
    (window as any).ethereum = { request: originalRequest };

    await import("../proxy-injected-providers");

    const tx1 = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x",
    };
    const tx2 = {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x3333333333333333333333333333333333333333",
      value: "0x0",
      data: "0x",
    };

    // EIP-1193 `request` takes a single object, but a non-standard wallet may
    // honour an array; the array has no top-level `.method` so it used to
    // forward ungated.
    await expect(
      (window as any).ethereum.request([
        { method: "eth_sendTransaction", params: [tx1] },
        { method: "eth_sendTransaction", params: [tx2] },
      ]),
    ).rejects.toMatchObject({ code: 4001 });

    expect(policyWrites()).toHaveLength(2);
    // The native request was only used for eth_chainId reads, never for the
    // ungated batch array.
    expect(
      originalRequest.mock.calls.every(
        ([r]) => (r as { method?: string }).method === "eth_chainId",
      ),
    ).toBe(true);
  });

  it("wraps newly added provider methods without double-gating request", async () => {
    streamState.responses.push(true);
    const provider = {
      request: vi.fn(async (request: { method?: string }) => {
        if (request.method === "eth_chainId") return "0x1";
        return "sent";
      }),
    };
    (window as any).ethereum = provider;

    await import("../proxy-injected-providers");
    (provider as any).sendAsync = vi.fn(() => "legacy-sent");

    window.dispatchEvent(
      new CustomEvent("eip6963:announceProvider", {
        detail: {
          info: {
            uuid: "metamask-provider",
            name: "MetaMask",
            icon: "data:image/png;base64,AA==",
            rdns: "io.metamask",
          },
          provider,
        },
      }),
    );

    await expect(
      (window as any).ethereum.request({
        method: "eth_sendTransaction",
        params: [
          {
            from: "0x1111111111111111111111111111111111111111",
            to: "0x2222222222222222222222222222222222222222",
            value: "0x0",
            data: "0x",
          },
        ],
      }),
    ).resolves.toBe("sent");

    expect(policyWrites()).toHaveLength(1);
  });

  it("checks metamask_batch inner wallet actions before forwarding the batch", async () => {
    streamState.responses.push(true, false);
    const originalRequest = vi.fn(
      async (_request: { method?: string }) => "sent",
    );
    const provider = {
      chainId: "0x1",
      request: originalRequest,
    };
    (window as any).ethereum = provider;

    await import("../proxy-injected-providers");

    const address = "0x1111111111111111111111111111111111111111";
    const tx = {
      from: address,
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x",
    };

    await expect(
      (window as any).ethereum.request({
        method: "metamask_batch",
        params: [
          [
            {
              jsonrpc: "2.0",
              id: 1,
              method: "eth_sendTransaction",
              params: [tx],
            },
            {
              jsonrpc: "2.0",
              id: 2,
              method: "personal_sign",
              params: ["sign me", address],
            },
          ],
        ],
      }),
    ).rejects.toMatchObject({ code: 4001 });

    expect(
      originalRequest.mock.calls.filter(
        ([request]) => request?.method === "metamask_batch",
      ),
    ).toHaveLength(0);
    expect(policyWrites()).toHaveLength(2);
    expect(policyWrites().map((write) => write.data)).toEqual([
      expect.objectContaining({
        type: "transaction",
        transaction: tx,
      }),
      expect.objectContaining({
        type: "untyped-signature",
        message: "sign me",
      }),
    ]);
  });

  it("gates every wallet_sendCalls call as a transaction and blocks on any denial", async () => {
    streamState.responses.push(true, false);
    const originalRequest = vi.fn(
      async (_request: { method?: string }) => "sent",
    );
    const provider = {
      chainId: "0x1",
      request: originalRequest,
    };
    (window as any).ethereum = provider;

    await import("../proxy-injected-providers");

    const from = "0x1111111111111111111111111111111111111111";
    const firstCall = {
      to: "0x2222222222222222222222222222222222222222",
      value: "0x0",
      data: "0x1234",
    };
    const secondCall = {
      to: "0x3333333333333333333333333333333333333333",
      data: "0xabcd",
    };

    await expect(
      (window as any).ethereum.request({
        method: "wallet_sendCalls",
        params: [
          {
            chainId: "0x2105",
            from,
            calls: [firstCall, secondCall],
          },
        ],
      }),
    ).rejects.toMatchObject({ code: 4001 });

    expect(
      originalRequest.mock.calls.filter(
        ([request]) => request?.method === "wallet_sendCalls",
      ),
    ).toHaveLength(0);
    expect(policyWrites()).toHaveLength(2);
    expect(policyWrites().map((write) => write.data)).toEqual([
      expect.objectContaining({
        type: "transaction",
        chainId: 8453,
        hostname: "app.example",
        transaction: { from, ...firstCall },
      }),
      expect.objectContaining({
        type: "transaction",
        chainId: 8453,
        hostname: "app.example",
        transaction: { from, ...secondCall },
      }),
    ]);
  });

  it("reports wallet confirmation when wallet_sendCalls succeeds", async () => {
    streamState.responses.push(true);
    const originalRequest = vi.fn(async (request: { method?: string }) => {
      if (request.method === "eth_chainId") return "0x1";
      return { id: "bundle-1" };
    });
    const provider = {
      chainId: "0x1",
      request: originalRequest,
    };
    (window as any).ethereum = provider;

    await import("../proxy-injected-providers");

    const from = "0x1111111111111111111111111111111111111111";
    await expect(
      (window as any).ethereum.request({
        method: "wallet_sendCalls",
        params: [
          {
            chainId: "0x2105",
            from,
            calls: [
              {
                to: "0x2222222222222222222222222222222222222222",
                data: "0x1234",
              },
            ],
          },
        ],
      }),
    ).resolves.toEqual({ id: "bundle-1" });

    expect(policyWrites()).toHaveLength(1);
    expect(executionReportWrites()).toHaveLength(1);
    expect(executionReportWrites()[0].data).toEqual(
      expect.objectContaining({
        type: "execution-report",
        wallet_id: {
          address: from,
          chains: ["eip155:8453"],
        },
        outcome: {
          kind: "wallet_confirmed",
          method: "wallet_sendCalls",
        },
        metadata: expect.objectContaining({
          source: "provider-proxy",
          method: "wallet_sendCalls",
        }),
      }),
    );
  });

  it("stops checking wallet_sendCalls after the first denied call", async () => {
    streamState.responses.push(false);
    const originalRequest = vi.fn(async () => "sent");
    const provider = {
      chainId: "0x1",
      request: originalRequest,
    };
    (window as any).ethereum = provider;

    await import("../proxy-injected-providers");

    await expect(
      (window as any).ethereum.request({
        method: "wallet_sendCalls",
        params: [
          {
            chainId: "0x1",
            from: "0x1111111111111111111111111111111111111111",
            calls: [
              {
                to: "0x2222222222222222222222222222222222222222",
                data: "0x1234",
              },
              {
                to: "0x3333333333333333333333333333333333333333",
                data: "0xabcd",
              },
            ],
          },
        ],
      }),
    ).rejects.toMatchObject({ code: 4001 });

    expect(originalRequest).not.toHaveBeenCalledWith(
      expect.objectContaining({ method: "wallet_sendCalls" }),
    );
    expect(policyWrites()).toHaveLength(1);
  });

  it("caches chain id reads for repeated gated requests on the same provider", async () => {
    streamState.responses.push(true, true);
    const originalRequest = vi.fn(async (request: { method?: string }) => {
      if (request.method === "eth_chainId") return "0x1";
      return "sent";
    });
    const provider = {
      request: originalRequest,
    };
    (window as any).ethereum = provider;

    await import("../proxy-injected-providers");

    await (window as any).ethereum.request({
      method: "eth_sendTransaction",
      params: [
        {
          from: "0x1111111111111111111111111111111111111111",
          to: "0x2222222222222222222222222222222222222222",
          value: "0x0",
          data: "0x1111",
        },
      ],
    });
    await (window as any).ethereum.request({
      method: "eth_sendTransaction",
      params: [
        {
          from: "0x1111111111111111111111111111111111111111",
          to: "0x3333333333333333333333333333333333333333",
          value: "0x0",
          data: "0x2222",
        },
      ],
    });

    expect(
      originalRequest.mock.calls.filter(
        ([request]) => request?.method === "eth_chainId",
      ),
    ).toHaveLength(1);
    expect(policyWrites()).toHaveLength(2);
  });

  it("does not reannounce an EIP-6963 provider that could not be wrapped", async () => {
    const dispatchSpy = vi.spyOn(window, "dispatchEvent");

    await import("../proxy-injected-providers");
    dispatchSpy.mockClear();

    window.dispatchEvent(
      new CustomEvent("eip6963:announceProvider", {
        detail: {
          info: {
            uuid: "bad-provider",
            name: "Frozen",
            icon: "data:image/png;base64,AA==",
            rdns: "io.metamask",
          },
          provider: {},
        },
      }),
    );

    const dambiAnnouncements = dispatchSpy.mock.calls.filter(([event]) => {
      return (
        event instanceof CustomEvent &&
        event.type === "eip6963:announceProvider" &&
        event.detail?.info?.rdns === "dev.dambi.wrapper"
      );
    });
    expect(dambiAnnouncements).toHaveLength(0);

    dispatchSpy.mockRestore();
  });

  it("reannounces a wrapped EIP-6963 provider and gates requests through it", async () => {
    streamState.responses.push(true);
    const dispatchSpy = vi.spyOn(window, "dispatchEvent");
    const originalRequest = vi.fn(async (request: { method?: string }) => {
      if (request.method === "eth_chainId") return "0x1";
      return "sent";
    });
    const provider = {
      request: originalRequest,
    };

    await import("../proxy-injected-providers");
    dispatchSpy.mockClear();

    window.dispatchEvent(
      new CustomEvent("eip6963:announceProvider", {
        detail: {
          info: {
            uuid: "metamask-provider",
            name: "MetaMask",
            icon: "data:image/png;base64,AA==",
            rdns: "io.metamask",
          },
          provider,
        },
      }),
    );

    const dambiAnnouncement = dispatchSpy.mock.calls.find(([event]) => {
      return (
        event instanceof CustomEvent &&
        event.type === "eip6963:announceProvider" &&
        event.detail?.info?.rdns === "dev.dambi.wrapper"
      );
    })?.[0] as CustomEvent | undefined;

    expect(dambiAnnouncement?.detail.provider).toBe(provider);

    await expect(
      dambiAnnouncement?.detail.provider.request({
        method: "eth_sendTransaction",
        params: [
          {
            from: "0x1111111111111111111111111111111111111111",
            to: "0x2222222222222222222222222222222222222222",
            value: "0x0",
            data: "0x",
          },
        ],
      }),
    ).resolves.toBe("sent");

    expect(policyWrites()).toHaveLength(1);
    dispatchSpy.mockRestore();
  });
});
