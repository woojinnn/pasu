# ScopeBall Registry Architecture — Proxy + GCS + KMS (Signed Bundles)

> Production architecture for the decode-rule registry: how the browser extension
> fetches "v3 bundles" (calldata decode rules), and how **bundle signing** makes the
> registry a **trusted supply chain** — a compromised or MITM'd registry cannot inject
> a decode rule that silently flips a pre-sign verdict.
>
> Status: signing implemented + verified end-to-end (Phases 0–4). Enforcement is
> **staged** — `DAMBI_REQUIRE_BUNDLE_SIGNATURE` ships **off** until the prod registry
> is signed + the proxy `/signatures/` allowlist is deployed.

---

## 1. Why this exists

ScopeBall decodes a wallet's *pending* action into a normalized `ActionBody`, evaluates
Cedar policies, and returns **pass / warn / fail** *before the wallet signs*. The decode
rules are not shipped in the extension — they are **JIT-fetched** from a registry so new
protocols can be onboarded without an extension release.

That fetch is a **trust boundary**. The registry (GCS bucket behind a Cloud Run proxy)
is reachable over the network; a compromised bucket, a hijacked proxy, or a MITM could
serve a *malicious decode rule* that mis-decodes calldata → wrong verdict → the user
signs a drain. Before signing, the only integrity signal was a `bundle_sha256` field
**inside the bundle** (self-attested — worthless against an attacker who controls the
bundle). **Bundle signing closes this.**

---

## 2. Topology

```
┌─ BUILD + SIGN (CI, keyless Workload Identity) ─────────────────────────────┐
│  build-index.ts ─→ sign-bundles.ts ──(WIF)──→ Cloud KMS (EC private key, HSM)│
│       │ index/, bundles/,    │ signatures/<sha>.sig                          │
│       │ contexts/            │ (DER → P1363)                                 │
│       └─ parity gate ────────┘                                              │
└───────┬────────────────────────────────────────────────────────────────────┘
        │ gated promote (git tag / Environment approval)
        ▼
   GCS bucket  (UBLA + PAP, object versioning ON)         ◀── rollback = prior version
        │  index/  bundles/  contexts/  tokens/  signatures/  manifests/
        ▼
   Cloud Run proxy (registry-api)                          ── path allowlist · rate-limit
        │  • verbatim for concrete callkeys                   · 5-min cache · private bucket
        │  • re-materializes 3-ref (sourced) callkeys          · NO integrity guarantee
        │  anonymous fetch / bucket stays private
        ▼
   ┌──────────────────────────────────────────────────────┐
   │  Browser Extension (CWS/AMO code-signed)              │
   │   • PINNED public key (SPKI, build-time)              │  ← trust anchor lives HERE,
   │   • verify ECDSA P-256 sig over canonicalize(bundle)  │     outside the registry
   │   • verify FAILS → fail-closed (never installs)       │
   └──────────────────────────────────────────────────────┘
```

**Key idea:** the verification key is pinned **inside the extension** — outside the
registry and proxy. So even a total registry compromise cannot forge a bundle the
extension will accept.

Components:

| Layer | Code | Role |
|---|---|---|
| Build/sign | `registryV2/scripts/build-index.ts`, `sign-bundles.ts` | generate + sign the served objects |
| Bucket | GCS `gs://<BUCKET>` | stores `index/ bundles/ contexts/ tokens/ signatures/ manifests/` |
| Proxy | `registry-api/src/{server,validation,gcs-client,cache}.ts` | path-allowlist reverse-proxy, caches, re-materializes 3-ref |
| Extension | `browser-extension/backend/service-worker/adapter-loader/{declarative-adapter-loader,bundle-verify}.ts` | fetch → **verify** → install decoder |
| KMS | Cloud KMS asymmetric `EC_SIGN_P256_SHA256` | holds the signing private key (HSM) |

---

## 3. The signing contract

### What is signed
The signed message is **`canonicalize(bundle)`** (RFC 8785 JCS) — byte-for-byte the
**preimage of `bundle_sha256`** that `build-index.ts` already computes
(`bundle_sha256 = "0x" + sha256(canonicalize(resolved))`).

This choice is deliberate: the proxy serves a *concrete* bundle **verbatim** but
**re-serializes** a *sourced (3-ref)* bundle after re-materializing it at request time
(`JSON.stringify(response, null, 2) + "\n"`). The raw wire bytes therefore differ by
path — but **`canonicalize(bundle)` is identical** in both, because canonicalization
normalizes ordering/whitespace. Signing the canonical form is robust to the proxy's
serialization.

