import { WindowPostMessageStream } from "@metamask/post-message-stream";
import { ethErrors } from "eth-rpc-errors";
import { Identifier, PROVIDER_MARKER } from "@lib/identifier";
import { generateRequestId, sendToStreamAndAwaitResponse } from "@lib/messages";
import { RequestType } from "@lib/types";
import type { MessageData } from "@lib/types";

declare global {
  interface Window {
    ethereum?: Eip1193Provider;
    coinbaseWalletExtension?: Eip1193Provider;
    [key: string]: unknown;
  }
}

interface Eip1193Provider {
  request?: (request: JsonRpcRequest) => Promise<unknown>;
  send?: (...args: unknown[]) => unknown;
  sendAsync?: (...args: unknown[]) => unknown;
  chainId?: string | number;
  providers?: Eip1193Provider[];
  constructor?: { name?: string };
  [PROVIDER_MARKER]?: boolean;
}

interface JsonRpcRequest {
  id?: string | number;
  jsonrpc?: string;
  method?: string;
  params?: unknown;
}

type JsonRpcCallback = (error: unknown, response?: unknown) => void;

type WritableStream = WindowPostMessageStream & {
  write(data: unknown): void;
};

const stream = new WindowPostMessageStream({
  name: Identifier.INPAGE,
  target: Identifier.CONTENT_SCRIPT,
}) as WritableStream;

const REJECT_TX = ethErrors.provider.userRejectedRequest(
  "Scopeball: transaction blocked by policy",
);
const REJECT_SIG = ethErrors.provider.userRejectedRequest(
  "Scopeball: signature blocked by policy",
);

const TYPED_SIGNATURE_METHODS = new Set([
  "eth_signTypedData",
  "eth_signTypedData_v3",
  "eth_signTypedData_v4",
]);
const UNTYPED_SIGNATURE_METHODS = new Set(["eth_sign", "personal_sign"]);
const TX_HASH_RE = /^0x[0-9a-fA-F]{64}$/;

const txRequestIds = new WeakMap<object, string>();
const wrappedProviders = new WeakSet<object>();
const unwrappableProviders = new WeakSet<object>();
const reannouncedProviders = new WeakSet<object>();

function asProvider(value: unknown): Eip1193Provider | undefined {
  return value && typeof value === "object"
    ? (value as Eip1193Provider)
    : undefined;
}

function paramsArray(params: unknown): unknown[] {
  if (Array.isArray(params)) return params;
  return params === undefined ? [] : [params];
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}

function looksLikeAddress(value: unknown): boolean {
  return /^0x[0-9a-fA-F]{40}$/.test(String(value));
}

function parseChainId(value: unknown): number | undefined {
  const parsed =
    typeof value === "string" && value.startsWith("0x")
      ? Number.parseInt(value, 16)
      : Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : undefined;
}

async function readChainId(provider: Eip1193Provider): Promise<number> {
  try {
    if (typeof provider.request !== "function")
      return parseChainId(provider.chainId) ?? 1;
    const result = await Promise.race([
      provider.request({ method: "eth_chainId" }),
      new Promise<never>((_, reject) => {
        window.setTimeout(
          () => reject(new Error("Scopeball: chainId timeout")),
          1_500,
        );
      }),
    ]);
    return parseChainId(result) ?? parseChainId(provider.chainId) ?? 1;
  } catch {
    return parseChainId(provider.chainId) ?? 1;
  }
}

async function checkTransaction(
  provider: Eip1193Provider,
  params: unknown[],
  chainIdOverride?: number,
): Promise<boolean> {
  const [transaction] = params;
  if (!transaction || typeof transaction !== "object") return true;

  const data = {
    type: RequestType.TRANSACTION,
    chainId: chainIdOverride ?? (await readChainId(provider)),
    hostname: location.hostname,
    transaction,
  } as MessageData;

  txRequestIds.set(transaction, generateRequestId(data));
  return sendToStreamAndAwaitResponse(stream, data);
}

async function checkWalletSendCalls(
  provider: Eip1193Provider,
  params: unknown[],
): Promise<boolean> {
  const envelope = asRecord(params[0]);
  const calls = Array.isArray(envelope?.calls) ? envelope.calls : [];
  if (calls.length === 0) return true;

  const chainId =
    parseChainId(envelope?.chainId) ?? (await readChainId(provider));
  let isAllowed = true;

  for (let i = 0; i < calls.length; i++) {
    const call = asRecord(calls[i]);
    if (!call) {
      continue;
    }

    const transaction: Record<string, unknown> = {
      ...call,
      from: call.from ?? envelope?.from,
    };
    const isCallAllowed = await checkTransaction(
      provider,
      [transaction],
      chainId,
    );
    if (!isCallAllowed) isAllowed = false;
  }

  return isAllowed;
}

