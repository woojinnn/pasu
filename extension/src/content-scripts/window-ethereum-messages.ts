import { WindowPostMessageStream } from "@metamask/post-message-stream";
import Browser from "webextension-polyfill";
import { Identifier } from "@lib/identifier";
import { sendToPortAndAwaitResponse } from "@lib/messages";
import type { Message, StreamResponse } from "@lib/types";

const stream = new WindowPostMessageStream({
  name: Identifier.CONTENT_SCRIPT,
  target: Identifier.INPAGE,
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
