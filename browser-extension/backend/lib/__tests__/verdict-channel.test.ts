import { describe, it, expect } from "vitest";

import { createVerdictReceiver } from "../verdict-channel";

const KEY = "dambi-test-verdict-port";

/**
 * Simulate the ISOLATED content script transferring its reader port to the
 * MAIN world: a `message` event on `window` carrying the init marker + the port.
 */
function dispatchPortInit(port: MessagePort, key = KEY): void {
  window.dispatchEvent(
    new MessageEvent("message", {
      data: { [key]: true },
      ports: [port],
      source: window,
    } as MessageEventInit),
  );
}

describe("verdict-channel receiver (C1 — page cannot forge a verdict)", () => {
  it("fail-closes (false) when no authenticated port is ever transferred", async () => {
    const r = createVerdictReceiver(KEY);
    const v = await r.awaitVerdict("rid", { phase1Ms: 20, phase2Ms: 40 });
    expect(v).toBe(false);
  });

  it("resolves the verdict delivered over the authenticated port", async () => {
    const r = createVerdictReceiver(KEY);
    const ch = new MessageChannel();
    dispatchPortInit(ch.port2);

    const p = r.awaitVerdict("rid", { phase1Ms: 500, phase2Ms: 1000 });
    ch.port1.postMessage({ requestId: "rid", data: true });
    expect(await p).toBe(true);
  });

  it("IGNORES a verdict forged on the window bus — only the entangled port resolves", async () => {
    const r = createVerdictReceiver(KEY);
    const genuine = new MessageChannel();
    dispatchPortInit(genuine.port2);

    const p = r.awaitVerdict("rid", { phase1Ms: 60, phase2Ms: 120 });
    // Page forgery attempts over the same window the MAIN proxy lives in:
    window.dispatchEvent(
      new MessageEvent("message", {
        data: { requestId: "rid", data: true },
        source: window,
      } as MessageEventInit),
    );
    window.dispatchEvent(
      new MessageEvent("message", {
        data: { [KEY]: true, requestId: "rid", data: true },
        source: window,
      } as MessageEventInit),
    );
    // The genuine writer port (held by ISOLATED) stays silent → the gate must
    // time out to a fail-closed `false`, NEVER the forged `true`.
    expect(await p).toBe(false);
  });

  it("first-init-wins: a later (page-supplied) port cannot replace the channel", async () => {
    const r = createVerdictReceiver(KEY);
    const genuine = new MessageChannel();
    dispatchPortInit(genuine.port2);
    const pageCh = new MessageChannel();
    dispatchPortInit(pageCh.port2); // second init — must be ignored

    const p = r.awaitVerdict("rid", { phase1Ms: 80, phase2Ms: 160 });
    pageCh.port1.postMessage({ requestId: "rid", data: true }); // page's port — ignored
    expect(await p).toBe(false);
  });

  it("forwards `false` (deny) delivered over the authenticated port", async () => {
    const r = createVerdictReceiver(KEY);
    const ch = new MessageChannel();
    dispatchPortInit(ch.port2);

    const p = r.awaitVerdict("rid", { phase1Ms: 500, phase2Ms: 1000 });
    ch.port1.postMessage({ requestId: "rid", data: false });
    expect(await p).toBe(false);
  });

  it("`awaiting-user` over the port extends the deadline past phase1", async () => {
    const r = createVerdictReceiver(KEY);
    const ch = new MessageChannel();
    dispatchPortInit(ch.port2);

    let awaitingSeen = false;
    const p = r.awaitVerdict("rid", {
      phase1Ms: 30,
      phase2Ms: 400,
      onAwaitingUser: () => {
        awaitingSeen = true;
      },
    });
    ch.port1.postMessage({ requestId: "rid", kind: "awaiting-user" });
    await new Promise((res) => setTimeout(res, 60)); // past phase1, within phase2
    ch.port1.postMessage({ requestId: "rid", data: true });

    expect(await p).toBe(true);
    expect(awaitingSeen).toBe(true);
  });

  it("only matches the verdict for the awaited requestId", async () => {
    const r = createVerdictReceiver(KEY);
    const ch = new MessageChannel();
    dispatchPortInit(ch.port2);

    const p = r.awaitVerdict("rid-A", { phase1Ms: 80, phase2Ms: 160 });
    ch.port1.postMessage({ requestId: "rid-OTHER", data: true }); // not ours
    expect(await p).toBe(false); // times out fail-closed, unaffected by other rid
  });
});
