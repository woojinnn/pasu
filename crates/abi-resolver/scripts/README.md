# abi-resolver build scripts

Two data sources back the resolver:

1. **`data/sourcify.json`** — small curated bundle (committed to the repo).
   ~15 mainnet major contracts (DEX routers, Aave, Morpho, ENS, stablecoins,
   Permit2, …). Includes EIP-1967 / ZOS / UUPS proxy resolution so things
   like USDC and Aave Pool surface their real implementation ABI.

2. **`sourcify.sqlite`** — full Sourcify mainnet dump (NOT committed; ~3 GB).
   Built from the official Parquet export at `export.sourcify.dev`. Provides
   wide-coverage fallback to ~800k verified mainnet contracts.

## Building the curated bundle

```bash
./crates/abi-resolver/scripts/curate_bundle.sh
```

Pulls Sourcify metadata for every entry in the script's `ENTRIES` array,
resolves proxies via RPC, and writes `data/sourcify.json`.

## Building the full SQLite dump

Two-stage Python pipeline. Needs `pyarrow`, `pandas`, and
`eth-hash[pycryptodome]` in a venv.

```bash
# 0. set up venv (one-time)
python3 -m venv /tmp/parquet_venv
source /tmp/parquet_venv/bin/activate
pip install pyarrow pandas 'eth-hash[pycryptodome]' eth-utils

# 1. download dumps from export.sourcify.dev (~24 GB, ~30 min)
mkdir -p /tmp/sourcify_dump/{deployments,verified,compiled}
cd /tmp/sourcify_dump

curl -s "https://export.sourcify.dev/?prefix=v2/contract_deployments/" \
  | grep -oE '<Key>v2/contract_deployments/[^<]+' | sed 's/<Key>//' \
  | xargs -n1 -P4 -I{} curl -sL "https://export.sourcify.dev/{}" -o "deployments/$(basename {})"

curl -s "https://export.sourcify.dev/?prefix=v2/verified_contracts/" \
  | grep -oE '<Key>v2/verified_contracts/[^<]+' | sed 's/<Key>//' \
  | xargs -n1 -P4 -I{} curl -sL "https://export.sourcify.dev/{}" -o "verified/$(basename {})"

curl -s "https://export.sourcify.dev/?prefix=v2/compiled_contracts/" \
  | grep -oE '<Key>v2/compiled_contracts/[^<]+' | sed 's/<Key>//' \
  | xargs -n1 -P8 -I{} curl -sL "https://export.sourcify.dev/{}" -o "compiled/$(basename {})"

# 2. extract mainnet (chain=1) join mapping (~5 min)
python3 -u extract_mapping.py

# 3. build the SQLite DB (~20 min, ~3 GB)
python3 -u build_db.py

# Final artifact:
#   /tmp/sourcify_dump/sourcify.sqlite
```

## Running the resolver against the SQLite dump

```bash
SOURCIFY_SQLITE_PATH=/tmp/sourcify_dump/sourcify.sqlite \
  cargo run -p web-server
```

If the env var is unset, the server defaults to looking at
`/tmp/sourcify_dump/sourcify.sqlite`. If neither path exists, the resolver
falls back to the curated bundle + openchain seeds only.
