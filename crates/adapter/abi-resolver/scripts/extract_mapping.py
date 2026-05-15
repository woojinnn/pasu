"""Extract mainnet (chain=1) (address → compilation_id) mapping from
contract_deployments + verified_contracts dumps."""
import os, glob, time
import pyarrow.parquet as pq
import pandas as pd

START = time.time()
os.chdir('/tmp/sourcify_dump')

print(f"[{time.time()-START:6.1f}s] Step 1: extract mainnet from contract_deployments")
mainnet_deployments = []
for f in sorted(glob.glob('contract_deployments/*.parquet')):
    pf = pq.ParquetFile(f)
    for batch in pf.iter_batches(batch_size=200000, columns=['id', 'chain_id', 'address']):
        df = batch.to_pandas()
        m = df[df['chain_id'] == 1]
        if len(m) > 0:
            mainnet_deployments.append(m[['id', 'address']])
mainnet_dep = pd.concat(mainnet_deployments, ignore_index=True)
print(f"[{time.time()-START:6.1f}s]   mainnet deployments: {len(mainnet_dep):,}")
mainnet_dep['address_hex'] = mainnet_dep['address'].apply(lambda b: '0x' + b.hex())
mainnet_dep_id_set = set(mainnet_dep['id'])

print(f"[{time.time()-START:6.1f}s] Step 2: filter verified_contracts by mainnet deployment_id")
verified_rows = []
for f in sorted(glob.glob('verified_contracts/*.parquet')):
    pf = pq.ParquetFile(f)
    for batch in pf.iter_batches(batch_size=200000, columns=['deployment_id', 'compilation_id']):
        df = batch.to_pandas()
        m = df[df['deployment_id'].isin(mainnet_dep_id_set)]
        if len(m) > 0:
            verified_rows.append(m)
verified_mainnet = pd.concat(verified_rows, ignore_index=True)
print(f"[{time.time()-START:6.1f}s]   mainnet verified rows: {len(verified_mainnet):,}")

print(f"[{time.time()-START:6.1f}s] Step 3: join")
merged = verified_mainnet.merge(
    mainnet_dep[['id', 'address_hex']].rename(columns={'id': 'deployment_id'}),
    on='deployment_id'
)
print(f"[{time.time()-START:6.1f}s]   merged rows: {len(merged):,}")
print(f"[{time.time()-START:6.1f}s]   unique addresses: {merged['address_hex'].nunique():,}")
print(f"[{time.time()-START:6.1f}s]   unique compilation_ids: {merged['compilation_id'].nunique():,}")

merged[['address_hex', 'compilation_id']].to_parquet('mainnet_mapping.parquet')
print(f"[{time.time()-START:6.1f}s] saved mainnet_mapping.parquet "
      f"({os.path.getsize('mainnet_mapping.parquet') / 1024 / 1024:.1f} MB)")
print(f"[{time.time()-START:6.1f}s] DONE")
