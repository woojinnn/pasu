/**
 * ISOLATED-world bridge for the MAIN-world {@link fetch-hook}.
 *
 * A near-verbatim clone of `window-ethereum-messages.ts`, but on the DEDICATED
 * `Identifier.FETCH_*` channel so its `WindowPostMessageStream` handshake never
 * races the provider proxy's. It relays each venue-order verdict request from
 * the page to the service worker over a `runtime.connect` port and writes the
 * boolean verdict back to the page.
 *
 * Fail-CLOSED: if the service worker is unreachable (extension reloaded, SW
 * terminated), we reply `false` so the fetch hook blocks the order rather than
 * letting it through unevaluated — identical to the provider bridge.
 */
import { WindowPostMessageStream } from "@metamask/post-message-stream";
import Browser from "webextension-polyfill";
import { Identifier } from "@lib/identifier";
import {
  sendToPortAndAwaitResponse,
  sendToPortAndDisregard,
} from "@lib/messages";
import {
  isExecutionReport,
  type Message,
  type StreamResponse,
} from "@lib/types";

const stream = new WindowPostMessageStream({
  name: Identifier.FETCH_CONTENT_SCRIPT,
  target: Identifier.FETCH_INPAGE,
}) as WindowPostMessageStream & {
  on(event: "data", callback: (message: Message) => void): void;
  write(data: StreamResponse): boolean;
};

stream.on("data", async (message: Message) => {
  // Drop post-init handshake echoes / malformed envelopes (see the provider
  // bridge for the same guard).
  if (
    !message ||
    typeof message !== "object" ||
    !("data" in message) ||
    !message.data ||
    typeof message.data !== "object" ||
    !("type" in message.data)
  ) {
    return;
  }

  let port: Browser.Runtime.Port;
  try {
    port = Browser.runtime.connect({ name: Identifier.CONTENT_SCRIPT });
  } catch (err) {
    // Fail-CLOSED: no SW ⇒ no verdict ⇒ block the order.
    console.error(
      "[Pasu] cannot reach service worker (extension reloaded?) — " +
        "venue order blocked. Reload this tab to restore policy evaluation.",
      err,
    );
    stream.write({ requestId: message.requestId, data: false });
    return;
  }

  const data: Message["data"] = {
    ...message.data,
    hostname: location.hostname,
  };
  if (isExecutionReport({ ...message, data })) {
    sendToPortAndDisregard(port, data);
    port.disconnect();
    return;
  }

  port.onMessage.addListener((msg: { kind?: string; requestId?: string }) => {
    if (msg?.kind === "awaiting-user" && msg.requestId === message.requestId) {
      stream.write({ requestId: message.requestId, kind: "awaiting-user" });
    }
  });
  const ok = await sendToPortAndAwaitResponse(port, data);
  stream.write({ requestId: message.requestId, data: ok });
  port.disconnect();
});
