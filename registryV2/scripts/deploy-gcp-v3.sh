#!/usr/bin/env bash
# Phase 3G — GCP 자원 신설 명령서  (hardened — chore/registry-hardening)
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
# - Cloud Run service: registry-api-v3 (asia-northeast3, no-allow-unauthenticated + allUsers invoker)
#     · --max-instances 로 worst-case 비용 상한 (denial-of-wallet backstop — 위협모델 A5).
#       미설정 시 Cloud Run 기본 100 으로 backstop 상실 → 반드시 명시.
#     · cpu/memory/concurrency/timeout pin + cache/rate-limit env 명시 (v2 배포값과 동일).
# - Service Account: registry-api-v3-sa@...iam.gserviceaccount.com (objectViewer 만, read-only).
# - browser-extension/.env: REGISTRY_BASE_URL 갱신 (Phase 4 에서).
#
# 재실행 안전(idempotent): bucket/SA 존재 시 생성 skip. 업로드는 기본 additive;
# 버킷 orphan 까지 정리하려면 PRUNE=1 (로컬 index 완전성 전제 — 위협모델 F1).
# 모든 운영 파라미터는 env-overridable (CPU/MEMORY/MAX_INSTANCES/... 참조).

set -euo pipefail

PROJECT_ID="scopeball-registry-poc-g"
REGION="asia-northeast3"
BUCKET="scopeball-registry-v3-seoul"
SA_NAME="registry-api-v3-sa"
SA_EMAIL="${SA_NAME}@${PROJECT_ID}.iam.gserviceaccount.com"
SERVICE_NAME="registry-api-v3"
IMAGE_TAG="${IMAGE_TAG:-v3-ref-materializer-20260601}"
IMAGE="asia-northeast3-docker.pkg.dev/${PROJECT_ID}/scopeball/registry-api:${IMAGE_TAG}"

# Cloud Run runtime shape — 전부 env-overridable. max-instances 가 denial-of-wallet
# 최종 비용 상한(위협모델 A5)이라 반드시 명시한다. cache/rate-limit 기본값은 v2
# 배포값(docs/REGISTRY_GCP_SPEC.md §3.3)과 일치.
CPU="${CPU:-1}"
MEMORY="${MEMORY:-256Mi}"
# Keep 1 warm instance: with low/single-user traffic a scale-to-zero (0) proxy
# cold-starts on the first request after idle, and the extension's JIT registry
# fetch has no per-fetch timeout — a cold start blows past the 8s lifecycle cap
# and the wallet pre-sign verdict surfaces `__engine::timeout`. min=1 keeps the
# fetch ~0.1s. Set MIN_INSTANCES=0 to opt back into scale-to-zero (cheaper, but
# reintroduces the cold-start timeout for sporadic traffic).
MIN_INSTANCES="${MIN_INSTANCES:-1}"
MAX_INSTANCES="${MAX_INSTANCES:-3}"
CONCURRENCY="${CONCURRENCY:-80}"
TIMEOUT="${TIMEOUT:-300}"
CACHE_TTL_MS="${CACHE_TTL_MS:-300000}"
CACHE_NEGATIVE_TTL_MS="${CACHE_NEGATIVE_TTL_MS:-60000}"
RATE_LIMIT_BURST="${RATE_LIMIT_BURST:-60}"
RATE_LIMIT_REFILL_PER_SEC="${RATE_LIMIT_REFILL_PER_SEC:-10}"
# 아래 3개는 코드 default 와 동일하지만, --set-env-vars 가 revision env 집합을 통째로
# 교체(REPLACE)하므로 명시해 둬야 이전 revision 의 out-of-band 튜닝이 묵시적으로 안 사라진다.
CACHE_MAX_ENTRIES="${CACHE_MAX_ENTRIES:-1024}"
CACHE_CONTROL="${CACHE_CONTROL:-public, max-age=300, stale-while-revalidate=600}"
RATE_LIMIT_MAX_IPS="${RATE_LIMIT_MAX_IPS:-10000}"
# Direct *.run.app: GFE 가 실제 client IP 를 X-Forwarded-For 맨 오른쪽에 append →
# 0 (rightmost) 가 정답·unspoofable. 외부 HTTPS LB 를 앞단에 두면 그 hop 수만큼 올린다.
TRUSTED_PROXY_HOPS="${TRUSTED_PROXY_HOPS:-0}"

# 잘못된 gcloud 계정으로의 오배포 방지 (misfire guard — 보안 통제 아님, 위협모델 F3).
EXPECTED_ACCOUNT="${EXPECTED_ACCOUNT:-sujini000522@gmail.com}"

echo "=== Step 1: gcloud config 활성 ==="
gcloud config configurations activate scopeball
gcloud config set project "${PROJECT_ID}"

echo "=== Step 1b: gcloud 계정 가드 (오배포 방지) ==="
ACTIVE_ACCOUNT="$(gcloud config get-value account 2>/dev/null || true)"
if [[ "${ACTIVE_ACCOUNT}" != "${EXPECTED_ACCOUNT}" ]]; then
  echo "ABORT: 활성 gcloud 계정 '${ACTIVE_ACCOUNT}' != 기대 '${EXPECTED_ACCOUNT}'." >&2
  echo "  의도한 계정이면 EXPECTED_ACCOUNT='${ACTIVE_ACCOUNT}' 로 재실행." >&2
  exit 1
