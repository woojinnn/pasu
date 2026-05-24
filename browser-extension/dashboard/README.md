# scopeball dashboard

Web UI for managing policies in the scopeball browser extension.

Standalone Vite app — runs as a regular web page at **http://localhost:5174**
(the extension's content-script bridge is pinned to that origin in its
manifest, so any other port will silently fail).

## Setup

```bash
cd browser-extension/dashboard
yarn install      # or npm install
yarn dev          # → http://localhost:5174
```

The Scopeball extension must be loaded as an unpacked extension in your
browser. See `../README.md` for that step. Without the extension installed
and enabled, every SDK call times out after 5 seconds.

## Talking to the extension

All communication goes through the SDK in `../sdk/extension-client.ts`
(aliased as `@scopeball/sdk`):

```ts
import { createExtensionClient } from "@scopeball/sdk";

const c = createExtensionClient();
await c.ping();                                  // handshake → { version: 1 }
await c.getCatalog();                            // defaults ∪ adapter-loader ∪ dashboard
await c.listManaged();                           // dashboard:: policies only
await c.putRaw({ id: "dashboard::my/rule", text: "..." });
await c.putTemplate({ id, templateText, paramsSchema, paramValues });
await c.delete("dashboard::my/rule");
await c.setEnabledIds([...]);
const unsubscribe = c.onChange((keys) => { /* storage changed */ });
```

Do **not** call `window.postMessage` or `chrome.runtime.sendMessage`
directly — the SDK is the only sanctioned surface, and it pins the
message envelope so the bridge accepts it.

## Constraints (enforced by the extension SW)

- Policy id must match `/^dashboard::[A-Za-z0-9_./-]{1,128}$/`.
- Policy body capped at 32 KiB.
- Maximum 200 stored policies.
- Every put auto-enables the policy and triggers a WASM reinstall.
- If WASM rejects the policy, the SW rolls back storage so the bad
  entry doesn't linger.

## What this scaffold gives you

- A working Vite + TypeScript dev loop on port 5174.
- A tiny `src/main.ts` that pings the extension and renders the result.
- `window.c` exposed in DevTools so you can poke the SDK manually
  before any UI exists.

Replace `src/main.ts` and `index.html` with your real app (React, Vue,
Svelte, vanilla — your call). Keep `vite.config.ts`'s `server.port: 5174`
or the bridge will stop injecting.
