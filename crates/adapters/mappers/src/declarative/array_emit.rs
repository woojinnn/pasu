//! `array_emit` strategy execution (Phase 7B).
//!
//! Fans one ABI tuple-array argument out into N `ActionEnvelope`s — one per
//! element. Each element reuses the `single_emit` field-tree → envelope
//! builder, with a synthetic `element` arg (and optional `parallel_paths`
//! arrays) inserted into the args object so existing `$.args.<name>[...]`
//! `JsonPath` resolution applies unchanged.
//!
//! Covers Permit2 `permit(PermitBatch)` (`0x2a2d80d1`),
//! `transferFrom(AllowanceTransferDetails[])` (`0x0d58b1db`),
//! `permitTransferFrom`/`permitWitnessTransferFrom` batch
//! (`0xedd9444b` / `0xfe8ec1a7`) — emitting one envelope per granted token /
//! transferred token keeps every token-level intent visible to Cedar.
//!
//! Design — **no synthetic root.** The current array element is inserted into
//! the outer `args` object under the key `element` (and each parallel row
//! under its `parallel_paths` key), so manifest paths read `$.args.element[0]`
//! and the unmodified `eval::walk_args` walker resolves them. `eval.rs` needs
//! zero changes: `array_path` / `parallel_paths` are evaluated through the
//! existing public `eval::evaluate` with a synthetic `ValueExpr::FromArg`.

use std::collections::BTreeMap;

use abi_resolver::DecodedCall;
use policy_engine::ActionEnvelope;

use crate::mapper::{MapContext, MapperError};

use super::eval::{args_to_json, evaluate};
use super::single_emit;
use super::types::ValueExpr;

/// Defence-in-depth ceiling regardless of bundle `max_elements`.
/// Mirrors [`super::multicall::MAX_MULTICALL_CHILDREN`] (= 64).
pub const MAX_ARRAY_ELEMENTS: usize = 64;

/// Resolve a `$.args.*` (or `$.tx.*` / `$.context.*`) `JsonPath` to a JSON
/// value by reusing the public [`evaluate`] entry point with a synthetic
/// `FromArg` expression. Keeps `eval.rs` free of any `array_emit`-specific
/// surface (plan §4.1 (c)).
fn resolve_path(
    ctx: &MapContext<'_>,
    args_json: &serde_json::Value,
    path: &str,
) -> Result<serde_json::Value, MapperError> {
    evaluate(
        ctx,
        args_json,
        &ValueExpr::FromArg {
            from: path.to_owned(),
            via: None,
            kind: None,
        },
    )
}

