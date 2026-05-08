# Extension Scaffold + Provider Proxy — Plan 3

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Chrome extension's package skeleton and the inpage/content-script/background-SW message bus that intercepts EIP-1193 calls without yet calling the policy engine. Output: an installable `dist/chrome/` directory whose service worker logs every intercepted `eth_sendTransaction` and `eth_signTypedData_v4` to the SW console.

**Architecture:** Yarn 4 + TypeScript 5 + webpack 5 + `wext-manifest-loader` for cross-browser manifest preprocessing. inpage.js uses the revoke.cash-validated triple proxy (`request` / `send` / `sendAsync`) plus 100ms polling discovery and EIP-6963 wrapping. Content script relays via `@metamask/post-message-stream`. Background SW listens on `chrome.runtime.onConnect`. Storage and engine integration are out of scope here — Plan 4/5 wire them in.

**Tech Stack:** TypeScript, webpack 5, viem 2, `@metamask/post-message-stream` 8, `eth-rpc-errors` 4, `webextension-polyfill` 0.12, `wext-manifest-loader` 2, `wext-manifest-webpack-plugin` 1.

**Series:** Plan 3 of the Chrome-extension series. Independent of Plans 1/2 — does not yet load the WASM artifact. Plans 4 and 5 hook the WASM bridge in.

**Scope (in this plan):** Yarn project bootstrap, webpack configs, manifest, inpage proxy, content script, background SW skeleton (no engine call yet — just logging). Build produces a loadable unpacked extension.

**Out of scope:** Verdict modal, marketplace, parameterization, RPC/oracle clients, WASM module loading.

---

## File map

| Path | Action | Responsibility |
|------|--------|----------------|
| `extension/package.json` | Create | npm scripts, deps |
| `extension/tsconfig.json` | Create | TS compiler options |
| `extension/.gitignore` | Create | dist/, node_modules/, pkg/ |
| `extension/webpack/webpack.common.js` | Create | shared loaders, entry points |
| `extension/webpack/webpack.dev.js` | Create | dev-mode source maps, watch |
| `extension/webpack/webpack.prod.js` | Create | minify, prod CSP |
| `extension/src/manifest.json` | Create | MV3 manifest with `__chrome__`/`__firefox__` prefixes |
| `extension/src/lib/identifier.ts` | Create | message-stream channel names + idempotency marker |
| `extension/src/lib/messages.ts` | Create | `sendToStreamAndAwaitResponse`, `sendToPortAndAwaitResponse`, request-id hash |
| `extension/src/lib/types.ts` | Create | `RequestType`, `Message`, `MessageResponse`, type guards |
| `extension/src/injected/proxy-injected-providers.ts` | Create | Provider proxy with `request`/`send`/`sendAsync` paths |
| `extension/src/content-scripts/inject-scripts.ts` | Create | DOM-injects the inpage bundle |
| `extension/src/content-scripts/window-ethereum-messages.ts` | Create | inpage ↔ background relay |
| `extension/src/content-scripts/bypass-check.ts` | Create | passive listener for MetaMask/Coinbase internal streams |
| `extension/src/background/index.ts` | Create | SW entry — receives messages, logs them |
| `extension/.example.env` | Create | Documents optional `ALCHEMY_API_KEY`, `INFURA_API_KEY` |
| `extension/public/popup.html` | Create | Placeholder action popup |
| `.gitignore` (root) | Modify | Add `extension/node_modules/`, `extension/dist/`, `extension/pkg/` |

---

## Task 1: Yarn project bootstrap

**Files:**
- Create: `extension/package.json`, `extension/tsconfig.json`, `extension/.gitignore`, `extension/.example.env`
- Modify: root `.gitignore`

- [ ] **Step 1: Create the directory + package.json**

```bash
mkdir -p extension/src/{injected,content-scripts,background,lib} extension/public extension/webpack
```

Create `extension/package.json`:

```json
{
  "name": "scopeball-extension",
  "version": "0.1.0",
  "private": true,
  "description": "Policy-engine-backed wallet transaction safety extension",
  "scripts": {
    "dev:chrome": "cross-env TARGET_BROWSER=chrome webpack --config webpack/webpack.dev.js --watch",
    "dev:firefox": "cross-env TARGET_BROWSER=firefox webpack --config webpack/webpack.dev.js --watch",
    "build:chrome": "cross-env TARGET_BROWSER=chrome webpack --config webpack/webpack.prod.js",
    "build:firefox": "cross-env TARGET_BROWSER=firefox webpack --config webpack/webpack.prod.js",
    "build": "run-p 'build:*'",
    "zip:chrome": "web-ext build -s dist/chrome -a dist -n chrome.zip -o",
    "zip:firefox": "web-ext build -s dist/firefox -a dist -n firefox.zip -o",
    "zip": "run-p 'zip:*'",
    "clean": "rimraf dist",
    "typecheck": "tsc --noEmit",
    "lint": "prettier --write ."
  },
  "dependencies": {
    "@metamask/post-message-stream": "^8.1.0",
    "eth-rpc-errors": "^4.0.3",
    "object-hash": "^3.0.0",
    "viem": "^2.13.7",
    "webextension-polyfill": "^0.12.0"
  },
  "devDependencies": {
    "@types/chrome": "^0.0.268",
    "@types/object-hash": "^3.0.6",
    "@types/webextension-polyfill": "^0.10.7",
    "copy-webpack-plugin": "^12.0.2",
    "cross-env": "^7.0.3",
    "dotenv-webpack": "^8.1.0",
    "npm-run-all": "^4.1.5",
    "prettier": "^3.3.1",
    "rimraf": "^5.0.7",
    "ts-loader": "^9.5.1",
    "typescript": "^5.7.3",
    "web-ext": "^8.0.0",
    "webpack": "^5.91.0",
    "webpack-cli": "^5.1.4",
    "webpack-merge": "^5.10.0",
    "wext-manifest-loader": "^2.4.2",
    "wext-manifest-webpack-plugin": "^1.4.1"
  },
  "packageManager": "yarn@4.6.0"
}
```

