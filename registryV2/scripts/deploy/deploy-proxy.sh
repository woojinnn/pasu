#!/usr/bin/env bash
# Deploy the registry-api PROXY CODE: build a Docker image from the CURRENT
# registry-api/ source (Cloud Build) and roll it onto the Cloud Run service.
# Run ONLY when the proxy code changed (e.g. a new path allowlist like
# index/by-selector/, cache/rate-limit logic). Index DATA is separate — use
# publish-index.sh for that.
#
#   bash registryV2/scripts/deploy/deploy-proxy.sh                    # build current code → deploy
#   IMAGE_TAG=e1aebcbc SKIP_BUILD=1 bash .../deploy-proxy.sh          # deploy an image already in AR
#
# WHY default-build-from-source: the old monolith pinned a fixed IMAGE_TAG, so a
# re-run could silently ROLL BACK the live proxy to a stale image. Here the tag
# defaults to the current git short-sha and the image is built fresh, so deploy
# always ships the code you have checked out (override IMAGE_TAG to be explicit).
#
# **replaces the live shared service revision.** User runs directly. Cloud Run
# keeps the prior revision; an unhealthy new revision fails the deploy (traffic
# stays on the old one). Roll back: gcloud run services update-traffic
# "${SERVICE_NAME}" --region "${REGION}" --to-revisions <PRIOR>=100
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"
rv3_activate_and_guard

GIT_SHA="$(cd "${REPO_ROOT}" && git rev-parse --short HEAD 2>/dev/null || echo manual)"
IMAGE_TAG="${IMAGE_TAG:-${GIT_SHA}}"
IMAGE="${AR_REPO}:${IMAGE_TAG}"

if [[ "${SKIP_BUILD:-0}" == "1" ]]; then
  echo "=== SKIP_BUILD=1 — deploy existing image ${IMAGE} (must already be in AR) ==="
else
  echo "=== build proxy image from registry-api/ (Cloud Build) → ${IMAGE} ==="
  gcloud builds submit "${REPO_ROOT}/registry-api" --tag "${IMAGE}" --project "${PROJECT_ID}"
fi

echo "=== Cloud Run deploy ${SERVICE_NAME} ← ${IMAGE} ==="
gcloud run deploy "${SERVICE_NAME}" \
  --region="${REGION}" \
  --image="${IMAGE}" \
  --service-account="${SA_EMAIL}" \
  --cpu="${CPU}" \
  --memory="${MEMORY}" \
  --min-instances="${MIN_INSTANCES}" \
  --max-instances="${MAX_INSTANCES}" \
  --concurrency="${CONCURRENCY}" \
  --timeout="${TIMEOUT}" \
  --cpu-boost \
  --ingress=all \
  --set-env-vars="$(rv3_env_vars)" \
  --no-allow-unauthenticated

echo "=== allUsers 에 run.invoker (extension anonymous fetch; 버킷은 private) ==="
gcloud run services add-iam-policy-binding "${SERVICE_NAME}" \
  --region="${REGION}" \
  --member=allUsers \
  --role=roles/run.invoker

URL=$(gcloud run services describe "${SERVICE_NAME}" --region="${REGION}" --format='value(status.url)')
echo "deploy-proxy 완료. Cloud Run URL: ${URL}"
