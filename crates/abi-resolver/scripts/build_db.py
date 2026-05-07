"""Build the SQLite functions DB — fast variant.

Tweaks vs first attempt:
- synchronous = OFF, wal_autocheckpoint = 200 (truncate frequently)
- temp_store = MEMORY, large cache
- commit + WAL checkpoint every ~50k rows (instead of every file)
- progress log every batch when batch contained inserts
"""
import os, glob, time, json, sqlite3, sys
from collections import defaultdict
import pyarrow.parquet as pq
import pandas as pd
from eth_utils import keccak

START = time.time()
os.chdir('/tmp/sourcify_dump')

DB_PATH = 'sourcify.sqlite'
COMMIT_EVERY = 50_000
LOG_EVERY = 200_000

print(f"[{time.time()-START:6.1f}s] loading mainnet mapping", flush=True)
mapping = pd.read_parquet('mainnet_mapping.parquet')
print(f"[{time.time()-START:6.1f}s]   addresses: {len(mapping):,}", flush=True)
compid_to_addrs = defaultdict(list)
for row in mapping.itertuples(index=False):
    compid_to_addrs[row.compilation_id].append(row.address_hex)
target_compids = set(compid_to_addrs.keys())
print(f"[{time.time()-START:6.1f}s]   unique compilation_ids: {len(target_compids):,}", flush=True)

print(f"[{time.time()-START:6.1f}s] preparing SQLite at {DB_PATH}", flush=True)
if os.path.exists(DB_PATH):
    os.remove(DB_PATH)
for ext in ('-wal', '-shm'):
    p = DB_PATH + ext
    if os.path.exists(p):
        os.remove(p)
conn = sqlite3.connect(DB_PATH)
conn.executescript("""
PRAGMA journal_mode = WAL;
PRAGMA synchronous = OFF;
PRAGMA wal_autocheckpoint = 200;
PRAGMA cache_size = -200000;
PRAGMA temp_store = MEMORY;
CREATE TABLE functions (
    chain_id  INTEGER NOT NULL,
    address   BLOB    NOT NULL,
    selector  BLOB    NOT NULL,
    name      TEXT    NOT NULL,
    signature TEXT    NOT NULL,
    abi_json  TEXT    NOT NULL,
    PRIMARY KEY (chain_id, address, selector)
) WITHOUT ROWID;
""")

def canonical_signature(fn):
    def fmt(p):
        if p.get('components'):
            inner = ','.join(fmt(c) for c in p['components'])
            base = f"({inner})"
            t = p.get('type', 'tuple')
            if t.startswith('tuple'):
                suffix = t[len('tuple'):]
                return base + suffix
            return base
        return p['type']
    return f"{fn['name']}({','.join(fmt(p) for p in fn.get('inputs', []))})"

def selector_for(sig: str) -> bytes:
    return keccak(text=sig)[:4]

print(f"[{time.time()-START:6.1f}s] processing compiled_contracts files", flush=True)
files = sorted(glob.glob('compiled/compiled_contracts_*.parquet'))
print(f"[{time.time()-START:6.1f}s]   files to process: {len(files)}", flush=True)

total_inserted = 0
last_log_at = 0
buffer = []

for idx, f in enumerate(files, 1):
    pf = pq.ParquetFile(f)
    schema_names = [s.name for s in pf.schema_arrow]
    if 'id' not in schema_names or 'compilation_artifacts' not in schema_names:
        continue

    for batch in pf.iter_batches(batch_size=20000, columns=['id', 'compilation_artifacts']):
        df = batch.to_pandas()
        m = df[df['id'].isin(target_compids)]
        if len(m) == 0:
            continue
        for row in m.itertuples(index=False):
            comp_id = row.id
            artifacts_str = row.compilation_artifacts
            if not artifacts_str:
                continue
            try:
                artifacts = json.loads(artifacts_str) if isinstance(artifacts_str, str) else artifacts_str
            except json.JSONDecodeError:
                continue
            abi = artifacts.get('abi') if isinstance(artifacts, dict) else None
            if not abi:
                continue
            for fn in abi:
                if fn.get('type') != 'function':
                    continue
                try:
                    sig = canonical_signature(fn)
                    sel = selector_for(sig)
                except Exception:
                    continue
                fn_json = json.dumps(fn, separators=(',', ':'))
                fn_name = fn.get('name', '')
                for addr_hex in compid_to_addrs[comp_id]:
                    addr_bytes = bytes.fromhex(addr_hex[2:])
                    buffer.append((1, addr_bytes, sel, fn_name, sig, fn_json))

        if len(buffer) >= COMMIT_EVERY:
            conn.executemany(
                "INSERT OR REPLACE INTO functions (chain_id, address, selector, name, signature, abi_json) VALUES (?, ?, ?, ?, ?, ?)",
                buffer,
            )
            conn.commit()
            total_inserted += len(buffer)
            buffer.clear()
            if total_inserted - last_log_at >= LOG_EVERY:
                last_log_at = total_inserted
                print(
                    f"[{time.time()-START:6.1f}s]   file {idx}/{len(files)}, "
                    f"{total_inserted:,} rows committed",
                    flush=True,
                )
                conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")

# flush remainder
if buffer:
    conn.executemany(
        "INSERT OR REPLACE INTO functions (chain_id, address, selector, name, signature, abi_json) VALUES (?, ?, ?, ?, ?, ?)",
        buffer,
    )
    conn.commit()
    total_inserted += len(buffer)
    buffer.clear()

conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
n = conn.execute("SELECT COUNT(*) FROM functions").fetchone()[0]
size_mb = os.path.getsize(DB_PATH) / 1024 / 1024
print(f"[{time.time()-START:6.1f}s] DONE — {n:,} function rows, DB {size_mb:.1f} MB", flush=True)
conn.close()
