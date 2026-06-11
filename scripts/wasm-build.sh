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
  echo "==> wasm-pack build (target=web, release, opt-level=z)"
  # opt-level=z (size-optimal codegen) ONLY for this wasm build — scoped via env
  # var so native [profile.release] (server) keeps opt-level 3. Measured on the
  # 2026-06-11 tree: 13.06 MiB -> 7.57 MiB (-42%). wasm-opt -Oz alone cannot
  # recover this; the win is at rustc codegen. panic=abort measured ~0 on
  # wasm32-unknown-unknown (already abort-style) — deliberately not set.
  CARGO_PROFILE_RELEASE_OPT_LEVEL=z \
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
