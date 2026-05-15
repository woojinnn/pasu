//! Opcode-stream dispatcher.
//!
//! Several routers encode a sequence of operations as parallel arrays:
//!
//! - `bytes commands`  — one byte per step. The byte's low bits are the
//!   opcode; the high bit is typically a `allowRevert` flag (`0x80` for
//!   Uniswap UR, `0x40` for Pancake UR — see [`OpcodeTable::mask`]).
//! - `bytes[] inputs`  — `inputs[i]` is the ABI-encoded argument tuple for
//!   the opcode in `commands[i]`.
//!
//! [`dispatch`] walks the two arrays in lockstep, looks each opcode up in a
//! protocol-specific [`OpcodeTable`], and ABI-decodes `inputs[i]` against the
//! opcode's schema. Unrecognised opcodes are reported with raw input bytes;
//! recognised opcodes whose ABI decode fails likewise surface raw input plus
//! an error message so the caller can render a partial view.

use alloy_dyn_abi::JsonAbiExt;
use alloy_json_abi::{Function, Param};

use crate::decode::DecodedArg;

/// One entry in a router's opcode table. Pre-static, intended to live in a
/// `&'static [OpcodeEntry]` per protocol.
#[derive(Debug, Clone, Copy)]
pub struct OpcodeEntry {
    /// Opcode value after the [`OpcodeTable::mask`] is applied.
    pub opcode: u8,
    /// Human-readable name of the opcode (e.g. `"V3_SWAP_EXACT_IN"`).
    pub name: &'static str,
    /// Candidate Solidity tuple types to try (in order) when ABI-decoding
    /// `inputs[i]`. Multiple entries support routers whose dispatcher signature
    /// changed between deployments — e.g. UR added a trailing
    /// `uint256[] minHopPriceX36` to its V2/V3 swap opcodes, but older
    /// deployments still in production use the shorter shape. The dispatcher
    /// tries each signature in order and accepts the first one that decodes
    /// cleanly.
    ///
    /// Empty slice means no schema is registered yet (or [`Self::input_json_abi`]
    /// supplies a richer schema); the input stays as raw hex with a
    /// [`StepDecodeError::NoSchema`] marker if both forms are absent.
    pub input_signatures: &'static [&'static str],
    /// Optional JSON ABI describing the inputs as a `Vec<Param>`. Preferred
    /// over [`Self::input_signatures`] when present because alloy's Solidity
    /// signature parser drops field names inside tuple types — supplying a
    /// JSON ABI lets us preserve names at every level (e.g.
    /// `params.path[].intermediateCurrency` instead of an unnamed positional
    /// tuple). Format: a JSON array literal of standard ABI Param objects.
    pub input_json_abi: Option<&'static str>,
}

/// One protocol's opcode dispatch table.
#[derive(Debug, Clone, Copy)]
pub struct OpcodeTable {
    /// Mask applied to the raw command byte to extract the opcode. `0x7f`
    /// reserves the high bit for `allowRevert` (Uniswap UR); `0x3f` reserves
    /// the top two bits (Pancake UR).
    pub mask: u8,
    /// Bit set in the raw command byte signalling that a step's revert should
    /// be tolerated by the dispatcher (typically `0x80`).
    pub allow_revert_bit: u8,
    /// Known opcode entries. The dispatcher does linear scan — fine for the
    /// dozens of opcodes per router.
    pub entries: &'static [OpcodeEntry],
}

impl OpcodeTable {
    fn lookup(&self, opcode: u8) -> Option<&'static OpcodeEntry> {
        self.entries.iter().find(|e| e.opcode == opcode)
    }
}

/// Why an opcode step couldn't be fully decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepDecodeError {
    /// Opcode was not present in the dispatch table.
    UnknownOpcode,
    /// Opcode was known but its declared schema didn't parse as a Solidity
    /// signature. Indicates a bug in the table.
    BadSignature(String),
    /// Opcode was known and the schema parsed, but the given input bytes did
    /// not ABI-decode against it (e.g. version mismatch between deployed
    /// router and our table).
    AbiDecode(String),
    /// Opcode was known but its entry has no `input_signature`, so we don't
    /// attempt to decode the input.
    NoSchema,
}

/// One decoded step of a opcode stream.
#[derive(Debug, Clone)]
pub struct DecodedStep {
    /// Index in the parent `commands` byte string.
    pub index: usize,
    /// Raw command byte before masking.
    pub raw_byte: u8,
    /// Opcode after masking.
    pub opcode: u8,
    /// True when the high bit signalling `allowRevert` was set.
    pub allow_revert: bool,
    /// Human-readable opcode name, or `"UNKNOWN"` when the table lookup
    /// missed.
    pub name: &'static str,
    /// Decoded arguments for `inputs[i]` when the opcode had a known schema
    /// and the input ABI-decoded cleanly.
    pub args: Option<Vec<DecodedArg>>,
    /// Why decode failed (or didn't run) when [`Self::args`] is `None`.
    pub error: Option<StepDecodeError>,
    /// Raw `inputs[i]` bytes — always populated so callers can fall back to
    /// hex display.
    pub raw_input: Vec<u8>,
}

