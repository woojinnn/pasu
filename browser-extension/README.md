# Pasu extension

Chrome MV3 / Firefox extension that intercepts wallet transactions and
signatures, evaluates them against the policy engine (in-SW WASM Cedar + the
remote policy-server), and surfaces warnings / confirmations. It ships a service
worker (evaluation), content scripts, a popup (policy catalog + sign-in), a
transaction-confirm page, and an **options page that hosts the full dashboard**.

## Build & run

The loadable extension is produced by **two** pipelines into `dist/<browser>/`:

- **webpack** — service worker, content scripts, popup, confirm page.
- **Vite (dashboard)** — the options page (`options.html`) + assets, emitted
  into the *same* dir (`emptyOutDir: false`). The same dashboard bundle also
  runs standalone at `http://127.0.0.1:5173`.

### Modes

|                | dev                                            | prod (release)                              |
| -------------- | ---------------------------------------------- | ------------------------------------------- |
| command        | `yarn dev:chrome` (webpack `--watch`)          | `yarn build:ext`                            |
| webpack config | `webpack.dev.js` (unminified, sourcemaps)      | `webpack.prod.js` (minified)                |
| dashboard      | separate: `cd dashboard && yarn dev` (:5173)   | built into `dist/chrome` by `build:ext`     |
| registry guard | none                                           | requires an **https** `REGISTRY_BASE_URL`   |

> `dev:chrome` builds only the webpack half. For a *loadable* dev build that
> also has the options page, build the dashboard too — otherwise the manifest's
> `options_page` is missing and Chrome rejects the unpacked extension.

### Configuration (build-time env vars)

Passed on the command line, or via a mode-specific gitignored env file:

| var                                  | what                                                                                                | default                  |
| ------------------------------------ | --------------------------------------------------------------------------------------------------- | ------------------------ |
| `PASU_SERVER_URL`                    | policy-server the SW + dashboard call (eval / auth / wallets)                                        | `http://127.0.0.1:8788`  |
| `REGISTRY_BASE_URL`                  | policy / token / adapter registry. **Required (https) for a prod build** — the guard fails otherwise | `http://localhost:8000`  |
| `PASU_ALLOW_INSECURE_REGISTRY=1`     | bypass the prod registry guard (local smoke test only)                                              | —                        |

The server URL can also be switched at **runtime** from the dashboard's
**Settings** page (writes `localStorage` + `chrome.storage`) — no rebuild.

Env-file convention:

- production builds read `browser-extension/.env`
- development builds read `browser-extension/.env.development`
- command-line env vars still win in both modes

This keeps a local production `.env` from accidentally pointing `yarn
dev:chrome` or `cd dashboard && yarn dev` at prod. If you want dev to target a
non-local server, export `PASU_SERVER_URL=...` for that command or put it in
`.env.development`.

### Production build (full, loadable)

```bash
PASU_SERVER_URL=https://<your-server-host> \
REGISTRY_BASE_URL=https://<your-registry-host>/ \
yarn build:ext            # = build:chrome (webpack) → build:options (dashboard)
```

`build:ext` runs webpack first (it `clean`s `dist/chrome`), then the dashboard
Vite build adds `options.html` — the order matters. (`yarn build` = chrome +
firefox webpack only, no dashboard; `build:ext` is the chrome loadable one.)

### Dev build (full, loadable, unminified)

```bash
yarn prepare:defaults && yarn prepare:wasm
# webpack.dev.js sets `watch: true`, so --no-watch forces a one-shot build
# (otherwise webpack never exits and the dashboard step below never runs).
TARGET_BROWSER=chrome yarn webpack --config webpack/webpack.dev.js --no-watch
yarn workspace pasu-dashboard exec vite build --mode development
```

That dev build targets `http://127.0.0.1:8788` by default. To point it
elsewhere for a specific run, prefix both build commands with
`PASU_SERVER_URL=https://<your-server-host>`, or write that value to
`.env.development`.

Or, for live iteration, run the two halves separately: `yarn dev:chrome`
(webpack watch) **and** `cd dashboard && yarn dev` (dashboard at `:5173`, reached
via the `dashboard-bridge` content script).

### Develop against a local server

When you're changing **server** code too, run the policy-server locally and
point the extension at it. Both paths below expose it on
`http://127.0.0.1:8788` — which is already the extension's default, so a plain
dev build targets it with no `PASU_SERVER_URL` at all.

- **Quick — `cargo run`:** copy the server's `.env.local.example` → `.env.local`
  (set `DATABASE_URL`, `REDIS_URL`), then `scripts/start-policy-server.sh local`.
- **Prod-like — minikube:** `minikube start --driver=docker` →
  `kubectl config use-context minikube` →
  `scripts/policy-server-local-k8s.sh up`. This builds the server image inside
  minikube's Docker daemon, applies Postgres/Redis, creates the Secret, installs
  the Helm chart with local values, port-forwards the API to
  `127.0.0.1:8788`, and checks readiness. Use minikube for this local k8s loop;
  Docker Desktop Kubernetes can use a separate containerd image store and is not
  the documented self-service path.

Sanity check: `curl http://127.0.0.1:8788/readyz` → `200`.

