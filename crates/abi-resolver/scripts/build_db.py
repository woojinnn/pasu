"""Build SQLite functions DB from compiled_contracts parquet files.

Strategy:
- In-memory SQLite while inserting (no WAL pressure), backup to disk at end.
- Skip ABI artifacts > 5 MB (rare; usually generated/proxy-rewritten and
  blocking the previous attempts around the 8.8M-row mark).
- Progress logged every 1% of files.

Schema must match abi-resolver's `sqlite_index.rs`:

    CREATE TABLE functions (
        chain_id  INTEGER NOT NULL,
        address   BLOB    NOT NULL,
        selector  BLOB    NOT NULL,
        name      TEXT    NOT NULL,
        signature TEXT    NOT NULL,
        abi_json  TEXT    NOT NULL,
        PRIMARY KEY (chain_id, address, selector)
    ) WITHOUT ROWID;

Run from /tmp/sourcify_dump (where the Parquet dump + mainnet_mapping live):

    python3 -u build_db.py
"""
import os, glob, time, json, sqlite3
from collections import defaultdict
import pyarrow.parquet as pq
import pandas as pd
from eth_utils import keccak

START = time.time()
DUMP_DIR = '/tmp/sourcify_dump'
DISK_PATH = f'{DUMP_DIR}/sourcify.sqlite'

COMMIT_EVERY = 50_000
ABI_BYTES_LIMIT = 5_000_000   # skip artifacts > 5 MB (auto-generated, slow)
MAX_FANOUT = 5_000            # cap addresses per compilation_id (factory clones)


def fmt_t():
    return f"[{time.time() - START:7.1f}s]"


def canonical_signature(fn):
    def fmt(p):
        if p.get('components'):
            inner = ','.join(fmt(c) for c in p['components'])
            base = f"({inner})"
            t = p.get('type', 'tuple')
            if t.startswith('tuple'):
                return base + t[len('tuple'):]
            return base
        return p['type']
    return f"{fn['name']}({','.join(fmt(p) for p in fn.get('inputs', []))})"


def selector_for(sig):
    return keccak(text=sig)[:4]


