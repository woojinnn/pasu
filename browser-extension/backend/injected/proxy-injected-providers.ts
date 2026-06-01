import { WindowPostMessageStream } from "@metamask/post-message-stream";
import { ethErrors } from "eth-rpc-errors";
import { Identifier, PROVIDER_MARKER } from "@lib/identifier";
import {
  sendToStreamAndAwaitResponse,
  sendToStreamAndDisregard,
} from "@lib/messages";
import { RequestType } from "@lib/types";
import type {
  ExecutionReportOutcome,
  ExecutionReportPayload,
  MessageData,
} from "@lib/types";

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
  on?: (event: string, listener: (...args: unknown[]) => void) => unknown;
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
type ProviderRequest = NonNullable<Eip1193Provider["request"]>;

type WritableStream = WindowPostMessageStream & {
  write(data: unknown): void;
};

interface InstallState {
  eip6963Listener?: (event: Event) => void;
  pollHandle: number | undefined;
}

const INSTALL_STATE = Symbol.for("__scopeball_provider_proxy_install_state__");
const windowWithInstallState = window as unknown as Record<
  PropertyKey,
  unknown
>;
const installState = (windowWithInstallState[INSTALL_STATE] ?? {
  pollHandle: undefined,
}) as InstallState;
if (installState.eip6963Listener) {
  window.removeEventListener(
    "eip6963:announceProvider",
    installState.eip6963Listener,
  );
}
if (installState.pollHandle !== undefined) {
  window.clearInterval(installState.pollHandle);
}
windowWithInstallState[INSTALL_STATE] = installState;

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

const chainIdCache = new WeakMap<object, number>();
const chainIdListeners = new WeakSet<object>();
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

function rememberChainId(
  provider: Eip1193Provider,
  value: unknown,
): number | undefined {
  const parsed = parseChainId(value);
  if (parsed !== undefined) chainIdCache.set(provider, parsed);
  return parsed;
}

function installChainIdListener(provider: Eip1193Provider): void {
  if (chainIdListeners.has(provider) || typeof provider.on !== "function")
    return;
  try {
    provider.on("chainChanged", (chainId) => {
      rememberChainId(provider, chainId);
    });
    chainIdListeners.add(provider);
  } catch {
    // Some provider shims expose `on` but reject unknown listeners.
  }
}

async function readChainId(
  provider: Eip1193Provider,
  request: ProviderRequest | undefined = provider.request,
): Promise<number> {
  const providerChainId = rememberChainId(provider, provider.chainId);
  if (providerChainId !== undefined) return providerChainId;

  const cached = chainIdCache.get(provider);
  if (cached !== undefined) return cached;

  try {
    if (typeof request !== "function") return 1;
    const result = await Promise.race([
      Reflect.apply(request, provider, [{ method: "eth_chainId" }]),
      new Promise<never>((_, reject) => {
        window.setTimeout(
          () => reject(new Error("Scopeball: chainId timeout")),
          1_500,
        );
      }),
    ]);
    return (
      rememberChainId(provider, result) ??
      rememberChainId(provider, provider.chainId) ??
      1
    );
  } catch {
    return rememberChainId(provider, provider.chainId) ?? 1;
  }
}

async function checkTransaction(
  provider: Eip1193Provider,
  params: unknown[],
  chainIdOverride?: number,
  readRequest?: ProviderRequest,
): Promise<boolean> {
  const [transaction] = params;
  if (!transaction || typeof transaction !== "object") return true;

  const data = {
    type: RequestType.TRANSACTION,
    chainId: chainIdOverride ?? (await readChainId(provider, readRequest)),
    hostname: location.hostname,
    transaction,
  } as MessageData;

  return sendToStreamAndAwaitResponse(stream, data);
}

async function checkWalletSendCalls(
  provider: Eip1193Provider,
  params: unknown[],
  readRequest?: ProviderRequest,
): Promise<boolean> {
  const envelope = asRecord(params[0]);
  const calls = Array.isArray(envelope?.calls) ? envelope.calls : [];
  if (calls.length === 0) return true;

  const chainId =
    parseChainId(envelope?.chainId) ??
    (await readChainId(provider, readRequest));

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
      readRequest,
    );
    if (!isCallAllowed) return false;
  }

  return true;
}