/// Walk `commands` and `inputs` in lockstep, dispatching each step against
/// the supplied table.
///
/// `commands.len()` and `inputs.len()` are expected to match. When they
/// don't, the function still returns a step per `commands[i]` but uses an
/// empty `raw_input` for any out-of-bounds index.
#[must_use]
pub fn dispatch(commands: &[u8], inputs: &[Vec<u8>], table: &OpcodeTable) -> Vec<DecodedStep> {
    commands
        .iter()
        .copied()
        .enumerate()
        .map(|(index, raw_byte)| {
            let opcode = raw_byte & table.mask;
            let allow_revert = (raw_byte & table.allow_revert_bit) != 0;
            let raw_input = inputs.get(index).cloned().unwrap_or_default();

            let Some(entry) = table.lookup(opcode) else {
                return DecodedStep {
                    index,
                    raw_byte,
                    opcode,
                    allow_revert,
                    name: "UNKNOWN",
                    args: None,
                    error: Some(StepDecodeError::UnknownOpcode),
                    raw_input,
                };
            };

            let (args, error) = decode_step_input(entry, &raw_input);
            DecodedStep {
                index,
                raw_byte,
                opcode,
                allow_revert,
                name: entry.name,
                args,
                error,
                raw_input,
            }
        })
        .collect()
}

fn decode_step_input(
    entry: &OpcodeEntry,
    input: &[u8],
) -> (Option<Vec<DecodedArg>>, Option<StepDecodeError>) {
    if entry.input_signatures.is_empty() && entry.input_json_abi.is_none() {
        return (None, Some(StepDecodeError::NoSchema));
    }
    let mut last_error = None;

    // Prefer JSON ABI when present — it preserves named fields inside tuple
    // types (alloy's Solidity signature parser drops them).
    if let Some(json) = entry.input_json_abi {
        match function_from_json_inputs(json, entry.name) {
            Ok(function) => match function.abi_decode_input(input, true) {
                Ok(values) => return (Some(build_decoded_args(&function, values)), None),
                Err(e) => {
                    last_error = Some(StepDecodeError::AbiDecode(format!(
                        "JSON ABI decode failed: {e}",
                    )));
                }
            },
            Err(e) => {
                last_error = Some(StepDecodeError::BadSignature(format!(
                    "could not parse JSON ABI for `{}`: {e}",
                    entry.name
                )));
            }
        }
    }

    // Fall back to Solidity signature strings; first that ABI-decodes wins.
    for sig in entry.input_signatures {
        // Wrap the tuple signature into a synthetic function so we can reuse
        // `Function::parse` + `abi_decode_input`. The leading "step" name is
        // arbitrary and isn't surfaced anywhere.
        let synthetic = format!("step{sig}");
        let function = match Function::parse(&synthetic) {
            Ok(f) => f,
            Err(e) => {
                last_error = Some(StepDecodeError::BadSignature(e.to_string()));
                continue;
            }
        };
        let Ok(values) = function.abi_decode_input(input, true) else {
            last_error = Some(StepDecodeError::AbiDecode(format!(
                "input did not match candidate `{sig}`",
            )));
            continue;
        };
        return (Some(build_decoded_args(&function, values)), None);
    }
    (None, last_error)
}

/// Parse a JSON-ABI `Vec<Param>` literal into a synthetic [`Function`].
///
/// `entry_name` is only used for the generated function's `name` field; it
/// doesn't affect ABI decoding (we always use `abi_decode_input` which
/// ignores the function name and selector).
fn function_from_json_inputs(json: &str, entry_name: &str) -> Result<Function, String> {
    let inputs: Vec<Param> =
        serde_json::from_str(json).map_err(|e| format!("invalid Param JSON: {e}"))?;
    Ok(Function {
        name: entry_name.to_string(),
        inputs,
        outputs: Vec::new(),
        state_mutability: alloy_json_abi::StateMutability::NonPayable,
    })
}

