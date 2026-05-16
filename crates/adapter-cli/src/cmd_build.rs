use std::path::Path;
use std::process::Command;

pub fn run(crate_path: &Path, profile: &str) -> anyhow::Result<()> {
    let manifest = crate_path.join("Cargo.toml");
    anyhow::ensure!(
        manifest.exists(),
        "no Cargo.toml at {}",
        manifest.display()
    );

    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--target")
        .arg("wasm32-unknown-unknown");
    if profile == "release" {
        cmd.arg("--release");
    }
    let status = cmd.status()?;
    anyhow::ensure!(status.success(), "cargo build failed");

    let crate_name = read_crate_name(&manifest)?.replace('-', "_");
    let wasm_path = crate_path
        .join("target/wasm32-unknown-unknown")
        .join(profile)
        .join(format!("{crate_name}.wasm"));
    let resolved = if wasm_path.exists() {
        wasm_path
    } else {
        find_workspace_target(crate_path)?
            .join("wasm32-unknown-unknown")
            .join(profile)
            .join(format!("{crate_name}.wasm"))
    };
    anyhow::ensure!(
        resolved.exists(),
        "expected wasm at {} (built but missing)",
        resolved.display()
    );

    println!("built: {}", resolved.display());
    Ok(())
}

fn read_crate_name(manifest: &Path) -> anyhow::Result<String> {
    let content = std::fs::read_to_string(manifest)?;
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("name") {
            if let Some(eq) = rest.find('=') {
                let value = rest[eq + 1..].trim().trim_matches('"');
                if !value.is_empty() {
                    return Ok(value.to_string());
                }
            }
        }
    }
    anyhow::bail!("could not find `name = \"...\"` in {}", manifest.display())
}

fn find_workspace_target(start: &Path) -> anyhow::Result<std::path::PathBuf> {
    let mut cur = start.canonicalize()?;
    loop {
        let target = cur.join("target");
        if target.exists() {
            return Ok(target);
        }
        if !cur.pop() {
            anyhow::bail!("no target/ directory found upwards from {}", start.display());
        }
    }
}
