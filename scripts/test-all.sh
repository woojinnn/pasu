#!/usr/bin/env bash
# Run the full test suite — Rust workspace + extension when present.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> cargo test --workspace"
cargo test --workspace --all-targets

echo "==> cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> cargo fmt --all -- --check"
cargo fmt --all -- --check

if [ -d extension ] && [ -f extension/package.json ]; then
  echo "==> yarn typecheck (extension)"
  (cd extension && yarn typecheck)

  if grep -q '"test"' extension/package.json; then
    echo "==> yarn test (extension)"
    (cd extension && yarn test --run 2>/dev/null || yarn test)
  fi

  echo "==> yarn build:chrome (extension)"
  (cd extension && yarn build:chrome >/dev/null)
fi

echo "==> all checks passed"