Then connect the extension, either way:

- **Build-time** — default already targets local, so `yarn dev:chrome` (no env)
  hits `127.0.0.1:8788`. To be explicit: `PASU_SERVER_URL=http://127.0.0.1:8788`.
- **Runtime (no rebuild)** — dashboard → **Settings** → **로컬 (테스트)** preset
  (`http://127.0.0.1:8788`) → Save. The SW applies it immediately; the dashboard
  on next reload. Handy for flipping a prod build to your local server.

#### Minimal local stack (quickstart)

```bash
# 1. infra — Postgres (:5544, user/pass pasu) + Redis (:6379)
docker run -d --name pasu-pg -e POSTGRES_USER=pasu -e POSTGRES_PASSWORD=pasu \
  -e POSTGRES_DB=pasu -p 5544:5432 postgres:16
docker run -d --name pasu-redis -p 6379:6379 redis:7

# 2. server env (repo root, gitignored). Copy the example, then set at least:
cp .env.local.example .env.local
#   DATABASE_URL=postgres://pasu:pasu@127.0.0.1:5544/pasu
#   REDIS_URL=redis://127.0.0.1:6379
#   JWT_SECRET=$(openssl rand -hex 32)
#   RUN_MIGRATIONS_ON_STARTUP=true     # migrate the fresh DB on boot
#   REQUIRE_SYNC_CONFIG=false          # skip the sync-worker config for dev
#   OAUTH_ALLOWED_REDIRECT_URIS=https://<ext-id>.chromiumapp.org/   # in-ext login

# 3. run — migrates the DB, listens on 127.0.0.1:8788 (first compile ~1m)
scripts/start-policy-server.sh local
curl http://127.0.0.1:8788/readyz      # 200 once DB + Redis are connected
```

**Local auth without a Google client.** Placeholder `GOOGLE_*` can't complete a
real sign-in, and a prod token won't validate (different `JWT_SECRET`). Seed a
user and mint a dev token signed with the local secret, then inject it:

```bash
set -a; source .env.local; set +a       # exposes JWT_SECRET to the shell
docker exec pasu-pg psql -U pasu -d pasu -c \
  "INSERT INTO users (user_id,email,provider,created_at,last_login_at) VALUES \
   ('u_localdev01','dev@local.test','local',extract(epoch from now())::int,extract(epoch from now())::int) \
   ON CONFLICT DO NOTHING"
node -e 'const c=require("crypto"),s=process.env.JWT_SECRET,b=o=>Buffer.from(JSON.stringify(o)).toString("base64url"),n=Math.floor(Date.now()/1e3),h=b({alg:"HS256",typ:"JWT"}),p=b({sub:"u_localdev01",email:"dev@local.test",typ:"access",iat:n,exp:n+86400});console.log(h+"."+p+"."+c.createHmac("sha256",s).update(h+"."+p).digest("base64url"))'
```

Inject the printed token in the SW console (`chrome://extensions` → the
extension's *service worker* → inspect):

```js
chrome.storage.local.set({ pasu_jwt: "<token>", pasu_jwt_refresh: "<token>" })
```

**Real OAuth locally (optional).** To use *Sign in with Google* against the
local server instead of injecting a token, put a Google OAuth client's
`GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET` in `.env.local` **and** add
`http://127.0.0.1:8788/auth/google/callback` to that client's *Authorized
redirect URIs* in the Google Console. Restart the server afterwards —
`/auth/google` reads the client at request time, but a running process keeps
the env it booted with, so it won't see a `.env.local` edit until you
re-run `scripts/start-policy-server.sh local`. The in-extension login also
relies on the `https://<ext-id>.chromiumapp.org/` entry already in
`OAUTH_ALLOWED_REDIRECT_URIS`.

### Load & use

1. `chrome://extensions` → enable **Developer mode** → **Load unpacked** →
   `browser-extension/dist/chrome`. The id is stable (manifest `key`).
2. **Popup** (extension icon): the policy catalog (browse / enable–disable) and
   **Sign in with Google**.
3. **Dashboard**: popup → **Open dashboard** (or chrome://extensions →
   *Extension options*) opens `chrome-extension://<id>/options.html`.
4. **Login** — either *Sign in with Google* (popup or dashboard) drives the SW's
   `chrome.identity.launchWebAuthFlow`, which asks the server to bounce the token
   to `https://<id>.chromiumapp.org/`. That exact URL must be in the server's
   `OAUTH_ALLOWED_REDIRECT_URIS` allowlist (policy-server Helm values). One
   sign-in authenticates both the SW (tx eval) and the options-page dashboard.

## Manual smoke test

1. `yarn build:chrome`
2. Load `dist/chrome/` as an unpacked extension.
3. Visit any dApp, trigger a swap or signature, and observe the service-worker console:
   - `[Pasu] tx { hostname, chainId, to, data, bypassed }`
   - `[Pasu] typed-sig { hostname, chainId, primaryType, bypassed }`
   - `[Pasu] personal-sign { hostname, messageLen, bypassed }`

`bypassed: true` indicates the request was caught by the bypass-check observer, not by the inpage proxy.
