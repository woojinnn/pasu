#!/usr/bin/env bash
# End-to-end smoke test for Plan 1.
#   1. Build the sample adapter to WASM
#   2. Spin up the mock registry
#   3. Publish via adapter-cli
#   4. Verify /chains/{1}/{usdc_address} resolves
set -euo pipefail

CRATE_DIR=crates/adapter-samples/erc20-transfer
USDC_ADDR=0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48
PORT="${PORT:-18080}"
REGISTRY="http://127.0.0.1:${PORT}"

cleanup() {
    if [[ -n "${SERVER_PID:-}" ]]; then
        kill "${SERVER_PID}" 2>/dev/null || true
        wait "${SERVER_PID}" 2>/dev/null || true
    fi
    rm -rf "${TMP:-}"
}
trap cleanup EXIT

echo "=> Ensuring wasm32-unknown-unknown target"
rustup target add wasm32-unknown-unknown >/dev/null

TMP=$(mktemp -d)
echo "=> Building sample WASM"
cargo build -p adapter-sample-erc20-transfer --target wasm32-unknown-unknown --release

WASM_RAW="$(pwd)/target/wasm32-unknown-unknown/release/adapter_sample_erc20_transfer.wasm"
WASM_OPT="${WASM_RAW%.wasm}.opt.wasm"

if which wasm-opt >/dev/null 2>&1 && which wasm-tools >/dev/null 2>&1; then
    echo "=> Optimising WASM (wasm-opt -Oz, wasm-tools strip)"
    wasm-opt -Oz "$WASM_RAW" -o "$WASM_OPT"
    # Strip non-essential custom sections but keep `adapter_manifest`
    # (consumed by adapter-cli validate / publish).
    wasm-tools strip -d '^(name|producers|target_features)$' "$WASM_OPT" -o "$WASM_OPT"
    WASM_FINAL="$WASM_OPT"
else
    echo "=> Skipping wasm-opt/wasm-tools (not installed)"
    WASM_FINAL="$WASM_RAW"
fi

BUDGET_BYTES=$((182 * 1024))
ACTUAL=$(wc -c < "$WASM_FINAL" | tr -d ' ')
if (( ACTUAL > BUDGET_BYTES )); then
    echo "FAIL: optimised wasm ${ACTUAL} bytes exceeds budget ${BUDGET_BYTES}" >&2
    exit 1
fi
echo "WASM size OK: ${ACTUAL} bytes (budget ${BUDGET_BYTES})"

WASM="$WASM_FINAL"
test -f "$WASM"

echo "=> Building adapter-cli + registry-mock"
cargo build -p adapter-cli -p registry-mock --release

echo "=> Starting registry-mock on :${PORT}"
REGISTRY_STATE="${TMP}/state" REGISTRY_BIND="127.0.0.1:${PORT}" \
    target/release/registry-mock &
SERVER_PID=$!
ready=0
for i in $(seq 1 20); do
    if curl -sf "${REGISTRY}/healthz" >/dev/null 2>&1; then ready=1; break; fi
    sleep 0.5
done
if [[ "$ready" -ne 1 ]]; then
    echo "FAIL: registry never became healthy at ${REGISTRY}/healthz" >&2
    exit 1
fi

echo "=> Validating"
target/release/adapter-cli validate "${WASM}"

echo "=> Publishing"
target/release/adapter-cli publish "${WASM}" --registry "${REGISTRY}"

echo "=> Resolving (chainId=1, USDC)"
RESP=$(curl -sf "${REGISTRY}/chains/1/${USDC_ADDR}")
echo "${RESP}"
echo "${RESP}" | grep -q '"name":"erc20-transfer"'
echo "${RESP}" | grep -q '"version":"0.1.0"'

echo "=> SUCCESS"