/// Execute an `array_emit` rule against the given decoded call.
///
/// The bundle declares one `array_path` tuple-array argument; this fans it
/// out into one `ActionEnvelope` per element, reusing the `single_emit`
/// `(category, action)` builder for each. Optional `parallel_paths` arrays
/// are index-synchronised with the primary array.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
    category: &str,
    action: &str,
    array_path: &str,
    max_elements: u8,
    parallel_paths: &BTreeMap<String, String>,
    fields: &BTreeMap<String, ValueExpr>,
) -> Result<Vec<ActionEnvelope>, MapperError> {
    let args_json = args_to_json(decoded);

    // Resolve the primary array.
    let array_value = resolve_path(ctx, &args_json, array_path)?;
    let elements = array_value.as_array().ok_or_else(|| {
        MapperError::Internal(anyhow::anyhow!(
            "array_emit: array_path {array_path:?} is not an array"
        ))
    })?;

    // Caps: global ceiling first, then the per-bundle declared limit.
    if elements.len() > MAX_ARRAY_ELEMENTS {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "array_emit: {} elements exceeds MAX_ARRAY_ELEMENTS {MAX_ARRAY_ELEMENTS}",
            elements.len()
        )));
    }
    if elements.len() > usize::from(max_elements) {
        return Err(MapperError::Internal(anyhow::anyhow!(
            "array_emit: {} elements exceeds bundle max_elements {max_elements}",
            elements.len()
        )));
    }

    // Resolve parallel arrays; each MUST have the same length as the primary.
    let mut parallels: Vec<(String, Vec<serde_json::Value>)> =
        Vec::with_capacity(parallel_paths.len());
    for (name, path) in parallel_paths {
        let value = resolve_path(ctx, &args_json, path)?;
        let arr = value.as_array().ok_or_else(|| {
            MapperError::Internal(anyhow::anyhow!(
                "array_emit: parallel path {path:?} ({name}) is not an array"
            ))
        })?;
        if arr.len() != elements.len() {
            return Err(MapperError::Internal(anyhow::anyhow!(
                "array_emit: parallel array {name} len {} != primary len {}",
                arr.len(),
                elements.len()
            )));
        }
        parallels.push((name.clone(), arr.clone()));
    }

    let mut envelopes = Vec::with_capacity(elements.len());
    for (idx, element) in elements.iter().enumerate() {
        // Per-element args object: outer args + `element` + each parallel row.
        // `$.args.element[...]` / `$.args.<parallelKey>[...]` then resolve via
        // the unmodified `walk_args` walker.
        let mut per_elem = args_json.as_object().cloned().unwrap_or_default();
        per_elem.insert("element".to_owned(), element.clone());
        for (name, arr) in &parallels {
            per_elem.insert(name.clone(), arr[idx].clone());
        }
        let per_elem_json = serde_json::Value::Object(per_elem);

        let envelope =
            single_emit::execute_with_args(ctx, &per_elem_json, category, action, fields).map_err(
                |e| {
                    MapperError::Internal(anyhow::anyhow!(
                        "array_emit: element #{idx} emit failed: {e}"
                    ))
                },
            )?;
        envelopes.push(envelope);
    }
    Ok(envelopes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use abi_resolver::{DecodedArg, DecodedValue, DecoderId};
    use alloy_primitives::U256;
    use policy_engine::action::{Action, Address, AmountKind, AssetKind, Category};
    use std::str::FromStr as _;

    use crate::token_registry::EmptyTokenRegistry;

    // ── Test scaffolding ──────────────────────────────────────────────────

    fn addr(label: u8) -> Address {
        Address::from_str(&format!("0x{}{label:02x}", "0".repeat(38))).unwrap()
    }

    fn ctx<'a>(
        registry: &'a EmptyTokenRegistry,
        from: &'a Address,
        to: &'a Address,
        value: &'a policy_engine::action::DecimalString,
    ) -> MapContext<'a> {
        MapContext {
            chain_id: 1,
            from,
            to,
            value_wei: value,
            block_timestamp: Some(1_700_000_000),
            token_registry: registry,
            parent_calldata: None,
            depth: 0,
            resolver: None,
        }
    }

    /// Run `execute` with a freshly built host context.
    fn run(
        decoded: &DecodedCall,
        category: &str,
        action: &str,
        array_path: &str,
        max_elements: u8,
        parallel_paths: &BTreeMap<String, String>,
        fields_json: serde_json::Value,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let registry = EmptyTokenRegistry;
        let from = addr(0xAA);
        let to = addr(0xBB);
        let value = policy_engine::action::DecimalString::from_str("0").unwrap();
        let c = ctx(&registry, &from, &to, &value);
        let fields: BTreeMap<String, ValueExpr> =
            serde_json::from_value(fields_json).expect("fields parse");
        execute(
            &c,
            decoded,
            category,
            action,
            array_path,
            max_elements,
            parallel_paths,
            &fields,
        )
    }

    fn uint(v: u64) -> DecodedValue {
        DecodedValue::Uint(U256::from(v))
    }

    /// One Permit2 `PermitDetails` tuple `(token, amount, expiration, nonce)`.
    fn permit_details(token: &Address, amount: u64, expiration: u64) -> DecodedValue {
        DecodedValue::Tuple(vec![
            DecodedValue::Address(token.clone()),
            uint(amount),
            uint(expiration),
            uint(0), // nonce — unused by the emit rule
        ])
    }

    /// `permit(address owner, PermitBatch permitBatch, bytes signature)` where
    /// `PermitBatch = (PermitDetails[] details, address spender, uint256 sigDeadline)`.
    fn permit_batch_decoded(details: Vec<DecodedValue>, spender: &Address) -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new("declarative.uniswap/permit2/permit-batch"),
            function_signature:
                "permit(address,((address,uint160,uint48,uint48)[],address,uint256),bytes)".into(),
            args: vec![
                DecodedArg {
                    name: "owner".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(addr(0xCC)),
                },
                DecodedArg {
                    name: "permitBatch".into(),
                    abi_type: "tuple".into(),
                    value: DecodedValue::Tuple(vec![
                        DecodedValue::Array(details),
                        DecodedValue::Address(spender.clone()),
                        uint(1_800_000_000), // sigDeadline
                    ]),
                },
                DecodedArg {
                    name: "signature".into(),
                    abi_type: "bytes".into(),
                    value: DecodedValue::Bytes(vec![0xab, 0xcd]),
                },
            ],
            nested: vec![],
        }
    }

    /// Field map for the Permit2 `permit-batch` emit rule (`misc/permit`).
    fn permit_batch_fields() -> serde_json::Value {
        serde_json::json!({
            "permitKind": { "literal": "permit2_batch" },
            "token.kind": { "literal": "erc20" },
            "token.address": { "from": "$.args.element[0]" },
            "owner": { "from": "$.args.owner" },
            "spender": { "from": "$.args.permitBatch[1]" },
            "amount.kind": { "literal": "max" },
            "amount.value": { "from": "$.args.element[1]" },
            "validity.expiresAt": { "from": "$.args.element[2]" },
            "validity.source": { "literal": "grant-expiration" }
        })
    }

    // ── TDD 1 — permit batch 3 elements fans out ──────────────────────────

    #[test]
    fn permit_batch_3_elements_fans_out() {
        let token_a = addr(0x11);
        let token_b = addr(0x22);
        let token_c = addr(0x33);
        let spender = addr(0x44);
        let decoded = permit_batch_decoded(
            vec![
                permit_details(&token_a, 100, 1_700_000_100),
                permit_details(&token_b, 200, 1_700_000_200),
                permit_details(&token_c, 300, 1_700_000_300),
            ],
            &spender,
        );

        let envelopes = run(
            &decoded,
            "misc",
            "permit",
            "$.args.permitBatch[0]",
            64,
            &BTreeMap::new(),
            permit_batch_fields(),
        )
        .expect("array_emit succeeds");

        assert_eq!(envelopes.len(), 3, "one envelope per PermitDetails element");

        let expected = [
            (token_a, "100", "1700000100"),
            (token_b, "200", "1700000200"),
            (token_c, "300", "1700000300"),
        ];
        for (i, (token, amount, expiry)) in expected.into_iter().enumerate() {
            assert_eq!(envelopes[i].category, Category::Misc);
            let Action::Permit(p) = &envelopes[i].action else {
                panic!(
                    "element {i}: expected Permit, got {:?}",
                    envelopes[i].action
                );
            };
            assert_eq!(p.token.kind, AssetKind::Erc20);
            assert_eq!(p.token.address.as_ref(), Some(&token));
            assert_eq!(
                p.amount
                    .as_ref()
                    .and_then(|a| a.value.as_ref())
                    .map(ToString::to_string),
                Some(amount.to_owned())
            );
            // spender / owner stay constant across the fan-out (outer args).
            assert_eq!(p.spender.as_ref(), Some(&spender));
            assert_eq!(p.validity.expires_at.to_string(), expiry);
        }
    }

    // ── TDD 2 — transferFrom batch 2 elements ─────────────────────────────

    /// `transferFrom(AllowanceTransferDetails[] transferDetails)` where each
    /// `AllowanceTransferDetails = (address from, address to, uint160 amount, address token)`.
    fn transfer_from_batch_decoded(details: Vec<DecodedValue>) -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new("declarative.uniswap/permit2/transferFrom-batch"),
            function_signature: "transferFrom((address,address,uint160,address)[])".into(),
            args: vec![DecodedArg {
                name: "transferDetails".into(),
                abi_type: "tuple[]".into(),
                value: DecodedValue::Array(details),
            }],
            nested: vec![],
        }
    }

    fn allowance_transfer_details(
        from: &Address,
        to: &Address,
        amount: u64,
        token: &Address,
    ) -> DecodedValue {
        DecodedValue::Tuple(vec![
            DecodedValue::Address(from.clone()),
            DecodedValue::Address(to.clone()),
            uint(amount),
            DecodedValue::Address(token.clone()),
        ])
    }

    fn transfer_from_batch_fields() -> serde_json::Value {
        serde_json::json!({
            "token.asset.kind": { "literal": "erc20" },
            "token.asset.address": { "from": "$.args.element[3]" },
            "token.amount.kind": { "literal": "exact" },
            "token.amount.value": { "from": "$.args.element[2]" },
            "from": { "from": "$.args.element[0]" },
            "recipient": { "from": "$.args.element[1]" }
        })
    }

    #[test]
    fn transfer_from_batch_2_elements() {
        let from_a = addr(0x01);
        let to_a = addr(0x02);
        let token_a = addr(0x03);
        let from_b = addr(0x04);
        let to_b = addr(0x05);
        let token_b = addr(0x06);
        let decoded = transfer_from_batch_decoded(vec![
            allowance_transfer_details(&from_a, &to_a, 111, &token_a),
            allowance_transfer_details(&from_b, &to_b, 222, &token_b),
        ]);

        let envelopes = run(
            &decoded,
            "misc",
            "transfer",
            "$.args.transferDetails",
            64,
            &BTreeMap::new(),
            transfer_from_batch_fields(),
        )
        .expect("array_emit succeeds");

        assert_eq!(envelopes.len(), 2);

        let Action::Transfer(t0) = &envelopes[0].action else {
            panic!("expected Transfer, got {:?}", envelopes[0].action);
        };
        assert_eq!(t0.from, from_a);
        assert_eq!(t0.recipient, to_a);
        assert_eq!(t0.token.asset.address.as_ref(), Some(&token_a));
        assert_eq!(t0.token.amount.kind, AmountKind::Exact);
        assert_eq!(
            t0.token.amount.value.as_ref().map(ToString::to_string),
            Some("111".to_owned())
        );

        let Action::Transfer(t1) = &envelopes[1].action else {
            panic!("expected Transfer, got {:?}", envelopes[1].action);
        };
        assert_eq!(t1.from, from_b);
        assert_eq!(t1.recipient, to_b);
        assert_eq!(t1.token.asset.address.as_ref(), Some(&token_b));
        assert_eq!(
            t1.token.amount.value.as_ref().map(ToString::to_string),
            Some("222".to_owned())
        );
    }

    // ── TDD 3 — batch with parallel arrays (index-synchronised) ───────────
    //
    // Mirrors the Permit2 `permitTransferFrom(batch)` shape — a primary
    // `TokenPermissions[]`-style array index-aligned with a parallel
    // `SignatureTransferDetails[]`-style array. The emit `action` is
    // `transfer` (not `permit`) so the assertions land on fields the
    // builder actually populates: `build_transfer_envelope` reads
    // `token.asset.address`, `token.amount.value`, `from`, `recipient` —
    // `build_permit_envelope` hardcodes `recipient` / `requestedAmount` to
    // `None`, so a permit-action test could not observe the parallel row.

    /// A 4-arg call: `permitted` (primary `tuple[]`, each `(from, token)`),
    /// `transferDetails` (parallel `tuple[]`, each `(to, requestedAmount)`),
    /// plus two scalar args used to prove non-array `array_path` rejection.
    fn parallel_batch_decoded(
        permitted: Vec<DecodedValue>,
        transfer_details: Vec<DecodedValue>,
        owner: &Address,
    ) -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(
                "declarative.uniswap/permit2/permitTransferFrom-batch",
            ),
            function_signature: "permitTransferFrom(((address,address)[],uint256,uint256),(address,uint256)[],address,bytes)".into(),
            args: vec![
                DecodedArg {
                    name: "permit".into(),
                    abi_type: "tuple".into(),
                    value: DecodedValue::Tuple(vec![
                        DecodedValue::Array(permitted),
                        uint(7),               // nonce
                        uint(1_900_000_000),   // deadline
                    ]),
                },
                DecodedArg {
                    name: "transferDetails".into(),
                    abi_type: "tuple[]".into(),
                    value: DecodedValue::Array(transfer_details),
                },
                DecodedArg {
                    name: "owner".into(),
                    abi_type: "address".into(),
                    value: DecodedValue::Address(owner.clone()),
                },
                DecodedArg {
                    name: "signature".into(),
                    abi_type: "bytes".into(),
                    value: DecodedValue::Bytes(vec![0x01]),
                },
            ],
            nested: vec![],
        }
    }

    /// Primary element `(address from, address token)`.
    fn primary_row(from: &Address, token: &Address) -> DecodedValue {
        DecodedValue::Tuple(vec![
            DecodedValue::Address(from.clone()),
            DecodedValue::Address(token.clone()),
        ])
    }

    /// Parallel element `(address to, uint256 requestedAmount)`.
    fn parallel_row(to: &Address, requested: u64) -> DecodedValue {
        DecodedValue::Tuple(vec![DecodedValue::Address(to.clone()), uint(requested)])
    }

    /// Emit rule: `from` + `token.asset.address` from the primary `element`,
    /// `recipient` + `token.amount.value` from the parallel `td` row.
    fn parallel_batch_fields() -> serde_json::Value {
        serde_json::json!({
            "token.asset.kind": { "literal": "erc20" },
            "token.asset.address": { "from": "$.args.element[1]" },
            "token.amount.kind": { "literal": "exact" },
            "token.amount.value": { "from": "$.args.td[1]" },
            "from": { "from": "$.args.element[0]" },
            "recipient": { "from": "$.args.td[0]" }
        })
    }

    #[test]
    fn permit_transfer_from_batch_parallel() {
        let from_a = addr(0x11);
        let token_a = addr(0x12);
        let to_a = addr(0x31);
        let from_b = addr(0x13);
        let token_b = addr(0x14);
        let to_b = addr(0x32);
        let decoded = parallel_batch_decoded(
            vec![
                primary_row(&from_a, &token_a),
                primary_row(&from_b, &token_b),
            ],
            vec![parallel_row(&to_a, 900), parallel_row(&to_b, 1900)],
            &addr(0xCC),
        );

        let mut parallel = BTreeMap::new();
        parallel.insert("td".to_owned(), "$.args.transferDetails".to_owned());

        let envelopes = run(
            &decoded,
            "misc",
            "transfer",
            "$.args.permit[0]",
            64,
            &parallel,
            parallel_batch_fields(),
        )
        .expect("array_emit succeeds");

        assert_eq!(envelopes.len(), 2);

        // Element 0 — primary[0] ∥ transferDetails[0].
        let Action::Transfer(t0) = &envelopes[0].action else {
            panic!("expected Transfer, got {:?}", envelopes[0].action);
        };
        assert_eq!(t0.from, from_a, "from comes from primary row 0");
        assert_eq!(t0.token.asset.address.as_ref(), Some(&token_a));
        assert_eq!(
            t0.recipient, to_a,
            "recipient must come from the parallel transferDetails row 0"
        );
        assert_eq!(
            t0.token.amount.value.as_ref().map(ToString::to_string),
            Some("900".to_owned()),
            "amount must come from the parallel transferDetails row 0"
        );

        // Element 1 — primary[1] ∥ transferDetails[1].
        let Action::Transfer(t1) = &envelopes[1].action else {
            panic!("expected Transfer, got {:?}", envelopes[1].action);
        };
        assert_eq!(t1.from, from_b, "from comes from primary row 1");
        assert_eq!(t1.token.asset.address.as_ref(), Some(&token_b));
        assert_eq!(
            t1.recipient, to_b,
            "recipient must come from the parallel transferDetails row 1"
        );
        assert_eq!(
            t1.token.amount.value.as_ref().map(ToString::to_string),
            Some("1900".to_owned()),
            "amount must come from the parallel transferDetails row 1"
        );
    }

    // ── TDD 4 — empty array yields zero envelopes ─────────────────────────

    #[test]
    fn empty_array_yields_zero_envelopes() {
        let decoded = transfer_from_batch_decoded(vec![]);
        let envelopes = run(
            &decoded,
            "misc",
            "transfer",
            "$.args.transferDetails",
            64,
            &BTreeMap::new(),
            transfer_from_batch_fields(),
        )
        .expect("array_emit succeeds on empty array");
        assert!(envelopes.is_empty());
    }

    // ── TDD 5 — element-count caps ────────────────────────────────────────

    #[test]
    fn exceeds_max_elements_errors() {
        // 3 elements but the bundle declares max_elements = 2.
        let decoded = transfer_from_batch_decoded(vec![
            allowance_transfer_details(&addr(0x01), &addr(0x02), 1, &addr(0x03)),
            allowance_transfer_details(&addr(0x04), &addr(0x05), 2, &addr(0x06)),
            allowance_transfer_details(&addr(0x07), &addr(0x08), 3, &addr(0x09)),
        ]);
        let err = run(
            &decoded,
            "misc",
            "transfer",
            "$.args.transferDetails",
            2,
            &BTreeMap::new(),
            transfer_from_batch_fields(),
        )
        .unwrap_err();
        match err {
            MapperError::Internal(e) => {
                assert!(
                    e.to_string().contains("bundle max_elements"),
                    "unexpected message: {e}"
                );
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn exceeds_global_cap_errors() {
        // 65 elements — over MAX_ARRAY_ELEMENTS (64) even though the bundle
        // declares max_elements = 64.
        let details: Vec<DecodedValue> = (0..65)
            .map(|_| allowance_transfer_details(&addr(0x01), &addr(0x02), 1, &addr(0x03)))
            .collect();
        let decoded = transfer_from_batch_decoded(details);
        let err = run(
            &decoded,
            "misc",
            "transfer",
            "$.args.transferDetails",
            64,
            &BTreeMap::new(),
            transfer_from_batch_fields(),
        )
        .unwrap_err();
        match err {
            MapperError::Internal(e) => {
                assert!(
                    e.to_string().contains("MAX_ARRAY_ELEMENTS"),
                    "unexpected message: {e}"
                );
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // ── TDD 6 — non-array array_path ──────────────────────────────────────

    #[test]
    fn non_array_path_errors() {
        // `$.args.owner` is a bare address string — `array_emit` must reject
        // it before any element processing. (A `tuple[]` element decodes to
        // a JSON array, so the rejection target must be a genuine scalar.)
        let decoded = parallel_batch_decoded(
            vec![primary_row(&addr(0x11), &addr(0x12))],
            vec![parallel_row(&addr(0x31), 900)],
            &addr(0xCC),
        );
        let err = run(
            &decoded,
            "misc",
            "transfer",
            "$.args.owner", // an address scalar — not an array
            64,
            &BTreeMap::new(),
            parallel_batch_fields(),
        )
        .unwrap_err();
        match err {
            MapperError::Internal(e) => {
                assert!(
                    e.to_string().contains("is not an array"),
                    "unexpected message: {e}"
                );
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // ── TDD 7 — parallel array length mismatch ────────────────────────────

    #[test]
    fn parallel_length_mismatch_errors() {
        let decoded = parallel_batch_decoded(
            vec![
                primary_row(&addr(0x11), &addr(0x12)),
                primary_row(&addr(0x13), &addr(0x14)),
            ],
            // Only ONE transferDetails row — primary array has two.
            vec![parallel_row(&addr(0x31), 900)],
            &addr(0xCC),
        );
        let mut parallel = BTreeMap::new();
        parallel.insert("td".to_owned(), "$.args.transferDetails".to_owned());

        let err = run(
            &decoded,
            "misc",
            "transfer",
            "$.args.permit[0]",
            64,
            &parallel,
            parallel_batch_fields(),
        )
        .unwrap_err();
        match err {
            MapperError::Internal(e) => {
                assert!(
                    e.to_string().contains("parallel array")
                        && e.to_string().contains("!= primary len"),
                    "unexpected message: {e}"
                );
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
