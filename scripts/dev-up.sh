#!/usr/bin/env bash
# Bring the dev container up, building if needed, and drop the user into a shell.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! docker compose ps --status running --services | grep -q '^dev$'; then
  echo "==> building + starting dev container"
  docker compose up -d --build dev
fi

echo "==> attaching shell to scopeball-dev"
exec docker compose exec dev bash -l