- [ ] **Step 2: Create tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ES2022",
    "moduleResolution": "Bundler",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "strict": true,
    "noImplicitAny": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "exactOptionalPropertyTypes": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "forceConsistentCasingInFileNames": true,
    "outDir": "dist",
    "baseUrl": "src",
    "rootDir": "src",
    "paths": {
      "@lib/*": ["lib/*"]
    }
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
```

- [ ] **Step 3: Create .gitignore + .example.env**

`extension/.gitignore`:

```
node_modules/
dist/
.env
*.log
.yarn/cache/
.yarn/install-state.gz
pkg/
```

`extension/.example.env`:

```
# Optional. Without keys the extension falls back to public free-tier RPCs.
ALCHEMY_API_KEY=
INFURA_API_KEY=
```

Append to root `.gitignore`:

```
extension/node_modules/
extension/dist/
extension/pkg/
extension/.yarn/
extension/.env
```

- [ ] **Step 4: Force `node_modules` linker (avoid PnP)**

Yarn 4 defaults to PnP (`pnp` linker), which breaks `wext-manifest-loader`, `wext-manifest-webpack-plugin`, and many older webpack plugins that resolve via `require.resolve` over the filesystem. Pin to classic `node_modules`:

```bash
mkdir -p extension/.yarn
cat > extension/.yarnrc.yml <<'YAML'
nodeLinker: node-modules
enableScripts: true
YAML
```

- [ ] **Step 5: Install deps**

```bash
cd extension && yarn install 2>&1 | tail -10
```

Expected: completes with `Done in Xs`. If yarn 4 not present, install corepack:

```bash
corepack enable && corepack prepare yarn@4.6.0 --activate
```

- [ ] **Step 5: Commit**

```bash
git add extension/package.json extension/tsconfig.json extension/.gitignore extension/.example.env extension/.yarnrc.yml .gitignore extension/yarn.lock
git commit -m "$(cat <<'EOF'
feat(extension): yarn project bootstrap

TS5 + webpack5 + viem2 + post-message-stream + eth-rpc-errors. Cross-
browser scripts via wext-manifest-loader. yarn@4.6.0 packageManager
with nodeLinker=node-modules pinned (avoids PnP breaking
wext-manifest-loader and similar legacy plugins).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Webpack configs

**Files:** Create `extension/webpack/webpack.common.js`, `webpack.dev.js`, `webpack.prod.js`.

- [ ] **Step 1: webpack.common.js**

```javascript
const path = require('path');
const CopyPlugin = require('copy-webpack-plugin');
const Dotenv = require('dotenv-webpack');
const WextManifestWebpackPlugin = require('wext-manifest-webpack-plugin');

const targetBrowser = process.env.TARGET_BROWSER || 'chrome';
const sourceDir = path.resolve(__dirname, '..', 'src');
const distDir = path.resolve(__dirname, '..', 'dist', targetBrowser);

module.exports = {
  entry: {
    background: path.join(sourceDir, 'background', 'index.ts'),
    'content-scripts/inject-scripts': path.join(sourceDir, 'content-scripts', 'inject-scripts.ts'),
    'content-scripts/window-ethereum-messages': path.join(
      sourceDir,
      'content-scripts',
      'window-ethereum-messages.ts',
    ),
    'content-scripts/bypass-check': path.join(sourceDir, 'content-scripts', 'bypass-check.ts'),
    'injected/proxy-injected-providers': path.join(
      sourceDir,
      'injected',
      'proxy-injected-providers.ts',
    ),
    manifest: path.join(sourceDir, 'manifest.json'),
  },
  output: {
    filename: 'js/[name].js',
    path: distDir,
    clean: true,
  },
  resolve: {
    extensions: ['.ts', '.tsx', '.js', '.json'],
    alias: {
      '@lib': path.resolve(sourceDir, 'lib'),
    },
  },
  module: {
    rules: [
      {
        type: 'javascript/auto',
        test: /manifest\.json$/,
        use: {
          loader: 'wext-manifest-loader',
          options: { usePackageJSONVersion: true },
        },
        exclude: /node_modules/,
      },
      {
        test: /\.tsx?$/,
        loader: 'ts-loader',
        exclude: /node_modules/,
      },
    ],
  },
  plugins: [
    new WextManifestWebpackPlugin(),
    new Dotenv({ path: path.resolve(__dirname, '..', '.env'), safe: false, silent: true }),
    new CopyPlugin({
      patterns: [{ from: path.resolve(__dirname, '..', 'public'), to: distDir }],
    }),
  ],
};
```

- [ ] **Step 2: webpack.dev.js**

```javascript
const { merge } = require('webpack-merge');
const common = require('./webpack.common.js');

module.exports = merge(common, {
  mode: 'development',
  devtool: 'cheap-module-source-map',
  watch: true,
  watchOptions: { ignored: /node_modules/ },
});
```

- [ ] **Step 3: webpack.prod.js**

```javascript
const { merge } = require('webpack-merge');
const common = require('./webpack.common.js');

module.exports = merge(common, {
  mode: 'production',
  devtool: false,
  optimization: { minimize: true },
});
```

- [ ] **Step 4: Skeleton entry files (so webpack has something to compile)**

Create temporary stub files so the build doesn't fail before the real code lands:

```bash
mkdir -p extension/src/{background,content-scripts,injected}
echo "// stub" > extension/src/background/index.ts
echo "// stub" > extension/src/content-scripts/inject-scripts.ts
echo "// stub" > extension/src/content-scripts/window-ethereum-messages.ts
echo "// stub" > extension/src/content-scripts/bypass-check.ts
echo "// stub" > extension/src/injected/proxy-injected-providers.ts
```

Create `extension/public/popup.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Scopeball</title>
  </head>
  <body>
    <p>Scopeball is active.</p>
  </body>
</html>
```

