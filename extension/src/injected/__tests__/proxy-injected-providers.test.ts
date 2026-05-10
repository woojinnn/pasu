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
});
