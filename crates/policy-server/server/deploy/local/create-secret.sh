#!/usr/bin/env bash
# Create or update the Secret expected by the policy-server Helm chart.
# Values can be overridden through environment variables before invocation.
set -euo pipefail

NS="${1:-${POLICY_SERVER_LOCAL_NAMESPACE:-scopeball}}"
SECRET_NAME="${POLICY_SERVER_LOCAL_SECRET_NAME:-policy-server-secrets}"

DATABASE_URL="${DATABASE_URL:-postgres://scopeball:scopeball@postgres:5432/scopeball}"
REDIS_URL="${REDIS_URL:-redis://redis:6379/0}"
GOOGLE_CLIENT_ID="${GOOGLE_CLIENT_ID:-}"
GOOGLE_CLIENT_SECRET="${GOOGLE_CLIENT_SECRET:-}"
GOOGLE_REDIRECT_URI="${GOOGLE_REDIRECT_URI:-http://127.0.0.1:8788/auth/google/callback}"
ETHERSCAN_API_KEY="${ETHERSCAN_API_KEY:-}"
COINGECKO_API_KEY="${COINGECKO_API_KEY:-}"

generate_jwt_secret() {
  if command -v openssl >/dev/null 2>&1; then
    openssl rand -hex 32
  else
    dd if=/dev/urandom bs=32 count=1 2>/dev/null | od -An -tx1 | tr -d ' \n'
  fi
}

decode_base64() {
  printf '%s' "$1" | base64 --decode 2>/dev/null || printf '%s' "$1" | base64 -D 2>/dev/null || true
}

read_existing_secret_key() {
  local key="$1"
  local encoded
  encoded="$(kubectl -n "${NS}" get secret "${SECRET_NAME}" -o "jsonpath={.data.${key}}" 2>/dev/null || true)"
  if [[ -n "${encoded}" ]]; then
    decode_base64 "${encoded}"
  fi
}

kubectl create namespace "${NS}" --dry-run=client -o yaml | kubectl apply -f -

if [[ -z "${JWT_SECRET:-}" ]]; then
  JWT_SECRET="$(read_existing_secret_key JWT_SECRET)"
fi
JWT_SECRET="${JWT_SECRET:-$(generate_jwt_secret)}"

kubectl -n "${NS}" create secret generic "${SECRET_NAME}" \
  --from-literal=DATABASE_URL="${DATABASE_URL}" \
  --from-literal=REDIS_URL="${REDIS_URL}" \
  --from-literal=GOOGLE_CLIENT_ID="${GOOGLE_CLIENT_ID}" \
  --from-literal=GOOGLE_CLIENT_SECRET="${GOOGLE_CLIENT_SECRET}" \
  --from-literal=GOOGLE_REDIRECT_URI="${GOOGLE_REDIRECT_URI}" \
  --from-literal=JWT_SECRET="${JWT_SECRET}" \
  --from-literal=ETHERSCAN_API_KEY="${ETHERSCAN_API_KEY}" \
  --from-literal=COINGECKO_API_KEY="${COINGECKO_API_KEY}" \
  --dry-run=client -o yaml | kubectl apply -f -

echo "Secret '${SECRET_NAME}' applied in namespace '${NS}'."
