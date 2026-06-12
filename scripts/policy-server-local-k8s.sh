#!/usr/bin/env bash
# One-command local Kubernetes loop for policy-server.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHART="${ROOT}/crates/policy-server/server/deploy/helm/policy-server"
LOCAL_DIR="${ROOT}/crates/policy-server/server/deploy/local"
VALUES_LOCAL="${CHART}/values-local.yaml"
DOCKERFILE="${ROOT}/crates/policy-server/server/Dockerfile"

CMD="${1:-up}"
NS="${POLICY_SERVER_LOCAL_NAMESPACE:-dambi}"
RELEASE="${POLICY_SERVER_LOCAL_RELEASE:-dambi}"
IMAGE_REPOSITORY="${POLICY_SERVER_LOCAL_IMAGE_REPOSITORY:-dambi-policy-server}"
IMAGE_TAG="${POLICY_SERVER_LOCAL_IMAGE_TAG:-dev}"
IMAGE="${IMAGE_REPOSITORY}:${IMAGE_TAG}"
LOCAL_PORT="${POLICY_SERVER_LOCAL_PORT:-8788}"
HELM_TIMEOUT="${POLICY_SERVER_LOCAL_HELM_TIMEOUT:-10m}"
FULLNAME="${POLICY_SERVER_LOCAL_FULLNAME:-${RELEASE}-policy-server}"
PID_FILE="${TMPDIR:-/tmp}/policy-server-local-k8s-${NS}-${RELEASE}.pf.pid"
LOG_FILE="${TMPDIR:-/tmp}/policy-server-local-k8s-${NS}-${RELEASE}.pf.log"
BUILT_IN_MINIKUBE=0

usage() {
  cat <<USAGE
usage: scripts/policy-server-local-k8s.sh <command>

commands:
  up            build image, deploy deps/chart, port-forward, health check
  down          stop port-forward and remove the local Helm release/deps
  status        show local policy-server Kubernetes resources
  port-forward  run a foreground port-forward to localhost:${LOCAL_PORT}

environment:
  POLICY_SERVER_LOCAL_NAMESPACE=${NS}
  POLICY_SERVER_LOCAL_RELEASE=${RELEASE}
  POLICY_SERVER_LOCAL_IMAGE_REPOSITORY=${IMAGE_REPOSITORY}
  POLICY_SERVER_LOCAL_IMAGE_TAG=${IMAGE_TAG}
  POLICY_SERVER_LOCAL_PORT=${LOCAL_PORT}
USAGE
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

context_name() {
  kubectl config current-context 2>/dev/null || true
}

ensure_namespace() {
  kubectl create namespace "${NS}" --dry-run=client -o yaml | kubectl apply -f -
}

build_image() {
  local context
  context="$(context_name)"
  if [[ "${context}" == "minikube" || "${context}" == *"minikube"* ]]; then
    require_cmd minikube
    echo "Building ${IMAGE} inside the minikube Docker daemon..."
    eval "$(minikube docker-env)"
    BUILT_IN_MINIKUBE=1
  else
    echo "Building ${IMAGE}..."
  fi
  docker build -t "${IMAGE}" -f "${DOCKERFILE}" "${ROOT}"
}

load_image_for_cluster() {
  local context
  context="$(context_name)"
  case "${context}" in
    minikube|*minikube*)
      require_cmd minikube
      if [[ "${BUILT_IN_MINIKUBE}" == "1" ]]; then
        echo "${IMAGE} was built inside minikube and is already available to Kubernetes."
      else
        echo "Loading ${IMAGE} into minikube..."
        minikube image load "${IMAGE}"
      fi
      ;;
    kind-*|kind)
      if command -v kind >/dev/null 2>&1; then
        echo "Loading ${IMAGE} into kind..."
        kind load docker-image "${IMAGE}"
      else
        echo "Current context looks like kind, but kind is not installed." >&2
        exit 1
      fi
      ;;
    docker-desktop|desktop-linux)
      echo "Docker Desktop Kubernetes can use a separate containerd image store." >&2
      echo "Use minikube for the self-contained local loop, or push ${IMAGE} to a registry and override image values." >&2
      exit 1
      ;;
    *)
      echo "Current context '${context}' is not minikube or Docker Desktop." >&2
      echo "Continuing; ensure this cluster can run local image ${IMAGE} with pullPolicy=Never." >&2
      ;;
  esac
}

