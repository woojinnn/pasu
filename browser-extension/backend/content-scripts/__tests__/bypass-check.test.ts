// @vitest-environment happy-dom
/**
 * N7 — `bypass-check.ts` taps the wallets' OWN window-level message streams
 * (MetaMask / Coinbase) to OBSERVE actions the request-proxy did not intercept.
 * Its `window.addEventListener("message", …)` had no source check, so any
 * MAIN-world page could FORGE a MetaMask-shaped message and inject spurious
 * observe-only audit rows. The fix rejects messages whose `event.source` is not
 * the top window (blocks cross-frame injection). Same-origin same-page forgery
 * remains a documented residual — these rows are observe-only and never gate a
 * real verdict.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Identifier } from "@lib/identifier";

const connect = vi.fn(() => ({ name: "port" }));
const sendToPortAndDisregard = vi.fn();

vi.mock("webextension-polyfill", () => ({
  default: { runtime: { connect } },
}));
vi.mock("@lib/messages", () => ({ sendToPortAndDisregard }));

const TX = {
  method: "eth_sendTransaction",
  params: [
    {
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      data: "0x",
    },
  ],
};

/** A MetaMask-provider-shaped envelope the bypass-check listener acts on. */
function metamaskMessage() {
  return {
    target: Identifier.METAMASK_CONTENT_SCRIPT,
    data: { name: Identifier.METAMASK_PROVIDER, data: TX },
  };
}

describe("bypass-check source guard (N7)", () => {
  beforeEach(async () => {
    vi.resetModules();
    connect.mockClear();
    sendToPortAndDisregard.mockClear();
    await import("../bypass-check"); // registers the window listeners
  });

  it("forwards an observe-only bypass for a message from the top window", () => {
    window.dispatchEvent(
      new MessageEvent("message", { data: metamaskMessage(), source: window }),
    );
    expect(connect).toHaveBeenCalledTimes(1);
    expect(sendToPortAndDisregard).toHaveBeenCalledTimes(1);
  });

  it("IGNORES a forged message whose source is NOT the top window (cross-frame)", () => {
    window.dispatchEvent(
      new MessageEvent("message", { data: metamaskMessage(), source: null }),
    );
    expect(connect).not.toHaveBeenCalled();
    expect(sendToPortAndDisregard).not.toHaveBeenCalled();
  });
});
