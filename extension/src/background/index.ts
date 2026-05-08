import Browser from 'webextension-polyfill';
import { Identifier } from '@lib/identifier';
import { decideMessage, recordTxHash } from './orchestrator';
import { ensureDefaultPoliciesInstalled } from './policies-loader';
import { installReceiptPoller } from './receipt-poller';
import type { Message, MessageResponse } from '@lib/types';

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

async function handleMessage(message: Message, port: Browser.Runtime.Port): Promise<void> {
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

  const { ok } = await decideMessage(message);
  if (!message.data.bypassed) {
    const response: MessageResponse = { requestId: message.requestId, data: ok };
    try {
      port.postMessage(response);
    } catch {
      /* dApp tab gone */
    }
  }
}
