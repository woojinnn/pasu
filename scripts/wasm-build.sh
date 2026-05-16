#!/usr/bin/env bash
# Build the policy-engine WASM artifact + copy into browser-extension/ for static webpack import.
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

if [ -d browser-extension ]; then
  mkdir -p browser-extension/src/wasm browser-extension/public/wasm
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm.js browser-extension/src/wasm/
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm.d.ts browser-extension/src/wasm/ 2>/dev/null || true
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm browser-extension/src/wasm/
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm browser-extension/public/wasm/
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm.d.ts browser-extension/src/wasm/ 2>/dev/null || true
  echo "==> wasm artifact copied to browser-extension/{src,public}/wasm/"
fi
