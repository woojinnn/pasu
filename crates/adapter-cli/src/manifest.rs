use adapter_sdk::manifest::{Manifest, CUSTOM_SECTION_NAME};
use std::path::Path;
use wasmparser::Parser;

pub fn extract_manifest(wasm_path: &Path) -> anyhow::Result<Manifest> {
    let bytes = std::fs::read(wasm_path)?;
    for payload in Parser::new(0).parse_all(&bytes) {
        let payload = payload?;
        if let wasmparser::Payload::CustomSection(s) = payload {
            if s.name() == CUSTOM_SECTION_NAME {
                let m: Manifest = serde_json::from_slice(s.data())?;
                return Ok(m);
            }
        }
    }
    anyhow::bail!(
        "no `{}` custom section in {} — was the crate built with #[adapter]?",
        CUSTOM_SECTION_NAME,
        wasm_path.display()
    );
}
