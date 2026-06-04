#!/usr/bin/env bash
# Load a chosen env profile then `cargo run -p policy-server`.
#
# Usage:
#   scripts/start-policy-server.sh local   # 5173 dashboard dev
#   scripts/start-policy-server.sh ext     # extension OAuth testing
#
# The profile maps 1-1 to `.env.<profile>` at repo root. Example files
# (`.env.<profile>.example`) ship with the repo; copy them to the real
# `.env.<profile>` and fill in the secrets before the first run.
#
# Extra args after the profile are forwarded to `cargo run`, e.g.
#   scripts/start-policy-server.sh local --release

set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="${1:-}"
if [[ -z "${PROFILE}" ]]; then
  cat <<USAGE >&2
usage: $0 <profile> [cargo args…]

profiles:
  local   load .env.local — DASHBOARD_URL=http://127.0.0.1:5173
  ext     load .env.ext   — DASHBOARD_URL=https://<ext-id>.chromiumapp.org

example:
  $0 local            # default debug build
  $0 ext --release    # release build, extension profile
USAGE
  exit 1
fi
shift

ENV_FILE=".env.${PROFILE}"
EXAMPLE_FILE="${ENV_FILE}.example"

if [[ ! -f "${ENV_FILE}" ]]; then
  if [[ -f "${EXAMPLE_FILE}" ]]; then
    echo "missing ${ENV_FILE}" >&2
    echo "→ cp ${EXAMPLE_FILE} ${ENV_FILE} and fill in GOOGLE_*, JWT_SECRET" >&2
  else
    echo "unknown profile '${PROFILE}' — no ${ENV_FILE} or ${EXAMPLE_FILE}" >&2
  fi
  exit 1
fi

# Auto-export everything sourced from the env file so child processes
# (cargo → policy-server) inherit it. The `set +a` flip-back keeps
# the rest of this script's own vars un-exported.
set -a
# shellcheck disable=SC1090
source "${ENV_FILE}"
set +a

echo "→ loaded ${ENV_FILE} (DASHBOARD_URL=${DASHBOARD_URL:-<unset>})"
exec cargo run -p policy-server --bin policy-server "$@"
