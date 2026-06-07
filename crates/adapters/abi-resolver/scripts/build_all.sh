#!/usr/bin/env bash
# One-shot DB builder.
#
# Pipeline (idempotent on the artifact, not on the steps):
#   0. If $DB_PATH already exists, exit unless --force.
#   1. Create the Python venv at $VENV (skipped if already there).
#   2. Enumerate URLs for the three Sourcify Parquet tables.
#   3. Parallel download (~24 GB, ~30 min on a 100 Mbps line).
#   4. Extract mainnet (chain=1) mapping (~5 min).
#   5. Build SQLite DB (~30 min, ~9 GB).
#   6. Drop the parquet cache (mapping kept for re-runs; pass --purge to drop too).
#
# Output:
#   $DB_PATH  → sourcify.sqlite, ready for the resolver to attach.
#
# Env knobs:
#   PASU_DUMP_DIR  default /tmp/sourcify_dump
#   PASU_VENV      default /tmp/parquet_venv
#
# Flags:
#   --force   rebuild even if $DB_PATH exists
#   --purge   also drop venv + mapping after the build

set -euo pipefail

# Resolve to the script's parent directory so this works regardless of CWD.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"

DUMP_DIR="${PASU_DUMP_DIR:-/tmp/sourcify_dump}"
VENV="${PASU_VENV:-/tmp/parquet_venv}"
DB_PATH="$DUMP_DIR/sourcify.sqlite"

FORCE=0
PURGE=0
for arg in "$@"; do
  case "$arg" in
    --force) FORCE=1 ;;
    --purge) PURGE=1 ;;
    -h|--help)
      sed -n '2,/^set -e/p' "$0" | sed 's/^# \{0,1\}//' | sed '$d'
      exit 0 ;;
    *) echo "unknown flag: $arg" >&2; exit 2 ;;
  esac
done

step() { printf '\n\033[1;36m[step %s]\033[0m %s\n' "$1" "$2"; }
note() { printf '  %s\n' "$1"; }

# ---------------------------------------------------------------------------
# 0. Bail out fast if the DB is already there.
# ---------------------------------------------------------------------------
if [ -f "$DB_PATH" ] && [ "$FORCE" -eq 0 ]; then
  size=$(du -sh "$DB_PATH" | cut -f1)
  echo "DB already exists at $DB_PATH ($size)"
  echo "Pass --force to rebuild from scratch."
  exit 0
fi

mkdir -p "$DUMP_DIR"/contract_deployments \
         "$DUMP_DIR"/verified_contracts \
         "$DUMP_DIR"/compiled_contracts

# ---------------------------------------------------------------------------
# 1. Python venv (~200 MB, one-time setup)
# ---------------------------------------------------------------------------
step 1 "Python venv at $VENV"
if [ ! -x "$VENV/bin/python3" ]; then
  python3 -m venv "$VENV"
  # shellcheck disable=SC1091
  source "$VENV/bin/activate"
  pip install --quiet --upgrade pip
  pip install --quiet pyarrow pandas 'eth-hash[pycryptodome]' eth-utils
  note "venv ready"
else
  # shellcheck disable=SC1091
  source "$VENV/bin/activate"
  note "reusing existing venv"
fi

# ---------------------------------------------------------------------------
# 2. URL enumeration
# ---------------------------------------------------------------------------
step 2 "Enumerating Sourcify dump URLs"
cd "$DUMP_DIR"
for prefix in contract_deployments verified_contracts compiled_contracts; do
  curl -s "https://export.sourcify.dev/?prefix=v2/${prefix}/" \
    | python3 -c "import sys, re; print('\n'.join(f'https://export.sourcify.dev/{k}' for k in re.findall(r'<Key>([^<]+)</Key>', sys.stdin.read())))" \
    > "${prefix}_urls.txt"
  note "  $prefix: $(wc -l < "${prefix}_urls.txt") files"
done
cat contract_deployments_urls.txt verified_contracts_urls.txt compiled_contracts_urls.txt > all_urls.txt
note "total: $(wc -l < all_urls.txt) files (~24 GB)"

# ---------------------------------------------------------------------------
# 3. Parallel download
# ---------------------------------------------------------------------------
step 3 "Downloading parquet files (×8 parallel, ~30 min)"
xargs -n1 -P8 -I{} bash -c '
  url="$1"
  rel="${url#https://export.sourcify.dev/v2/}"
  out="$0/${rel}"
  if [ -f "$out" ] && [ -s "$out" ]; then exit 0; fi
  curl -sL --create-dirs -o "$out" "$url"
' "$DUMP_DIR" {} < all_urls.txt

dep=$(ls contract_deployments/*.parquet 2>/dev/null | wc -l | tr -d ' ')
ver=$(ls verified_contracts/*.parquet  2>/dev/null | wc -l | tr -d ' ')
comp=$(ls compiled_contracts/*.parquet 2>/dev/null | wc -l | tr -d ' ')
note "contract_deployments: $dep"
note "verified_contracts:   $ver"
note "compiled_contracts:   $comp"

# ---------------------------------------------------------------------------
# 4. Mainnet mapping extraction
# ---------------------------------------------------------------------------
step 4 "Extracting mainnet mapping"
python3 -u "$HERE/extract_mapping.py"

# ---------------------------------------------------------------------------
# 5. SQLite build
# ---------------------------------------------------------------------------
step 5 "Building SQLite DB"
python3 -u "$HERE/build_db.py"

# ---------------------------------------------------------------------------
# 6. Cleanup
# ---------------------------------------------------------------------------
step 6 "Cleaning up build cache"
rm -rf "$DUMP_DIR/contract_deployments" \
       "$DUMP_DIR/verified_contracts" \
       "$DUMP_DIR/compiled_contracts"
rm -f "$DUMP_DIR"/*_urls.txt "$DUMP_DIR/all_urls.txt"
note "removed parquet dumps + url lists"

if [ "$PURGE" -eq 1 ]; then
  rm -f "$DUMP_DIR/mainnet_mapping.parquet"
  rm -rf "$VENV"
  note "purged mapping + venv (--purge)"
else
  note "kept mainnet_mapping.parquet (rerun with --purge to drop)"
  note "kept venv at $VENV (rerun with --purge to drop)"
fi

# ---------------------------------------------------------------------------
# Final report
# ---------------------------------------------------------------------------
db_size=$(du -sh "$DB_PATH" | cut -f1)
remaining=$(du -sh "$DUMP_DIR" 2>/dev/null | cut -f1)
echo
printf '\033[1;32mDONE\033[0m\n'
echo "  DB:           $DB_PATH ($db_size)"
echo "  Dump dir now: $remaining"
echo
echo "Start the server:"
echo "  WEB_SERVER_ADDR=127.0.0.1:8080 cargo run -p adapter-debug-dashboard"
