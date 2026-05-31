//! Manifest-driven policy RPC planning and context materialization.

mod manifest;
mod manifest_v2;
mod materialize;
mod materialize_v2;
mod planning_v2;
mod selector;
mod trigger;

pub use manifest::{
    manifest_set_hash, validate_manifests, ContextProjection, PolicyManifest, PolicyRpcCall,
    PolicyRpcError, PolicyRpcErrorBody, PolicyRpcResponse, PolicyRpcResult, ProjectionType,
    Requirement, RequirementWhen, RootInput,
};
pub use manifest_v2::{
    CustomContext, ManifestV2, PolicyRpcCallSpec, Trigger, TriggerConstraint, TriggerField,
    TriggerScope, MANIFEST_V2_SCHEMA_VERSION,
};
pub use materialize::{system_fail_verdict, SYSTEM_POLICY_ID};
pub use materialize_v2::materialize_v2;
pub use planning_v2::{plan_policy_rpc_v2, PlannedCallV2};
pub use selector::resolve_selector;
pub use trigger::{evaluate as evaluate_trigger, TxView};
