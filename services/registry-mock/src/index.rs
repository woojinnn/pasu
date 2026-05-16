//! (chainId, address) → (package_name, version) index.
//! v0 mock: only `applies_to` matchers resolve. Proxy/factory matchers are
//! accepted at publish time but not yet resolvable.

use crate::manifest::Manifest;
use crate::storage::Storage;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Default)]
pub struct Index {
    inner: RwLock<HashMap<(u64, [u8; 20]), (String, String)>>,
}

impl Index {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, manifest: &Manifest) {
        let mut g = self.inner.write().unwrap();
        for a in &manifest.applies_to {
            g.insert(
                (a.chain, a.address.0),
                (manifest.name.clone(), manifest.version.clone()),
            );
        }
    }

    pub fn lookup(&self, chain: u64, addr: [u8; 20]) -> Option<(String, String)> {
        self.inner.read().unwrap().get(&(chain, addr)).cloned()
    }

    pub async fn rebuild_from_disk(&self, storage: &Storage) -> anyhow::Result<()> {
        let packages_dir = storage.package_root("").parent().unwrap().to_path_buf();
        let mut read = match tokio::fs::read_dir(&packages_dir).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = read.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(latest) = storage.latest_version(&name).await? {
                if let Ok(m) = storage.read_manifest(&name, &latest).await {
                    self.add(&m);
                }
            }
        }
        Ok(())
    }
}
