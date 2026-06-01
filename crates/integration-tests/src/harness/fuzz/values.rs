//! ABI-type → `DynSolValue` generation (seeded, with edge-value injection).

use alloy_dyn_abi::{DynSolType, DynSolValue};
use alloy_primitives::{Address, B256, I256, U256};
use anyhow::{anyhow, Result};

use crate::harness::adapters::AbiInput;
use crate::harness::prng::SplitMix64;

/// Generation mode for a single value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Fully random.
    Random,
    /// Boundary value (0 / max / empty / single-element).
    Edge,
}

/// Build a `DynSolType` from an ABI input, resolving tuple `components`.
///
/// `DynSolType::parse` cannot handle JSON-ABI `"tuple"` (it needs the component
/// list), so tuples are assembled recursively from `components`.
pub fn abi_input_to_soltype(input: &AbiInput) -> Result<DynSolType> {
    soltype(&input.ty, input.components.as_deref())
}

fn soltype(ty: &str, components: Option<&[AbiInput]>) -> Result<DynSolType> {
    // Dynamic array: `T[]`.
    if let Some(base) = ty.strip_suffix("[]") {
        return Ok(DynSolType::Array(Box::new(soltype(base, components)?)));
    }
    // Fixed array: `T[N]`.
    if ty.ends_with(']') {
        if let Some(open) = ty.rfind('[') {
            let n: usize = ty[open + 1..ty.len() - 1]
                .parse()
                .map_err(|_| anyhow!("bad fixed-array size in `{ty}`"))?;
            let base = &ty[..open];
            return Ok(DynSolType::FixedArray(
                Box::new(soltype(base, components)?),
                n,
            ));
        }
    }
    // Tuple: assemble from components.
    if ty == "tuple" {
        let comps = components.ok_or_else(|| anyhow!("`tuple` without components"))?;
        let inner = comps
            .iter()
            .map(abi_input_to_soltype)
            .collect::<Result<Vec<_>>>()?;
        return Ok(DynSolType::Tuple(inner));
    }
    // Scalar.
    ty.parse::<DynSolType>()
        .map_err(|e| anyhow!("parse soltype `{ty}`: {e}"))
}

/// Parse a Solidity signature fragment (a tuple or param-list, possibly with
/// parameter names) into a `DynSolType::Tuple`.
///
/// Used for `inputs_abi` strings on opcode-stream / tagged-dispatch entries,
/// e.g. `"(address recipient, uint256 amountIn, bytes path, bool payerIsUser)"`.
/// `DynSolType::parse` rejects parameter names and bare param-lists, so names
/// are stripped and components split paren/bracket-aware.
pub fn parse_sig_to_soltype(sig: &str) -> Result<DynSolType> {
    let s = sig.trim();
    let inner = s
        .strip_prefix('(')
        .and_then(|x| x.strip_suffix(')'))
        .unwrap_or(s);
    let mut types = Vec::new();
    for seg in split_top_level(inner) {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        types.push(parse_one_param(seg)?);
    }
    Ok(DynSolType::Tuple(types))
}

fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn parse_one_param(seg: &str) -> Result<DynSolType> {
    let seg = seg.trim();
    // Strip a trailing bare-identifier parameter name, if present.
    let ty = match seg.rfind(' ') {
        Some(idx) => {
            let (head, tail) = seg.split_at(idx);
            let tail = tail.trim();
            if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                head.trim()
            } else {
                seg
            }
        }
        None => seg,
    };
    soltype_str(ty)
}

fn soltype_str(ty: &str) -> Result<DynSolType> {
    if let Some(base) = ty.strip_suffix("[]") {
        return Ok(DynSolType::Array(Box::new(soltype_str(base)?)));
    }
    if ty.ends_with(']') {
        if let Some(open) = ty.rfind('[') {
            let n: usize = ty[open + 1..ty.len() - 1]
                .parse()
                .map_err(|_| anyhow!("bad array size in `{ty}`"))?;
            return Ok(DynSolType::FixedArray(
                Box::new(soltype_str(&ty[..open])?),
                n,
            ));
        }
    }
    if ty.starts_with('(') {
        return parse_sig_to_soltype(ty);
    }
    ty.parse::<DynSolType>()
        .map_err(|e| anyhow!("parse soltype `{ty}`: {e}"))
}

/// Generate a `DynSolValue` for `ty`.
pub fn gen_value(rng: &mut SplitMix64, ty: &DynSolType, edge: Edge) -> DynSolValue {
    match ty {
        DynSolType::Bool => DynSolValue::Bool(match edge {
            Edge::Edge => true,
            Edge::Random => rng.boolean(),
        }),
        DynSolType::Address => DynSolValue::Address(gen_address(rng, edge)),
        DynSolType::Uint(bits) => DynSolValue::Uint(gen_uint(rng, *bits, edge), *bits),
        DynSolType::Int(bits) => DynSolValue::Int(gen_int(rng, *bits, edge), *bits),
        DynSolType::FixedBytes(n) => DynSolValue::FixedBytes(gen_word(rng, *n, edge), *n),
        DynSolType::Bytes => DynSolValue::Bytes(gen_bytes(rng, edge)),
        DynSolType::String => DynSolValue::String(gen_string(rng, edge)),
        DynSolType::Array(inner) => {
            // Real calldata arrays (e.g. a swap `path`) are non-empty and are
            // index-addressed by manifest bodies (`$args.path[0]`/`[-1]`). Empty
            // arrays are a degenerate, would-revert shape; the dedicated
            // `array_emit` fuzzer (Phase 2) exercises the len-0 case explicitly.
            let len = match edge {
                Edge::Edge => 1,
                Edge::Random => 1 + rng.below(3) as usize, // 1..=3
            };
            DynSolValue::Array(
                (0..len)
                    .map(|_| gen_value(rng, inner, Edge::Random))
                    .collect(),
            )
        }
        DynSolType::FixedArray(inner, n) => {
            DynSolValue::FixedArray((0..*n).map(|_| gen_value(rng, inner, edge)).collect())
        }
        DynSolType::Tuple(items) => {
            DynSolValue::Tuple(items.iter().map(|t| gen_value(rng, t, edge)).collect())
        }
        DynSolType::Function => DynSolValue::Function(alloy_primitives::Function::ZERO),
    }
}

