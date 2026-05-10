import { WindowPostMessageStream } from "@metamask/post-message-stream";
import Browser from "webextension-polyfill";
import { Identifier } from "@lib/identifier";
import { sendToPortAndAwaitResponse } from "@lib/messages";
import type { Message, StreamResponse } from "@lib/types";

// `targetOrigin: "*"` because the proxy and bridge live in the *same*
// window — there's no cross-origin security need for a strict origin
// match. Default `location.origin` breaks in sandboxed iframes (e.g.
// third-party ad frames on Amazon) where Chrome's actual security origin
// is `null` even though `location.origin` returns the iframe URL,
// triggering noisy "target origin … does not match the recipient
// window's origin ('null')" errors during the SYN handshake. Same-window
// post is filtered downstream by `name`/`target`/`source`, so the wide
// targetOrigin doesn't relax any real boundary.
const stream = new WindowPostMessageStream({
  name: Identifier.CONTENT_SCRIPT,
  target: Identifier.INPAGE,
  targetOrigin: "*",
}) as WindowPostMessageStream & {
  on(event: "data", callback: (message: Message) => void): void;
  write(data: StreamResponse): boolean;
};

stream.on("data", async (message: Message) => {
  // Drop anything that doesn't look like a real wallet-action envelope.
  // BasePostMessageStream can deliver post-init handshake echoes ("SYN"/
  // "ACK" strings) up to the data handler in some delivery races. Those
  // would crash logging and silently push junk through to the SW, where
  // the wallet-action filter would drop them — looking from outside as if
  // the proxy never reached us.
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
  } catch {
    stream.write({ requestId: message.requestId, data: true });
    return;
  }
  const data: Message["data"] = {
    ...message.data,
    hostname: location.hostname,
  };
  port.onMessage.addListener((msg: any) => {
    if (msg?.kind === "awaiting-user" && msg.requestId === message.requestId) {
      stream.write({ requestId: message.requestId, kind: "awaiting-user" });
    }
  });
  const ok = await sendToPortAndAwaitResponse(port, data);
  stream.write({ requestId: message.requestId, data: ok });
  port.disconnect();
});
