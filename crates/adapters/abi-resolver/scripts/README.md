# abi-resolver build scripts

Two data sources back the resolver:

1. **`data/sourcify.json`** — small curated bundle (committed to the repo).
   ~15 mainnet major contracts (DEX routers, Aave, Morpho, ENS, stablecoins,
   Permit2, …). Includes EIP-1967 / ZOS / UUPS proxy resolution so things
   like USDC and Aave Pool surface their real implementation ABI.

2. **`sourcify.sqlite`** — full Sourcify mainnet dump (NOT committed; ~9 GB).
   Built from the official Parquet export at `export.sourcify.dev`. Provides
   wide-coverage fallback to ~1.4M verified mainnet contracts (~27M function
   rows).

## One-shot build

```bash
./crates/abi-resolver/scripts/build_all.sh
```

Runs the full pipeline end-to-end:

1. Set up Python venv (`/tmp/parquet_venv`)
2. Download Sourcify Parquet dump (~24 GB, ~30 min)
3. Extract mainnet mapping (~5 min)
4. Build SQLite DB (~30 min, ~9 GB)
5. Drop the parquet cache (mapping + venv kept for re-runs)

Total: about an hour on a 100 Mbps line. The artifact ends up at
`/tmp/sourcify_dump/sourcify.sqlite`.

Flags:

| Flag       | Effect                                                  |
|------------|---------------------------------------------------------|
| `--force`  | rebuild even if `sourcify.sqlite` already exists        |
| `--purge`  | also drop the venv + mainnet_mapping.parquet at the end |
| `--help`   | print the embedded usage block                          |

Env knobs (override defaults):

| Variable          | Default               |
|-------------------|-----------------------|
| `PASU_DUMP_DIR`   | `/tmp/sourcify_dump`  |
| `PASU_VENV`       | `/tmp/parquet_venv`   |

If the script is interrupted, just re-run it — the parallel downloader skips
files that already exist, so partial progress is preserved.

## Curated bundle (small)

```bash
./crates/abi-resolver/scripts/curate_bundle.sh
```

Pulls Sourcify metadata for every entry in the script's `ENTRIES` array,
resolves proxies via RPC, and writes `data/sourcify.json`.

## Running the resolver against the SQLite dump

```bash
SOURCIFY_SQLITE_PATH=/tmp/sourcify_dump/sourcify.sqlite \
  cargo run -p adapter-debug-dashboard
```

If the env var is unset, the server defaults to looking at
`/tmp/sourcify_dump/sourcify.sqlite`. If neither path exists, the resolver
falls back to the curated bundle + openchain seeds only.

## Internal scripts (called by `build_all.sh`)

- `extract_mapping.py` — joins `contract_deployments` × `verified_contracts`
  to produce `(address, compilation_id)` for every mainnet (chain=1) contract.
  Output: `mainnet_mapping.parquet`.
- `build_db.py` — streams the 304 `compiled_contracts` parquet files,
  filters by the mainnet mapping, expands each ABI into one row per function,
  caps `MAX_FANOUT` at 5000 (factory clones share an ABI; first 5000 are
  enough), and writes directly to disk SQLite with journaling off.
