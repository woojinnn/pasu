/**
 * Authenticated verdict channel (C1 hardening).
 *
 * The MAIN-world proxy / fetch-hook decide whether to forward a wallet action by
 * awaiting a boolean verdict from the ISOLATED content script. The request leg
 * still rides the page-observable `WindowPostMessageStream`, but the VERDICT leg
 * — the only security-critical direction — travels over a `MessageChannel` whose
 * WRITER port never leaves the ISOLATED world.
 *
 * Why this defeats page forgery: a page (same MAIN realm) can `window.postMessage`
 * anything and can even grab the transferred READER port from the init event, but
 * delivering a message to the MAIN proxy's `port.onmessage` requires posting on
 * the ENTANGLED WRITER port, which the ISOLATED content script holds and never
 * transfers. So the page can eavesdrop on verdicts (not secret) but cannot inject
 * one. Verdicts arriving on the window bus are ignored outright.
 *
 * Bootstrap: the ISOLATED content script (a `document_start` content script,
 * which runs before any page script) transfers its READER port once at load. The
 * receiver accepts only the FIRST init (`first-init-wins`); because the genuine
 * init is queued before the page can run, a later page-supplied port loses. (This
 * one timing assumption is the residual; it holds for a `document_start` content
 * script under the standard content-script/page execution ordering.)
 */
import type { StreamResponse } from "./types";

export interface AwaitVerdictOptions {
  /** Budget before the first verdict / `awaiting-user` arrives. */
  phase1Ms: number;
  /** Extended budget after an `awaiting-user` (user is deciding in a modal). */
  phase2Ms: number;
  /** Invoked when the port reports `awaiting-user` (test/diagnostic hook). */
  onAwaitingUser?: () => void;
}

export interface VerdictReceiver {
  /**
   * Resolve the verdict for `requestId`, read ONLY from the authenticated port.
   * `false` (fail-closed) on timeout or when no authenticated port was
   * transferred. `awaiting-user` re-arms the deadline to `phase2Ms`.
   */
  awaitVerdict(requestId: string, opts: AwaitVerdictOptions): Promise<boolean>;
}

export interface VerdictSender {
  /** Post a verdict / `awaiting-user` to the MAIN world over the writer port. */
  send(msg: StreamResponse): void;
}

function responseRequestId(msg: unknown): string | undefined {
  if (!msg || typeof msg !== "object") return undefined;
  const id = (msg as { requestId?: unknown }).requestId;
  return typeof id === "string" ? id : undefined;
}

function isAwaitingUser(msg: unknown): boolean {
  return (
    !!msg &&
    typeof msg === "object" &&
    (msg as { kind?: unknown }).kind === "awaiting-user"
  );
}

function verdictBool(msg: unknown): boolean {
  return (
    !!msg &&
    typeof msg === "object" &&
    (msg as { data?: unknown }).data === true
  );
}

/**
 * MAIN-world receiver. Captures the first authenticated port transferred under
 * `initKey` and resolves verdicts over it. See module docs for the threat model.
 */
export function createVerdictReceiver(initKey: string): VerdictReceiver {
  let port: MessagePort | null = null;
  const pending = new Map<string, (msg: StreamResponse) => void>();

  const onPortMessage = (ev: MessageEvent): void => {
    const requestId = responseRequestId(ev.data);
    if (requestId === undefined) return;
    pending.get(requestId)?.(ev.data as StreamResponse);
  };

  const onWindowMessage = (event: MessageEvent): void => {
    // first-init-wins: the genuine ISOLATED port (transferred at document_start,
    // before page scripts run) is captured first; ignore every later init so a
    // page-supplied port cannot replace the channel.
    if (port) return;
    if (event.source !== window) return;
    const data = event.data as Record<string, unknown> | null | undefined;
    if (!data || typeof data !== "object" || data[initKey] !== true) return;
    const transferred = event.ports && event.ports[0];
    if (!transferred) return;
    port = transferred;
    port.onmessage = onPortMessage;
    port.start?.();
  };
  window.addEventListener("message", onWindowMessage);

  return {
    awaitVerdict(requestId, { phase1Ms, phase2Ms, onAwaitingUser }) {
      return new Promise<boolean>((resolve) => {
        let settled = false;
        let timer: ReturnType<typeof setTimeout>;
        const finish = (value: boolean): void => {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          pending.delete(requestId);
          resolve(value);
        };
        const arm = (ms: number): void => {
          clearTimeout(timer);
          timer = setTimeout(() => finish(false), ms);
        };
        pending.set(requestId, (msg) => {
          if (isAwaitingUser(msg)) {
            arm(phase2Ms);
            onAwaitingUser?.();
            return;
          }
          finish(verdictBool(msg));
        });
        arm(phase1Ms);
      });
    },
  };
}

/**
 * ISOLATED-world sender. Creates the channel, transfers the READER port to the
 * MAIN world under `initKey`, and keeps the WRITER port so only this realm can
 * deliver a verdict. Must run in a `document_start` content script so the init
 * is queued before any page script.
 */
export function createVerdictSender(initKey: string): VerdictSender {
  const channel = new MessageChannel();
  // Transfer the READER (port2); keep the WRITER (port1) here. `location.origin`
  // targets the page's own origin (same realm as the MAIN proxy).
  window.postMessage({ [initKey]: true }, location.origin, [channel.port2]);
  return {
    send(msg: StreamResponse): void {
      channel.port1.postMessage(msg);
    },
  };
}