async function checkTypedSignature(
  provider: Eip1193Provider,
  params: unknown[],
): Promise<boolean> {
  const [first, second] = params;
  const address = looksLikeAddress(first) ? first : second;
  const typedData = looksLikeAddress(first) ? second : first;
  if (!looksLikeAddress(address) || typedData === undefined) return true;

  return sendToStreamAndAwaitResponse(stream, {
    type: RequestType.TYPED_SIGNATURE,
    chainId: await readChainId(provider),
    hostname: location.hostname,
    address: String(address) as `0x${string}`,
    typedData,
  });
}

async function checkUntypedSignature(params: unknown[]): Promise<boolean> {
  const [first, second] = params;
  if (first === undefined || second === undefined) return true;

  return sendToStreamAndAwaitResponse(stream, {
    type: RequestType.UNTYPED_SIGNATURE,
    hostname: location.hostname,
    message: String(looksLikeAddress(first) ? second : first),
  });
}

async function ensureAllowed(
  provider: Eip1193Provider,
  method: string | undefined,
  params: unknown[],
): Promise<void> {
  if (method === "eth_sendTransaction") {
    const isOk = await checkTransaction(provider, params);
    if (!isOk) throw REJECT_TX;
    return;
  }

  if (method === "wallet_sendCalls") {
    const isOk = await checkWalletSendCalls(provider, params);
    if (!isOk) throw REJECT_TX;
    return;
  }

  if (method && TYPED_SIGNATURE_METHODS.has(method)) {
    const isOk = await checkTypedSignature(provider, params);
    if (!isOk) throw REJECT_SIG;
    return;
  }

  if (method && UNTYPED_SIGNATURE_METHODS.has(method)) {
    const isOk = await checkUntypedSignature(params);
    if (!isOk) throw REJECT_SIG;
    return;
  }

  if (method === "eth_sendRawTransaction") {
    logRawTransaction(params);
  }
}

function requestIdForTransaction(params: unknown[]): string | undefined {
  const tx = params[0];
  return tx && typeof tx === "object" ? txRequestIds.get(tx) : undefined;
}

function logRawTransaction(params: unknown[]): void {
  const raw = String(params[0] ?? "");
  console.warn("Scopeball: eth_sendRawTransaction pass-through advisory", {
    hostname: location.hostname,
    rawPreview: raw.slice(0, 18),
  });
  stream.write({
    requestId: `raw-tx-${raw.slice(0, 18)}`,
    data: {
      type: "raw-transaction-advisory",
      hostname: location.hostname,
      rawPreview: raw.slice(0, 18),
    },
  });
}

function reportTransactionHash(params: unknown[], result: unknown): void {
  if (typeof result !== "string" || !TX_HASH_RE.test(result)) return;
  const requestId = requestIdForTransaction(params);
  if (!requestId) return;

  stream.write({
    requestId: `tx-hash-${requestId}`,
    data: {
      type: "tx-hash-report",
      requestId,
      txHash: result,
      hostname: location.hostname,
    },
  });
}

function rejectJsonRpc(
  callback: JsonRpcCallback,
  request: JsonRpcRequest,
  error: unknown,
): void {
  callback(error, {
    id: request.id,
    jsonrpc: request.jsonrpc ?? "2.0",
    error,
  });
}

function wrapCallbackForTxHash(
  callback: JsonRpcCallback,
  method: string | undefined,
  params: unknown[],
): JsonRpcCallback {
  return (error, response) => {
    if (!error && method === "eth_sendTransaction") {
      const result =
        response && typeof response === "object" && "result" in response
          ? (response as { result?: unknown }).result
          : response;
      reportTransactionHash(params, result);
    }
    callback(error, response);
  };
}

function reportFrozenProvider(provider: Eip1193Provider, error: unknown): void {
  console.error("Scopeball: provider is frozen and cannot be wrapped", error);
  stream.write({
    requestId: `frozen-provider-${Date.now().toString(16)}`,
    data: {
      type: "provider-frozen-warning",
      hostname: location.hostname,
      providerName: provider.constructor?.name ?? "unknown",
    },
  });
}

