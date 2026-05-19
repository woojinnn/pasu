//! `multicall_recurse` strategy execution (spec §5.2.4, §4.4).
//!
//! The strategy recognises a multicall-shaped outer call and dispatches each
//! inner sub-call back through the host's resolver. Spec §5.1 enumerates
//! several `recurse_rule_id` values; the Phase 4 PoC implements only
//! `"self_array_bytes_last_arg"`, which covers Uniswap V3 NPM `multicall(bytes[])`
//! (selector `0xac9650d8`), SwapRouter02 multicall overloads, and Multicall3.
//! Other rule ids (`safe_multisend_packed`, Cat E aggregator executors) parse
//! through Phase 0 [`super::types`] but return [`MapperError::Internal`].
//!
//! Flow (mirrors §5.2 lines 514-525):
//!
//! 1. Match the bundle's `recurse_rule_id` against the supported set.
//! 2. Pull the last argument of `decoded` and verify it is `Array<Bytes>`
//!    (`self_array_bytes_last_arg` semantics — V3 NPM, Multicall3, SR02).
//! 3. For each inner `Bytes`:
//!    * Reject calldata shorter than 4 bytes (no selector).
//!    * Build a child `CallMatchKey { chain_id, to: ctx.to, selector }`.
//!    * Build a child `MapContext` via [`MapContext::child`], which bumps
//!      `depth` and stores the inner calldata in `parent_calldata`.
//!    * Reject when the child's depth exceeds `max_depth` (spec §5.1).
//!    * Dispatch through `ctx.resolver.resolve_child(...)` and collect the
//!      resulting envelopes.
//! 4. Flatten and return.
//!
//! `ctx.resolver` MUST be wired by the host. If it is `None`, the interpreter
//! cannot recurse — we surface a clear error so the caller knows the host
//! capability is missing rather than returning an empty vector.

use abi_resolver::{CallMatchKey, DecodedCall, DecodedValue};
use policy_engine::ActionEnvelope;

use crate::mapper::{MapContext, MapperError};

use super::types::EmitRule;

/// Recurse rule id supported by the Phase 4 PoC. Matches spec §5.1 BNF
/// (`RecurseRuleId := "self_array_bytes_last_arg" | ...`).
pub const RULE_ID_SELF_ARRAY_BYTES_LAST_ARG: &str = "self_array_bytes_last_arg";

/// Hard cap on the number of inner sub-calls a `multicall_recurse` outer call
/// may carry. Round 1 audit (P1) — an attacker-shaped `bytes[]` payload could
/// otherwise force the host resolver to fan out N child dispatches per outer
/// call, multiplying CPU/allocation cost. In practice legitimate Uniswap V3
/// NFPM and SwapRouter02 multicalls ship 2-8 inner steps; 64 leaves room for
/// `Multicall3` aggregation while still bounding work. Exceeding this cap
/// surfaces as a clear `MapperError::Internal` so the caller knows the gate
/// tripped (vs. e.g. a generic ABI-decode error).
pub const MAX_MULTICALL_CHILDREN: usize = 64;

/// Absolute upper bound on recursion depth, independent of any per-bundle
/// `max_depth`. Spec §5.1 caps `max_depth` at 5; this constant gives the
/// interpreter a defence-in-depth guard that holds even if a host wires up a
/// non-conformant `MapContext` (e.g. by re-entering `multicall::execute` from
/// a custom `ChildResolver` that does not honour `max_depth`). Treat the
/// inequality `child_ctx.depth > MAX_GLOBAL_DEPTH` as a hard fail-closed
/// signal — exceeding it always returns [`MapperError::Internal`] without
/// invoking the resolver, so an attacker-shaped cycle cannot grow CPU /
/// stack use linearly with the inner `bytes[]` array.
///
/// The value 5 matches the upper bound of spec §5.1 `max_depth ∈ 1..=5`,
/// with a safety margin handled per call by the per-bundle `max_depth`
/// (typically 2-3 for the Uniswap NFPM, SR02 and Multicall3 bundles in
/// `registry/manifests/`).
pub const MAX_GLOBAL_DEPTH: u8 = 5;