fn uint_mask(bits: usize) -> U256 {
    if bits >= 256 {
        U256::MAX
    } else {
        (U256::from(1u64) << bits) - U256::from(1u64)
    }
}

fn gen_uint(rng: &mut SplitMix64, bits: usize, edge: Edge) -> U256 {
    match edge {
        Edge::Edge => {
            if rng.boolean() {
                U256::ZERO
            } else {
                uint_mask(bits)
            }
        }
        Edge::Random => match rng.below(3) {
            // tiny (0..=8) — exercises enum discriminants (value-map $cases like
            // Aave interestRateMode 1/2, Balancer SwapKind 0/1)
            0 => U256::from(rng.below(9)) & uint_mask(bits),
            // small (≤ u64) — exercises the ≤64-bit JSON-number coercion path
            1 => U256::from(rng.next_u64()) & uint_mask(bits),
            // full-width random — exercises the >64-bit decimal-string path
            _ => {
                let bytes = rng.bytes(32);
                U256::from_be_slice(&bytes) & uint_mask(bits)
            }
        },
    }
}

/// Generate a signed int **within the declared `bits` range** so it round-trips
/// through narrow ABI ints (e.g. int24 ticks → i32 ActionBody fields) without a
/// serde range error.
fn gen_int(rng: &mut SplitMix64, bits: usize, edge: Edge) -> I256 {
    match edge {
        Edge::Edge => I256::ZERO,
        #[allow(clippy::cast_possible_wrap)]
        Edge::Random => {
            let raw = rng.next_u64() as i64;
            let v = if bits >= 64 {
                raw
            } else {
                // map into the signed range [-2^(bits-1), 2^(bits-1))
                let half = 1i64 << (bits - 1);
                raw.rem_euclid(2 * half) - half
            };
            I256::try_from(v).unwrap_or(I256::ZERO)
        }
    }
}

fn gen_address(rng: &mut SplitMix64, edge: Edge) -> Address {
    match edge {
        Edge::Edge => Address::ZERO,
        Edge::Random => Address::from_slice(&rng.bytes(20)),
    }
}

fn gen_word(rng: &mut SplitMix64, n: usize, edge: Edge) -> B256 {
    let mut word = [0u8; 32];
    if matches!(edge, Edge::Random) {
        let b = rng.bytes(n);
        word[..n].copy_from_slice(&b);
    }
    B256::from(word)
}

fn gen_bytes(rng: &mut SplitMix64, edge: Edge) -> Vec<u8> {
    match edge {
        Edge::Edge => Vec::new(),
        Edge::Random => {
            let len = rng.below(40) as usize;
            rng.bytes(len)
        }
    }
}

fn gen_string(rng: &mut SplitMix64, edge: Edge) -> String {
    match edge {
        Edge::Edge => String::new(),
        Edge::Random => {
            let len = rng.below(16) as usize;
            (0..len)
                .map(|_| char::from(b'a' + (rng.below(26) as u8)))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{abi_input_to_soltype, gen_value, Edge};
    use crate::harness::adapters::AbiInput;
    use crate::harness::prng::SplitMix64;
    use alloy_dyn_abi::DynSolType;

    fn input(ty: &str) -> AbiInput {
        AbiInput {
            name: "x".into(),
            ty: ty.into(),
            components: None,
        }
    }

    #[test]
    fn parses_scalars_and_arrays() {
        assert!(matches!(
            abi_input_to_soltype(&input("uint256")).unwrap(),
            DynSolType::Uint(256)
        ));
        assert!(matches!(
            abi_input_to_soltype(&input("address[]")).unwrap(),
            DynSolType::Array(_)
        ));
        assert!(matches!(
            abi_input_to_soltype(&input("bytes32[3]")).unwrap(),
            DynSolType::FixedArray(_, 3)
        ));
    }

    #[test]
    fn parses_tuple_from_components() {
        let t = AbiInput {
            name: "p".into(),
            ty: "tuple".into(),
            components: Some(vec![input("address"), input("uint256")]),
        };
        let st = abi_input_to_soltype(&t).unwrap();
        match st {
            DynSolType::Tuple(items) => assert_eq!(items.len(), 2),
            other => panic!("expected tuple, got {other:?}"),
        }
    }

    #[test]
    fn gen_value_is_deterministic() {
        let ty = abi_input_to_soltype(&input("uint256")).unwrap();
        let a = gen_value(&mut SplitMix64::new(9), &ty, Edge::Random);
        let b = gen_value(&mut SplitMix64::new(9), &ty, Edge::Random);
        assert_eq!(a, b);
    }
}