// Marker on the wrapped function itself. Lets the polling loop detect
// when MetaMask (or another late-injecting provider) re-defined
// `provider.request` back to native after we'd already wrapped it —
// in which case we re-wrap. Tracking only on the provider object via
// PROVIDER_MARKER is not enough: the marker survives even when the
// wrapped methods were stomped, leaving us with a "wrapped" object
// whose request is the unwrapped native one.
const WRAP_MARKER = Symbol.for("__scopeball_wrapper__");
type WithMarker<T> = T & { [WRAP_MARKER]?: true };

function isAlreadyWrapped(fn: unknown): boolean {
  return typeof fn === "function" && (fn as WithMarker<Function>)[WRAP_MARKER] === true;
}

function proxyEthereumProvider(provider: Eip1193Provider | undefined): void {
  if (!provider) return;
  if (unwrappableProviders.has(provider)) return;
  // Already wrapped AND our wrap is still in place → nothing to do.
  if (
    wrappedProviders.has(provider) &&
    isAlreadyWrapped(provider.request) &&
    (typeof provider.sendAsync !== "function" || isAlreadyWrapped(provider.sendAsync)) &&
    (typeof provider.send !== "function" || isAlreadyWrapped(provider.send))
  ) {
    return;
  }

  if (typeof provider.request !== "function") {
    reportFrozenProvider(provider, new Error("provider.request is required"));
    unwrappableProviders.add(provider);
    return;
  }

  // If a previous wrap was stomped, peel back to the underlying native
  // methods so we don't double-wrap our own wrapper. (Our wrapper holds
  // the original via its closure, so we can't recover it from the
  // stomped state — but the live `provider.request` is the native one
  // again, which is exactly what we want.)
  const originalRequest = provider.request;
  const originalSendAsync = provider.sendAsync;
  const originalSend = provider.send;

  const proxiedRequest = new Proxy(originalRequest, {
    apply: async (target, _thisArg, args) => {
      const request = (args[0] ?? {}) as JsonRpcRequest;
      const method = request.method;
      const params = paramsArray(request.params);

      await ensureAllowed(provider, method, params);
      // Always forward with the original `provider` as the receiver, not the
      // dApp-supplied `thisArg`. EIP-6963 / wagmi-style callers can hand us
      // a `thisArg` that isn't the provider object MetaMask's stateful
      // `request` implementation expects (it reads `this._rpcRequest`,
      // `this._metamask`, …). When MetaMask gets the wrong `this` it
      // silently no-ops — no error, no popup. This was tolerable for
      // `eth_sendTransaction` (the dApp's call shape happened to align)
      // but breaks `wallet_sendCalls`, which arrives via a different stack.
      const result = await Reflect.apply(target, provider, args);
      if (method === "eth_sendTransaction") {
        reportTransactionHash(params, result);
      }
      return result;
    },
  });

  // For all three legacy entry points (request / sendAsync / send), forward
  // with the original `provider` as the receiver, not the dApp-supplied
  // `thisArg`. See the comment on `proxiedRequest` above for the rationale —
  // MetaMask's native methods are stateful and silently no-op when called
  // with the wrong `this`.
  const proxiedSendAsync =
    typeof originalSendAsync === "function"
      ? new Proxy(originalSendAsync, {
          apply: (target, _thisArg, args) => {
            const request = (args[0] ?? {}) as JsonRpcRequest;
            const callback = args[1] as JsonRpcCallback | undefined;
            const method = request.method;
            const params = paramsArray(request.params);

            if (typeof callback !== "function") {
              return Reflect.apply(target, provider, args);
            }

            void (async () => {
              try {
                await ensureAllowed(provider, method, params);
                Reflect.apply(target, provider, [
                  request,
                  wrapCallbackForTxHash(callback, method, params),
                  ...args.slice(2),
                ]);
              } catch (error) {
                rejectJsonRpc(callback, request, error);
              }
            })();
            return undefined;
          },
        })
      : undefined;

  const proxiedSend =
    typeof originalSend === "function"
      ? new Proxy(originalSend, {
          apply: (target, _thisArg, args) => {
            const [payloadOrMethod, callbackOrParams] = args;
            if (typeof payloadOrMethod === "string") {
              return provider.request?.({
                method: payloadOrMethod,
                params: callbackOrParams,
              });
            }

            const request = (payloadOrMethod ?? {}) as JsonRpcRequest;
            const method = request.method;
            const params = paramsArray(request.params);

            if (typeof callbackOrParams !== "function") {
              if (!method) return Reflect.apply(target, provider, args);
              return (async () => {
                await ensureAllowed(provider, method, params);
                const result = Reflect.apply(target, provider, args);
                if (method === "eth_sendTransaction") {
                  reportTransactionHash(params, result);
                }
                return result;
              })();
            }

            if (typeof provider.sendAsync === "function") {
              return provider.sendAsync(request, callbackOrParams);
            }

            void (async () => {
              try {
                await ensureAllowed(provider, method, params);
                Reflect.apply(target, provider, [
                  request,
                  wrapCallbackForTxHash(
                    callbackOrParams as JsonRpcCallback,
                    method,
                    params,
                  ),
                  ...args.slice(2),
                ]);
              } catch (error) {
                rejectJsonRpc(
                  callbackOrParams as JsonRpcCallback,
                  request,
                  error,
                );
              }
            })();
            return undefined;
          },
        })
      : undefined;

  // Tag wrapped functions so subsequent poll passes can detect mutation.
  (proxiedRequest as WithMarker<typeof proxiedRequest>)[WRAP_MARKER] = true;
  if (proxiedSendAsync)
    (proxiedSendAsync as WithMarker<typeof proxiedSendAsync>)[WRAP_MARKER] = true;
  if (proxiedSend)
    (proxiedSend as WithMarker<typeof proxiedSend>)[WRAP_MARKER] = true;

  try {
    Object.defineProperty(provider, "request", {
      configurable: true,
      value: proxiedRequest,
      writable: true,
    });

    if (proxiedSendAsync) {
      Object.defineProperty(provider, "sendAsync", {
        configurable: true,
        value: proxiedSendAsync,
        writable: true,
      });
    }

    if (proxiedSend) {
      Object.defineProperty(provider, "send", {
        configurable: true,
        value: proxiedSend,
        writable: true,
      });
    }
  } catch (error) {
    unwrappableProviders.add(provider);
    reportFrozenProvider(provider, error);
    return;
  }

  wrappedProviders.add(provider);
  try {
    Object.defineProperty(provider, PROVIDER_MARKER, {
      configurable: false,
      value: true,
      writable: false,
    });
  } catch {
    // WeakSet idempotency still prevents double-wrapping in this realm.
  }
}