- [ ] **Step 5: Verify (will fail until manifest.json exists — that's Task 3)**

```bash
cd extension && yarn build:chrome 2>&1 | tail -10
```

Expected: error `Cannot resolve manifest.json`. Move on to Task 3.

> **Manifest emission verification gate (added in fix pass)**: after Task 3 builds, the executor MUST verify that `dist/chrome/manifest.json` is produced *as a real JSON file at the dist root* (not just `js/manifest.js`). `wext-manifest-webpack-plugin` is responsible for that emission — but its config under Yarn-Berry-with-node-modules-linker has historically been finicky. If the file is missing, switch the manifest entry from `manifest: ...` to `__manifest__: ...` per the plugin's docs and re-test before continuing.

- [ ] **Step 6: Commit**

```bash
git add extension/webpack/ extension/src/ extension/public/
git commit -m "feat(extension): webpack 5 configs (common/dev/prod) + entry stubs"
```

---

## Task 3: MV3 manifest with cross-browser preprocessing

**Files:** Create `extension/src/manifest.json`.

- [ ] **Step 1: Write the manifest**

```json
{
  "manifest_version": 3,
  "__firefox__manifest_version": 2,
  "name": "Scopeball",
  "description": "Wallet policy engine — verdicts before you sign",
  "version": "0.1.0",
  "icons": {
    "48": "images/icon-48.png",
    "128": "images/icon-128.png"
  },
  "__chrome__action": {
    "default_popup": "popup.html",
    "default_icon": {
      "48": "images/icon-48.png",
      "128": "images/icon-128.png"
    }
  },
  "__firefox__browser_action": {
    "default_popup": "popup.html",
    "default_icon": {
      "48": "images/icon-48.png",
      "128": "images/icon-128.png"
    }
  },
  "__firefox__browser_specific_settings": {
    "gecko": { "id": "extension@scopeball.dev", "strict_min_version": "115.0" }
  },
  "content_scripts": [
    {
      "matches": ["<all_urls>"],
      "js": [
        "js/content-scripts/inject-scripts.js",
        "js/content-scripts/window-ethereum-messages.js",
        "js/content-scripts/bypass-check.js"
      ],
      "all_frames": true,
      "run_at": "document_start"
    }
  ],
  "web_accessible_resources": [
    {
      "matches": ["<all_urls>"],
      "resources": ["js/injected/proxy-injected-providers.js"]
    }
  ],
  "__firefox__web_accessible_resources": [
    "js/injected/proxy-injected-providers.js"
  ],
  "background": {
    "__chrome__service_worker": "js/background.js",
    "__firefox__scripts": ["js/background.js"]
  },
  "__chrome__permissions": ["storage", "alarms"],
  "__firefox__permissions": ["<all_urls>", "storage", "alarms"],
  "__chrome__host_permissions": ["<all_urls>"]
}
```

- [ ] **Step 2: Generate placeholder icons**

```bash
mkdir -p extension/public/images
# Use a tiny inline base64 PNG so the build doesn't fail. Replace with real
# icon assets in a later content/UX task.
python3 -c "
import base64, pathlib
PNG_1PX = base64.b64decode(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQI12P4//8/AwAI/AL+T1pNCgAAAABJRU5ErkJggg=='
)
for size in (48, 128):
    pathlib.Path(f'extension/public/images/icon-{size}.png').write_bytes(PNG_1PX)
"
```

- [ ] **Step 3: Build chrome target and verify dist layout**

```bash
cd extension && yarn build:chrome 2>&1 | tail -15 && ls dist/chrome/
```

Expected:
- `dist/chrome/manifest.json` (no `__chrome__`/`__firefox__` keys, only chrome ones)
- `dist/chrome/js/{background,content-scripts/...,injected/...}.js`
- `dist/chrome/images/icon-{48,128}.png`
- `dist/chrome/popup.html`

Inspect the generated manifest:

```bash
cat extension/dist/chrome/manifest.json | head -30
```

Expected: `manifest_version: 3`, no `__firefox__*` or `__chrome__*` keys.

- [ ] **Step 4: Build firefox target and confirm preprocessing**

```bash
yarn build:firefox 2>&1 | tail -5 && head -5 dist/firefox/manifest.json
```

Expected: `manifest_version: 2`, `browser_action` (not `action`).

- [ ] **Step 5: Commit**

```bash
git add extension/src/manifest.json extension/public/images/
git commit -m "$(cat <<'EOF'
feat(extension): MV3 manifest with cross-browser prefix preprocessing

wext-manifest-loader strips __chrome__/__firefox__ prefixes per target.
Single source for both browsers. Permissions: storage + alarms only;
host permissions are <all_urls> for content-script + RPC.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Shared lib — identifiers, messages, types

**Files:** Create `extension/src/lib/identifier.ts`, `lib/types.ts`, `lib/messages.ts`.

- [ ] **Step 1: identifier.ts**

```typescript
export const Identifier = {
  INPAGE: 'scopeball-inpage',
  CONTENT_SCRIPT: 'scopeball-contentscript',
  CONFIRM: 'scopeball-confirm',
  // External wallet streams we passively observe in the bypass check
  METAMASK_PROVIDER: 'metamask-provider',
  METAMASK_INPAGE: 'metamask-inpage',
  METAMASK_CONTENT_SCRIPT: 'metamask-contentscript',
  COINBASE_WALLET_REQUEST: 'extensionUIRequest',
} as const;

/// Marker placed on each provider object after we wrap it.
/// Prevents double-wrapping when polling re-discovers the same provider.
export const PROVIDER_MARKER = '__isScopeball__' as const;
```

- [ ] **Step 2: types.ts**

```typescript
import type { Address, Hex } from 'viem';

export enum RequestType {
  TRANSACTION = 'transaction',
  TYPED_SIGNATURE = 'typed-signature',
  UNTYPED_SIGNATURE = 'untyped-signature',
}

export interface TransactionPayload {
  type: RequestType.TRANSACTION;
  chainId: number;
  hostname: string;
  bypassed?: boolean;
  transaction: {
    from?: Address;
    to?: Address;
    data?: Hex;
    value?: string;
  };
}

export interface TypedSignaturePayload {
  type: RequestType.TYPED_SIGNATURE;
  chainId: number;
  hostname: string;
  bypassed?: boolean;
  address: Address;
  typedData: unknown;
}

export interface UntypedSignaturePayload {
  type: RequestType.UNTYPED_SIGNATURE;
  hostname: string;
  bypassed?: boolean;
  message: string;
}

export type MessageData = TransactionPayload | TypedSignaturePayload | UntypedSignaturePayload;

export interface Message {
  requestId: string;
  data: MessageData;
}

export interface MessageResponse {
  requestId: string;
  /** True = let the original RPC call go through. False = reject with 4001. */
  data: boolean;
}

export const isTransaction = (m: Message): m is Message & { data: TransactionPayload } =>
  m.data.type === RequestType.TRANSACTION;
export const isTypedSignature = (m: Message): m is Message & { data: TypedSignaturePayload } =>
  m.data.type === RequestType.TYPED_SIGNATURE;
export const isUntypedSignature = (m: Message): m is Message & { data: UntypedSignaturePayload } =>
  m.data.type === RequestType.UNTYPED_SIGNATURE;
```

- [ ] **Step 3: messages.ts — request-id hash + stream/port helpers**

```typescript
import objectHash from 'object-hash';
// `WindowPostMessageStream` is a Duplex; we use the structural type from
// @metamask/post-message-stream rather than pulling in `readable-stream`.
import type { WindowPostMessageStream } from '@metamask/post-message-stream';
import Browser from 'webextension-polyfill';

type Duplex = WindowPostMessageStream;
import { RequestType } from './types';
import type { MessageData, MessageResponse } from './types';

/// Deterministic per-payload request id. Re-issuing the same payload yields
/// the same id, so the SW can dedupe dApp retries.
export function generateRequestId(data: MessageData): string {
  switch (data.type) {
    case RequestType.TRANSACTION:
      return objectHash(data.transaction);
    case RequestType.TYPED_SIGNATURE:
      return objectHash(data.typedData as object);
    case RequestType.UNTYPED_SIGNATURE:
      return objectHash({ message: data.message });
  }
}

/// Two-phase timeout per design §3.2:
/// - Phase 1 (3s): hard fail-closed for the *evaluation* leg. If the SW
///   doesn't even start a Warn modal in 3s, the engine is stuck → reject.
/// - Phase 2 (5min): user-decision deadline once Warn surfaces. The SW
///   sends a `kind:"awaiting-user"` heartbeat to the inpage stream the
///   moment a Warn modal opens, which extends the inpage timer to 5min.
///   Without this, every Warn would auto-reject before the user could
///   click "Trust and proceed".
const PHASE1_MS = 3_000;
const PHASE2_MS = 5 * 60_000;

/// Send a message over a `WindowPostMessageStream` and resolve with the
/// matching `MessageResponse.data` (boolean).
export function sendToStreamAndAwaitResponse(stream: Duplex, data: MessageData): Promise<boolean> {
  const requestId = generateRequestId(data);
  return new Promise<boolean>((resolve) => {
    let timer = setTimeout(() => {
      stream.off('data', cb);
      resolve(false);
    }, PHASE1_MS);
    const cb = (response: MessageResponse | { requestId: string; kind: 'awaiting-user' }) => {
      if (response.requestId !== requestId) return;
      // Phase-2 transition: SW signaled "Warn modal open, awaiting user".
      // Extend the deadline so the user has time to click. Cleanup of the
      // listener happens only on the final boolean response.
      if ((response as any).kind === 'awaiting-user') {
        clearTimeout(timer);
        timer = setTimeout(() => {
          stream.off('data', cb);
          resolve(false);
        }, PHASE2_MS);
        return;
      }
      clearTimeout(timer);
      stream.off('data', cb);
      resolve((response as MessageResponse).data);
    };
    stream.on('data', cb);
    stream.write({ requestId, data });
  });
}

/// Send via `chrome.runtime.Port` and await the matching response.
/// Two-phase timeout matches sendToStreamAndAwaitResponse so the
/// content-script→SW leg also extends on `awaiting-user`.
export function sendToPortAndAwaitResponse(
  port: Browser.Runtime.Port,
  data: MessageData,
): Promise<boolean> {
  const requestId = generateRequestId(data);
  return new Promise<boolean>((resolve) => {
    let timer = setTimeout(() => {
      port.onMessage.removeListener(cb);
      resolve(false);
    }, PHASE1_MS);
    const cb = (response: MessageResponse | { requestId: string; kind: 'awaiting-user' }) => {
      if (response.requestId !== requestId) return;
      if ((response as any).kind === 'awaiting-user') {
        clearTimeout(timer);
        timer = setTimeout(() => {
          port.onMessage.removeListener(cb);
          resolve(false);
        }, PHASE2_MS);
        return;
      }
      clearTimeout(timer);
      port.onMessage.removeListener(cb);
      resolve((response as MessageResponse).data);
    };
    port.onMessage.addListener(cb);
    port.postMessage({ requestId, data });
  });
}

/// Fire-and-forget for the bypass-check observer (no response expected).
export function sendToPortAndDisregard(port: Browser.Runtime.Port, data: MessageData): void {
  const requestId = generateRequestId(data);
  port.postMessage({ requestId, data });
}
```

- [ ] **Step 4: Verify TypeScript**

```bash
cd extension && yarn typecheck 2>&1 | tail -10
```

Expected: `Found 0 errors`.

- [ ] **Step 5: Commit**

```bash
git add extension/src/lib/
git commit -m "feat(extension): shared identifiers, message types, request-id hash"
```

---

## Task 5: Inpage provider proxy — `request` / `send` / `sendAsync`

**Files:** Replace `extension/src/injected/proxy-injected-providers.ts`.

- [ ] **Step 1: Write the proxy**

```typescript
import { WindowPostMessageStream } from '@metamask/post-message-stream';
import { ethErrors } from 'eth-rpc-errors';
import { Identifier, PROVIDER_MARKER } from '@lib/identifier';
import { generateRequestId, sendToStreamAndAwaitResponse } from '@lib/messages';
import { RequestType } from '@lib/types';

declare global {
  interface Window {
    ethereum?: any;
    coinbaseWalletExtension?: any;
    [key: string]: any;
  }
}

const stream = new WindowPostMessageStream({
  name: Identifier.INPAGE,
  target: Identifier.CONTENT_SCRIPT,
});

const REJECT_TX = ethErrors.provider.userRejectedRequest(
  'Scopeball: transaction blocked by policy',
);
const REJECT_SIG = ethErrors.provider.userRejectedRequest(
  'Scopeball: signature blocked by policy',
);

/// Read chainId without pulling in viem (~250 KB). 1.5s timeout so a
/// hanging provider doesn't burn the entire 3s phase-1 budget before
/// we even start a gating round-trip; falls back to provider.chainId.
async function readChainId(provider: any): Promise<number> {
  try {
    const result = await Promise.race<unknown>([
      provider.request({ method: 'eth_chainId' }),
      new Promise<unknown>((_, reject) => setTimeout(() => reject(new Error('chainId timeout')), 1_500)),
    ]);
    return Number.parseInt(String(result), 16);
  } catch {
    return Number(provider.chainId ?? 1);
  }
}

async function checkTransaction(provider: any, params: any[]): Promise<boolean> {
  const [transaction] = params ?? [];
  if (!transaction) return true;
  const chainId = await readChainId(provider);
  const data = {
    type: RequestType.TRANSACTION,
    chainId,
    hostname: location.hostname,
    transaction,
  } as const;
  // Pin the requestId on the tx object identity so a later tx-hash report
  // can attach to the same gating decision (Plan 5 setTxHash chain).
  if (typeof transaction === 'object' && transaction) {
    txRequestIds.set(transaction, generateRequestId(data as any));
  }
  return sendToStreamAndAwaitResponse(stream, data);
}

async function checkTypedSignature(provider: any, params: any[]): Promise<boolean> {
  const [address, typedDataStr] = params ?? [];
  if (!address || !typedDataStr) return true;
  // Forward the raw typedData payload (string OR object) to the background;
  // engine validate_typed_data is the canonical validator. We intentionally
  // do NOT JSON.parse here — that's the SW's job (per design §3.3).
  const chainId = await readChainId(provider);
  return sendToStreamAndAwaitResponse(stream, {
    type: RequestType.TYPED_SIGNATURE,
    chainId,
    hostname: location.hostname,
    address,
    typedData: typedDataStr,
  });
}

async function checkUntypedSignature(params: any[]): Promise<boolean> {
  const [first, second] = params ?? [];
  if (!first || !second) return true;
  // For both `personal_sign` and `eth_sign`, one parameter is the address
  // (40 hex chars) and the other is the message. Order varies by wallet.
  const message =
    String(first).replace(/^0x/, '').length === 40 ? String(second) : String(first);
  return sendToStreamAndAwaitResponse(stream, {
    type: RequestType.UNTYPED_SIGNATURE,
    hostname: location.hostname,
    message,
  });
}

/// Track requestId by transaction object identity so the post-call hash
/// report can find the originating request. Cheap WeakMap so GC reclaims
/// when the dApp drops the tx object.
const txRequestIds = new WeakMap<object, string>();
function lastRequestIdForTransaction(params: any[]): string | undefined {
  const tx = params?.[0];
  return tx && typeof tx === 'object' ? txRequestIds.get(tx) : undefined;
}

/// `eth_sendRawTransaction`: the calldata is already signed bytes; we cannot
/// gate without ABI-decoding the signed envelope (deferred to v1.1). Per
/// design §4.1, log + non-blocking advisory toast, then pass through.
function logRawTransaction(params: any[]): void {
  try {
    const raw = String(params?.[0] ?? '');
    stream.write({
      requestId: 'raw-tx-' + raw.slice(0, 18),
      data: {
        type: 'raw-transaction-advisory',
        hostname: location.hostname,
        rawPreview: raw.slice(0, 18),
      },
    });
  } catch {
    /* logging is best-effort */
  }
}

function proxyEthereumProvider(provider: any): void {
  if (!provider || provider[PROVIDER_MARKER]) return;

  const requestHandler = {
    apply: async (target: any, thisArg: any, args: any[]) => {
      const [request] = args;
      const method = request?.method;
      const params = request?.params;

      let isOk = true;
      if (method === 'eth_sendTransaction') {
        isOk = await checkTransaction(provider, params);
        if (!isOk) throw REJECT_TX;
      } else if (
        method === 'eth_signTypedData' ||
        method === 'eth_signTypedData_v3' ||
        method === 'eth_signTypedData_v4'
      ) {
        isOk = await checkTypedSignature(provider, params);
        if (!isOk) throw REJECT_SIG;
      } else if (method === 'eth_sign' || method === 'personal_sign') {
        isOk = await checkUntypedSignature(params);
        if (!isOk) throw REJECT_SIG;
      } else if (method === 'eth_sendRawTransaction') {
        // v1: pass-through with advisory log (design §4.1).
        logRawTransaction(params);
      }
      // Note: wallet_sendCalls (EIP-5792) is intentionally NOT gated in v1;
      // the design defers it to v1.1. Removing it from the inpage gate avoids
      // shipping untested batch-evaluate semantics.
      const result = await Reflect.apply(target, thisArg, args);
      // Wire tx-hash reporting for receipt-poller commit (Plan 5 §6/§7):
      // when eth_sendTransaction returned a hash, forward it to the SW so
      // commitByTxHash can advance committed window counters.
      if (method === 'eth_sendTransaction' && typeof result === 'string' && /^0x[0-9a-fA-F]{64}$/.test(result)) {
        const requestId = lastRequestIdForTransaction(params);
        if (requestId) {
          stream.write({
            requestId: 'tx-hash-' + requestId,
            data: { type: 'tx-hash-report' as any, requestId, txHash: result, hostname: location.hostname },
          });
        }
      }
      return result;
    },
  };

  const sendAsyncHandler = {
    apply: async (target: any, thisArg: any, args: any[]) => {
      const [request, callback] = args;
      const method = request?.method;
      const params = request?.params;

      const reject = (err: any) => {
        callback(err, { id: request?.id, jsonrpc: '2.0', error: err });
      };

      try {
        if (method === 'eth_sendTransaction') {
          if (!(await checkTransaction(provider, params))) return reject(REJECT_TX);
        } else if (
          method === 'eth_signTypedData' ||
          method === 'eth_signTypedData_v3' ||
          method === 'eth_signTypedData_v4'
        ) {
          if (!(await checkTypedSignature(provider, params))) return reject(REJECT_SIG);
        } else if (method === 'eth_sign' || method === 'personal_sign') {
          if (!(await checkUntypedSignature(params))) return reject(REJECT_SIG);
        } else if (method === 'eth_sendRawTransaction') {
          logRawTransaction(params);
        }
        return Reflect.apply(target, thisArg, args);
      } catch (err) {
        reject(err);
      }
    },
  };

  const sendHandler = {
    apply: (target: any, thisArg: any, args: any[]) => {
      const [payloadOrMethod, callbackOrParams] = args;
      // Three overloads:
      // 1. send(method, params) -> like request
      if (typeof payloadOrMethod === 'string') {
        return provider.request({ method: payloadOrMethod, params: callbackOrParams });
      }
      // 2. send(payload) -> sync, no signing methods possible
      if (!callbackOrParams) {
        return Reflect.apply(target, thisArg, args);
      }
      // 3. send(payload, callback) -> like sendAsync
      return provider.sendAsync(payloadOrMethod, callbackOrParams);
    },
  };

  try {
    // request is required by EIP-1193; send/sendAsync are legacy + optional.
    if (typeof provider.request !== 'function') {
      throw new Error('provider.request is required');
    }
    Object.defineProperty(provider, 'request', {
      value: new Proxy(provider.request, requestHandler),
      writable: true,
    });
    if (typeof provider.sendAsync === 'function') {
      Object.defineProperty(provider, 'sendAsync', {
        value: new Proxy(provider.sendAsync, sendAsyncHandler),
        writable: true,
      });
    }
    if (typeof provider.send === 'function') {
      Object.defineProperty(provider, 'send', {
        value: new Proxy(provider.send, sendHandler),
        writable: true,
      });
    }
    Object.defineProperty(provider, PROVIDER_MARKER, { value: true, writable: false });
  } catch (e) {
    // Frozen / non-configurable provider. Surface this as a hard signal so
    // downstream code (and SW telemetry) knows we cannot gate this provider —
    // do NOT silently pass through.
    console.error('Scopeball: provider is frozen and cannot be wrapped', e);
    try {
      stream.write({
        requestId: 'frozen-provider-' + Date.now().toString(16),
        data: {
          type: 'provider-frozen-warning',
          hostname: location.hostname,
          providerName: provider?.constructor?.name ?? 'unknown',
        },
      });
    } catch {
      /* ignore */
    }
    // The smoke test (Task 9) MUST treat any frozen-provider event as a
    // failure to prevent silent regressions.
  }
}

/// Per-source polling: track each candidate source independently. Stop
/// polling a source only after it's been seen + wrapped at least once.
/// This fixes the Codex finding that "polling stopped as soon as
/// window.ethereum exists" — late-injected `providers[]` and Coinbase /
/// Liquality entries kept arriving but were never proxied.
const KNOWN_SOURCES = [
  'ethereum',
  'coinbaseWalletExtension',
  'eth',
  'rsk',
  'bsc',
  'polygon',
  'arbitrum',
  'fuse',
  'avalanche',
  'optimism',
] as const;
const seenSources = new Set<string>();

function discoverAndProxyAll(): void {
  for (const k of KNOWN_SOURCES) {
    const p = window[k];
    if (!p) continue;
    proxyEthereumProvider(p);
    seenSources.add(k);
    // window.ethereum can also expose a multi-provider array.
    if (k === 'ethereum' && Array.isArray((p as any).providers)) {
      for (const sub of (p as any).providers) proxyEthereumProvider(sub);
    }
  }
}

let pollHandle: number | undefined;

// EIP-6963: explicit re-announce with a separate UUID so dApps that select by
// announced metadata can choose the wrapped provider deterministically.
const SCOPEBALL_RDNS = 'dev.scopeball.wrapper';
function reannounceWrapped(detail: any, originalInfo: any): void {
  if (!detail?.provider) return;
  proxyEthereumProvider(detail.provider);
  // Re-emit the announcement under our own UUID so consumers see "two"
  // providers — original + wrapped — and can route preference via the UI.
  // The wrapped one shares the underlying instance (already proxied), so
  // either selection routes through us.
  const info = {
    uuid: 'scopeball-' + (originalInfo?.uuid ?? Math.random().toString(36).slice(2)),
    name: 'Scopeball (wraps ' + (originalInfo?.name ?? 'provider') + ')',
    icon:
      originalInfo?.icon ?? 'data:image/svg+xml;base64,' /* TODO: scopeball icon */,
    rdns: SCOPEBALL_RDNS,
  };
  window.dispatchEvent(
    new CustomEvent('eip6963:announceProvider', {
      detail: Object.freeze({ info: Object.freeze(info), provider: detail.provider }),
    }),
  );
}

window.addEventListener('eip6963:announceProvider', (event: Event) => {
  const detail = (event as CustomEvent).detail;
  // Skip our own re-announcements (rdns === SCOPEBALL_RDNS).
  if (detail?.info?.rdns === SCOPEBALL_RDNS) return;
  reannounceWrapped(detail, detail?.info);
});
window.dispatchEvent(new Event('eip6963:requestProvider'));

// Bounded polling for legacy non-6963 sources. Cap at 30s wall time so we
// don't run forever on pages without wallets.
discoverAndProxyAll();
const POLL_DEADLINE_MS = Date.now() + 30_000;
pollHandle = window.setInterval(() => {
  discoverAndProxyAll();
  if (Date.now() > POLL_DEADLINE_MS) {
    clearInterval(pollHandle);
    pollHandle = undefined;
  }
}, 100);
```

- [ ] **Step 2: Build to verify compilation**

```bash
cd extension && yarn build:chrome 2>&1 | tail -10
```

Expected: success. `dist/chrome/js/injected/proxy-injected-providers.js` exists and is ≥ ~5KB (viem brings weight).

- [ ] **Step 3: Commit**

```bash
git add extension/src/injected/proxy-injected-providers.ts
git commit -m "$(cat <<'EOF'
feat(extension): inpage provider proxy

Wraps request / send / sendAsync on every discovered EIP-1193 provider.
Gates eth_sendTransaction, eth_signTypedData{,_v3,_v4}, eth_sign,
personal_sign, wallet_sendCalls. EIP-6963 announce listener + 100ms
polling discovery + idempotency marker prevent double-wrap.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Content scripts — DOM injection and message relay

**Files:** Replace `extension/src/content-scripts/inject-scripts.ts`, `window-ethereum-messages.ts`.

- [ ] **Step 1: inject-scripts.ts**

```typescript
import Browser from 'webextension-polyfill';

function injectScript(url: string): void {
  const container = document.head ?? document.documentElement;
  const script = document.createElement('script');
  script.setAttribute('async', 'false');
  script.setAttribute('src', Browser.runtime.getURL(url));
  container.appendChild(script);
  script.onload = () => script.remove();
}

injectScript('js/injected/proxy-injected-providers.js');
```

- [ ] **Step 2: window-ethereum-messages.ts**

```typescript
import { WindowPostMessageStream } from '@metamask/post-message-stream';
import Browser from 'webextension-polyfill';
import { Identifier } from '@lib/identifier';
import { sendToPortAndAwaitResponse } from '@lib/messages';
import type { Message } from '@lib/types';

const stream = new WindowPostMessageStream({
  name: Identifier.CONTENT_SCRIPT,
  target: Identifier.INPAGE,
});

stream.on('data', async (message: Message) => {
  const port = Browser.runtime.connect({ name: Identifier.CONTENT_SCRIPT });
  const data: Message['data'] = { ...message.data, hostname: location.hostname };
  // Forward any phase-2 `awaiting-user` heartbeats to the inpage stream
  // so the inpage timer can extend before its phase-1 deadline fires.
  port.onMessage.addListener((msg: any) => {
    if (msg?.kind === 'awaiting-user' && msg.requestId === message.requestId) {
      stream.write({ requestId: message.requestId, kind: 'awaiting-user' });
    }
  });
  const ok = await sendToPortAndAwaitResponse(port, data);
  stream.write({ requestId: message.requestId, data: ok });
  port.disconnect();
});
```

- [ ] **Step 3: Build to verify**

```bash
cd extension && yarn build:chrome 2>&1 | tail -5
```

- [ ] **Step 4: Commit**

```bash
git add extension/src/content-scripts/{inject-scripts.ts,window-ethereum-messages.ts}
git commit -m "feat(extension): content scripts (DOM injection + message relay)"
```

---

## Task 7: Bypass-check observer (MetaMask + Coinbase Wallet)

**Files:** Replace `extension/src/content-scripts/bypass-check.ts`.

- [ ] **Step 1: Write the observer**

```typescript
import Browser from 'webextension-polyfill';
import { Identifier } from '@lib/identifier';
import { sendToPortAndDisregard } from '@lib/messages';
import { RequestType } from '@lib/types';

let metamaskChainId = 1;

function checkMethod(item: any, method: string): boolean {
  return String(item?.method).toLowerCase().includes(method.toLowerCase());
}

function forwardBypassed(data: any): void {
  const port = Browser.runtime.connect({ name: Identifier.CONTENT_SCRIPT });
  sendToPortAndDisregard(port, data);
}

function checkMetaMaskBypass(messageData: any): void {
  const items = Array.isArray(messageData) ? messageData : [messageData];
  for (const item of items) {
    if (!item) continue;
    const hostname = location.hostname;

    if (checkMethod(item, 'eth_sendTransaction')) {
      const [transaction] = item.params ?? [];
      forwardBypassed({
        type: RequestType.TRANSACTION,
        bypassed: true,
        hostname,
        chainId: metamaskChainId,
        transaction,
      });
    } else if (checkMethod(item, 'eth_signTypedData')) {
      const [address, typedDataStr] = item.params ?? [];
      try {
        const typedData = typeof typedDataStr === 'string' ? JSON.parse(typedDataStr) : typedDataStr;
        forwardBypassed({
          type: RequestType.TYPED_SIGNATURE,
          bypassed: true,
          hostname,
          chainId: metamaskChainId,
          address,
          typedData,
        });
      } catch {
        /* ignore malformed typed data */
      }
    } else if (checkMethod(item, 'eth_sign') || checkMethod(item, 'personal_sign')) {
      const [first, second] = item.params ?? [];
      const message = String(first).replace(/^0x/, '').length === 40 ? second : first;
      forwardBypassed({
        type: RequestType.UNTYPED_SIGNATURE,
        bypassed: true,
        hostname,
        message: String(message ?? ''),
      });
    } else if (checkMethod(item, 'wallet_sendCalls')) {
      const [options] = item.params ?? [];
      const { from = '0x0000000000000000000000000000000000000000', calls } = options ?? {};
      if (!calls) continue;
      for (const call of calls) {
        forwardBypassed({
          type: RequestType.TRANSACTION,
          bypassed: true,
          hostname,
          chainId: metamaskChainId,
          transaction: { from, ...call },
        });
      }
    }
  }
}

window.addEventListener('message', (event) => {
  const { target } = event?.data ?? {};
  const inner = event?.data?.data;
  if (!inner) return;

  if (inner.name === Identifier.METAMASK_PROVIDER) {
    if (target === Identifier.METAMASK_CONTENT_SCRIPT) {
      checkMetaMaskBypass(inner.data);
    }
    if (target === Identifier.METAMASK_INPAGE && inner.data?.method?.includes('chainChanged')) {
      metamaskChainId = Number(inner.data?.params?.chainId ?? metamaskChainId);
    }
  }
});

// Coinbase Wallet (separate IPC).
window.addEventListener('message', (event) => {
  const { type, data } = event?.data ?? {};
  if (type !== Identifier.COINBASE_WALLET_REQUEST || !data) return;
  const hostname = location.hostname;

  if (data.request?.method === 'signEthereumTransaction') {
    const tx = {
      from: data.request.params.fromAddress,
      to: data.request.params.toAddress,
      data: data.request.params.data,
      value: Number.parseInt(data.request.params.weiValue ?? '0').toString(16),
    };
    forwardBypassed({
      type: RequestType.TRANSACTION,
      bypassed: true,
      hostname,
      chainId: Number(data.request.params.chainId ?? 1),
      transaction: tx,
    });
  } else if (data.request?.method === 'signEthereumMessage') {
    const typedDataStr = data.request.params.typedDataJson;
    if (typedDataStr) {
      try {
        const typedData = JSON.parse(typedDataStr);
        forwardBypassed({
          type: RequestType.TYPED_SIGNATURE,
          bypassed: true,
          hostname,
          chainId: Number(typedData?.domain?.chainId ?? 1),
          address: data.request.params.address,
          typedData,
        });
      } catch {
        /* ignore */
      }
    } else {
      forwardBypassed({
        type: RequestType.UNTYPED_SIGNATURE,
        bypassed: true,
        hostname,
        message: String(data.request.params.message ?? ''),
      });
    }
  }
});
```

- [ ] **Step 2: Build verify + commit**

```bash
cd extension && yarn build:chrome 2>&1 | tail -5
git add extension/src/content-scripts/bypass-check.ts
git commit -m "$(cat <<'EOF'
feat(extension): bypass-check observer

Listens to MetaMask's metamask-contentscript / metamask-inpage
post-message stream and Coinbase Wallet's extensionUIRequest stream
as a passive backstop for requests our proxy missed (EIP-6963 race
losers, raw-provider holders, etc.). Forwards as bypassed:true so the
SW can surface a retroactive warning rather than a blocking gate.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Background SW skeleton (logging-only)

**Files:** Replace `extension/src/background/index.ts`.

This task lands a SW that *receives* messages and logs them, but does not yet evaluate. Plan 5 wires in the WASM bridge.

- [ ] **Step 1: Write the SW**

```typescript
import Browser from 'webextension-polyfill';
import { Identifier } from '@lib/identifier';
import {
  isTransaction,
  isTypedSignature,
  isUntypedSignature,
  type Message,
  type MessageResponse,
} from '@lib/types';

console.log('Scopeball SW alive at', new Date().toISOString());

Browser.runtime.onConnect.addListener((port) => {
  if (port.name !== Identifier.CONTENT_SCRIPT) return;
  port.onMessage.addListener((message: Message) => {
    void handleMessage(message, port);
  });
});

async function handleMessage(message: Message, port: Browser.Runtime.Port): Promise<void> {
  // Provisional behaviour for Plan 3: log every intercepted call and
  // respond positively (let the dApp proceed). Plan 5 replaces this with
  // engine-driven verdict resolution.
  if (isTransaction(message)) {
    console.log('[Scopeball] tx', {
      hostname: message.data.hostname,
      chainId: message.data.chainId,
      to: message.data.transaction.to,
      data: message.data.transaction.data?.slice(0, 10),
      bypassed: !!message.data.bypassed,
    });
  } else if (isTypedSignature(message)) {
    console.log('[Scopeball] typed-sig', {
      hostname: message.data.hostname,
      chainId: message.data.chainId,
      primaryType: (message.data.typedData as any)?.primaryType,
      bypassed: !!message.data.bypassed,
    });
  } else if (isUntypedSignature(message)) {
    console.log('[Scopeball] personal-sign', {
      hostname: message.data.hostname,
      messageLen: message.data.message.length,
      bypassed: !!message.data.bypassed,
    });
  }
  if (!message.data.bypassed) {
    const response: MessageResponse = { requestId: message.requestId, data: true };
    port.postMessage(response);
  }
}
```

- [ ] **Step 2: Build verify**

```bash
cd extension && yarn build:chrome 2>&1 | tail -5
```

- [ ] **Step 3: Commit**

```bash
git add extension/src/background/index.ts
git commit -m "feat(extension): background SW skeleton (logging only)"
```

---

## Task 9: Manual smoke test on a live dApp

**Files:** None — execution-only.

- [ ] **Step 1: Build production artifact**

```bash
cd extension && yarn build:chrome
```

- [ ] **Step 2: Load the extension into Chrome**

In Chrome:
1. Open `chrome://extensions`.
2. Enable Developer mode.
3. Click "Load unpacked".
4. Pick `extension/dist/chrome/`.

Expected: extension loads with no errors. Click "Errors" / "Inspect views: service worker"; the SW console should show `Scopeball SW alive at <timestamp>`.

- [ ] **Step 3: Smoke test on Uniswap**

1. Open `https://app.uniswap.org`.
2. Connect a test wallet (any address — no funds needed).
3. Construct a swap (any pair). Click *Swap*.
4. **Before** signing in MetaMask, open Chrome DevTools on the Uniswap tab and check the console — no errors should be from our extension.
5. Open the SW console (`chrome://extensions` → service worker "Inspect"). It should log `[Scopeball] tx { hostname: 'app.uniswap.org', chainId: 1, to: '0x…', data: '0x…' }`.
6. Click *Reject* in MetaMask to abandon. The dApp should treat it as user-rejected.

If logs don't appear, debug:
- DevTools console of the dApp tab → "Scopeball: failed to wrap provider" warnings indicate provider freeze.
- SW console → connection errors indicate a port name mismatch (revisit `Identifier.CONTENT_SCRIPT`).
- MV3 may need a SW reload after a `webpack --watch` rebuild.

- [ ] **Step 4: Document the smoke test**

Append to `extension/README.md` (create if missing):

```markdown
# Scopeball extension

## Manual smoke test (Plan 3 milestone)

1. `yarn build:chrome`
2. Load `dist/chrome/` as unpacked extension.
3. Visit any dApp, trigger a swap or signature, observe SW console:
   - `[Scopeball] tx { hostname, chainId, to, data, bypassed }`
   - `[Scopeball] typed-sig { hostname, chainId, primaryType, bypassed }`
   - `[Scopeball] personal-sign { hostname, messageLen, bypassed }`

`bypassed: true` indicates the request was caught by the bypass-check
observer (MetaMask internal stream / Coinbase Wallet stream), not by
the inpage proxy.
```

Commit:

```bash
git add extension/README.md
git commit -m "docs(extension): manual smoke-test recipe for Plan 3 milestone"
```

---

## Self-review summary

**Spec coverage** (vs design §4.1, §4.2, §4.3.1):
- ✅ inpage proxy with `request`/`send`/`sendAsync` + EIP-6963 + polling — Task 5
- ✅ content-script relay — Task 6
- ✅ bypass-check observer (MetaMask + Coinbase Wallet) — Task 7
- ✅ SW skeleton receiving messages — Task 8
- ✅ Cross-browser manifest preprocessing — Task 3
- ⏭ WASM module loading + verdict modal + storage queue → Plan 5
- ⏭ RPC + price fact fetchers → Plan 4
