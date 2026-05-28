#!/usr/bin/env bash
# Phase 3G — GCP 자원 신설 명령서
#
# scopeball-registry-v3 = registryV2/ 의 GCP 자원 (GCS bucket + Cloud Run + SA).
# v2 자원 (scopeball-registry-v2-seoul + registry-api-v2) 은 legacy 로 유지.
#
# **사용자 confirm 필요** — 본 스크립트는 destructive infra 변경 (GCS bucket + Cloud Run + IAM).
# auto mode 안에서 자동 실행 X. 사용자가 직접 실행:
#   bash registryV2/scripts/deploy-gcp-v3.sh
#
# 결과:
# - GCS bucket: scopeball-registry-v3-seoul (asia-northeast3, private, UBLA + PAP enforced)
# - Cloud Run service: registry-api-v3 (asia-northeast3, no-allow-unauthenticated)
# - Service Account: registry-api-v3-sa@scopeball-registry-poc-g.iam.gserviceaccount.com (objectViewer 만)
# - browser-extension/.env: REGISTRY_BASE_URL 갱신 (Phase 4 에서)

set -euo pipefail

PROJECT_ID="scopeball-registry-poc-g"
REGION="asia-northeast3"
BUCKET="scopeball-registry-v3-seoul"
SA_NAME="registry-api-v3-sa"
SA_EMAIL="${SA_NAME}@${PROJECT_ID}.iam.gserviceaccount.com"
SERVICE_NAME="registry-api-v3"
IMAGE="asia-northeast3-docker.pkg.dev/${PROJECT_ID}/scopeball/registry-api:v1"

echo "=== Step 1: gcloud config 활성 ==="
gcloud config configurations activate scopeball
gcloud config set project "${PROJECT_ID}"

echo "=== Step 2: GCS bucket 생성 ==="
gsutil mb -l "${REGION}" -b on -p "${PROJECT_ID}" "gs://${BUCKET}"

echo "=== Step 3: bucket policy (UBLA + PAP enforced) ==="
gsutil pap set enforced "gs://${BUCKET}"
gsutil ubla set on "gs://${BUCKET}"

echo "=== Step 4: Service Account 생성 ==="
gcloud iam service-accounts create "${SA_NAME}" \
  --project="${PROJECT_ID}" \
  --display-name="Registry API v3 SA (registryV2/ — Phase 3G)"

echo "=== Step 5: SA 에 objectViewer 권한 ==="
gsutil iam ch "serviceAccount:${SA_EMAIL}:objectViewer" "gs://${BUCKET}"

echo "=== Step 6: registryV2 build-index → GCS 업로드 ==="
cd "$(dirname "$0")/.."
npm install --no-audit --no-fund --silent
npx tsx scripts/build-index.ts
gsutil -m rsync -r -x '^(node_modules/|\.git/|package\.json$|package-lock\.json$|tsconfig\.json$|scripts/)' ./ "gs://${BUCKET}/"

echo "=== Step 7: Cloud Run 배포 ==="
gcloud run deploy "${SERVICE_NAME}" \
  --region="${REGION}" \
  --image="${IMAGE}" \
  --service-account="${SA_EMAIL}" \
  --set-env-vars="REGISTRY_BUCKET=${BUCKET}" \
  --no-allow-unauthenticated

echo "=== Step 7b: allUsers 에 run.invoker 권한 부여 (extension anonymous fetch) ==="
# Plan §M0 — v2 와 동일 패턴. registry-api-v3 가 GCS proxy 역할만 하고 GCS bucket
# 자체는 private (registry-api-v3-sa objectViewer 만) 이므로 invoker 만 anonymous.
# 위협 모델은 v2 와 동일 (docs/REGISTRY_THREAT_MODEL.md).
gcloud run services add-iam-policy-binding "${SERVICE_NAME}" \
  --region="${REGION}" \
  --member=allUsers \
  --role=roles/run.invoker

echo "=== Step 8: Cloud Run URL 출력 ==="
URL=$(gcloud run services describe "${SERVICE_NAME}" \
  --region="${REGION}" --format='value(status.url)')
echo "Cloud Run URL: ${URL}"

echo ""
echo "=== Step 9: 후속 (Phase 4 에서 진행) ==="
echo "browser-extension/.env 의 REGISTRY_BASE_URL 을 위 URL 로 갱신:"
echo "  REGISTRY_BASE_URL=${URL}"
echo ""
echo "Phase 3G 완료."
