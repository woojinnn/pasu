import { beforeEach, describe, expect, it, vi } from "vitest";

const streamState = vi.hoisted(() => ({
  instances: [] as MockStream[],
  responses: [] as boolean[],
}));

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
    const data = streamState.responses.shift() ?? true;
    queueMicrotask(() => {
      for (const listener of this.listeners)
        listener({ requestId: message.requestId, data });
    });
    return true;
  }
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

    expect(streamState.instances[0].writes).toHaveLength(1);
    expect(
      originalRequest.mock.calls.map(([request]) => request.method),
    ).toEqual(["eth_chainId", "eth_sendTransaction"]);
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
    expect(streamState.instances[0].writes).toHaveLength(1);
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
    expect(streamState.instances[0].writes).toHaveLength(1);
    expect(originalSendAsync).toHaveBeenCalledTimes(1);
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

    expect(streamState.instances[0].writes).toHaveLength(1);
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
    expect(streamState.instances[0].writes).toHaveLength(2);
    expect(streamState.instances[0].writes.map((write) => write.data)).toEqual([
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
    expect(streamState.instances[0].writes).toHaveLength(2);
    expect(streamState.instances[0].writes.map((write) => write.data)).toEqual([
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
    expect(streamState.instances[0].writes).toHaveLength(1);
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
    expect(streamState.instances[0].writes).toHaveLength(2);
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

    const scopeballAnnouncements = dispatchSpy.mock.calls.filter(([event]) => {
      return (
        event instanceof CustomEvent &&
        event.type === "eip6963:announceProvider" &&
        event.detail?.info?.rdns === "dev.scopeball.wrapper"
      );
    });
    expect(scopeballAnnouncements).toHaveLength(0);

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

    const scopeballAnnouncement = dispatchSpy.mock.calls.find(([event]) => {
      return (
        event instanceof CustomEvent &&
        event.type === "eip6963:announceProvider" &&
        event.detail?.info?.rdns === "dev.scopeball.wrapper"
      );
    })?.[0] as CustomEvent | undefined;

    expect(scopeballAnnouncement?.detail.provider).toBe(provider);

    await expect(
      scopeballAnnouncement?.detail.provider.request({
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

    expect(streamState.instances[0].writes).toHaveLength(1);
    dispatchSpy.mockRestore();
  });
});