/// Execute a `multicall_recurse` rule against the given decoded outer call.
///
/// Returns the flattened envelopes emitted by the inner steps, or an error if
/// the rule shape is unsupported, the host resolver is missing, the recursion
/// would exceed `max_depth`, or any child dispatch fails.
pub fn execute(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
    rule: &EmitRule,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    let (recurse_rule_id, max_depth) = match rule {
        EmitRule::MulticallRecurse {
            recurse_rule_id,
            max_depth,
        } => (recurse_rule_id.as_str(), *max_depth),
        other => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "multicall::execute called with non-multicall_recurse rule: {other:?}"
            )));
        }
    };

    if recurse_rule_id != RULE_ID_SELF_ARRAY_BYTES_LAST_ARG {
        return Err(MapperError::Unsupported(format!(
            "multicall_recurse/{recurse_rule_id}"
        )));
    }

    // Defence in depth — even if a host wires a custom ChildResolver that
    // re-enters multicall::execute past the per-bundle `max_depth`, this gate
    // ensures recursion cannot cross `MAX_GLOBAL_DEPTH`. Spec §5.1 caps
    // `max_depth` at 5; we use the same value as the absolute hard ceiling.
    if ctx.depth >= MAX_GLOBAL_DEPTH {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "multicall_recurse rejected: ctx.depth {} >= MAX_GLOBAL_DEPTH {} \
             (absolute cycle guard, independent of per-bundle max_depth)",
            ctx.depth,
            MAX_GLOBAL_DEPTH
        )));
    }

    let resolver = ctx.resolver.ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "multicall_recurse requires ctx.resolver (host:resolver), but it is None — \
             host did not wire a ChildResolver"
        ))
    })?;

    let children = extract_self_array_bytes(decoded)?;

    if children.len() > MAX_MULTICALL_CHILDREN {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "multicall_recurse rejected: child count {} exceeds MAX_MULTICALL_CHILDREN ({})",
            children.len(),
            MAX_MULTICALL_CHILDREN
        )));
    }

    let mut envelopes = Vec::new();
    for (index, child_calldata) in children.iter().enumerate() {
        if child_calldata.len() < 4 {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "multicall_recurse child #{index} has calldata shorter than 4 bytes \
                 (len={})",
                child_calldata.len()
            )));
        }
        let mut selector = [0u8; 4];
        selector.copy_from_slice(&child_calldata[..4]);

        // Child to == parent to (self_array_bytes_last_arg = self-multicall).
        let child_key = CallMatchKey {
            chain_id: ctx.chain_id,
            to: ctx.to.clone(),
            selector,
        };

        // Build a child context with depth+1 and parent_calldata = inner bytes.
        let child_ctx = ctx.child(ctx.to, child_calldata);

        // Spec §5.1: `max_depth: 1..5`. Reject when the child's depth would
        // exceed the bundle's bound. `depth` is already incremented in
        // `MapContext::child`, so a comparison against `max_depth` directly
        // reflects "how many recursion levels are still allowed".
        if u32::from(child_ctx.depth) > u32::from(max_depth) {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "multicall_recurse exceeded max_depth: child depth {} > max_depth {}",
                child_ctx.depth,
                max_depth
            )));
        }

        let child_envelopes =
            resolver.resolve_child(&child_key, &child_ctx, child_calldata)?;
        envelopes.extend(child_envelopes);
    }

    Ok(envelopes)
}

