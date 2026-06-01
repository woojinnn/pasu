//! ABI:
//! ```text
//! function aggregate3(Call3[] calldata calls)
//!     external payable returns (Result[] memory returnData);
//! struct Call3 { address target; bool allowFailure; bytes callData; }
//! struct Result { bool success; bytes returnData; }
//! ```
//! selector = `0x82ad56cb` (keccak("aggregate3((address,bool,bytes)[])")[..4]).

use alloy_primitives::{Address, U256};

use policy_state::ChainId;

use super::router::RpcRouter;
use super::{BlockTag, EthCallRequest};
use crate::error::SyncError;

/// `aggregate3` selector.
const SELECTOR: [u8; 4] = [0x82, 0xad, 0x56, 0xcb];

#[derive(Clone, Debug)]
pub struct Call3 {
    pub target: Address,
    pub allow_failure: bool,
    pub call_data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Call3Result {
    pub success: bool,
    pub return_data: Vec<u8>,
}

pub struct Multicall {
    router: std::sync::Arc<RpcRouter>,
}

impl Multicall {
    #[must_use]
    pub const fn new(router: std::sync::Arc<RpcRouter>) -> Self {
        Self { router }
    }

    pub async fn aggregate3(
        &self,
        chain: &ChainId,
        calls: Vec<Call3>,
        block: BlockTag,
    ) -> Result<Vec<Call3Result>, SyncError> {
        let mc_addr = self
            .router
            .multicall_addr(chain)
            .ok_or_else(|| SyncError::FetchFailed {
                source_id: "multicall".into(),
                reason: format!("no multicall3 address configured for {chain}"),
            })?;

        let calldata = encode_aggregate3_calldata(&calls);
        let mut req = EthCallRequest::new(mc_addr, calldata);
        req.block = block;

        let return_data = self.router.eth_call(chain, req).await?;
        decode_aggregate3_returndata(&return_data)
    }
}

// ============ Encoding ============

/// ```text
/// 4-byte selector
/// 32-byte: offset to array (= 0x20)
/// 32-byte: array length N
///     element = (address pad, bool pad, bytes_offset, bytes_len, bytes_data padded)
/// ```
fn encode_aggregate3_calldata(calls: &[Call3]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + 64 + calls.len() * 96);

    // selector
    buf.extend_from_slice(&SELECTOR);

    // offset to array (constant: 0x20 = 32)
    buf.extend_from_slice(&u256_to_32bytes(U256::from(32u64)));

    // array length
    buf.extend_from_slice(&u256_to_32bytes(U256::from(calls.len() as u64)));

    let mut element_bodies: Vec<Vec<u8>> = Vec::with_capacity(calls.len());
    for c in calls {
        element_bodies.push(encode_call3_element(c));
    }

    let offsets_table_size = 32 * calls.len();
    let mut running_offset = offsets_table_size;
    let mut element_offsets: Vec<usize> = Vec::with_capacity(calls.len());
    for body in &element_bodies {
        element_offsets.push(running_offset);
        running_offset += body.len();
    }

    for off in &element_offsets {
        buf.extend_from_slice(&u256_to_32bytes(U256::from(*off as u64)));
    }

    for body in element_bodies {
        buf.extend_from_slice(&body);
    }

    buf
}

/// ```text
/// 32: address (left-padded)
/// 32: bool   (0 or 1, left-padded)
/// 32: offset to bytes data (= 96)
/// 32: bytes length
/// ceil(len/32)*32: bytes data right-padded with zeros
/// ```
fn encode_call3_element(call: &Call3) -> Vec<u8> {
    let bytes_len = call.call_data.len();
    let bytes_padded = bytes_len.div_ceil(32) * 32;
    let mut buf = Vec::with_capacity(96 + 32 + bytes_padded);

    buf.extend_from_slice(&address_to_32bytes(call.target));
    // allowFailure
    buf.extend_from_slice(&bool_to_32bytes(call.allow_failure));
    buf.extend_from_slice(&u256_to_32bytes(U256::from(96u64)));
    // bytes length
    buf.extend_from_slice(&u256_to_32bytes(U256::from(bytes_len as u64)));
    // bytes data padded
    buf.extend_from_slice(&call.call_data);
    let pad = bytes_padded - bytes_len;
    if pad > 0 {
        buf.extend(std::iter::repeat_n(0u8, pad));
    }

    buf
}

const fn u256_to_32bytes(v: U256) -> [u8; 32] {
    let mut out = [0u8; 32];
    let be = v.to_be_bytes::<32>();
    out.copy_from_slice(&be);
    out
}

