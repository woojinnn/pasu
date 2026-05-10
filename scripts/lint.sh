#!/usr/bin/env bash
# Fix-all-the-things: cargo fmt + clippy + yarn lint where applicable.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> cargo fmt --all"
cargo fmt --all

echo "==> cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged"
cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged

if [ -f extension/package.json ]; then
  (cd extension && yarn lint || true)
fi

echo "==> done"