> **Parity invariant (Phase-0 gate).** Concrete-verbatim and 3-ref-materialized bundles
> must both hash (via the extension's `canonicalize@3`) to the `bundle_sha256` the build
> stamped (with `canonicalize@2/3`). `registry-api/src/__tests__/materialization-parity.test.ts`
> proves this for the **entire corpus** (every `by-callkey/by-typed-data/by-selector`
> entry). It runs in CI **before** signing — drift would publish signatures the extension
> cannot verify.

### Algorithm
**ECDSA P-256 + SHA-256** (KMS `EC_SIGN_P256_SHA256`; WebCrypto `{name:"ECDSA",hash:"SHA-256"}`).
KMS signs the **digest** (= `bundle_sha256` bytes) and returns **DER**; `sign-bundles.ts`
converts DER → **P1363** (raw `r‖s`, 64 bytes) via `@noble/curves`
(`p256.Signature.fromBytes(der,"der").toBytes("compact")`) because WebCrypto verify
requires P1363, not DER.

### Sidecar format
One detached sidecar per **unique** bundle, content-addressed:

```
signatures/<bundle_sha256>.sig
{ "alg": "ECDSA_P256_SHA256", "key_id": "<label>", "sig_b64": "<base64 P1363 r‖s>" }
```

`alg`/`key_id` are **telemetry only** — the extension hard-codes the algorithm and the
pinned key (§5 N2). One `.sig` is shared by every callkey that resolves to that bundle
(dedup by sha).

---

## 4. End-to-end flow

```
BUILD     build-index.ts  →  index/ + bundles/ + contexts/   (+ bundle_sha256 per entry)
GATE      parity test     →  ∀ served bundle: sha256(canonicalize(bundle)) == bundle_sha256
SIGN      sign-bundles.ts →  ∀ unique sha: KMS.asymmetricSign(digest) → DER→P1363
                            →  signatures/<sha>.sig
PUBLISH   gcloud storage rsync  bundles → contexts → tokens → signatures → manifests → index
                            (leaves incl. signatures BEFORE the index pointer; N3)
SERVE     proxy: GET /index/by-callkey/<key>.json → {…, bundle, bundle_sha256}
          extension also: GET /signatures/<localSha>.sig
VERIFY    extension (adapter-loader, BEFORE install):
            localSha = "0x"+sha256(canonicalize(parsedResponse.bundle))   # raw response bundle, N5
            assert  localSha == response.bundle_sha256                     # defense-in-depth
            sig     = fetch /signatures/<localSha>.sig                     # by RECOMPUTED hash, N4
            ok      = subtle.verify({ECDSA,SHA-256}, PINNED_KEY, sig, canonicalize(bundle))   # N2
            ok ? install : fail-closed
```

The bundle the extension verifies is **`parsedResponse.bundle`** (the raw response
object), **not** `parseBundleV3`'s output — the parser reconstructs a field-subset whose
canonical form would not match the signed preimage (N5).

---

## 5. Security invariants (the holes a naive design leaves)

| # | Invariant | Why |
|---|---|---|
| **N2** | Extension HARD-CODES the algorithm + pinned key; `.sig.alg`/`key_id` are ignored for selection | else a hostile registry sets `alg:"none"` to downgrade |
| **N4** | Sig is fetched by the **recomputed** `localSha`, never the response's self-asserted `bundle_sha256` | the claim is attacker-controlled; binds sig to the bytes actually parsed |
| **N5** | Hash the **raw** `parsedResponse.bundle`, not `parseBundleV3` output | the parser drops fields → different canonical → every sig would fail |
| **N3** | Publish uploads `signatures/` **before** `index/` (and prunes it after) | else a new index points at a bundle whose sig 404s during the publish window |
| **Verify-before-install** | The gate precedes `declarativeInstallV3` (and persistence) | a malicious bundle never reaches the WASM engine; only verified bundles are cached |

### Fail-direction (per install site)
A verification failure degrades to the same **warn-closed** outcome as a decode miss — it
never hard-denies a benign action — but the *mechanism* differs by call site (each
matches that function's existing fault contract):

| Flow / site | On verify fail | Net verdict |
|---|---|---|
| on-chain tx (`installDeclarativeBundleV3`) | `throw InstallDeclarativeV3Error("verify")` | warn-closed (fault → warn) |
| typed-sig (`…ByTypedData`) | `return { ok:false, reason:"verify_failed" }` | warn-closed (router null fall-through) |
| NFT selector (`…BySelector`) | `return null` | warn-closed (miss) |
| HyperLiquid venue | — | **unaffected** (HL is not a registry v3 bundle) |

When `DAMBI_REQUIRE_BUNDLE_SIGNATURE` is **off**, a missing/invalid sig is a **no-op**
(install proceeds) so an unsigned dev/staging registry keeps working.

---

## 6. Key management (Cloud KMS)

- **Private key** is created with `--purpose=asymmetric-signing --default-algorithm=ec-sign-p256-sha256`
  (HSM protection level recommended) and **never leaves the HSM**. CI signs by calling
  `asymmetricSign` over WIF — no SA key file on any runner.
- **IAM**: the signer SA gets `roles/cloudkms.signerVerifier` only (useToSign + getPublicKey;
  **not** export). Provisioned by `registryV2/scripts/deploy/provision-infra.sh`.
- **Pinned public key** (SPKI base64) is baked into the extension via webpack DefinePlugin
  (`PINNED_BUNDLE_PUBLIC_KEY`), **channel-specific** (dev vs prod) so a dev build verifying
  a dev-signed local registry never collides with prod.
  - prod: `gcloud kms keys versions get-public-key … --output-file=-` → strip PEM → base64.
  - dev:  `registryV2 npm run gen-signing-key` prints it (private key stays gitignored).
- **Rotation (v1):** create a new key version, re-sign (`sign-bundles.ts --force`), update the
  pinned key, **ship an extension release**. (Pinning couples key rotation to a store release —
  acceptable at v1; a TUF-style root→signing delegation that rotates without a redeploy is a
  documented future option.)

---

## 7. Deploy & operations

### Two deploy planes (publish ≠ deploy)
- **`publish-index.sh`** (DATA): build → parity → **sign** → `gcloud storage rsync`. Touches
  only bucket objects; the proxy reads them live after its 5-min cache TTL. **Frequent.**
- **`deploy-proxy.sh`** (CODE): rebuild + roll the Cloud Run proxy. Needed when the proxy
  *code* changes — e.g. the new `/signatures/` path allowlist. **Rare.**

### Publish modes (orthogonal to the trigger)
| Mode | What | How |
|---|---|---|
| incremental | upload changed/new objects only | `publish-index.sh` (default rsync) |
| full-sync | also delete orphaned objects | `PRUNE=1 publish-index.sh` (rsync `--delete-unmatched-destination-objects`) |

The **trigger** is separate: `registry-publish.yml` fires incremental on a `registry-v*`
tag, and full-sync only via `workflow_dispatch (mode=full-sync)` behind the `production`
Environment approval gate (full-sync deletes, so it is human-initiated, never automatic).

### CI gates
- `ci.yml` → `registry-api` job: build index → **parity gate** + proxy validation/serving
  tests + sign-bundles test + typecheck (every PR).
- `registry-publish.yml`: parity gate runs **before** signing on every publish.

### Rollback
Object **versioning** is enabled on the bucket. A bad publish rolls back by restoring the
prior object versions (or re-publishing a prior `registry-v*` tag), live after the 5-min
proxy cache TTL — **no extension change**.

### Staged rollout (cutover order — do NOT skip)
1. Sign the prod registry + publish `signatures/` (`registry-publish.yml`).
2. Deploy the proxy `/signatures/` allowlist (`deploy-proxy.sh`).
3. Confirm live `.sig` coverage across the corpus.
4. **Only then** set `DAMBI_REQUIRE_BUNDLE_SIGNATURE=true` (with the prod pinned key) and ship
   an extension release.

Flipping the flag before steps 1–3 would 404 every signature → fail-closed → the extension
decodes nothing.

---

## 8. Residual risks (honest limits)

- **Rollback-to-genuine (N1):** a compromised proxy can serve a *genuine but OLD* signed
  bundle + its genuine sig (a downgrade to a known-weaker decode rule). Signing proves
  authenticity, not freshness. Object versioning aids forensics but does not stop serve-time
  rollback. A future signed-manifest-with-version-pin (TUF timestamp/snapshot) addresses it.
- **Local chrome.storage tampering (R3):** the verify gate runs before install/persist, so
  only verified bundles are ever cached; cold-start rehydrate trusts them without a re-fetch.
  Defeating this requires DIRECT local storage write access — a strictly stronger attacker
  than the registry-MITM this guards — out of scope for v1.
- **canonicalize parity:** build (`canonicalize@3`) and extension (`canonicalize@3`) must agree
  byte-for-byte. The Phase-0 corpus gate is the permanent regression guard; both packages are
  aligned to the same major.
- **GCP provisioning** (project / KMS key / WIF / CWS account) is operator runbook
  (`provision-infra.sh` + this doc), not automated — by design.

---

## 9. File reference

| Concern | Path |
|---|---|
| sign step | `registryV2/scripts/sign-bundles.ts` · `gen-signing-key.ts` |
| parity gate | `registry-api/src/__tests__/materialization-parity.test.ts` |
| proxy serving | `registry-api/src/validation.ts` (`/signatures/` branch) · `server.ts` (dispatch) |
| extension verify | `…/adapter-loader/bundle-verify.ts` · `declarative-adapter-loader.ts` (3 sites) |
| build-time pin/flag | `browser-extension/webpack/{env,webpack.common,webpack.prod}.js` · `.env(.example)` |
| infra | `registryV2/scripts/deploy/{_common,provision-infra,publish-index,deploy-proxy}.sh` |
| CI | `.github/workflows/{ci,registry-publish}.yml` |

---

## 10. Prod project migration (D1 — dedicated prod, dual-run)

The live registry originally ran on a **shared/polluted project** (`scopeball-registry-poc-g`),
co-tenant with unrelated workloads (zkvote-staging, Cloud SQL, Redis, GKE) and broad `editor`
bindings — a poor home for a supply-chain signing trust-root. D1 moves prod to a **dedicated,
minimal-IAM project** and keeps the PoC project running (**dual-run, no teardown**) so already-
installed extensions pointing at the PoC URL never break.

### Two-project topology (live)

| | **Prod (canonical)** | **Legacy PoC (kept, dual-run)** |
|---|---|---|
| Project ID | `dambi-registry` (org `502922039207`) | `scopeball-registry-poc-g` (no org parent) |
| Project # | `1912792298` | `891268973493` |
| Bucket | `gs://dambi-registry-v3-seoul` | `gs://scopeball-registry-v3-seoul` |
| KMS key | `registry-signing/bundle-sign-p256` — **HSM** | same names — **SOFTWARE** |
| Proxy (Cloud Run) | `registry-api-v3` → `https://registry-api-v3-65uggwflcq-du.a.run.app` (stable; custom domain declined) | `registry-api-v3-891268973493.…run.app` |
| Runtime SA | `registry-api-v3-sa@dambi-registry` (objectViewer only) | (shared project SAs) |
| Signer SA (CI/WIF) | `registry-signer@dambi-registry` (signerVerifier + objectAdmin) | — |
| WIF | pool `github-pool` / provider `github-provider` (repo-pinned `woojinnn/scopeball`) | none |
| gcloud config | `dambi` | `scopeball` |

Bucket settings are identical across both (asia-northeast3 · versioning · PAP enforced · UBLA).
The prod **pinned public key differs** from PoC (new HSM key) — see channel pin in §6.

### Provisioned (idempotent gcloud / scripts)
1. `gcloud projects create dambi-registry --organization=502922039207` + billing link + enable
   APIs (cloudkms, run, storage, artifactregistry, iamcredentials, sts, cloudbuild).
2. Prod bucket (versioning + PAP + UBLA), data copied **bucket-to-bucket** from PoC
   (`gcloud storage rsync`) — object-count parity verified (62 534 = 62 534).
3. KMS **HSM** keyring/key (`EC_SIGN_P256_SHA256`); public key extracted → prod channel pin.
4. Runtime SA + `objectViewer`; AR repo `dambi`; **`PROJECT_ID=dambi-registry deploy-proxy.sh`**
   (proxy serving verified: proxy-fetch sha == bucket sha, CORS, 404 on bad path).
5. Signer SA + KMS `signerVerifier` + bucket `objectAdmin`; WIF pool/provider **pinned to
   `woojinnn/scopeball`**; `workloadIdentityUser` binding.
6. In-repo prod-targeting: `_common.sh` defaults (`PROJECT_ID=dambi-registry`, config map),
   `registry-publish.yml` env (`PROJECT_ID`/`BUCKET` → prod).

### Manual finish steps (operator)
- **GitHub secrets** (repo `woojinnn/scopeball`) for CI keyless signing/publish:
  - `GCP_WIF_PROVIDER = projects/1912792298/locations/global/workloadIdentityPools/github-pool/providers/github-provider`
  - `GCP_DEPLOY_SA = registry-signer@dambi-registry.iam.gserviceaccount.com`
- **Re-sign** — DONE: all 31285 unique bundles re-signed with the prod HSM key + published to
  `signatures/`, 20-sample verified end-to-end (proxy serve + prod pin). The ongoing path is CI
  (`registry-publish.yml`, KMS via WIF) once the GitHub secrets above are set.
- **Custom domain** — DECLINED: the `*.run.app` host is the stable production endpoint (baked into
  the build via DefinePlugin, never user-entered, no project number → no churn).
- **Extension cutover**: `.env` already targets the prod URL + prod `PINNED_BUNDLE_PUBLIC_KEY`
  (`DAMBI_REQUIRE_BUNDLE_SIGNATURE` staged OFF). Flipping REQUIRE + shipping a store release is the
  remaining rollout step (gated on the Tier-B coverage gate + Tier-D monitoring).

### Cutover order & rollback
Provision prod → copy data → re-sign → deploy proxy → smoke → **switch extension `.env`** →
deprecation window → (optional) decommission PoC. Until the extension `.env` switch, PoC remains
source-of-truth (**zero risk**). After it, roll back by re-releasing with the PoC URL. With
`DAMBI_REQUIRE_BUNDLE_SIGNATURE` **off**, a sig gap during transition is soft-pass (no hard break).
