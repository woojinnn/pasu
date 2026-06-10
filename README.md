# pasu

Wallet-side transaction & signature policy engine. **pasu** intercepts wallet
RPC in the browser, decodes each transaction or EIP-712 signature into a typed
semantic **action**, and evaluates that action against a Cedar policy set —
warning or blocking before the user signs.

The repo is a hybrid Rust + TypeScript monorepo:

- a **Rust/Cedar policy engine** compiled to WASM and embedded in the extension,
- a **browser extension** (Chrome MV3 / Firefox MV2) that hosts that WASM and a
  policy-management **dashboard**,
- a **stateful Rust backend** (`policy-server`) for auth, wallets, and predicted
  state deltas,
- a **registry** (`registryV2`) of adapter manifests + token metadata that drives
  decoding.

## Architecture

```text
dApp → window.ethereum  ──intercept──▶  extension service worker
                                          │
                                          ├─ decode calldata / typed-data  (registryV2 manifests)
                                          │       └─▶ ActionBody tree
                                          │
                                          ├─ lower ActionBody → Cedar request
                                          │       └─▶ in-SW WASM Cedar eval  → Verdict (Pass / Warn / Fail)
                                          │
                                          └─ (optional) policy-server  ── predicted StateDeltas, auth, wallets
```

1. The extension intercepts wallet transactions and EIP-712 / personal
   signatures.
