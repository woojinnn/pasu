use crate::manifest::extract_manifest;
use adapter_sdk::manifest::Capability;
use std::collections::BTreeSet;
use std::path::Path;
use wasmparser::Parser;

pub fn run(wasm: &Path) -> anyhow::Result<()> {
    let m = extract_manifest(wasm)?;
    m.validate()?;

    let bytes = std::fs::read(wasm)?;
    let mut exports = BTreeSet::new();
    for payload in Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ExportSection(s) = payload? {
            for e in s {
                exports.insert(e?.name.to_string());
            }
        }
    }
    let required_per_cap = |c: Capability| -> &'static [&'static str] {
        match c {
            Capability::Decoder => &["decode_call", "manifest_json"],
            Capability::CallAdapter => &["map_to_action"],
            Capability::SignAdapter => &["decode_sign"],
        }
    };
    for cap in &m.capabilities {
        for sym in required_per_cap(*cap) {
            anyhow::ensure!(
                exports.contains(*sym),
                "capability {:?} missing required export `{}`",
                cap,
                sym
            );
        }
    }
    println!("manifest valid; exports complete");
    println!("  name:    {}", m.name);
    println!("  version: {}", m.version);
    println!("  caps:    {:?}", m.capabilities);
    Ok(())
}
