//! Mapper trait — DecodedCall → ActionEnvelope[].

use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MapperId(pub String);

impl MapperId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapperMatchKey {
    pub decoder_id: abi_resolver::DecoderId,
}

/// `MapContext` carries everything an `EmitRule` strategy needs to evaluate a
/// `ValueExpr` against the current `DecodedCall`.
///
/// Spec §5.4 introduces a logical `HostHandles` grouping (`token_metadata`,
/// `resolver`). Phase 4 keeps the existing flat fields for backward
/// compatibility with the V2 / V3 / SR02 / ERC20 / WETH static mappers and
/// adds two `Option`-typed handles needed by `multicall_recurse`:
///
///   * [`MapContext::parent_calldata`] — the calldata of the outer call when
///     the interpreter is recursing into a child step (None at the top level).
///   * [`MapContext::resolver`] — a `ChildResolver` the interpreter consults
///     for `multicall_recurse` to dispatch each `ChildCall` to its mapper.
///   * [`MapContext::depth`] — the current recursion depth (0 at top level;
///     incremented when entering a child step). `multicall_recurse` checks
///     this against `max_depth` to bound recursion (spec §5.1, §5.2).
///
/// Static mappers ignore the new fields, so existing call sites continue to
/// work after appending three trailing struct fields — see the regression
/// tests in `protocols::{uniswap_v2, uniswap_v3, swap_router_02, erc20,
/// weth}`.
pub struct MapContext<'a> {
    pub chain_id: u64,
    pub from: &'a policy_engine::action::Address,
    pub to: &'a policy_engine::action::Address,
    pub value_wei: &'a policy_engine::action::DecimalString,
    pub block_timestamp: Option<u64>,
    pub token_registry: &'a dyn crate::token_registry::TokenRegistry,
    /// Parent calldata — populated by `multicall_recurse` when recursing into
    /// a child step. None at top level.
    pub parent_calldata: Option<&'a [u8]>,
    /// Recursion depth — 0 at the top level, incremented by one when entering
    /// a child step from `multicall_recurse`. Bounded by the bundle's
    /// `max_depth` per spec §5.1.
    pub depth: u8,
    /// Resolver used by `multicall_recurse` to dispatch a child `CallMatchKey`
    /// to its mapper. None when the host has not wired multicall support.
    pub resolver: Option<&'a dyn ChildResolver>,
}

impl<'a> MapContext<'a> {
    /// Construct a `MapContext` with the recurse-related fields at their
    /// defaults (`parent_calldata: None`, `depth: 0`, `resolver: None`).
    ///
    /// Static mappers that pre-date Phase 4 (`MapContext { ... }` struct
    /// literal) continue to work via this constructor — call sites can switch
    /// to `MapContext::new(...)` to avoid having to spell out the new fields
    /// when they don't need them.
    #[must_use]
    pub fn new(
        chain_id: u64,
        from: &'a policy_engine::action::Address,
        to: &'a policy_engine::action::Address,
        value_wei: &'a policy_engine::action::DecimalString,
        block_timestamp: Option<u64>,
        token_registry: &'a dyn crate::token_registry::TokenRegistry,
    ) -> Self {
        Self {
            chain_id,
            from,
            to,
            value_wei,
            block_timestamp,
            token_registry,
            parent_calldata: None,
            depth: 0,
            resolver: None,
        }
    }

    /// Construct a child context for `multicall_recurse` recursion.
    ///
    /// Returns a new `MapContext` borrowing the same registry/handles but with
    /// `parent_calldata = Some(parent)`, `depth = self.depth + 1`, and the
    /// child's `to` address updated. The chain id and token registry are
    /// inherited unchanged.
    #[must_use]
    pub fn child(
        &self,
        child_to: &'a policy_engine::action::Address,
        parent_calldata: &'a [u8],
    ) -> Self {
        Self {
            chain_id: self.chain_id,
            from: self.from,
            to: child_to,
            value_wei: self.value_wei,
            block_timestamp: self.block_timestamp,
            token_registry: self.token_registry,
            parent_calldata: Some(parent_calldata),
            depth: self.depth.saturating_add(1),
            resolver: self.resolver,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MapperError {
    #[error("missing argument {0}")]
    MissingArgument(String),
    #[error("unexpected argument type for {name}: {message}")]
    ArgumentMismatch { name: String, message: String },
    /// Round 4 audit — surfaced when the interpreter encounters a strategy
    /// or feature that the current Phase intentionally does not implement.
    /// Distinct from [`Self::Internal`] so the orchestrator can classify
    /// it as a known limitation (audit reason: "unsupported") rather than
    /// a runtime fault. Carries a human-readable label such as
    /// `"enum_tagged_dispatch"` or `"single_emit/dex/add_liquidity"`.
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}

pub trait Mapper: Send + Sync {
    fn id(&self) -> MapperId;
    fn accepts(&self, decoded: &abi_resolver::DecodedCall) -> bool;
    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &abi_resolver::DecodedCall,
    ) -> Result<Vec<policy_engine::ActionEnvelope>, MapperError>;
}

pub trait MapperRegistry: Send + Sync {
    fn resolve(&self, key: &MapperMatchKey) -> Option<Arc<dyn Mapper>>;
}

/// Resolver used by `multicall_recurse` to dispatch a child step.
///
/// Spec §5.4 places this on `HostHandles.resolver`. Concretely, the host
/// provides an impl that — given a child `(chain_id, to, selector)` — decodes
/// the child calldata and runs the matched mapper, returning the resulting
/// envelopes. The interpreter is intentionally agnostic to how the host
/// implements this (e.g. via `CallAdapterRegistry`, a WASM lookup, or a test
/// stub).
pub trait ChildResolver: Send + Sync {
    /// Resolve and execute a child call.
    ///
    /// * `child` — match key of the child (chain id, to address, 4-byte
    ///   selector).
    /// * `ctx` — the *child* `MapContext`, with `depth` already incremented
    ///   and `parent_calldata` set to the outer call's payload.
    /// * `child_calldata` — raw inner calldata pulled out by the host's
    ///   recursion rule (see `abi_resolver::subdecode::recurse::extract_children`).
    ///
    /// Implementations should return one or more envelopes for the child
    /// step. Returning an empty vector is allowed (e.g. for steps the host
    /// chooses to ignore) but most resolvers will surface an error instead.
    fn resolve_child(
        &self,
        child: &abi_resolver::CallMatchKey,
        ctx: &MapContext<'_>,
        child_calldata: &[u8],
    ) -> Result<Vec<policy_engine::ActionEnvelope>, MapperError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mapper_match_key_serde_roundtrip() {
        let value = json!({
            "decoderId": "uniswap-v2/swap",
        });

        let key: MapperMatchKey = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            key.decoder_id,
            abi_resolver::DecoderId::new("uniswap-v2/swap")
        );

        assert_eq!(serde_json::to_value(key).unwrap(), value);
    }
}
