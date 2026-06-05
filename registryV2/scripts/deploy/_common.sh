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
PROJECT_ID="${PROJECT_ID:-scopeball-registry-poc-g}"
REGION="${REGION:-asia-northeast3}"
BUCKET="${BUCKET:-scopeball-registry-v3-seoul}"
SA_NAME="${SA_NAME:-registry-api-v3-sa}"
SA_EMAIL="${SA_EMAIL:-${SA_NAME}@${PROJECT_ID}.iam.gserviceaccount.com}"
SERVICE_NAME="${SERVICE_NAME:-registry-api-v3}"
AR_REPO="${AR_REPO:-${REGION}-docker.pkg.dev/${PROJECT_ID}/scopeball/registry-api}"

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
TIMEOUT="${TIMEOUT:-300}"
# --set-env-vars REPLACES the whole env set on each deploy, so every var the
# service needs must be listed here or prior out-of-band tuning silently drops.
CACHE_TTL_MS="${CACHE_TTL_MS:-300000}"
CACHE_NEGATIVE_TTL_MS="${CACHE_NEGATIVE_TTL_MS:-60000}"
CACHE_MAX_ENTRIES="${CACHE_MAX_ENTRIES:-1024}"
CACHE_CONTROL="${CACHE_CONTROL:-public, max-age=300, stale-while-revalidate=600}"
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

# Activate the scopeball gcloud config + project, then assert the active account
# matches EXPECTED_ACCOUNT (misfire guard). Fatal on mismatch.
rv3_activate_and_guard() {
  echo "=== gcloud config 활성 + 계정 가드 ==="
  gcloud config configurations activate scopeball >/dev/null
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
