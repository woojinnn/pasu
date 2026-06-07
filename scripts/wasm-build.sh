#!/usr/bin/env bash
# Build the policy-engine WASM artifact + copy into browser-extension/ for static webpack import.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ ! -d crates/policy-engine-wasm ]; then
  echo "skip: crates/policy-engine-wasm/ not yet present (Plan 2 not landed)"
  exit 0
fi

# CI dedupe hook: when SKIP_WASM_BUILD=1 and a prebuilt pkg/ already exists
# (e.g. downloaded as a workflow artifact), skip the expensive wasm-pack build
# and reuse it. The copy step below still runs so consumers get their artifacts.
# A developer running this directly (flag unset) always gets a fresh build.
if [ "${SKIP_WASM_BUILD:-}" = "1" ] && [ -f crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm ]; then
  echo "==> SKIP_WASM_BUILD=1 and prebuilt pkg/ found — reusing wasm-pack output"
else
  echo "==> wasm-pack build (target=web, release)"
  wasm-pack build crates/policy-engine-wasm \
    --target web \
    --release \
    --out-dir pkg \
    --out-name policy_engine_wasm
fi

if [ -d browser-extension ]; then
  mkdir -p browser-extension/backend/wasm browser-extension/public/wasm
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm.js browser-extension/backend/wasm/
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm.d.ts browser-extension/backend/wasm/ 2>/dev/null || true
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm browser-extension/backend/wasm/
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm browser-extension/public/wasm/
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm.d.ts browser-extension/backend/wasm/ 2>/dev/null || true
  echo "==> wasm artifact copied to browser-extension/backend/wasm/ + public/wasm/"
fi
