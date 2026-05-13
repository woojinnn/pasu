//! Enum-tagged sub-decoder.
//!
//! Some protocols pack a `(kind, …)` payload inside a single `bytes`
//! argument where the first 32-byte word is an `enum` discriminator and the
//! tail's shape depends on which discriminator value was passed. The classic
//! example is Balancer V2's `joinPool.userData` / `exitPool.userData`, where
//! the first word is `JoinKind` / `ExitKind` and the rest of the payload is
//! `(uint256[] amountsIn, uint256 minBPTAmountOut)` for `EXACT_TOKENS_IN_FOR_BPT_OUT`,
//! `(uint256 bptAmountOut, uint256 tokenIndex)` for `TOKEN_IN_FOR_EXACT_BPT_OUT`,
//! and so on.
//!
//! This module provides the generic engine — read the discriminator, look it
//! up in a per-protocol [`EnumTable`], decode the full payload against the
//! kind-specific JSON ABI. Per-protocol tables live in
//! [`crate::subdecode::protocols`].

use alloy_dyn_abi::JsonAbiExt;
use alloy_json_abi::{Function, Param, StateMutability};

use crate::decode::DecodedArg;

/// One protocol's enum-discriminated sub-format table.
#[derive(Debug, Clone, Copy)]
pub struct EnumTable {
    /// Human-readable name (used in error messages).
    pub name: &'static str,
    /// Per-discriminator entries.
    pub entries: &'static [EnumEntry],
}

/// One discriminator entry — kind value, name, and the full payload shape
/// (including the leading `kind` field, since Solidity encodes
/// `abi.decode(input, (Kind, Tail))` as a tuple).
#[derive(Debug, Clone, Copy)]
pub struct EnumEntry {
    /// Discriminator value (the leading `uint256`).
    pub kind: u32,
    /// Human-readable name (e.g. `"INIT"`, `"EXACT_TOKENS_IN_FOR_BPT_OUT"`).
    pub name: &'static str,
    /// JSON ABI describing the full payload (including the leading `kind`
    /// uint256). Format: a JSON array literal of standard ABI Param objects.
    pub payload_json_abi: &'static str,
}

/// Result of a successful enum-tagged decode.
#[derive(Debug, Clone)]
pub struct DecodedEnum {
    /// Discriminator value.
    pub kind: u32,
    /// Discriminator name (from the matched [`EnumEntry`]).
    pub kind_name: &'static str,
    /// The protocol's own table name (for context).
    pub table_name: &'static str,
    /// Decoded payload args. The first arg is always the `kind` itself; the
    /// rest are kind-specific.
    pub args: Vec<DecodedArg>,
}

/// Try to decode `input` as an enum-tagged payload against `table`.
///
/// Returns `None` when:
/// - `input` is shorter than 32 bytes (no room for the discriminator),
/// - the discriminator doesn't match any entry in the table,
/// - the JSON ABI for the matched entry doesn't deserialise (table bug),
/// - or ABI decoding the payload against the matched JSON ABI fails.
#[must_use]
pub fn dispatch(input: &[u8], table: &EnumTable) -> Option<DecodedEnum> {
    if input.len() < 32 {
        return None;
    }
    // Discriminator is encoded as uint256; in practice the value fits in
    // u32 for every table we register. Read from the rightmost 4 bytes of
    // the first 32-byte word.
    let kind = u32::from_be_bytes([input[28], input[29], input[30], input[31]]);
    // Reject pathologically large kinds early — they can't possibly index a
    // small enum table.
    if input[..28].iter().any(|b| *b != 0) {
        return None;
    }
    let entry = table.entries.iter().find(|e| e.kind == kind)?;

    let inputs: Vec<Param> = serde_json::from_str(entry.payload_json_abi).ok()?;
    let function = Function {
        name: entry.name.to_string(),
        inputs,
        outputs: Vec::new(),
        state_mutability: StateMutability::NonPayable,
    };
    let values = function.abi_decode_input(input, true).ok()?;

    let args = function
        .inputs
        .iter()
        .enumerate()
        .zip(values)
        .map(|((idx, param), value)| {
            let name = if param.name.is_empty() {
                format!("arg{idx}")
            } else {
                param.name.clone()
            };
            DecodedArg {
                name,
                sol_type: param.ty.clone(),
                value,
                components: param.components.clone(),
            }
        })
        .collect();

    Some(DecodedEnum {
        kind,
        kind_name: entry.name,
        table_name: table.name,
        args,
    })
}

