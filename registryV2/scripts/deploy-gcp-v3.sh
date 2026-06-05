#!/usr/bin/env bash
# DEPRECATED — split by purpose into registryV2/scripts/deploy/.
#
# This monolith bundled three independent concerns (provision infra, publish
# index DATA, deploy proxy CODE). Bundling them meant a routine index publish
# dragged a Cloud Run redeploy along, and its pinned image risked rolling back
# the live proxy. Use the purpose-separated scripts instead:
#
#   deploy/provision-infra.sh   — GCS bucket + SA + IAM           (rare/one-time)
#   deploy/publish-index.sh      — build-index + GCS upload        (frequent; SAFE, no Cloud Run)
#   deploy/deploy-proxy.sh       — build image + Cloud Run deploy  (only when proxy CODE changes)
#   deploy/deploy-all.sh         — all three in order              (first-time/full)
#
# This shim delegates to deploy-all.sh so existing references keep working.
set -euo pipefail
echo "[deploy-gcp-v3.sh] DEPRECATED → delegating to deploy/deploy-all.sh" >&2
echo "  (routine data publish? prefer: bash registryV2/scripts/deploy/publish-index.sh)" >&2
exec bash "$(dirname "$0")/deploy/deploy-all.sh" "$@"
