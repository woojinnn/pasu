#!/usr/bin/env bash
# registry/scripts/upload.sh
#
# ScopeBall Adapter Marketplace registry — 로컬 registry/ 콘텐츠를
# 비공개 GCS 버킷으로 additive 업로드.
#
#   대상       : 비공개 버킷 gs://scopeball-registry-seoul (asia-northeast3)
#   업로드 전  : `npm run build` 로 callkey index 재생성 (manifests <-> index 정합)
#   동기 방식  : `gcloud storage rsync`. --delete-unmatched-destination-objects
#                (= gsutil rsync -d) 를 쓰지 않음 → 버킷의 잉여 객체를 삭제하지
#                않는 additive only. gsutil 아님: `gsutil -m` 은 macOS 에서 hang.
#   업로드 범위: index/ manifests/ tokens/ 3개 prefix 만. node_modules /
#                package.json / scripts 등은 prefix 를 명시해 자동 제외.
#
# 사용:
#   ./scripts/upload.sh              # 업로드 실행
#   ./scripts/upload.sh --dry-run    # 미리보기만 (실제 업로드 X)

set -euo pipefail

BUCKET="gs://scopeball-registry-seoul"
EXPECTED_ACCOUNT="sujini000522@gmail.com"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REGISTRY_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DRY_RUN=""
if [[ "${1:-}" == "--dry-run" ]]; then
  DRY_RUN="1"
  echo "[upload] DRY-RUN 모드 — 실제 업로드 안 함"
fi

# --- 1. gcloud 계정 가드 -----------------------------------------------------
# 잘못된 계정 (예: UPSide audit) 으로 실행되는 것을 차단.
ACTIVE_ACCOUNT="$(gcloud config get-value account 2>/dev/null || true)"
if [[ "$ACTIVE_ACCOUNT" != "$EXPECTED_ACCOUNT" ]]; then
  echo "[upload] 중단 — 활성 gcloud 계정: '${ACTIVE_ACCOUNT}'" >&2
  echo "[upload] 기대 계정: '${EXPECTED_ACCOUNT}'" >&2
  echo "[upload] 해결: gcloud config configurations activate scopeball" >&2
  exit 1
fi
echo "[upload] gcloud 계정 : $ACTIVE_ACCOUNT"
echo "[upload] 대상 버킷   : $BUCKET"
echo "[upload] registry    : $REGISTRY_ROOT"

# --- 2. callkey index 재생성 -------------------------------------------------
# build-index.ts 가 index/by-callkey/ 를 wipe 후 manifests 기준 재생성한다.
# manifest 가 깨져 있으면 여기서 실패 → set -e 로 업로드 전에 중단.
echo "[upload] index 재생성 — npm run build ..."
( cd "$REGISTRY_ROOT" && npm run build )

# --- 2.5. 주소 on-chain 존재검증 게이트 (감사 Phase E) -----------------------
# verify-addresses = audit-addresses.ts. manifest 의 (chain,addr) 를 eth_getCode
# 로 검증 — bogus(미배포 = dead callkey) 가 있으면 set -e 로 업로드 전 중단.
# --allow-unknown: 공개 RPC 간헐 실패(unknown)는 warn 만 — 거짓 차단 방지.
echo "[upload] 주소 검증 — npm run verify-addresses ..."
( cd "$REGISTRY_ROOT" && npm run verify-addresses -- --allow-unknown )

# --- 3. additive 업로드 ------------------------------------------------------
for prefix in index manifests tokens; do
  src="$REGISTRY_ROOT/$prefix"
  if [[ ! -d "$src" ]]; then
    echo "[upload] 경고 — $src 없음, 건너뜀" >&2
    continue
  fi
  echo "[upload] rsync $prefix/ -> $BUCKET/$prefix/"
  if [[ -n "$DRY_RUN" ]]; then
    gcloud storage rsync "$src" "$BUCKET/$prefix" --recursive --dry-run
  else
    gcloud storage rsync "$src" "$BUCKET/$prefix" --recursive
  fi
done

# --- 4. 검증 — 버킷 객체 수 --------------------------------------------------
if [[ -z "$DRY_RUN" ]]; then
  echo "[upload] 업로드 후 버킷 .json 객체 수:"
  for prefix in index manifests tokens; do
    n="$(gcloud storage ls -r "$BUCKET/$prefix/**" 2>/dev/null | grep -c '\.json$' || true)"
    echo "  $prefix: $n"
  done
fi

echo "[upload] 완료."
