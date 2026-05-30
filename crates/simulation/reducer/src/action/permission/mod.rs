//! `PermissionAction` — protocol-level authorization grants and revocations.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use simulation_state::primitives::{Address, ChainId};

/// Protocol-level permission actions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PermissionAction {
    /// Grant or revoke a manager/operator/relayer permission for a protocol.
    ProtocolAuthorization(ProtocolAuthorizationAction),
}

impl PermissionAction {
    /// The action's `serde` `action` tag.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::ProtocolAuthorization(_) => "protocol_authorization",
        }
    }

    /// Protocol family label, used by policy triggers as a venue-like name.
    #[must_use]
    pub fn venue_name(&self) -> Option<&str> {
        match self {
            Self::ProtocolAuthorization(a) => Some(a.protocol_name.as_str()),
        }
    }
}

/// Grant or revoke protocol-level authorization.
///
/// This covers permission primitives whose meaning is not a token allowance and
/// not specific to lending positions: Balancer Vault relayer approvals,
/// protocol manager/operator approvals, and similar account-control gates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ProtocolAuthorizationAction {
    /// Chain hosting the protocol.
    pub chain: ChainId,
    /// Protocol contract enforcing the permission.
    #[tsify(type = "string")]
    pub protocol: Address,
    /// Protocol family label, e.g. `balancer_v2`.
    pub protocol_name: String,
    /// Kind of permission being toggled.
    pub permission: ProtocolPermissionKind,
    /// Optional human-readable permission label from the protocol payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub permission_label: Option<String>,
    /// Optional protocol-native permission limit. Kept as a string because
    /// some off-chain venues use decimal percentages or non-EVM units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub permission_limit: Option<String>,
    /// Account granting/revoking permission, when explicit in calldata or
    /// typed data. Direct calls may omit this when the submitter is implicit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub authorizer: Option<Address>,
    /// Address being granted or revoked.
    #[tsify(type = "string")]
    pub authorized: Address,
    /// `true` = grant authorization, `false` = revoke.
    pub is_authorized: bool,
}

/// Protocol permission primitive kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolPermissionKind {
    /// Relayer may act on behalf of the authorizer for protocol-specific calls.
    Relayer,
    /// Manager may control an account/position.
    Manager,
    /// Operator may act across a protocol-specific scope.
    Operator,
    /// Agent/API wallet permission.
    Agent,
    /// Fee recipient/builder permission.
    BuilderFee,
    /// Delegation-style permission.
    Delegate,
}

impl ProtocolPermissionKind {
    /// Stable string used by Cedar contexts and UI summaries.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Relayer => "relayer",
            Self::Manager => "manager",
            Self::Operator => "operator",
            Self::Agent => "agent",
            Self::BuilderFee => "builder_fee",
            Self::Delegate => "delegate",
        }
    }
}