2. Decoders (driven by `registryV2` manifests) turn raw calldata / typed-data
   into a typed **`ActionBody`** tree (see [Action model](#action-model)).
3. The Rust engine **lowers** that `ActionBody` to a Cedar request and evaluates
   it against the installed policies — all inside the service worker via WASM. No
   network round-trip is required for a verdict.
4. `policy-server` is the **stateful backend**: Google OAuth, wallet/account
   state, and an `/evaluate` endpoint that returns predicted `StateDelta`s from
   the asset-model reducer. The extension owns the final verdict; the server
   supplies state context.

## Naming guide (versioned names you will meet in the code)

The version suffixes are **protocol names, not "old vs new"** — they coexist on
purpose and renaming them would churn every call site. What each one means:

| Name | What it is |
|------|------------|
| `ActionBody` / **v3 decode** | The declarative calldata/typed-data decoder driven by `registryV2` manifests (`declarative_route_*_v3`). "v3" names the decode-registry format. |
| **v2 evaluation** (`evaluate_action_v2`, `manifest_v2`) | The stateless per-request Cedar evaluation: `{policy, manifest}` bundles + trigger matching + `policy_rpc` enrichment planning. "v2" names the policy-bundle contract. |
| **`ps2:*`** | The extension's policy-storage namespace (account ⊃ wallets ⊃ bindings/packages) and its service-worker message family. The dashboard/popup manage policies exclusively through it. |
| `verdictSource: "declarative-v2" \| "fail_closed"` | Where a verdict came from: a real policy evaluation, or the fail-closed tail (undecodable request, engine fault). `fail_closed` is a safety semantic, not a legacy marker. |
| `registryV2/` | The adapter-manifest registry that feeds the v3 decoder. The `registry-api/` proxy fronts its private deployment. |

If you add a new protocol revision, give it a name here and keep the old one
documented until it is actually deleted.

## Workspace layout

| Path | What |
|------|------|
| `browser-extension/` | Chrome MV3 / Firefox MV2 extension: service worker (eval), content scripts, popup, confirm page, and the options-page **dashboard**. See [its README](browser-extension/README.md). |
| `browser-extension/dashboard/` | Standalone Vite + React policy-management UI, also built into the extension's options page. See [its README](browser-extension/dashboard/README.md). |
| `browser-extension/sdk/` | The `@pasu/sdk` extension client the dashboard talks to. |
| `crates/policy-engine/` | Core runtime: the `ActionBody` → Cedar lowering, the Cedar `PolicyEngine` wrapper, and bundled schema composition. |
| `crates/policy-engine-wasm/` | `wasm-bindgen` bridge that exposes the engine (+ typed `.d.ts`) to the extension. |
| `crates/policy-server/` | Stateful backend. Sub-crates: `server/` (Axum HTTP), `db/`, `sync/`, and the `asset-model/{state,action,transition}` reducer. See [local deploy README](crates/policy-server/server/deploy/local/README.md). |
| `crates/adapters/` | Decode support: `abi-resolver` (Sourcify-backed ABI lookup) and `mappers`. |
| `crates/integration-tests/` | `ActionBody[]` decode harness — replays real calldata / typed-data through the production decoders. See [its README](crates/integration-tests/README.md). |
| `registryV2/` | Adapter manifests, token metadata, and the build script that emits the runtime decode `index/`. |
| `registry-api/` | Cloud Run reverse-proxy that fronts the private adapter registry. See [its README](registry-api/README.md). |
| `schema/policy-schema/` | Cedar schema: `core.cedarschema` + per-domain action schemas under `actions/`. |

The authoritative Rust crate list is the `[workspace] members` in
[`Cargo.toml`](Cargo.toml).

## Action model

The policy input is the v3 hierarchical **`ActionBody`** tree, defined in
`crates/policy-server/asset-model/action` (`policy-action`) and re-exported by
`crates/policy-engine` as `action::v3`. An `Action` is `{ meta, body }` where
`meta` carries submission info (`OnchainTx` vs `OffchainSig`) and `body` is one
domain variant:

```rust
// crates/policy-server/asset-model/action/src/lib.rs
pub enum ActionBody {
    Token(TokenAction),
    Amm(AmmAction),
    Lending(LendingAction),
    Airdrop(AirdropAction),
    Launchpad(LaunchpadAction),
    Perp(PerpAction),
    LiquidStaking(LiquidStakingAction),
    Permission(PermissionAction),
    Yield(YieldAction),
    Restaking(RestakingAction),
    Staking(StakingAction),
    Governance(GovernanceAction),
    HyperliquidCore(HyperliquidCoreAction),
    Marketplace(MarketplaceAction),
    Multicall { actions: Vec<ActionBody> },  // batched (e.g. Universal Router)
    Unknown { target, chain, calldata, value },  // unidentified — policy default: warn / deny
}
```

Each domain has a matching Cedar schema fragment under
`schema/policy-schema/actions/<domain>/`. `Multicall` recurses (a Universal
Router execution decodes into nested `ActionBody` entries), and `Unknown` is the
fail-closed branch for calls no adapter recognizes.

## Build & run

### Rust workspace

```bash
cargo test --workspace        # or: scripts/test-all.sh  (adds clippy/fmt + extension)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

`scripts/test-all.sh` runs the full sweep (cargo test + clippy + fmt, then the
extension typecheck / vitest / chrome build); `scripts/lint.sh` is the
fix-everything counterpart (`cargo fmt` + `clippy --fix` + `yarn lint`).

> The `crates/integration-tests` decode harness reads a generated index at
> `registryV2/index/` (a build artifact, not tracked). Regenerate it first:
> ```bash
> cd registryV2 && npm ci && npm run build
> ```

### WASM artifact

```bash
scripts/wasm-build.sh
```

Runs `wasm-pack build crates/policy-engine-wasm --target web --release` and
copies the artifact into `browser-extension/backend/wasm/` and
`browser-extension/public/wasm/`. The extension's `yarn prepare:wasm` step calls
this script, so a normal extension build picks it up automatically.

### Browser extension

A Yarn 4 workspace at `browser-extension/` (the dashboard is a nested
workspace). Loadable builds are produced by **two** pipelines into
`dist/<browser>/`: webpack (SW / content scripts / popup / confirm) and Vite
(the options-page dashboard).

```bash
cd browser-extension
yarn install
yarn build:ext         # chrome (webpack) + dashboard (vite) → dist/chrome/
# or, for live dev:
yarn dev:chrome        # webpack --watch; run `cd dashboard && yarn dev` alongside for the dashboard
```

Build-time config is via env vars — most importantly `PASU_SERVER_URL` (the
policy-server the extension calls; default `http://127.0.0.1:8788`) and
`REGISTRY_BASE_URL`. See [`browser-extension/README.md`](browser-extension/README.md)
for the full matrix, load instructions, and the local-server workflow.

### policy-server (local)

```bash
cp .env.local.example .env.local       # fill JWT_SECRET, GOOGLE_*, DATABASE_URL, …
scripts/start-policy-server.sh local   # = cargo run -p policy-server --bin policy-server
# or via the root package.json:
yarn server:local
curl http://127.0.0.1:8788/readyz
```

`start-policy-server.sh <profile>` loads `.env.<profile>` then runs the server;
`local` targets the dashboard at `http://127.0.0.1:5173`, `ext` targets the
in-extension OAuth flow. For a prod-like local Kubernetes loop see
[`crates/policy-server/server/deploy/local/README.md`](crates/policy-server/server/deploy/local/README.md)
(`scripts/policy-server-local-k8s.sh`).

## CI

[`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs four jobs on every
PR and push to `main`:

- **rust** — builds the `registryV2` index (npm), then `cargo fmt --check`,
  `cargo check`, `cargo clippy -D warnings`, `cargo test`, doctests, and
  `cargo doc` with `-D warnings`.
- **wasm** — `wasm-pack build crates/policy-engine-wasm`, native tests on the
  wasm crate, and headless-Chrome `wasm-bindgen` tests.
- **extension** — `yarn typecheck`, vitest, Chrome MV3 + Firefox MV2 builds, the
  dashboard build, and packaged zips. The Chrome/Firefox/dashboard artifacts are
  uploaded per-PR so reviewers can sideload without recompiling.
- **dependency-policy** — `cargo audit` + `cargo deny`.

To reproduce the suite locally, run `cargo test` (with the registry index built,
above) and `cd browser-extension && yarn test`.

## Policy RPC & remote facts

Cedar evaluates against calldata-derived primitives plus optional remote facts.
A policy bundle may ship a `.policy-rpc.json` manifest that declares which
backend method supplies a fact (e.g. a USD valuation), which params to send, and
which response field is projected into the Cedar context. The extension asks
WASM to *plan* those calls, sends them to `policy-server`, then asks WASM to
*materialize* the responses and evaluate Cedar. The engine itself performs no
direct oracle / balance / allowance lookups.

## License

MIT.
