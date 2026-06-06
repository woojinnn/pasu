#!/usr/bin/env bash
# Load the shared env base + a chosen profile overlay, then
# `cargo run -p policy-server`.
#
# Usage:
#   scripts/start-policy-server.sh local   # 5173 dashboard dev
#   scripts/start-policy-server.sh ext     # extension OAuth testing
#
# Loading model — base then overlay:
#   1. `.env`            shared config + secrets (GOOGLE_*, JWT_SECRET,
#                        DATABASE_URL, REDIS_URL, …). Sourced first IF it
#                        exists. Copy from `.env.example` and fill in.
#   2. `.env.<profile>`  thin overlay — only the vars that differ per
#                        profile (DASHBOARD_URL, GOOGLE_REDIRECT_URI,
#                        CORS_ALLOWED_ORIGINS). Sourced second so it wins.
#
# The profile maps 1-1 to `.env.<profile>` at repo root. Example files ship
# with the repo: copy `.env.example` → `.env` (shared secrets) and
# `.env.<profile>.example` → `.env.<profile>` (overrides) before first run.
# Backward-compatible: a `.env.<profile>` that still holds the FULL set of
# vars keeps working — base-then-overlay just re-sets the same names.
#
# Extra args after the profile are forwarded to `cargo run`, e.g.
#   scripts/start-policy-server.sh local --release

set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="${1:-}"
if [[ -z "${PROFILE}" ]]; then
  cat <<USAGE >&2
usage: $0 <profile> [cargo args…]

profiles (overlay .env with):
  local   .env.local — DASHBOARD_URL=http://127.0.0.1:5173
  ext     .env.ext   — DASHBOARD_URL=https://<ext-id>.chromiumapp.org

example:
  $0 local            # default debug build
  $0 ext --release    # release build, extension profile
USAGE
  exit 1
fi
shift

BASE_ENV=".env"
ENV_FILE=".env.${PROFILE}"
EXAMPLE_FILE="${ENV_FILE}.example"

if [[ ! -f "${ENV_FILE}" ]]; then
  if [[ -f "${EXAMPLE_FILE}" ]]; then
    echo "missing ${ENV_FILE}" >&2
    echo "→ cp .env.example .env             # shared secrets: GOOGLE_*, JWT_SECRET, DATABASE_URL …" >&2
    echo "→ cp ${EXAMPLE_FILE} ${ENV_FILE}   # profile overrides for '${PROFILE}'" >&2
  else
    echo "unknown profile '${PROFILE}' — no ${ENV_FILE} or ${EXAMPLE_FILE}" >&2
  fi
  exit 1
fi

# Auto-export everything sourced from the env files so child processes
# (cargo → policy-server) inherit it. The `set +a` flip-back keeps
# the rest of this script's own vars un-exported.
#
# Source the shared base first (if present), then the profile overlay so
# its values win. The base is optional — a profile file that still holds
# the full var set keeps working on its own.
if [[ -f "${BASE_ENV}" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "${BASE_ENV}"
  set +a
  echo "→ loaded ${BASE_ENV} (shared base)"
fi

set -a
# shellcheck disable=SC1090
source "${ENV_FILE}"
set +a

echo "→ loaded ${ENV_FILE} (DASHBOARD_URL=${DASHBOARD_URL:-<unset>})"
exec cargo run -p policy-server --bin policy-server "$@"