def main():
    os.chdir(DUMP_DIR)

    print(f"{fmt_t()} loading mainnet mapping", flush=True)
    mapping = pd.read_parquet('mainnet_mapping.parquet')
    print(f"{fmt_t()}   addresses: {len(mapping):,}", flush=True)

    compid_to_addrs = defaultdict(list)
    for row in mapping.itertuples(index=False):
        compid_to_addrs[row.compilation_id].append(row.address_hex)
    target_compids = set(compid_to_addrs.keys())
    print(f"{fmt_t()}   unique compilation_ids: {len(target_compids):,}", flush=True)

    for ext in ('', '-wal', '-shm'):
        p = DISK_PATH + ext
        if os.path.exists(p):
            os.remove(p)

    # disk-backed (in-memory ate 24 GB RAM + 30 GB swap on the previous attempt
    # and the backup phase ground to a halt). All durability options off — this
    # is a one-shot build, not a server.
    conn = sqlite3.connect(DISK_PATH)
    conn.executescript("""
    PRAGMA journal_mode = OFF;
    PRAGMA synchronous = OFF;
    PRAGMA temp_store = MEMORY;
    PRAGMA cache_size = -200000;
    PRAGMA locking_mode = EXCLUSIVE;
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
    # One big transaction: commit after each batch is wrapped in BEGIN/COMMIT
    # via conn.commit(). With journal_mode=OFF rollback isn't available, but
    # a crash just means re-run.
    conn.execute("BEGIN")
    INSERT_SQL = ("INSERT OR REPLACE INTO functions "
                  "(chain_id, address, selector, name, signature, abi_json) "
                  "VALUES (?, ?, ?, ?, ?, ?)")

    print(f"{fmt_t()} processing compiled_contracts files (ABI cap {ABI_BYTES_LIMIT // 1_000_000} MB)", flush=True)
    files = sorted(glob.glob('compiled_contracts/compiled_contracts_*.parquet'))
    total_files = len(files)
    print(f"{fmt_t()}   files: {total_files}", flush=True)

    total_inserted = 0
    total_huge = 0
    last_pct = -1
    buffer = []

    for idx, f in enumerate(files, 1):
        pf = pq.ParquetFile(f)
        schema_names = [s.name for s in pf.schema_arrow]
        if 'id' not in schema_names or 'compilation_artifacts' not in schema_names:
            continue

        file_huge = 0
        for batch in pf.iter_batches(batch_size=20_000, columns=['id', 'compilation_artifacts']):
            df = batch.to_pandas()
            m = df[df['id'].isin(target_compids)]
            if len(m) == 0:
                continue
            for row in m.itertuples(index=False):
                comp_id = row.id
                artifacts_str = row.compilation_artifacts
                if not artifacts_str:
                    continue
                if isinstance(artifacts_str, str) and len(artifacts_str) > ABI_BYTES_LIMIT:
                    file_huge += 1
                    continue
                try:
                    artifacts = (json.loads(artifacts_str)
                                 if isinstance(artifacts_str, str)
                                 else artifacts_str)
                except json.JSONDecodeError:
                    continue
                abi = artifacts.get('abi') if isinstance(artifacts, dict) else None
                if not abi:
                    continue
                # Cap fanout: a few compilation_ids map to 1M+ addresses
                # (Uniswap V2 Pair clone, ERC-1167 factories, etc.) Inserting
                # all of them would queue 100M+ rows in one go. We take only
                # the first MAX_FANOUT — clones share ABI by definition, so
                # the truncated set still surfaces the same functions.
                addrs = compid_to_addrs[comp_id]
                if len(addrs) > MAX_FANOUT:
                    addrs = addrs[:MAX_FANOUT]

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
                    for addr_hex in addrs:
                        addr_bytes = bytes.fromhex(addr_hex[2:])
                        buffer.append((1, addr_bytes, sel, fn_name, sig, fn_json))
                        # Inner commit so a single (compilation × ABI) doesn't
                        # balloon the buffer unboundedly even with the fanout cap.
                        if len(buffer) >= COMMIT_EVERY:
                            conn.executemany(INSERT_SQL, buffer)
                            total_inserted += len(buffer)
                            buffer.clear()
                            # Wrap up tx and start a new one so memory is released
                            # back to the OS rather than accumulating in the
                            # journal cache.
                            if total_inserted % 1_000_000 < COMMIT_EVERY:
                                conn.execute("COMMIT")
                                conn.execute("BEGIN")

            if len(buffer) >= COMMIT_EVERY:
                conn.executemany(INSERT_SQL, buffer)
                total_inserted += len(buffer)
                buffer.clear()

        total_huge += file_huge
        pct = int(idx * 100 / total_files)
        if pct != last_pct:
            last_pct = pct
            extra = f", huge_skipped={file_huge}" if file_huge else ""
            print(
                f"{fmt_t()} [{pct:3d}%]  file {idx}/{total_files}, "
                f"{total_inserted:,} rows in memory{extra}",
                flush=True,
            )

    if buffer:
        conn.executemany(INSERT_SQL, buffer)
        total_inserted += len(buffer)
        buffer.clear()

    conn.execute("COMMIT")
    print(f"{fmt_t()} insert phase done — {total_inserted:,} rows, "
          f"{total_huge} huge ABIs skipped", flush=True)

    # Verify count + close (no backup needed, we wrote directly to disk).
    n = conn.execute("SELECT COUNT(*) FROM functions").fetchone()[0]
    print(f"{fmt_t()} verified row count: {n:,}", flush=True)
    conn.close()

    size_mb = os.path.getsize(DISK_PATH) / 1024 / 1024
    print(f"{fmt_t()} DONE — {size_mb:.0f} MB at {DISK_PATH}", flush=True)


if __name__ == '__main__':
    main()