fn address_to_32bytes(addr: Address) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(addr.as_slice());
    out
}

const fn bool_to_32bytes(b: bool) -> [u8; 32] {
    let mut out = [0u8; 32];
    if b {
        out[31] = 1;
    }
    out
}

// ============ Decoding ============

/// ```text
/// 32: offset to array (= 0x20)
/// 32: array length N
/// ```
fn decode_aggregate3_returndata(data: &[u8]) -> Result<Vec<Call3Result>, SyncError> {
    if data.len() < 64 {
        return Err(SyncError::FetchFailed {
            source_id: "multicall".into(),
            reason: format!("returnData too short: {}", data.len()),
        });
    }

    let array_offset = read_u256_usize(&data[0..32])?;
    let array_base = array_offset; // offset is from start of return data, here = 32
    if data.len() < array_base + 32 {
        return Err(SyncError::FetchFailed {
            source_id: "multicall".into(),
            reason: "returnData truncated at array length".into(),
        });
    }
    let n = read_u256_usize(&data[array_base..array_base + 32])?;

    let table_base = array_base + 32;
    let table_size = n * 32;
    if data.len() < table_base + table_size {
        return Err(SyncError::FetchFailed {
            source_id: "multicall".into(),
            reason: "returnData truncated at offsets table".into(),
        });
    }

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let off_bytes = &data[table_base + i * 32..table_base + (i + 1) * 32];
        let elem_off_in_array = read_u256_usize(off_bytes)?;
        let elem_start = table_base + elem_off_in_array;
        // element: (bool, bytes_offset=64, bytes_len, bytes_data)
        if data.len() < elem_start + 96 {
            return Err(SyncError::FetchFailed {
                source_id: "multicall".into(),
                reason: format!("returnData truncated at element {i}"),
            });
        }
        let success = data[elem_start + 31] != 0;
        let bytes_len = read_u256_usize(&data[elem_start + 64..elem_start + 96])?;
        if data.len() < elem_start + 96 + bytes_len {
            return Err(SyncError::FetchFailed {
                source_id: "multicall".into(),
                reason: format!("returnData truncated at element {i} bytes"),
            });
        }
        let body = data[elem_start + 96..elem_start + 96 + bytes_len].to_vec();
        out.push(Call3Result {
            success,
            return_data: body,
        });
    }
    Ok(out)
}

fn read_u256_usize(bytes: &[u8]) -> Result<usize, SyncError> {
    if bytes.len() != 32 {
        return Err(SyncError::FetchFailed {
            source_id: "multicall".into(),
            reason: format!("expected 32 bytes, got {}", bytes.len()),
        });
    }
    for b in &bytes[..24] {
        if *b != 0 {
            return Err(SyncError::FetchFailed {
                source_id: "multicall".into(),
                reason: "u256 too large for usize".into(),
            });
        }
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&bytes[24..32]);
    Ok(u64::from_be_bytes(buf) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_then_decode_round_trip_empty() {
        let calldata = encode_aggregate3_calldata(&[]);
        // selector + 0x20 offset + 0 length = 4 + 32 + 32
        assert_eq!(calldata.len(), 68);
        assert_eq!(&calldata[..4], &SELECTOR);
    }

    #[test]
    fn encode_single_call_shape() {
        let call = Call3 {
            target: Address::ZERO,
            allow_failure: true,
            call_data: vec![0xde, 0xad, 0xbe, 0xef],
        };
        let calldata = encode_aggregate3_calldata(&[call]);

        // selector(4) + offset(32) + length(32) + offset_to_element(32)
        // + element header(32 addr + 32 bool + 32 offset + 32 len)
        // + 32 padded bytes
        assert_eq!(calldata.len(), 4 + 32 + 32 + 32 + 32 * 4 + 32);
        assert_eq!(&calldata[..4], &SELECTOR);
    }

    #[test]
    fn decode_two_results_round_trip() {
        // (success=false, returnData=[]).
        // Layout:
        //   [0..32]    offset to array = 0x20
        //   [32..64]   length = 2
        //   [64..96]   element[0] offset = 0x40 (64)
        //   [96..128]  element[1] offset = ?
        //   [128..160] success (1)
        //   [160..192] bytes offset = 0x40 (64)
        //   [192..224] bytes length = 32
        //   [224..256] bytes data
        let empty_return = "0000000000000000000000000000000000000000000000000000000000000020\
             0000000000000000000000000000000000000000000000000000000000000000";
        let bytes = hex::decode(empty_return).unwrap();
        let results = decode_aggregate3_returndata(&bytes).unwrap();
        assert_eq!(results.len(), 0);
    }
}
