#!/usr/bin/env bash
# Shared config + guards for the registry-v3 deploy scripts (registryV2/).
# SOURCE this file — do not execute it directly.
#
# The former monolithic `deploy-gcp-v3.sh` bundled three independent concerns:
# provisioning infra, publishing index DATA, and deploying the proxy CODE.
# That coupling meant a routine index publish dragged a Cloud Run redeploy
# along — and the pinned image risked ROLLING BACK the live service. These are
# now split by purpose so a data publish never touches the running proxy:
#
#   provision-infra.sh  — GCS bucket + SA + IAM          (rare / one-time, idempotent)
#   publish-index.sh     — build-index + GCS rsync upload (FREQUENT; SAFE, no Cloud Run)
#   deploy-proxy.sh      — build image + Cloud Run deploy (ONLY when proxy CODE changes)
#   deploy-all.sh        — provision → publish → proxy    (first-time / full bring-up)
#
# Layer model (why publish ≠ deploy):
#   extension → registry-api (Cloud Run proxy: path allowlist + cache) → GCS bucket (objects)
#   · publish-index updates the OBJECTS (bucket). New by-callkey/tokens work immediately.
#   · A NEW path prefix (e.g. index/by-selector/) is gated by the proxy's allowlist,
#     which lives in the proxy SOURCE — so it only goes live after deploy-proxy.sh.

set -euo pipefail

# --- Identity / resources -----------------------------------------------------
PROJECT_ID="${PROJECT_ID:-dambi-registry}"
REGION="${REGION:-asia-northeast3}"
BUCKET="${BUCKET:-dambi-registry-v3-seoul}"
SA_NAME="${SA_NAME:-registry-api-v3-sa}"
SA_EMAIL="${SA_EMAIL:-${SA_NAME}@${PROJECT_ID}.iam.gserviceaccount.com}"
SERVICE_NAME="${SERVICE_NAME:-registry-api-v3}"
AR_REPO="${AR_REPO:-${REGION}-docker.pkg.dev/${PROJECT_ID}/dambi/registry-api}"

# --- Bundle signing (Cloud KMS, asymmetric EC_SIGN_P256_SHA256) ---------------
# The detached signatures/<sha>.sig published with the index are produced by
# signing each bundle_sha256 digest with this KMS key. The private key never
# leaves the HSM; CI signs via Workload Identity (roles/cloudkms.signerVerifier).
# The matching PUBLIC key (SPKI) is pinned in the extension build (.env
# PINNED_BUNDLE_PUBLIC_KEY) — get it with `kms keys versions get-public-key`.
KMS_KEYRING="${KMS_KEYRING:-registry-signing}"
KMS_KEY="${KMS_KEY:-bundle-sign-p256}"
KMS_LOCATION="${KMS_LOCATION:-${REGION}}"
KMS_KEY_VERSION="${KMS_KEY_VERSION:-1}"
# Full key-VERSION resource name consumed by scripts/sign-bundles.ts (kms mode).
KMS_KEY_NAME="${KMS_KEY_NAME:-projects/${PROJECT_ID}/locations/${KMS_LOCATION}/keyRings/${KMS_KEYRING}/cryptoKeys/${KMS_KEY}/cryptoKeyVersions/${KMS_KEY_VERSION}}"
# The CI/deploy SA that signs (Workload Identity). Distinct from the read-only
# proxy SA above. Leave empty to skip the IAM grant in provision-infra.sh.
SIGNER_SA_EMAIL="${SIGNER_SA_EMAIL:-}"

