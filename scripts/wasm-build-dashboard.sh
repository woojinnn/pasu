#!/usr/bin/env bash
# Build both WASM artifacts and copy them into browser-extension/dashboard/.
#
# Dashboard needs both:
#  - policy-engine-wasm  → Policy Test (evaluate_policy_rpc_json) verdicts
#                          rendered in-process, no network hop.
#  - policy-builder-wasm → Builder mode rule → Cedar text compilation,
#                          plus parse_cedar_json for Code → Builder roundtrip.
#
# Outputs to browser-extension/dashboard/src/wasm/ (TS glue the app imports)
# and browser-extension/dashboard/public/wasm/ (static .wasm binary Vite
# serves at runtime; wasm-bindgen glue uses import.meta.url to locate it).
set -euo pipefail

cd "$(dirname "$0")/.."

dest_src="browser-extension/dashboard/src/wasm"
dest_pub="browser-extension/dashboard/public/wasm"
mkdir -p "$dest_src" "$dest_pub"

if [ -d crates/policy-engine-wasm ]; then
  echo "==> wasm-pack build policy-engine-wasm (target=web, release)"
  wasm-pack build crates/policy-engine-wasm \
    --target web \
    --release \
    --out-dir pkg \
    --out-name policy_engine_wasm
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm.js              "$dest_src/"
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm.d.ts            "$dest_src/" 2>/dev/null || true
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm         "$dest_src/"
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm         "$dest_pub/"
  cp crates/policy-engine-wasm/pkg/policy_engine_wasm_bg.wasm.d.ts    "$dest_src/" 2>/dev/null || true
fi

if [ -d crates/policy-builder-wasm ]; then
  echo "==> wasm-pack build policy-builder-wasm (target=web, release)"
  wasm-pack build crates/policy-builder-wasm \
    --target web \
    --release \
    --out-dir pkg \
    --out-name policy_builder_wasm
  cp crates/policy-builder-wasm/pkg/policy_builder_wasm.js            "$dest_src/"
  cp crates/policy-builder-wasm/pkg/policy_builder_wasm.d.ts          "$dest_src/" 2>/dev/null || true
  cp crates/policy-builder-wasm/pkg/policy_builder_wasm_bg.wasm       "$dest_src/"
  cp crates/policy-builder-wasm/pkg/policy_builder_wasm_bg.wasm       "$dest_pub/"
  cp crates/policy-builder-wasm/pkg/policy_builder_wasm_bg.wasm.d.ts  "$dest_src/" 2>/dev/null || true
fi

echo "==> dashboard WASM artifacts ready at browser-extension/dashboard/{src,public}/wasm/"
