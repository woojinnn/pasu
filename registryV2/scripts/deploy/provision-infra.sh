#!/usr/bin/env bash
# Provision the registry-v3 GCP infra: GCS bucket + Service Account + IAM.
# Rare / one-time — idempotent (existing resources are re-asserted, not recreated).
# Does NOT publish data and does NOT touch Cloud Run. See deploy/_common.sh.
#
#   bash registryV2/scripts/deploy/provision-infra.sh
#
# **destructive infra** (bucket / SA / IAM). User runs directly — not in unattended automation.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"
rv3_activate_and_guard

echo "=== GCS bucket (UBLA + PAP enforced) ==="
# gcloud storage (not gsutil) — macOS hang avoidance (REGISTRY_GCP_SPEC §7.6).
if ! gcloud storage buckets describe "gs://${BUCKET}" >/dev/null 2>&1; then
  gcloud storage buckets create "gs://${BUCKET}" \
    --project="${PROJECT_ID}" \
    --location="${REGION}" \
    --uniform-bucket-level-access \
    --public-access-prevention
else
  echo "bucket gs://${BUCKET} 이미 존재 — PAP/UBLA enforced 재확인"
  gcloud storage buckets update "gs://${BUCKET}" \
    --uniform-bucket-level-access \
    --public-access-prevention
fi

echo "=== Service Account (idempotent) ==="
if ! gcloud iam service-accounts describe "${SA_EMAIL}" >/dev/null 2>&1; then
  gcloud iam service-accounts create "${SA_NAME}" \
    --project="${PROJECT_ID}" \
    --display-name="Registry API v3 SA (registryV2/)"
fi

echo "=== SA 에 objectViewer (최소권한, read-only) ==="
gcloud storage buckets add-iam-policy-binding "gs://${BUCKET}" \
  --member="serviceAccount:${SA_EMAIL}" \
  --role="roles/storage.objectViewer"

echo "provision-infra 완료."