# --- Cloud Run runtime shape (env-overridable) --------------------------------
# max-instances = denial-of-wallet cost ceiling (threat model A5) — always pin.
# min=1 keeps a warm instance: the extension's JIT registry fetch has no
# per-fetch timeout, so a scale-to-zero cold start blows the 8s pre-sign budget
# and surfaces `__engine::timeout`. Set MIN_INSTANCES=0 for scale-to-zero.
CPU="${CPU:-1}"
MEMORY="${MEMORY:-256Mi}"
MIN_INSTANCES="${MIN_INSTANCES:-1}"
MAX_INSTANCES="${MAX_INSTANCES:-3}"
CONCURRENCY="${CONCURRENCY:-80}"
# Cloud Run request timeout. The proxy answers small JSON in <1s; a long ceiling
# only helps slowloris pin the max-instances×concurrency fleet. 15s is generous.
TIMEOUT="${TIMEOUT:-15}"
# --set-env-vars REPLACES the whole env set on each deploy, so every var the
# service needs must be listed here or prior out-of-band tuning silently drops.
CACHE_TTL_MS="${CACHE_TTL_MS:-300000}"
CACHE_NEGATIVE_TTL_MS="${CACHE_NEGATIVE_TTL_MS:-60000}"
CACHE_MAX_ENTRIES="${CACHE_MAX_ENTRIES:-1024}"
# max-age only — the proxy cache does hard TTL expiry (no background revalidation),
# so advertising stale-while-revalidate promised semantics it never implemented.
CACHE_CONTROL="${CACHE_CONTROL:-public, max-age=300}"
RATE_LIMIT_BURST="${RATE_LIMIT_BURST:-60}"
RATE_LIMIT_REFILL_PER_SEC="${RATE_LIMIT_REFILL_PER_SEC:-10}"
RATE_LIMIT_MAX_IPS="${RATE_LIMIT_MAX_IPS:-10000}"
# Direct *.run.app: GFE appends the real client IP rightmost in X-Forwarded-For,
# so 0 (rightmost) is correct + unspoofable. Front an HTTPS LB → raise by hops.
TRUSTED_PROXY_HOPS="${TRUSTED_PROXY_HOPS:-0}"

# Misfire guard (NOT a security control — threat model F3): refuse to act under
# an unexpected gcloud account. Override EXPECTED_ACCOUNT to your own to proceed.
EXPECTED_ACCOUNT="${EXPECTED_ACCOUNT:-sujini000522@gmail.com}"

# --- Paths (resolved from this file's location: scripts/deploy/) --------------
RV2_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"   # registryV2/
REPO_ROOT="$(cd "${RV2_DIR}/.." && pwd)"                        # repo root

# Activate the gcloud config (GCLOUD_CONFIG, default `dambi`) + project, then
# assert the active account matches EXPECTED_ACCOUNT (misfire guard). Fatal on
# mismatch. Config map: PROD = config `dambi` / project `dambi-registry`;
# the legacy PoC = config `scopeball` / project `scopeball-registry-poc-g`
# (override GCLOUD_CONFIG=scopeball PROJECT_ID=scopeball-registry-poc-g to target it).
rv3_activate_and_guard() {
  echo "=== gcloud config 활성 + 계정 가드 ==="
  gcloud config configurations activate "${GCLOUD_CONFIG:-dambi}" >/dev/null
  gcloud config set project "${PROJECT_ID}" >/dev/null
  local active
  active="$(gcloud config get-value account 2>/dev/null || true)"
  if [[ "${active}" != "${EXPECTED_ACCOUNT}" ]]; then
    echo "ABORT: 활성 gcloud 계정 '${active}' != 기대 '${EXPECTED_ACCOUNT}'." >&2
    echo "  의도한 계정이면 EXPECTED_ACCOUNT='${active}' 로 재실행." >&2
    exit 1
  fi
  echo "account=${active}  project=${PROJECT_ID}"
}

# The `--set-env-vars` payload (custom delimiter `@` so CACHE_CONTROL's comma is safe).
rv3_env_vars() {
  printf '^@^REGISTRY_BUCKET=%s@CACHE_TTL_MS=%s@CACHE_NEGATIVE_TTL_MS=%s@CACHE_MAX_ENTRIES=%s@CACHE_CONTROL=%s@RATE_LIMIT_BURST=%s@RATE_LIMIT_REFILL_PER_SEC=%s@RATE_LIMIT_MAX_IPS=%s@TRUSTED_PROXY_HOPS=%s' \
    "${BUCKET}" "${CACHE_TTL_MS}" "${CACHE_NEGATIVE_TTL_MS}" "${CACHE_MAX_ENTRIES}" \
    "${CACHE_CONTROL}" "${RATE_LIMIT_BURST}" "${RATE_LIMIT_REFILL_PER_SEC}" \
    "${RATE_LIMIT_MAX_IPS}" "${TRUSTED_PROXY_HOPS}"
}
