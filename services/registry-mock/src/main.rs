//! Binary entry point — delegates to lib.
use registry_mock::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run().await
}
