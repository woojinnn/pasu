//! adapter-cli — build, validate, and publish adapter packages.

mod cmd_build;
mod cmd_validate;
mod manifest;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "adapter-cli", version, about = "Adapter authoring toolchain")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build the adapter crate at the given path to wasm32-unknown-unknown.
    Build {
        #[arg(long, default_value = ".")]
        manifest_path: PathBuf,
        #[arg(long, default_value = "release")]
        profile: String,
    },
    Validate {
        /// Path to the built .wasm file.
        wasm: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Build { manifest_path, profile } => cmd_build::run(&manifest_path, &profile),
        Cmd::Validate { wasm } => cmd_validate::run(&wasm),
    }
}
