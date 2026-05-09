import Browser from 'webextension-polyfill';
import { Identifier } from '@lib/identifier';
import { decideMessage, recordTxHash } from './orchestrator';
import {
  ensureDefaultPoliciesInstalled,
  reinstallAllPolicies,
} from './policies-loader';
import { applyEnabledIds, getCatalog } from './policy-selection';
import { installReceiptPoller } from './receipt-poller';
import { RequestType, type Message, type MessageResponse } from '@lib/types';

const WALLET_ACTION_TYPES = new Set<string>([
  RequestType.TRANSACTION,
  RequestType.TYPED_SIGNATURE,
  RequestType.UNTYPED_SIGNATURE,
]);

console.log('Scopeball SW alive at', new Date().toISOString());
installReceiptPoller();

// Cold-start prewarm: kick off WASM module load + default policy install
// the moment the SW boots so the first dApp request doesn't pay the 4.77MB
// compile cost inside the 3s lifecycle budget. Best-effort; failures are
// logged and the first decideMessage call retries.
void ensureDefaultPoliciesInstalled().catch((err) => {
  console.warn('[Scopeball] cold-start prewarm failed:', err);
});
Browser.runtime.onInstalled.addListener(() => {
  void ensureDefaultPoliciesInstalled().catch(() => {});
});
Browser.runtime.onStartup.addListener(() => {
  void ensureDefaultPoliciesInstalled().catch(() => {});
});

Browser.runtime.onConnect.addListener((port) => {
  if (port.name !== Identifier.CONTENT_SCRIPT) return;

  port.onMessage.addListener((message: Message) => {
    void handleMessage(message, port);
  });
});

async function handleMessage(
  message: Message,
  port: Browser.Runtime.Port,
): Promise<void> {
  // Tx-hash reports come in over the same port from the inpage proxy.
  if (message.data.type === 'tx-hash-report') {
    recordTxHash(message.data.requestId, message.data.txHash).catch((err) => {
      console.warn('[Scopeball] tx-hash record failed:', err);
    });
    return;
  }
  // Raw / frozen advisories: log only (Plan 5 doesn't gate, but surfaces
  // them so the user can see something happened).
  if (message.data.type === 'raw-transaction-advisory') {
    console.warn('[Scopeball] raw-tx advisory', message.data);
    return;
  }
  if (message.data.type === 'provider-frozen-warning') {
    console.error('[Scopeball] provider frozen', message.data);
    return;
  }

  // Skip messages that aren't wallet actions (transaction / typed sig /
  // untyped sig). The proxy is injected into every iframe (manifest
  // <all_urls> + all_frames), so probes from third-party widgets like
  // Cloudflare's bot challenge can deliver shapes the engine doesn't
  // know how to evaluate. Treating them as policy verdicts would pop a
  // "Blocked: __engine::unsupported" modal on every page that embeds
  // such a widget.
  if (!WALLET_ACTION_TYPES.has(message.data.type)) {
    return;
  }

  const { ok } = await decideMessage(message, {
    onAwaitingUser: () => {
      try {
        port.postMessage({
          requestId: message.requestId,
          kind: 'awaiting-user',
        });
      } catch {
        /* dApp tab gone */
      }
    },
  });
  if (!message.data.bypassed) {
    const response: MessageResponse = {
      requestId: message.requestId,
      data: ok,
    };
    try {
      port.postMessage(response);
    } catch {
      /* dApp tab gone */
    }
  }
}

interface PolicyCatalogRequest {
  type: 'policy-catalog';
}
interface SetEnabledIdsRequest {
  type: 'set-enabled-ids';
  ids: string[];
}
type PopupRequest = PolicyCatalogRequest | SetEnabledIdsRequest;

// webextension-polyfill's listener type accepts `true | void | Promise<any>`,
// not `boolean`. Returning `undefined` (bare `return;`) closes the channel
// just like a literal `false` would — do not "fix" it back to `return false`.
Browser.runtime.onMessage.addListener(
  (message: unknown, _sender, sendResponse: (r: unknown) => void) => {
    const req = message as Partial<PopupRequest> | null;
    if (!req || typeof req !== 'object') return;

    if (req.type === 'policy-catalog') {
      void getCatalog()
        .then((cat) => sendResponse({ ok: true, data: cat }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: 'catalog_failed', message: String(err) },
          }),
        );
      return true; // keep the channel open for the async response
    }

    if (req.type === 'set-enabled-ids') {
      // Reject malformed `ids` instead of silently coercing to []. A
      // non-array, or an array containing non-strings, would otherwise
      // disable all policies without telling the caller.
      if (!Array.isArray(req.ids) || !req.ids.every((id) => typeof id === 'string')) {
        sendResponse({
          ok: false,
          error: { kind: 'invalid_request', message: 'ids must be string[]' },
        });
        return true;
      }
      const ids = req.ids;
      void applyEnabledIds(ids, reinstallAllPolicies)
        .then((result) => sendResponse(result))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: 'apply_failed', message: String(err) },
          }),
        );
      return true;
    }

    return;
  },
);
