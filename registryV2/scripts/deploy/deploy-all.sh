#!/usr/bin/env bash
# Full bring-up: provision infra → publish index → deploy proxy, in order.
# Use for a first-time deploy or a clean re-bring-up. For routine work prefer
# the single-purpose scripts (publish-index.sh for data, deploy-proxy.sh for code).
#
#   bash registryV2/scripts/deploy/deploy-all.sh
#   PRUNE=1 bash registryV2/scripts/deploy/deploy-all.sh   # prune orphans during publish
#
# **destructive infra + replaces the live proxy.** User runs directly.
set -euo pipefail
HERE="$(dirname "${BASH_SOURCE[0]}")"
bash "${HERE}/provision-infra.sh"
bash "${HERE}/publish-index.sh"
bash "${HERE}/deploy-proxy.sh"
echo "deploy-all 완료."
