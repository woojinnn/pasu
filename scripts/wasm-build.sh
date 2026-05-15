#!/usr/bin/env bash
# Build the policy-engine WASM artifact + copy into extension/ for static webpack import.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ ! -d crates/policy-engine-wasm ]; then
  echo "skip: crates/policy-engine-wasm/ not yet present (Plan 2 not landed)"
  exit 0
fi

echo "==> wasm-pack build (target=web, release)"
wasm-pack build crates/policy-engine-wasm \
  --target web \
  --release \
  --out-dir pkg \
  --out-name policy_engine_wasm

if [ -d extension ]; then
  mkdir -p extension/src/wasm extension/public/wasm
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm.js extension/src/wasm/
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm.d.ts extension/src/wasm/ 2>/dev/null || true
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm extension/src/wasm/
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm extension/public/wasm/
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm.d.ts extension/src/wasm/ 2>/dev/null || true
  echo "==> wasm artifact copied to extension/{src,public}/wasm/"
fi
