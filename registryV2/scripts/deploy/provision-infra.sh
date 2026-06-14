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

echo "=== Object versioning (rollback / supply-chain forensics) ==="
# A bad publish (or a forced re-sign) can be rolled back to the prior object
# version, and noncurrent versions give an audit trail of what was ever served.
gcloud storage buckets update "gs://${BUCKET}" --versioning

echo "=== KMS keyring + asymmetric-signing key (EC_SIGN_P256_SHA256) ==="
if ! gcloud kms keyrings describe "${KMS_KEYRING}" \
  --location="${KMS_LOCATION}" --project="${PROJECT_ID}" >/dev/null 2>&1; then
  gcloud kms keyrings create "${KMS_KEYRING}" \
    --location="${KMS_LOCATION}" --project="${PROJECT_ID}"
fi
if ! gcloud kms keys describe "${KMS_KEY}" \
  --keyring="${KMS_KEYRING}" --location="${KMS_LOCATION}" \
  --project="${PROJECT_ID}" >/dev/null 2>&1; then
  # protection-level=hsm keeps the private key in a FIPS 140-2 L3 HSM (never
  # extractable). Use --protection-level=software if HSM is unavailable in the
  # region; the sign/verify flow is identical.
  gcloud kms keys create "${KMS_KEY}" \
    --keyring="${KMS_KEYRING}" --location="${KMS_LOCATION}" --project="${PROJECT_ID}" \
    --purpose=asymmetric-signing \
    --default-algorithm=ec-sign-p256-sha256 \
    --protection-level=hsm
fi

echo "=== grant cloudkms.signerVerifier to the signer SA ==="
if [[ -n "${SIGNER_SA_EMAIL}" ]]; then
  # Sign-only: this role can useToSign + getPublicKey, NOT export the key.
  gcloud kms keys add-iam-policy-binding "${KMS_KEY}" \
    --keyring="${KMS_KEYRING}" --location="${KMS_LOCATION}" --project="${PROJECT_ID}" \
    --member="serviceAccount:${SIGNER_SA_EMAIL}" \
    --role="roles/cloudkms.signerVerifier"
else
  echo "  SIGNER_SA_EMAIL unset — skipping KMS IAM grant (set it for CI signing)."
fi
echo "  Pin the public key in the extension build:"
echo "    gcloud kms keys versions get-public-key ${KMS_KEY_VERSION} \\"
echo "      --key=${KMS_KEY} --keyring=${KMS_KEYRING} --location=${KMS_LOCATION} \\"
echo "      --project=${PROJECT_ID} --output-file=/tmp/pub.pem"
echo "    # strip the PEM header/footer + newlines → base64 SPKI → .env PINNED_BUNDLE_PUBLIC_KEY"

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