const KNOWN_SOURCES = [
  "ethereum",
  "coinbaseWalletExtension",
  "eth",
  "rsk",
  "bsc",
  "polygon",
  "arbitrum",
  "fuse",
  "avalanche",
  "optimism",
] as const;

function discoverAndProxyAll(): void {
  for (const key of KNOWN_SOURCES) {
    const provider = asProvider(window[key]);
    if (!provider) continue;

    proxyEthereumProvider(provider);
    if (key === "ethereum" && Array.isArray(provider.providers)) {
      for (const subProvider of provider.providers) {
        proxyEthereumProvider(subProvider);
      }
    }
  }
}

const SCOPEBALL_RDNS = "dev.scopeball.wrapper";
const FALLBACK_ICON =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQI12P4//8/AwAI/AL+T1pNCgAAAABJRU5ErkJggg==";

function reannounceWrapped(detail: unknown): void {
  const providerDetail = detail as {
    info?: { uuid?: string; name?: string; icon?: string; rdns?: string };
    provider?: Eip1193Provider;
  };
  const provider = asProvider(providerDetail.provider);
  if (!provider || reannouncedProviders.has(provider)) return;

  proxyEthereumProvider(provider);
  reannouncedProviders.add(provider);

  const info = Object.freeze({
    uuid: `scopeball-${providerDetail.info?.uuid ?? Math.random().toString(36).slice(2)}`,
    name: `Scopeball (wraps ${providerDetail.info?.name ?? "provider"})`,
    icon: providerDetail.info?.icon ?? FALLBACK_ICON,
    rdns: SCOPEBALL_RDNS,
  });

  window.dispatchEvent(
    new CustomEvent("eip6963:announceProvider", {
      detail: Object.freeze({ info, provider }),
    }),
  );
}

window.addEventListener("eip6963:announceProvider", (event: Event) => {
  const detail = (event as CustomEvent).detail;
  if (detail?.info?.rdns === SCOPEBALL_RDNS) return;
  reannounceWrapped(detail);
});

window.dispatchEvent(new Event("eip6963:requestProvider"));

discoverAndProxyAll();
const pollDeadline = Date.now() + 30_000;
let pollHandle: number | undefined = window.setInterval(() => {
  discoverAndProxyAll();
  if (Date.now() > pollDeadline && pollHandle !== undefined) {
    window.clearInterval(pollHandle);
    pollHandle = undefined;
  }
}, 100);