fn build_decoded_args(
    function: &Function,
    values: Vec<alloy_dyn_abi::DynSolValue>,
) -> Vec<DecodedArg> {
    function
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
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_dyn_abi::DynSolValue;
    use alloy_primitives::{Address, U256};

    static DEMO_TABLE: OpcodeTable = OpcodeTable {
        mask: 0x7f,
        allow_revert_bit: 0x80,
        entries: &[
            OpcodeEntry {
                opcode: 0x0b,
                name: "WRAP_ETH",
                input_signatures: &["(address,uint256)"],
                input_json_abi: None,
            },
            OpcodeEntry {
                opcode: 0x0c,
                name: "UNWRAP_WETH",
                input_signatures: &["(address,uint256)"],
                input_json_abi: None,
            },
            OpcodeEntry {
                opcode: 0x40,
                name: "OPCODE_WITHOUT_SCHEMA",
                input_signatures: &[],
                input_json_abi: None,
            },
            OpcodeEntry {
                opcode: 0x50,
                name: "OPCODE_WITH_FALLBACK",
                input_signatures: &["(address,uint256,uint256)", "(address,uint256)"],
                input_json_abi: None,
            },
        ],
    };

    fn encode_address_uint256(addr: [u8; 20], value: u128) -> Vec<u8> {
        let func = Function::parse("step(address,uint256)").unwrap();
        let values = vec![
            DynSolValue::Address(Address::from(addr)),
            DynSolValue::Uint(U256::from(value), 256),
        ];
        // abi_encode_input prepends the synthetic 4-byte selector — slice it
        // off so the result matches what `inputs[i]` looks like in real UR
        // calldata.
        let raw = func.abi_encode_input(&values).unwrap();
        raw[4..].to_vec()
    }

    #[test]
    fn dispatches_two_known_opcodes() {
        let commands = vec![0x0b, 0x0c];
        let inputs = vec![
            encode_address_uint256([0x11; 20], 1_000_000),
            encode_address_uint256([0x22; 20], 2_000_000),
        ];
        let steps = dispatch(&commands, &inputs, &DEMO_TABLE);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].name, "WRAP_ETH");
        assert_eq!(steps[1].name, "UNWRAP_WETH");
        for s in &steps {
            assert!(s.args.is_some());
            assert!(s.error.is_none());
            assert!(!s.allow_revert);
        }
    }

    #[test]
    fn allow_revert_high_bit_observed() {
        let commands = vec![0x0b | 0x80];
        let inputs = vec![encode_address_uint256([0x11; 20], 1)];
        let steps = dispatch(&commands, &inputs, &DEMO_TABLE);
        assert!(steps[0].allow_revert);
        assert_eq!(steps[0].opcode, 0x0b);
        assert_eq!(steps[0].name, "WRAP_ETH");
    }

    #[test]
    fn unknown_opcode_keeps_raw_input() {
        let commands = vec![0x55];
        let raw = vec![0xde, 0xad, 0xbe, 0xef];
        let steps = dispatch(&commands, std::slice::from_ref(&raw), &DEMO_TABLE);
        assert_eq!(steps[0].name, "UNKNOWN");
        assert_eq!(steps[0].error, Some(StepDecodeError::UnknownOpcode));
        assert_eq!(steps[0].raw_input, raw);
    }

    #[test]
    fn opcode_without_schema_marks_no_schema() {
        let commands = vec![0x40];
        let steps = dispatch(&commands, &[vec![]], &DEMO_TABLE);
        assert_eq!(steps[0].name, "OPCODE_WITHOUT_SCHEMA");
        assert_eq!(steps[0].error, Some(StepDecodeError::NoSchema));
        assert!(steps[0].args.is_none());
    }

    #[test]
    fn malformed_input_surfaces_abi_error() {
        let commands = vec![0x0b];
        // Too short for (address,uint256) — should fail ABI decode.
        let steps = dispatch(&commands, &[vec![0x00, 0x01]], &DEMO_TABLE);
        assert_eq!(steps[0].name, "WRAP_ETH");
        assert!(matches!(
            steps[0].error,
            Some(StepDecodeError::AbiDecode(_))
        ));
    }

    #[test]
    fn fallback_signature_is_tried_when_first_fails() {
        // Encode a (address, uint256) payload; the entry lists a 3-tuple
        // first, then a 2-tuple as fallback. The dispatcher should try the
        // 3-tuple, observe an ABI error, and accept the 2-tuple instead.
        let commands = vec![0x50];
        let inputs = vec![encode_address_uint256([0x33; 20], 7)];
        let steps = dispatch(&commands, &inputs, &DEMO_TABLE);
        assert!(steps[0].args.is_some(), "fallback should decode the input");
        let args = steps[0].args.as_ref().unwrap();
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn missing_input_index_does_not_panic() {
        let commands = vec![0x0b, 0x0c];
        // Only one input provided.
        let inputs = vec![encode_address_uint256([0x11; 20], 1)];
        let steps = dispatch(&commands, &inputs, &DEMO_TABLE);
        assert_eq!(steps.len(), 2);
        // First decodes; second has empty raw_input and fails.
        assert!(steps[0].args.is_some());
        assert!(steps[1].args.is_none());
        assert!(steps[1].raw_input.is_empty());
    }
}
