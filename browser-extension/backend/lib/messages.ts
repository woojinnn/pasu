import type { WindowPostMessageStream } from "@metamask/post-message-stream";
import objectHash from "object-hash";
import type Browser from "webextension-polyfill";
import { RequestType } from "./types";
import type { AwaitingUserMessage, MessageData, StreamResponse } from "./types";
import type { VerdictReceiver } from "./verdict-channel";

// Phase-1 covers the SW round-trip from the moment we write to the stream
// to the moment the SW posts back a verdict. The cold-path budget includes
// policy-rpc planning/fetching, Cedar evaluation, and audit persistence on a
// fresh service-worker boot. Keep it high enough that legitimate engine work
// resolves before we give up; a dApp blocked on a misbehaving SW can still
// cancel from its own UI.
const PHASE1_MS = 10_000;
const PHASE2_MS = 5 * 60_000;

type Duplex<Incoming, Outgoing> = WindowPostMessageStream & {
  on(event: "data", callback: (data: Incoming) => void): void;
  removeListener(event: "data", callback: (data: Incoming) => void): void;
  write(data: Outgoing): boolean;
};

const isAwaitingUser = (
  response: StreamResponse,
): response is AwaitingUserMessage =>
  "kind" in response && response.kind === "awaiting-user";

export function generateRequestId(data: MessageData): string {
  switch (data.type) {
    case RequestType.TRANSACTION:
      return objectHash(data.transaction);
    case RequestType.TYPED_SIGNATURE:
      return objectHash(data.typedData as object);
    case RequestType.UNTYPED_SIGNATURE:
      return objectHash({ message: data.message });
    case RequestType.VENUE_ORDER:
      return objectHash({
        venue: data.venue,
        endpoint: data.endpoint,
        hlAction: data.hlAction,
      });
    case RequestType.EXECUTION_REPORT:
      return objectHash({
        wallet_id: data.wallet_id,
        evaluation_id: data.evaluation_id,
        action_index: data.action_index,
        outcome: data.outcome,
      });
    case "raw-transaction-advisory":
      return objectHash({
        hostname: data.hostname,
        rawPreview: data.rawPreview,
      });
    case "provider-frozen-warning":
      return objectHash({
        hostname: data.hostname,
        providerName: data.providerName,
      });
  }
}

export function sendToStreamAndDisregard(
  stream: WindowPostMessageStream,
  data: MessageData,
): void {
  const requestId = generateRequestId(data);
  const messageStream = stream as Duplex<
    StreamResponse,
    { requestId: string; data: MessageData }
  >;
  messageStream.write({ requestId, data });
}

export function sendToStreamAndAwaitResponse(
  stream: WindowPostMessageStream,
  data: MessageData,
): Promise<boolean> {
  const requestId = generateRequestId(data);
  const messageStream = stream as Duplex<
    StreamResponse,
    { requestId: string; data: MessageData }
  >;

  return new Promise<boolean>((resolve) => {
    let settled = false;
    let timer: ReturnType<typeof setTimeout>;

    const finish = (value: boolean) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      messageStream.removeListener("data", onData);
      resolve(value);
    };

    const armTimer = (ms: number) => {
      clearTimeout(timer);
      timer = setTimeout(() => finish(false), ms);
    };

    const onData = (response: StreamResponse) => {
      if (response.requestId !== requestId) return;
      if (isAwaitingUser(response)) {
        armTimer(PHASE2_MS);
        return;
      }
      finish(response.data);
    };

    messageStream.on("data", onData);
    armTimer(PHASE1_MS);
    messageStream.write({ requestId, data });
  });
}

/**
 * C1: write the action request over the page-observable `WindowPostMessageStream`
 * (so the ISOLATED bridge reads it) but await the VERDICT over the authenticated
 * `receiver` ({@link VerdictReceiver}) — a `MessageChannel` whose writer port
 * never leaves the ISOLATED world. A verdict forged on the window bus is ignored;
 * only the ISOLATED writer port resolves the gate. `awaiting-user` re-arms the
 * deadline to {@link PHASE2_MS} inside the receiver. Replaces
 * {@link sendToStreamAndAwaitResponse} for every verdict-bearing call.
 */
export function sendRequestAndAwaitVerdict(
  stream: WindowPostMessageStream,
  receiver: VerdictReceiver,
  data: MessageData,
): Promise<boolean> {
  const requestId = generateRequestId(data);
  const messageStream = stream as Duplex<
    StreamResponse,
    { requestId: string; data: MessageData }
  >;
  messageStream.write({ requestId, data });
  return receiver.awaitVerdict(requestId, {
    phase1Ms: PHASE1_MS,
    phase2Ms: PHASE2_MS,
  });
}

export function sendToPortAndAwaitResponse(
  port: Browser.Runtime.Port,
  data: MessageData,
): Promise<boolean> {
  const requestId = generateRequestId(data);

  return new Promise<boolean>((resolve) => {
    let settled = false;
    let timer: ReturnType<typeof setTimeout>;

    const finish = (value: boolean) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      port.onMessage.removeListener(onMessage);
      resolve(value);
    };

    const armTimer = (ms: number) => {
      clearTimeout(timer);
      timer = setTimeout(() => finish(false), ms);
    };

    const onMessage = (response: StreamResponse) => {
      if (response.requestId !== requestId) return;
      if (isAwaitingUser(response)) {
        armTimer(PHASE2_MS);
        return;
      }
      finish(response.data);
    };

    port.onMessage.addListener(onMessage);
    armTimer(PHASE1_MS);
    port.postMessage({ requestId, data });
  });
}

export function sendToPortAndDisregard(
  port: Browser.Runtime.Port,
  data: MessageData,
): void {
  const requestId = generateRequestId(data);
  port.postMessage({ requestId, data });
}