async function checkTypedSignature(
  provider: Eip1193Provider,
  params: unknown[],
  readRequest?: ProviderRequest,
): Promise<boolean> {
  const [first, second] = params;
  const address = looksLikeAddress(first) ? first : second;
  const typedData = looksLikeAddress(first) ? second : first;
  if (!looksLikeAddress(address) || typedData === undefined) return true;

  return sendToStreamAndAwaitResponse(stream, {
    type: RequestType.TYPED_SIGNATURE,
    chainId: await readChainId(provider, readRequest),
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

function metaMaskBatchRequests(params: unknown[]): JsonRpcRequest[] {
  const [first] = params;
  const candidate = Array.isArray(first) ? first : params;
  const requests: JsonRpcRequest[] = [];
  for (const item of candidate) {
    const record = asRecord(item);
    if (record) requests.push(record as JsonRpcRequest);
  }
  return requests;
}

async function checkMetaMaskBatch(
  provider: Eip1193Provider,
  params: unknown[],
  readRequest?: ProviderRequest,
): Promise<void> {
  for (const request of metaMaskBatchRequests(params)) {
    await ensureAllowed(
      provider,
      request.method,
      paramsArray(request.params),
      readRequest,
    );
  }
}

async function ensureAllowed(
  provider: Eip1193Provider,
  method: string | undefined,
  params: unknown[],
  readRequest?: ProviderRequest,
): Promise<void> {
  if (method === "eth_sendTransaction") {
    const isOk = await checkTransaction(
      provider,
      params,
      undefined,
      readRequest,
    );
    if (!isOk) throw REJECT_TX;
    return;
  }

  if (method === "wallet_sendCalls") {
    const isOk = await checkWalletSendCalls(provider, params, readRequest);
    if (!isOk) throw REJECT_TX;
    return;
  }

  if (method === "metamask_batch") {
    await checkMetaMaskBatch(provider, params, readRequest);
    return;
  }

  if (method && TYPED_SIGNATURE_METHODS.has(method)) {
    const isOk = await checkTypedSignature(provider, params, readRequest);
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

function chainCaip2(chainId: number): string {
  return `eip155:${chainId}`;
}

function knownChainId(provider: Eip1193Provider): number | undefined {
  return chainIdCache.get(provider) ?? rememberChainId(provider, provider.chainId);
}

function resultChainId(
  provider: Eip1193Provider,
  method: string | undefined,
  params: unknown[],
): number | undefined {
  if (method === "wallet_sendCalls") {
    return parseChainId(asRecord(params[0])?.chainId) ?? knownChainId(provider);
  }
  return knownChainId(provider);
}

function signatureAddress(params: unknown[]): string | undefined {
  const [first, second] = params;
  if (looksLikeAddress(first)) return String(first);
  if (looksLikeAddress(second)) return String(second);
  return undefined;
}

function transactionAddress(params: unknown[]): string | undefined {
  const tx = asRecord(params[0]);
  const from = tx?.from;
  return looksLikeAddress(from) ? String(from) : undefined;
}

function reportWalletId(
  provider: Eip1193Provider,
  method: string | undefined,
  params: unknown[],
): ExecutionReportPayload["wallet_id"] | undefined {
  const address =
    method === "eth_sendTransaction"
      ? transactionAddress(params)
      : method === "wallet_sendCalls"
      ? transactionAddress(params)
      : method &&
          (TYPED_SIGNATURE_METHODS.has(method) ||
            UNTYPED_SIGNATURE_METHODS.has(method))
        ? signatureAddress(params)
        : undefined;
  const chainId = resultChainId(provider, method, params);
  if (!address || chainId === undefined) return undefined;
  return {
    address,
    chains: [chainCaip2(chainId)],
  };
}

function errorReason(error: unknown): string | undefined {
  if (error instanceof Error && error.message) return error.message;
  const record = asRecord(error);
  if (typeof record?.message === "string") return record.message;
  if (typeof record?.reason === "string") return record.reason;
  if (typeof error === "string" && error.length > 0) return error;
  return undefined;
}

function successOutcome(
  provider: Eip1193Provider,
  method: string | undefined,
  result: unknown,
): ExecutionReportOutcome | undefined {
  if (method === "eth_sendTransaction" && typeof result === "string") {
    const chainId = knownChainId(provider);
    if (chainId === undefined) return undefined;
    return {
      kind: "onchain_submitted",
      chain: chainCaip2(chainId),
      tx_hash: result,
    };
  }
  if (method === "wallet_sendCalls") {
    return {
      kind: "wallet_confirmed",
      method,
    };
  }
  if (
    method &&
    (TYPED_SIGNATURE_METHODS.has(method) ||
      UNTYPED_SIGNATURE_METHODS.has(method)) &&
    typeof result === "string"
  ) {
    return {
      kind: "wallet_signed",
      signature: result,
    };
  }
  return undefined;
}

function rejectedOutcome(
  method: string | undefined,
  error: unknown,
): ExecutionReportOutcome | undefined {
  if (
    method === "eth_sendTransaction" ||
    method === "wallet_sendCalls" ||
    (method &&
      (TYPED_SIGNATURE_METHODS.has(method) ||
        UNTYPED_SIGNATURE_METHODS.has(method)))
  ) {
    const outcome: ExecutionReportOutcome = {
      kind: "wallet_rejected",
    };
    const reason = errorReason(error);
    if (reason !== undefined) outcome.reason = reason;
    return outcome;
  }
  return undefined;
}

function reportProviderOutcome(
  provider: Eip1193Provider,
  method: string | undefined,
  params: unknown[],
  outcome: ExecutionReportOutcome | undefined,
): void {
  if (!outcome) return;
  const report: ExecutionReportPayload = {
    type: RequestType.EXECUTION_REPORT,
    hostname: location.hostname,
    outcome,
    metadata: {
      source: "provider-proxy",
      method,
    },
  };
  const walletId = reportWalletId(provider, method, params);
  if (walletId) report.wallet_id = walletId;
  sendToStreamAndDisregard(stream, report);
}

async function forwardProviderResult<T>(
  provider: Eip1193Provider,
  method: string | undefined,
  params: unknown[],
  invoke: () => T | Promise<T>,
): Promise<T> {
  try {
    const result = await invoke();
    reportProviderOutcome(
      provider,
      method,
      params,
      successOutcome(provider, method, result),
    );
    return result;
  } catch (error) {
    reportProviderOutcome(provider, method, params, rejectedOutcome(method, error));
    throw error;
  }
}

function wrapProviderCallback(
  provider: Eip1193Provider,
  method: string | undefined,
  params: unknown[],
  callback: JsonRpcCallback,
): JsonRpcCallback {
  return (error, response) => {
    const responseRecord = asRecord(response);
    const responseError = responseRecord?.error;
    if (error !== null && error !== undefined) {
      reportProviderOutcome(provider, method, params, rejectedOutcome(method, error));
    } else if (responseError !== undefined) {
      reportProviderOutcome(
        provider,
        method,
        params,
        rejectedOutcome(method, responseError),
      );
    } else {
      reportProviderOutcome(
        provider,
        method,
        params,
        successOutcome(provider, method, responseRecord?.result ?? response),
      );
    }
    callback(error, response);
  };
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
const WRAP_TARGET = Symbol.for("__scopeball_wrapper_target__");
type WithMarker<T> = T & { [WRAP_MARKER]?: true; [WRAP_TARGET]?: T };

function isAlreadyWrapped(fn: unknown): boolean {
  return (
    typeof fn === "function" &&
    (fn as WithMarker<Function>)[WRAP_MARKER] === true
  );
}

function unwrapFunction<T extends Function>(fn: T): T {
  return (fn as WithMarker<T>)[WRAP_TARGET] ?? fn;
}

function proxyEthereumProvider(provider: Eip1193Provider | undefined): boolean {
  if (!provider) return false;
  if (unwrappableProviders.has(provider)) return false;
  // Already wrapped AND our wrap is still in place → nothing to do.
  if (
    wrappedProviders.has(provider) &&
    isAlreadyWrapped(provider.request) &&
    (typeof provider.sendAsync !== "function" ||
      isAlreadyWrapped(provider.sendAsync)) &&
    (typeof provider.send !== "function" || isAlreadyWrapped(provider.send))
  ) {
    installChainIdListener(provider);
    return true;
  }

  if (typeof provider.request !== "function") {
    reportFrozenProvider(provider, new Error("provider.request is required"));
    unwrappableProviders.add(provider);
    return false;
  }

  // If a previous wrap was stomped, peel back to the underlying native
  // methods so we don't double-wrap our own wrapper. (Our wrapper holds
  // the original via its closure, so we can't recover it from the
  // stomped state — but the live `provider.request` is the native one
  // again, which is exactly what we want.)
  const originalRequest = unwrapFunction(provider.request);
  const originalSendAsync =
    typeof provider.sendAsync === "function"
      ? unwrapFunction(provider.sendAsync)
      : undefined;
  const originalSend =
    typeof provider.send === "function"
      ? unwrapFunction(provider.send)
      : undefined;
  installChainIdListener(provider);

  const proxiedRequest = new Proxy(originalRequest, {
    apply: async (target, _thisArg, args) => {
      const request = (args[0] ?? {}) as JsonRpcRequest;
      const method = request.method;
      const params = paramsArray(request.params);

      await ensureAllowed(provider, method, params, originalRequest);
      // Always forward with the original `provider` as the receiver, not the
      // dApp-supplied `thisArg`. EIP-6963 / wagmi-style callers can hand us
      // a `thisArg` that isn't the provider object MetaMask's stateful
      // `request` implementation expects (it reads `this._rpcRequest`,
      // `this._metamask`, …). When MetaMask gets the wrong `this` it
      // silently no-ops — no error, no popup. This was tolerable for
      // `eth_sendTransaction` (the dApp's call shape happened to align)
      // but breaks `wallet_sendCalls`, which arrives via a different stack.
      return forwardProviderResult(provider, method, params, () =>
        Reflect.apply(target, provider, args),
      );
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
              if (!method) return Reflect.apply(target, provider, args);
              return (async () => {
                await ensureAllowed(provider, method, params, originalRequest);
                return forwardProviderResult(provider, method, params, () =>
                  Reflect.apply(target, provider, args),
                );
              })();
            }

            void (async () => {
              try {
                await ensureAllowed(provider, method, params, originalRequest);
                Reflect.apply(target, provider, [
                  request,
                  wrapProviderCallback(provider, method, params, callback),
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
                await ensureAllowed(provider, method, params, originalRequest);
                return forwardProviderResult(provider, method, params, () =>
                  Reflect.apply(target, provider, args),
                );
              })();
            }

            if (typeof provider.sendAsync === "function") {
              return provider.sendAsync(request, callbackOrParams);
            }

            void (async () => {
              try {
                await ensureAllowed(provider, method, params, originalRequest);
                Reflect.apply(target, provider, [
                  request,
                  wrapProviderCallback(
                    provider,
                    method,
                    params,
                    callbackOrParams as JsonRpcCallback,
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
  (proxiedRequest as WithMarker<typeof proxiedRequest>)[WRAP_TARGET] =
    originalRequest as typeof proxiedRequest;
  if (proxiedSendAsync)
    (proxiedSendAsync as WithMarker<typeof proxiedSendAsync>)[WRAP_MARKER] =
      true;
  if (proxiedSendAsync)
    (proxiedSendAsync as WithMarker<typeof proxiedSendAsync>)[WRAP_TARGET] =
      originalSendAsync as typeof proxiedSendAsync;
  if (proxiedSend) {
    (proxiedSend as WithMarker<typeof proxiedSend>)[WRAP_MARKER] = true;
    (proxiedSend as WithMarker<typeof proxiedSend>)[WRAP_TARGET] =
      originalSend as typeof proxiedSend;
  }

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
    return false;
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
  return true;
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

function proxyProviderTree(provider: Eip1193Provider | undefined): boolean {
  if (!provider) return false;
  const wrapped = proxyEthereumProvider(provider);
  if (Array.isArray(provider.providers)) {
    for (const subProvider of provider.providers) {
      proxyEthereumProvider(subProvider);
    }
  }
  return wrapped;
}

function installEthereumAccessorHook(): void {
  const descriptor = Object.getOwnPropertyDescriptor(window, "ethereum");
  if (descriptor && !descriptor.configurable) {
    proxyProviderTree(asProvider(window.ethereum));
    return;
  }

  const originalGet = descriptor?.get;
  const originalSet = descriptor?.set;
  let assignedValue =
    descriptor && "value" in descriptor ? descriptor.value : window.ethereum;
  const existing = originalGet ? originalGet.call(window) : assignedValue;

  Object.defineProperty(window, "ethereum", {
    configurable: true,
    enumerable: descriptor?.enumerable ?? true,
    get() {
      return originalGet ? originalGet.call(window) : assignedValue;
    },
    set(value) {
      if (originalSet) {
        originalSet.call(window, value);
      } else {
        assignedValue = value;
      }
      proxyProviderTree(asProvider(value));
    },
  });

  proxyProviderTree(asProvider(existing));
}

function discoverAndProxyAll(): void {
  for (const key of KNOWN_SOURCES) {
    const provider = asProvider(window[key]);
    if (!provider) continue;
    proxyProviderTree(provider);
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

  if (!proxyEthereumProvider(provider)) return;
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

const eip6963Listener = (event: Event) => {
  const detail = (event as CustomEvent).detail;
  if (detail?.info?.rdns === SCOPEBALL_RDNS) return;
  reannounceWrapped(detail);
};
installState.eip6963Listener = eip6963Listener;
window.addEventListener("eip6963:announceProvider", eip6963Listener);

installEthereumAccessorHook();
window.dispatchEvent(new Event("eip6963:requestProvider"));

discoverAndProxyAll();
const pollDeadline = Date.now() + 30_000;
let pollHandle: number | undefined = window.setInterval(() => {
  discoverAndProxyAll();
  if (Date.now() > pollDeadline && pollHandle !== undefined) {
    window.clearInterval(pollHandle);
    if (installState.pollHandle === pollHandle) {
      installState.pollHandle = undefined;
    }
    pollHandle = undefined;
  }
}, 100);
installState.pollHandle = pollHandle;