/// Try multiple tables in order; return the first successful decode. Used
/// when the same `bytes` field can appear with different kind tables across
/// pool types (e.g. Balancer V2 ExitKind differs between Weighted and
/// Stable pools — try Weighted first, fall back to Stable).
#[must_use]
pub fn try_dispatch(input: &[u8], tables: &[&EnumTable]) -> Option<DecodedEnum> {
    for t in tables {
        if let Some(d) = dispatch(input, t) {
            return Some(d);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_dyn_abi::DynSolValue;
    use alloy_primitives::U256;

    static DEMO_TABLE: EnumTable = EnumTable {
        name: "demo",
        entries: &[
            EnumEntry {
                kind: 0,
                name: "INIT",
                payload_json_abi: r#"[
                    {"name":"kind","type":"uint256"},
                    {"name":"amountsIn","type":"uint256[]"}
                ]"#,
            },
            EnumEntry {
                kind: 1,
                name: "EXACT_BPT",
                payload_json_abi: r#"[
                    {"name":"kind","type":"uint256"},
                    {"name":"bptAmountOut","type":"uint256"}
                ]"#,
            },
        ],
    };

    fn encode(sig: &str, values: Vec<DynSolValue>) -> Vec<u8> {
        let func = Function::parse(&format!("step{sig}")).unwrap();
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    #[test]
    fn matches_first_kind() {
        let payload = encode(
            "(uint256,uint256[])",
            vec![
                DynSolValue::Uint(U256::ZERO, 256),
                DynSolValue::Array(vec![
                    DynSolValue::Uint(U256::from(100u64), 256),
                    DynSolValue::Uint(U256::from(200u64), 256),
                ]),
            ],
        );
        let d = dispatch(&payload, &DEMO_TABLE).unwrap();
        assert_eq!(d.kind, 0);
        assert_eq!(d.kind_name, "INIT");
        assert_eq!(d.args.len(), 2);
        assert_eq!(d.args[0].name, "kind");
        assert_eq!(d.args[1].name, "amountsIn");
    }

    #[test]
    fn matches_second_kind() {
        let payload = encode(
            "(uint256,uint256)",
            vec![
                DynSolValue::Uint(U256::from(1u64), 256),
                DynSolValue::Uint(U256::from(999u64), 256),
            ],
        );
        let d = dispatch(&payload, &DEMO_TABLE).unwrap();
        assert_eq!(d.kind, 1);
        assert_eq!(d.kind_name, "EXACT_BPT");
        assert_eq!(d.args[1].name, "bptAmountOut");
    }

    #[test]
    fn unknown_kind_returns_none() {
        let payload = encode("(uint256)", vec![DynSolValue::Uint(U256::from(99u64), 256)]);
        assert!(dispatch(&payload, &DEMO_TABLE).is_none());
    }

    #[test]
    fn short_input_returns_none() {
        assert!(dispatch(&[0x00; 16], &DEMO_TABLE).is_none());
    }

    #[test]
    fn try_dispatch_picks_first_match() {
        let payload = encode(
            "(uint256,uint256)",
            vec![
                DynSolValue::Uint(U256::from(1u64), 256),
                DynSolValue::Uint(U256::from(42u64), 256),
            ],
        );
        let d = try_dispatch(&payload, &[&DEMO_TABLE, &DEMO_TABLE]).unwrap();
        assert_eq!(d.kind_name, "EXACT_BPT");
    }
}