fi

echo "=== Step 2: GCS bucket (UBLA + PAP enforced, 한 번에) ==="
# gsutil 대신 gcloud storage 사용 (macOS hang 회피 — REGISTRY_GCP_SPEC §7.6).
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

echo "=== Step 3: Service Account (idempotent) ==="
if ! gcloud iam service-accounts describe "${SA_EMAIL}" >/dev/null 2>&1; then
  gcloud iam service-accounts create "${SA_NAME}" \
    --project="${PROJECT_ID}" \
    --display-name="Registry API v3 SA (registryV2/ — Phase 3G)"
fi

echo "=== Step 4: SA 에 objectViewer (최소권한, read-only) ==="
gcloud storage buckets add-iam-policy-binding "gs://${BUCKET}" \
  --member="serviceAccount:${SA_EMAIL}" \
  --role="roles/storage.objectViewer"

echo "=== Step 5: registryV2 build-index → GCS 업로드 ==="
cd "$(dirname "$0")/.."
npm install --no-audit --no-fund --silent
npx tsx scripts/build-index.ts
# 서빙에 필요한 prefix 만 업로드 (build/dev 산출물 surface/·cache/ 제외 — 비용·노출 축소).
# proxy 의 path 화이트리스트(validation.ts)와 정확히 일치: index / tokens / bundles /
# contexts. manifests 는 provenance 용으로 v2 관례대로 함께 업로드.
# OS cruft (.DS_Store 등)는 업로드 전에 로컬에서 제거 — 비공개 버킷 오염 방지.
# Finder/iCloud 가 재생성하는 무해 파일이라 로컬 삭제도 안전.
find bundles contexts tokens manifests index -name '.DS_Store' -delete 2>/dev/null || true
# Phase 1 — additive upload, LEAVES before POINTERS (no inconsistency window).
# index entries are 3-ref docs the proxy resolves by re-reading bundles/<sha> +
# contexts/...; if index/ landed before its targets, new entries would 502
# ref_materialization_failed until the targets upload. So upload the referenced
# objects first and index/ LAST.
for prefix in bundles contexts tokens manifests index; do
  if [[ -d "${prefix}" ]]; then
    echo "  rsync (additive) ${prefix}/ → gs://${BUCKET}/${prefix}"
    gcloud storage rsync --recursive "${prefix}" "gs://${BUCKET}/${prefix}"
  fi
done
# Phase 2 — optional prune, POINTERS before LEAVES (reverse of upload) so no live
# index ref ever names a just-deleted leaf (위협모델 F1; 로컬 index 완전성 전제).
if [[ "${PRUNE:-0}" == "1" ]]; then
  echo "  PRUNE=1 — orphan 객체 삭제 (pointers→leaves 순)"
  for prefix in index manifests tokens contexts bundles; do
    if [[ -d "${prefix}" ]]; then
      gcloud storage rsync --recursive --delete-unmatched-destination-objects "${prefix}" "gs://${BUCKET}/${prefix}"
    fi
  done
fi

echo "=== Step 6: Cloud Run 배포 ==="
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
  --set-env-vars="^@^REGISTRY_BUCKET=${BUCKET}@CACHE_TTL_MS=${CACHE_TTL_MS}@CACHE_NEGATIVE_TTL_MS=${CACHE_NEGATIVE_TTL_MS}@CACHE_MAX_ENTRIES=${CACHE_MAX_ENTRIES}@CACHE_CONTROL=${CACHE_CONTROL}@RATE_LIMIT_BURST=${RATE_LIMIT_BURST}@RATE_LIMIT_REFILL_PER_SEC=${RATE_LIMIT_REFILL_PER_SEC}@RATE_LIMIT_MAX_IPS=${RATE_LIMIT_MAX_IPS}@TRUSTED_PROXY_HOPS=${TRUSTED_PROXY_HOPS}" \
  --no-allow-unauthenticated

echo "=== Step 6b: allUsers 에 run.invoker (extension anonymous fetch) ==="
# 익스텐션은 비인증 client → 진입점 public. 버킷은 private (SA objectViewer 만).
# 보안 경계 = 버킷 private + rate-limit (docs/REGISTRY_THREAT_MODEL.md §8.1).
gcloud run services add-iam-policy-binding "${SERVICE_NAME}" \
  --region="${REGION}" \
  --member=allUsers \
  --role=roles/run.invoker

echo "=== Step 7: Cloud Run URL 출력 ==="
URL=$(gcloud run services describe "${SERVICE_NAME}" \
  --region="${REGION}" --format='value(status.url)')
echo "Cloud Run URL: ${URL}"

echo ""
echo "=== Step 8: 후속 (Phase 4 에서 진행) ==="
echo "browser-extension/.env 의 REGISTRY_BASE_URL 을 위 URL 로 갱신:"
echo "  REGISTRY_BASE_URL=${URL}"
echo ""
echo "Phase 3G 완료 (hardened)."