apply_local_dependencies() {
  ensure_namespace
  kubectl -n "${NS}" apply -f "${LOCAL_DIR}/postgres.yaml" -f "${LOCAL_DIR}/redis.yaml"
  kubectl -n "${NS}" rollout status deploy/postgres --timeout=120s
  kubectl -n "${NS}" rollout status deploy/redis --timeout=120s
}

apply_secret() {
  "${LOCAL_DIR}/create-secret.sh" "${NS}"
}

deploy_chart() {
  helm upgrade --install "${RELEASE}" "${CHART}" \
    -n "${NS}" \
    -f "${VALUES_LOCAL}" \
    --set "image.repository=${IMAGE_REPOSITORY}" \
    --set "image.tag=${IMAGE_TAG}" \
    --wait \
    --wait-for-jobs \
    --timeout "${HELM_TIMEOUT}"

  kubectl -n "${NS}" rollout status "deploy/${FULLNAME}-api" --timeout=180s
  kubectl -n "${NS}" rollout status "deploy/${FULLNAME}-worker" --timeout=180s
}

stop_port_forward() {
  if [[ -f "${PID_FILE}" ]]; then
    local pid
    pid="$(cat "${PID_FILE}")"
    if [[ -n "${pid}" ]] && kill -0 "${pid}" >/dev/null 2>&1; then
      kill "${pid}" >/dev/null 2>&1 || true
    fi
    rm -f "${PID_FILE}"
  fi
}

start_background_port_forward() {
  stop_port_forward
  nohup kubectl -n "${NS}" port-forward "svc/${FULLNAME}-api" "${LOCAL_PORT}:80" >"${LOG_FILE}" 2>&1 &
  echo "$!" >"${PID_FILE}"
}

health_check() {
  local health_url="http://127.0.0.1:${LOCAL_PORT}/health"
  local ready_url="http://127.0.0.1:${LOCAL_PORT}/readyz"

  for _ in $(seq 1 30); do
    if curl -fsS "${health_url}" >/dev/null 2>&1 && curl -fsS "${ready_url}" >/dev/null 2>&1; then
      echo "Health checks passed:"
      curl -fsS "${health_url}"
      echo
      curl -fsS "${ready_url}"
      echo
      return 0
    fi
    sleep 1
  done

  echo "Health check failed. Port-forward log:" >&2
  sed -n '1,120p' "${LOG_FILE}" >&2 || true
  kubectl -n "${NS}" get pods >&2 || true
  exit 1
}

up() {
  require_cmd docker
  require_cmd kubectl
  require_cmd helm
  require_cmd curl

  build_image
  load_image_for_cluster
  apply_local_dependencies
  apply_secret
  deploy_chart
  start_background_port_forward
  health_check

  echo "policy-server is available at http://127.0.0.1:${LOCAL_PORT}"
  echo "Port-forward PID: $(cat "${PID_FILE}")"
}

down() {
  require_cmd kubectl
  require_cmd helm
  stop_port_forward
  helm uninstall "${RELEASE}" -n "${NS}" >/dev/null 2>&1 || true
  kubectl -n "${NS}" delete -f "${LOCAL_DIR}/postgres.yaml" -f "${LOCAL_DIR}/redis.yaml" --ignore-not-found=true
  kubectl -n "${NS}" delete secret policy-server-secrets --ignore-not-found=true
  echo "Local policy-server release and dependencies removed from namespace '${NS}'."
}

status() {
  require_cmd kubectl
  kubectl -n "${NS}" get deploy,job,pod,svc,secret -l app.kubernetes.io/instance="${RELEASE}" || true
  kubectl -n "${NS}" get deploy/postgres deploy/redis svc/postgres svc/redis secret/policy-server-secrets || true
  if [[ -f "${PID_FILE}" ]]; then
    echo "port-forward pid: $(cat "${PID_FILE}")"
  fi
}

port_forward() {
  require_cmd kubectl
  exec kubectl -n "${NS}" port-forward "svc/${FULLNAME}-api" "${LOCAL_PORT}:80"
}

case "${CMD}" in
  up)
    up
    ;;
  down)
    down
    ;;
  status)
    status
    ;;
  port-forward)
    port_forward
    ;;
  help|-h|--help)
    usage
    ;;
  *)
    usage >&2
    exit 1
    ;;
esac
