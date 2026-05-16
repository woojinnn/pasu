// ==UserScript==
// @name         ScopeBall RPC Hook
// @namespace    scopeball.local
// @version      0.1.0
// @description  Hook window.ethereum.request, extract RPC fields, and stream them to the ScopeBall web-server (Phase 1).
// @author       you
// @match        *://*/*
// @run-at       document-start
// @grant        GM_xmlhttpRequest
// @grant        GM_addElement
// @grant        unsafeWindow
// @connect      127.0.0.1
// @connect      localhost
// ==/UserScript==

(function () {
  'use strict';

  // Sandbox-side liveness log. If you see this in the dApp page Console,
  // the userscript is at least running. If you don't, Tampermonkey isn't
  // injecting it (developer mode / @match / extension disabled).
  console.log('[scopeball] userscript loaded @', location.href);

  // Backend listens here. Override by editing this constant.
  const ENDPOINT = 'http://127.0.0.1:8080/api/event';

  // -------- Page-world hook (injected as <script>) --------
  // Must be self-contained: no closures over userscript-sandbox vars.
  function pageWorldHook() {
    function isAddress(v) {
      return typeof v === 'string' && /^0x[a-fA-F0-9]{40}$/.test(v);
    }
    function isHexData(v) {
      return typeof v === 'string' && /^0x[a-fA-F0-9]*$/.test(v);
    }
    function looksLikeCalldata(v) {
      return typeof v === 'string' && /^0x[a-fA-F0-9]+$/.test(v) && v.length >= 10;
    }
    function normalizeChainId(v) {
      if (typeof v === 'string') {
        if (/^0x[0-9a-fA-F]+$/.test(v)) return v.toLowerCase();
        const n = Number(v);
        return Number.isFinite(n) ? '0x' + n.toString(16) : null;
      }
      if (typeof v === 'number') return '0x' + v.toString(16);
      return null;
    }
    const ADDRESS_KEYS = new Set(['address','account','owner','spender','recipient','sender','verifyingContract','contractAddress']);
    const CALLDATA_KEYS = new Set(['data','input','calldata','callData']);
    const GAS_KEYS = new Set(['gas','gasLimit','gasPrice','maxFeePerGas','maxPriorityFeePerGas']);

    function extractRpcFields(args, opts) {
      opts = opts || {};
      const result = {
        method: args.method,
        origin: opts.origin,
        currentChainId: opts.currentChainId,
        primaryChainId: undefined,
        chainIds: [],
        addresses: [],
        from: undefined,
        to: undefined,
        value: undefined,
        calldata: [],
        gasFields: {},
        rawParams: args.params == null ? [] : args.params,
        parsedTypedData: undefined,
      };
      function visit(value, key) {
        if (value == null) return;
        if (typeof value === 'string') {
          const t = value.trim();
          if (t.startsWith('{') && t.endsWith('}')) {
            try {
              const parsed = JSON.parse(t);
              if (key === undefined || key === '0' || key === '1') result.parsedTypedData = parsed;
              visit(parsed);
              return;
            } catch (_) { /* not JSON */ }
          }
        }
        if (typeof value === 'string' || typeof value === 'number') {
          if (key === 'chainId') {
            const c = normalizeChainId(value);
            if (c) result.chainIds.push(c);
          }
          if (key === 'from' && isAddress(value)) { result.from = value; result.addresses.push(value); }
          if (key === 'to'   && isAddress(value)) { result.to   = value; result.addresses.push(value); }
          if (key && ADDRESS_KEYS.has(key) && isAddress(value)) result.addresses.push(value);
          if (key && CALLDATA_KEYS.has(key) && looksLikeCalldata(value)) result.calldata.push(value);
          if (key === 'value' && isHexData(value)) result.value = value;
          if (key && GAS_KEYS.has(key)) result.gasFields[key] = String(value);
          if (typeof value === 'string' && isAddress(value)) result.addresses.push(value);
          return;
        }
        if (Array.isArray(value)) { for (const it of value) visit(it); return; }
        if (typeof value === 'object') {
          for (const k in value) {
            if (Object.prototype.hasOwnProperty.call(value, k)) visit(value[k], k);
          }
        }
      }
      visit(args.params);
      result.chainIds  = Array.from(new Set(result.chainIds));
      result.addresses = Array.from(new Set(result.addresses));
      result.calldata  = Array.from(new Set(result.calldata));
      result.primaryChainId = result.chainIds[0] || result.currentChainId;
      return result;
    }

    function hookProvider(provider) {
      if (!provider || typeof provider.request !== 'function') return false;
      if (provider.__scopeballHooked) return true;
      const originalRequest = provider.request.bind(provider);

      let cachedChainId = null;

      function patchedRequest(args) {
        // Loud log so we can see in Console exactly when interception fires.
        try {
          console.log('[scopeball] intercept', (args && args.method) || '?');
        } catch (_) {}
        try {
          if (cachedChainId == null) {
            originalRequest({ method: 'eth_chainId' }).then(
              function (id) { if (typeof id === 'string') cachedChainId = id; },
              function () {},
            );
          }
          const extracted = extractRpcFields(args || {}, {
            origin: location.origin,
            currentChainId: cachedChainId || undefined,
          });
          window.postMessage(
            { source: 'scopeball', payload: extracted },
            location.origin,
          );
          if (args && (args.method === 'wallet_switchEthereumChain'
                       || args.method === 'wallet_addEthereumChain')) {
            cachedChainId = null;
          }
        } catch (e) {
          console.warn('[scopeball] hook error', e);
        }
        return originalRequest(args);
      }

      // Strategy 1: plain assignment. Works on most providers but can be
      // silently blocked by Proxy traps or read-only descriptors.
      let installed = false;
      try {
        provider.request = patchedRequest;
        if (provider.request === patchedRequest) installed = true;
      } catch (_) {}

      // Strategy 2: Object.defineProperty — bypasses some setters/proxies.
      if (!installed) {
        try {
          Object.defineProperty(provider, 'request', {
            value: patchedRequest,
            writable: true,
            configurable: true,
          });
          if (provider.request === patchedRequest) installed = true;
        } catch (_) {}
      }

      // Strategy 3: wrap the entire provider in a Proxy and re-publish on
      // window.ethereum. Catches dApps that read .request via a getter trap.
      if (!installed && provider === window.ethereum) {
        try {
          const wrapped = new Proxy(provider, {
            get: function (target, prop, recv) {
              if (prop === 'request') return patchedRequest;
              const v = Reflect.get(target, prop, recv);
              return typeof v === 'function' ? v.bind(target) : v;
            },
          });
          Object.defineProperty(window, 'ethereum', {
            value: wrapped,
            writable: true,
            configurable: true,
          });
          installed = true;
          console.log('[scopeball] installed via Proxy wrap on window.ethereum');
        } catch (e) {
          console.warn('[scopeball] proxy wrap failed', e);
        }
      }

      if (!installed) {
        console.warn('[scopeball] FAILED to install patched request on', provider);
        return false;
      }

      provider.__scopeballHooked = true;
      console.log('[scopeball] hooked', provider);
      return true;
    }

    function tryHookDefault() {
      return Boolean(window.ethereum && hookProvider(window.ethereum));
    }

    if (!tryHookDefault()) {
      window.addEventListener('ethereum#initialized', tryHookDefault, { once: true });
      let tries = 0;
      const t = setInterval(function () {
        if (tryHookDefault() || ++tries > 40) clearInterval(t); // ~10s @ 250ms
      }, 250);
    }

    // EIP-6963: hook every announced provider (Coinbase, Rainbow, …)
    window.addEventListener('eip6963:announceProvider', function (e) {
      try {
        const detail = e && e.detail;
        if (detail && detail.provider) hookProvider(detail.provider);
      } catch (_) {}
    });
    try { window.dispatchEvent(new Event('eip6963:requestProvider')); } catch (_) {}
  }

  // -------- Inject hook into page world --------
  // Strict CSPs (e.g. Uniswap, many wallet sites) block inline <script> tags
  // appended via document.createElement('script'). We try several strategies in
  // order of cleanliness:
  //   1. GM_addElement('script', { textContent }) — official CSP bypass
  //   2. <script src=blob:...> — works on some CSPs that allow blob:
  //   3. unsafeWindow.ethereum patch from sandbox — last resort, no inject
  const pageWorldCode =
    'console.log("[scopeball] page-world inject running");' +
    '(' + pageWorldHook.toString() + ')();';

  let injected = false;

  if (typeof GM_addElement === 'function') {
    try {
      GM_addElement('script', { textContent: pageWorldCode });
      injected = true;
      console.log('[scopeball] injected via GM_addElement');
    } catch (e) {
      console.warn('[scopeball] GM_addElement failed', e);
    }
  }

  if (!injected) {
    try {
      const blob = new Blob([pageWorldCode], { type: 'application/javascript' });
      const url = URL.createObjectURL(blob);
      const s = document.createElement('script');
      s.src = url;
      s.onload = function () { URL.revokeObjectURL(url); s.remove(); };
      (document.head || document.documentElement).appendChild(s);
      injected = true;
      console.log('[scopeball] injected via blob URL');
    } catch (e) {
      console.warn('[scopeball] blob inject failed', e);
    }
  }

  if (!injected) {
    // Sandbox-side fallback. Less ideal: the patched function lives in our
    // sandbox so cross-realm Promise return values can be flaky on some sites.
    try {
      const w = typeof unsafeWindow !== 'undefined' ? unsafeWindow : window;
      hookProviderFromSandbox(w);
      console.log('[scopeball] hooked via unsafeWindow fallback');
    } catch (e) {
      console.warn('[scopeball] all inject strategies failed', e);
    }
  }

  // Sandbox-side fallback hook (used only when page-world inject is blocked).
  function hookProviderFromSandbox(w) {
    function tryHook() {
      const eth = w.ethereum;
      if (!eth || typeof eth.request !== 'function' || eth.__scopeballHooked) return false;
      const original = eth.request.bind(eth);
      eth.__scopeballHooked = true;
      eth.request = function patched(args) {
        try {
          // Re-use the same extractor we ship via the inline page-world code
          // by stringifying it; here we just send the raw method/params and
          // let the backend (eventually) parse, OR keep a JS copy in sandbox.
          // For simplicity, post raw + a marker.
          const payload = {
            method: (args && args.method) || 'unknown',
            origin: w.location && w.location.origin,
            primaryChainId: undefined,
            chainIds: [],
            addresses: [],
            calldata: [],
            gasFields: {},
            rawParams: (args && args.params) || [],
            _sandboxFallback: true,
          };
          window.postMessage(
            { source: 'scopeball', payload: payload },
            location.origin,
          );
        } catch (_) {}
        return original(args);
      };
      console.log('[scopeball] sandbox-hooked', eth);
      return true;
    }
    if (!tryHook()) {
      let n = 0;
      const t = setInterval(function () {
        if (tryHook() || ++n > 40) clearInterval(t);
      }, 250);
    }
  }

  // -------- Sandbox-side relay: postMessage → backend --------
  // Listen on both the sandbox window AND (if available) the page-world
  // unsafeWindow. Some Tampermonkey configs send postMessage to a different
  // window object than the sandbox listens on.
  function onScopeballMessage(e) {
    const d = e && e.data;
    if (!d || d.source !== 'scopeball' || !d.payload) return;
    console.log('[scopeball] sandbox got message', d.payload.method);

    GM_xmlhttpRequest({
      method: 'POST',
      url: ENDPOINT,
      headers: { 'Content-Type': 'application/json' },
      data: JSON.stringify(d.payload),
      onload: function (res) {
        if (res.status >= 200 && res.status < 300) {
          console.log('[scopeball] POST ok', res.status);
        } else {
          console.warn('[scopeball] POST non-2xx', res.status, res.responseText);
        }
      },
      onerror: function (err) {
        console.warn('[scopeball] POST failed', err);
      },
      ontimeout: function () {
        console.warn('[scopeball] POST timeout');
      },
      timeout: 3000,
    });
  }

  window.addEventListener('message', onScopeballMessage);
  try {
    if (typeof unsafeWindow !== 'undefined' && unsafeWindow !== window) {
      unsafeWindow.addEventListener('message', onScopeballMessage);
      console.log('[scopeball] sandbox listening on both window and unsafeWindow');
    }
  } catch (_) {}
})();
