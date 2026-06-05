//!   1. <https://repo.sourcify.dev/contracts/full_match/{chain}/{addr}/metadata.json>

use crate::error::SyncError;

#[allow(dead_code, clippy::unused_async)]
pub async fn fetch_function_output_type(
    _chain_id: u64,
    _contract: alloy_primitives::Address,
    _function_name: &str,
) -> Result<alloy_dyn_abi::DynSolType, SyncError> {
    Err(SyncError::FetchFailed {
        source_id: "sourcify".into(),
        reason: "Sourcify auto-fetch not implemented yet — use AbiTypeRegistry::with_builtins() / register()".into(),
    })
}