/// Pull the inner `bytes[]` payload from the *last* argument of a decoded
/// outer call. Matches the structural assumption of
/// `self_array_bytes_last_arg`: the outer ABI ends in a `bytes[]` argument
/// whose elements are each calldata for the *same* contract (self-multicall).
///
/// This mirrors `abi_resolver::subdecode::recurse::extract_subcalls`, but
/// operates against `decoder::DecodedCall` (the mapper-side decoded view) so
/// the interpreter does not need to re-decode raw bytes.
fn extract_self_array_bytes(decoded: &DecodedCall) -> Result<Vec<Vec<u8>>, MapperError> {
    let last = decoded.args.last().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "multicall_recurse expected at least 1 argument, got 0"
        ))
    })?;

    let array_items = match &last.value {
        DecodedValue::Array(items) => items,
        other => {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "multicall_recurse last argument must be Array<Bytes>, got {other:?}"
            )));
        }
    };

    let mut out = Vec::with_capacity(array_items.len());
    for (index, item) in array_items.iter().enumerate() {
        match item {
            DecodedValue::Bytes(bytes) => out.push(bytes.clone()),
            other => {
                return Err(MapperError::Internal(anyhow::anyhow!(
                    "multicall_recurse child #{index} is not Bytes, got {other:?}"
                )));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::str::FromStr as _;
    use std::sync::Mutex;

    use abi_resolver::{DecodedArg, DecoderId};
    use policy_engine::action::dex::{SwapAction, SwapMode};
    use policy_engine::action::{
        Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
        AssetRefWithAmountConstraint, Category, DecimalString,
    };

    use crate::mapper::{ChildResolver, MapContext, MapperError};
    use crate::token_registry::EmptyTokenRegistry;

    use super::*;

    /// Captures every child the resolver is asked about and returns a
    /// pre-configured envelope per call. `inner_response` controls whether a
    /// given call returns envelopes or surfaces an error.
    struct RecordingResolver {
        calls: Mutex<Vec<RecordedCall>>,
        responses: Mutex<Vec<Result<Vec<ActionEnvelope>, MapperError>>>,
    }

    struct RecordedCall {
        key: CallMatchKey,
        calldata: Vec<u8>,
        depth: u8,
        had_parent: bool,
    }

    impl RecordingResolver {
        fn new(responses: Vec<Result<Vec<ActionEnvelope>, MapperError>>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                responses: Mutex::new(responses),
            }
        }

        fn calls(&self) -> std::sync::MutexGuard<'_, Vec<RecordedCall>> {
            self.calls.lock().unwrap()
        }
    }

    impl ChildResolver for RecordingResolver {
        fn resolve_child(
            &self,
            child: &CallMatchKey,
            ctx: &MapContext<'_>,
            child_calldata: &[u8],
        ) -> Result<Vec<ActionEnvelope>, MapperError> {
            self.calls.lock().unwrap().push(RecordedCall {
                key: child.clone(),
                calldata: child_calldata.to_vec(),
                depth: ctx.depth,
                had_parent: ctx.parent_calldata.is_some(),
            });
            let mut responses = self.responses.lock().unwrap();
            responses
                .pop()
                .unwrap_or_else(|| {
                    Err(MapperError::Internal(anyhow::anyhow!(
                        "RecordingResolver exhausted"
                    )))
                })
        }
    }

    fn fake_envelope(amount: &str) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(SwapAction {
                swap_mode: SwapMode::ExactIn,
                input_token: AssetRefWithAmountConstraint {
                    asset: AssetRef {
                        kind: AssetKind::Erc20,
                        address: Some(Address::from_str("0x1111111111111111111111111111111111111111").unwrap()),
                        token_id: None,
                        symbol: None,
                        decimals: None,
                    },
                    amount: AmountConstraint {
                        kind: AmountKind::Exact,
                        value: Some(DecimalString::from_str(amount).unwrap()),
                    },
                },
                output_token: AssetRefWithAmountConstraint {
                    asset: AssetRef {
                        kind: AssetKind::Erc20,
                        address: Some(Address::from_str("0x2222222222222222222222222222222222222222").unwrap()),
                        token_id: None,
                        symbol: None,
                        decimals: None,
                    },
                    amount: AmountConstraint {
                        kind: AmountKind::Min,
                        value: Some(DecimalString::from_str("0").unwrap()),
                    },
                },
                recipient: Address::from_str("0x3333333333333333333333333333333333333333").unwrap(),
                validity: None,
                fee_bps: None,
            }),
        }
    }

    fn multicall_decoded(items: Vec<DecodedValue>) -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new("declarative.uniswap-v3/nfpm-multicall"),
            function_signature: "multicall(bytes[])".into(),
            args: vec![DecodedArg {
                name: "data".into(),
                abi_type: "bytes[]".into(),
                value: DecodedValue::Array(items),
            }],
            nested: vec![],
        }
    }

    fn rule(depth: u8) -> EmitRule {
        EmitRule::MulticallRecurse {
            recurse_rule_id: RULE_ID_SELF_ARRAY_BYTES_LAST_ARG.into(),
            max_depth: depth,
        }
    }

    fn ctx_with_resolver<'a>(
        resolver: &'a dyn ChildResolver,
        registry: &'a EmptyTokenRegistry,
        from: &'a Address,
        to: &'a Address,
        value: &'a DecimalString,
        depth: u8,
    ) -> MapContext<'a> {
        MapContext {
            chain_id: 1,
            from,
            to,
            value_wei: value,
            block_timestamp: Some(1_700_000_000),
            token_registry: registry,
            parent_calldata: None,
            depth,
            resolver: Some(resolver),
        }
    }

    #[test]
    fn two_inner_calls_dispatch_to_resolver_and_flatten() {
        // Two inner bytes, each = 4-byte selector + 4 padding bytes.
        let inner_a: Vec<u8> = vec![0x12, 0x34, 0x56, 0x78, 0xaa, 0xaa, 0xaa, 0xaa];
        let inner_b: Vec<u8> = vec![0xde, 0xad, 0xbe, 0xef, 0xbb, 0xbb, 0xbb, 0xbb];
        let decoded = multicall_decoded(vec![
            DecodedValue::Bytes(inner_a.clone()),
            DecodedValue::Bytes(inner_b.clone()),
        ]);

        // responses are popped from the back — so the first child consumes
        // the LAST response. Stack two envelopes in reverse order.
        let resolver = RecordingResolver::new(vec![
            Ok(vec![fake_envelope("200")]),
            Ok(vec![fake_envelope("100")]),
        ]);

        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let envelopes = execute(&ctx, &decoded, &rule(3)).unwrap();
        assert_eq!(envelopes.len(), 2);

        let calls = resolver.calls();
        assert_eq!(calls.len(), 2);
        // Both child keys share the parent's chain_id + to address and a
        // selector pulled from the first 4 bytes of the inner calldata.
        assert_eq!(calls[0].key.chain_id, 1);
        assert_eq!(calls[0].key.to, to);
        assert_eq!(calls[0].key.selector, [0x12, 0x34, 0x56, 0x78]);
        assert_eq!(calls[0].calldata, inner_a);
        assert_eq!(calls[0].depth, 1, "child depth must be parent depth + 1");
        assert!(calls[0].had_parent, "child must have parent_calldata set");

        assert_eq!(calls[1].key.selector, [0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(calls[1].calldata, inner_b);
        assert_eq!(calls[1].depth, 1);
    }

    #[test]
    fn missing_resolver_errors() {
        let decoded = multicall_decoded(vec![DecodedValue::Bytes(vec![
            0x12, 0x34, 0x56, 0x78, 0xaa, 0xaa, 0xaa, 0xaa,
        ])]);

        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = MapContext {
            chain_id: 1,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: Some(1_700_000_000),
            token_registry: &registry,
            parent_calldata: None,
            depth: 0,
            resolver: None,
        };

        let err = execute(&ctx, &decoded, &rule(3)).unwrap_err();
        assert!(err.to_string().contains("requires ctx.resolver"));
    }

    #[test]
    fn unsupported_rule_id_errors() {
        let decoded = multicall_decoded(vec![]);
        let resolver = RecordingResolver::new(vec![]);
        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);
        let bad_rule = EmitRule::MulticallRecurse {
            recurse_rule_id: "safe_multisend_packed".into(),
            max_depth: 3,
        };

        let err = execute(&ctx, &decoded, &bad_rule).unwrap_err();
        // Round 4 audit — the rule mismatch now surfaces as
        // `MapperError::Unsupported("multicall_recurse/<rule_id>")` so the
        // orchestrator can classify it as a known limitation rather than a
        // runtime fault.
        assert!(
            matches!(&err, MapperError::Unsupported(s) if s == "multicall_recurse/safe_multisend_packed"),
            "expected Unsupported(\"multicall_recurse/safe_multisend_packed\"), got {err:?}"
        );
    }

    #[test]
    fn depth_check_blocks_recursion_at_max_depth() {
        // ctx.depth = 3, max_depth = 3 → child depth would be 4 → reject.
        let decoded = multicall_decoded(vec![DecodedValue::Bytes(vec![
            0x12, 0x34, 0x56, 0x78, 0xaa, 0xaa, 0xaa, 0xaa,
        ])]);
        let resolver = RecordingResolver::new(vec![Ok(vec![fake_envelope("1")])]);

        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 3);

        let err = execute(&ctx, &decoded, &rule(3)).unwrap_err();
        assert!(err.to_string().contains("exceeded max_depth"));
        // The resolver must not be invoked when the depth gate trips first.
        assert!(resolver.calls().is_empty());
    }

    #[test]
    fn depth_check_allows_recursion_below_max_depth() {
        // ctx.depth = 2, max_depth = 3 → child depth 3 ≤ 3 → allow.
        let decoded = multicall_decoded(vec![DecodedValue::Bytes(vec![
            0x12, 0x34, 0x56, 0x78, 0xaa, 0xaa, 0xaa, 0xaa,
        ])]);
        let resolver = RecordingResolver::new(vec![Ok(vec![fake_envelope("1")])]);

        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 2);

        let envelopes = execute(&ctx, &decoded, &rule(3)).unwrap();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(resolver.calls()[0].depth, 3);
    }

    #[test]
    fn calldata_shorter_than_four_bytes_errors() {
        let decoded = multicall_decoded(vec![DecodedValue::Bytes(vec![0x12, 0x34])]);
        let resolver = RecordingResolver::new(vec![]);

        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let err = execute(&ctx, &decoded, &rule(3)).unwrap_err();
        assert!(err.to_string().contains("shorter than 4 bytes"));
    }

    #[test]
    fn non_array_last_arg_errors() {
        let decoded = DecodedCall {
            decoder_id: DecoderId::new("declarative.uniswap-v3/nfpm-multicall"),
            function_signature: "multicall(uint256)".into(),
            args: vec![DecodedArg {
                name: "x".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(alloy_primitives::U256::from(1u8)),
            }],
            nested: vec![],
        };
        let resolver = RecordingResolver::new(vec![]);

        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let err = execute(&ctx, &decoded, &rule(3)).unwrap_err();
        assert!(err.to_string().contains("must be Array<Bytes>"));
    }

    #[test]
    fn empty_array_returns_empty_envelopes() {
        let decoded = multicall_decoded(vec![]);
        let resolver = RecordingResolver::new(vec![]);

        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let envelopes = execute(&ctx, &decoded, &rule(3)).unwrap();
        assert!(envelopes.is_empty());
        assert!(resolver.calls().is_empty());
    }

    /// Stub to silence the unused-import lint when `RefCell` isn't needed in
    /// this module — leftover from an earlier iteration.
    #[allow(dead_code)]
    fn _refcell_marker() {
        let _ = RefCell::new(0);
    }

    #[test]
    fn multicall_recurse_exceeds_max_global_depth_errors() {
        // Even if the per-bundle `max_depth` says recursion is allowed
        // (max_depth=255 here), the absolute MAX_GLOBAL_DEPTH guard MUST trip
        // when `ctx.depth >= MAX_GLOBAL_DEPTH`. The resolver must not be
        // invoked in that case.
        let decoded = multicall_decoded(vec![DecodedValue::Bytes(vec![
            0x12, 0x34, 0x56, 0x78, 0xaa, 0xaa, 0xaa, 0xaa,
        ])]);
        let resolver = RecordingResolver::new(vec![Ok(vec![fake_envelope("1")])]);

        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        // ctx.depth == MAX_GLOBAL_DEPTH (5) — the absolute guard must reject
        // even before we reach the per-bundle max_depth gate.
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, MAX_GLOBAL_DEPTH);

        // Use a huge per-bundle max_depth so we are NOT tripping the per-bundle
        // gate; the only thing that can reject this is the global guard.
        let rule_with_high_max = EmitRule::MulticallRecurse {
            recurse_rule_id: RULE_ID_SELF_ARRAY_BYTES_LAST_ARG.into(),
            max_depth: u8::MAX,
        };

        let err = execute(&ctx, &decoded, &rule_with_high_max).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("MAX_GLOBAL_DEPTH"),
            "expected MAX_GLOBAL_DEPTH error, got: {message}"
        );
        // The resolver MUST NOT have been invoked.
        assert!(
            resolver.calls().is_empty(),
            "resolver invoked after global guard trip"
        );
    }

    // -----------------------------------------------------------------
    // T-TEST-MULTICALL edge cases (Phase 7 T-B4 — WasmChildResolver +
    // MAX_GLOBAL_DEPTH cycle guard). Each test exercises a specific
    // adversarial / boundary shape the interpreter must handle without
    // panicking, allocating unbounded work, or silently dropping calls.
    // -----------------------------------------------------------------

    /// `outer multicall(bytes[]) → inner single call (no nested multicall)`.
    /// ctx.depth=0 (top level) and the single child resolves at depth=1 with
    /// a normal envelope. This is the happy-path boundary at the lowest
    /// recursion depth — proves the interpreter does not require any prior
    /// recursion frames to function. (#1)
    #[test]
    fn multicall_recurse_depth_0_succeeds() {
        let inner: Vec<u8> = vec![0xab, 0xcd, 0xef, 0x01, 0x42, 0x42, 0x42, 0x42];
        let decoded = multicall_decoded(vec![DecodedValue::Bytes(inner.clone())]);
        let resolver = RecordingResolver::new(vec![Ok(vec![fake_envelope("777")])]);

        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let envelopes = execute(&ctx, &decoded, &rule(3)).unwrap();
        assert_eq!(envelopes.len(), 1);
        let calls = resolver.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].depth, 1, "first-level child runs at depth 1");
        assert_eq!(calls[0].calldata, inner);
    }

    /// `ctx.depth == MAX_GLOBAL_DEPTH (5)` with `max_depth=u8::MAX` MUST trip
    /// the absolute guard before the resolver is invoked. Defence-in-depth
    /// even if a malformed bundle were to bypass the per-bundle max_depth
    /// gate. (#2)
    #[test]
    fn multicall_recurse_depth_5_at_global_cap_errors() {
        let decoded = multicall_decoded(vec![DecodedValue::Bytes(vec![
            0xde, 0xad, 0xbe, 0xef, 0x00, 0x00, 0x00, 0x00,
        ])]);
        let resolver = RecordingResolver::new(vec![Ok(vec![fake_envelope("1")])]);
        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, MAX_GLOBAL_DEPTH);
        let high_rule = EmitRule::MulticallRecurse {
            recurse_rule_id: RULE_ID_SELF_ARRAY_BYTES_LAST_ARG.into(),
            max_depth: u8::MAX,
        };

        let err = execute(&ctx, &decoded, &high_rule).unwrap_err();
        assert!(err.to_string().contains("MAX_GLOBAL_DEPTH"));
        assert!(resolver.calls().is_empty(), "resolver invoked past global cap");
    }

    /// Cycle: A.multicall calls B.multicall calls A.multicall ... A custom
    /// `ChildResolver` re-enters `multicall::execute` for the same shape, so
    /// each recursion strictly increments `ctx.depth`. The MAX_GLOBAL_DEPTH
    /// cap amortises the cycle — the chain MUST terminate (with an error)
    /// rather than infinite-loop. We rely on `depth` monotonicity; address
    /// identity is irrelevant. (#3)
    #[test]
    fn multicall_recurse_cycle_a_b_a_detected() {
        // Resolver that, on every call, re-invokes `multicall::execute` with
        // the SAME shape so depth grows by 1 per hop. The outer test starts
        // at depth 0 — the chain MUST terminate at MAX_GLOBAL_DEPTH (5).
        struct CyclingResolver {
            hops: std::sync::atomic::AtomicUsize,
        }
        impl ChildResolver for CyclingResolver {
            fn resolve_child(
                &self,
                _child: &CallMatchKey,
                ctx: &MapContext<'_>,
                _child_calldata: &[u8],
            ) -> Result<Vec<ActionEnvelope>, MapperError> {
                self.hops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                // Re-enter with the same bytes — depth from `ctx` is already
                // incremented. Use the same self-shaped multicall payload.
                let decoded = multicall_decoded(vec![DecodedValue::Bytes(vec![
                    0x11, 0x22, 0x33, 0x44, 0xff, 0xff, 0xff, 0xff,
                ])]);
                let rule_inner = EmitRule::MulticallRecurse {
                    recurse_rule_id: RULE_ID_SELF_ARRAY_BYTES_LAST_ARG.into(),
                    max_depth: u8::MAX,
                };
                execute(ctx, &decoded, &rule_inner)
            }
        }

        let resolver = CyclingResolver {
            hops: std::sync::atomic::AtomicUsize::new(0),
        };
        let decoded = multicall_decoded(vec![DecodedValue::Bytes(vec![
            0x11, 0x22, 0x33, 0x44, 0xff, 0xff, 0xff, 0xff,
        ])]);
        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to_a = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = MapContext {
            chain_id: 1,
            from: &from,
            to: &to_a,
            value_wei: &value,
            block_timestamp: None,
            token_registry: &registry,
            parent_calldata: None,
            depth: 0,
            resolver: Some(&resolver),
        };
        let outer_rule = EmitRule::MulticallRecurse {
            recurse_rule_id: RULE_ID_SELF_ARRAY_BYTES_LAST_ARG.into(),
            max_depth: u8::MAX,
        };

        let err = execute(&ctx, &decoded, &outer_rule).unwrap_err();
        assert!(
            err.to_string().contains("MAX_GLOBAL_DEPTH"),
            "cycle MUST be cut by MAX_GLOBAL_DEPTH, got: {err}"
        );
        // 0 → 1 → 2 → 3 → 4 → reject at depth 5. Five hops total.
        let hops = resolver.hops.load(std::sync::atomic::Ordering::SeqCst);
        assert!(hops <= MAX_GLOBAL_DEPTH as usize, "hops {hops} > cap");
    }

    /// `children = []` (empty `bytes[]`) is a degenerate but valid outer
    /// multicall — the interpreter MUST return 0 envelopes without invoking
    /// the resolver. (#4)
    #[test]
    fn multicall_recurse_empty_bytes_array_yields_no_envelopes() {
        let decoded = multicall_decoded(vec![]);
        let resolver = RecordingResolver::new(vec![]);
        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let envelopes = execute(&ctx, &decoded, &rule(3)).unwrap();
        assert!(envelopes.is_empty());
        assert!(resolver.calls().is_empty(), "no calls on empty children");
    }

    /// Child calldata < 4 bytes can carry no selector. The interpreter MUST
    /// reject before invoking the resolver — surfaced as `MapperError::Internal`
    /// with a "shorter than 4 bytes" message. (#5)
    #[test]
    fn multicall_recurse_malformed_selector_in_child_errors() {
        let decoded = multicall_decoded(vec![DecodedValue::Bytes(vec![0xab, 0xcd, 0xef])]);
        let resolver = RecordingResolver::new(vec![]);
        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let err = execute(&ctx, &decoded, &rule(3)).unwrap_err();
        assert!(matches!(&err, MapperError::Internal(_)));
        assert!(err.to_string().contains("shorter than 4 bytes"));
        assert!(resolver.calls().is_empty());
    }

    /// Child (chain, to, selector) not present in the bridge. The
    /// `WasmChildResolver` decision (T-B4) surfaces this as a
    /// `MapperError::Internal("WasmChildResolver: no declarative mapper
    /// bridged for ...")`. The interpreter MUST propagate this verbatim
    /// instead of silently dropping the child. (#6)
    #[test]
    fn multicall_recurse_unknown_child_key_returns_empty() {
        let decoded = multicall_decoded(vec![DecodedValue::Bytes(vec![
            0xca, 0xfe, 0xba, 0xbe, 0x00, 0x00, 0x00, 0x00,
        ])]);
        // Resolver mirrors WasmChildResolver's "no bridged mapper" decision:
        // returns Err, NOT an empty Ok vec. The interpreter MUST surface it.
        let resolver = RecordingResolver::new(vec![Err(MapperError::Internal(anyhow::anyhow!(
            "WasmChildResolver: no declarative mapper bridged for chain_id=1 to=... selector=0xcafebabe"
        )))]);
        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let err = execute(&ctx, &decoded, &rule(3)).unwrap_err();
        assert!(err.to_string().contains("no declarative mapper bridged"));
        // The resolver WAS consulted (this is a "bridge miss", not a guard
        // trip) — confirms the interpreter delegates lookup to the host.
        assert_eq!(resolver.calls().len(), 1);
    }

    /// Mixed children `[valid_mint, malformed_3byte, valid_burn]`. The
    /// interpreter processes children in order and fails fast on the first
    /// malformed entry — the second `valid_burn` is NEVER dispatched. (#7)
    #[test]
    fn multicall_recurse_mixed_valid_invalid_children() {
        let valid_mint = vec![0x88, 0x31, 0x64, 0x5d, 0xaa, 0xaa, 0xaa, 0xaa];
        let malformed = vec![0x12, 0x34]; // < 4 bytes
        let valid_burn = vec![0xfc, 0x6f, 0x78, 0x65, 0xbb, 0xbb, 0xbb, 0xbb];
        let decoded = multicall_decoded(vec![
            DecodedValue::Bytes(valid_mint.clone()),
            DecodedValue::Bytes(malformed.clone()),
            DecodedValue::Bytes(valid_burn.clone()),
        ]);
        // Responses popped from the back — first call consumes the LAST.
        let resolver = RecordingResolver::new(vec![Ok(vec![fake_envelope("99")])]);
        let registry = EmptyTokenRegistry;
        let from = Address::from_str("0x000000000000000000000000000000000000aaaa").unwrap();
        let to = Address::from_str("0xC36442b4a4522E871399CD717aBDD847Ab11FE88").unwrap();
        let value = DecimalString::from_str("0").unwrap();
        let ctx = ctx_with_resolver(&resolver, &registry, &from, &to, &value, 0);

        let err = execute(&ctx, &decoded, &rule(3)).unwrap_err();
        assert!(err.to_string().contains("shorter than 4 bytes"));
        // 1st child dispatched; 2nd fails before resolver invoked; 3rd never reached.
        let calls = resolver.calls();
        assert_eq!(calls.len(), 1, "only the first valid child reached resolver");
        assert_eq!(calls[0].calldata, valid_mint);
    }
}
