#!/usr/bin/env bash
# Enable the repo's tracked git hooks (one-time, per clone).
# The pre-commit hook runs gitleaks over staged changes to block secret commits.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
git config core.hooksPath .githooks
echo "Enabled repo git hooks (core.hooksPath=.githooks)."

if ! command -v gitleaks >/dev/null 2>&1; then
  echo "Tip: install gitleaks so the pre-commit secret scan runs locally:"
  echo "       brew install gitleaks      # macOS"
  echo "     (CI scans every PR regardless.)"
fi
