//! Safe `multiSend(bytes transactions)` packed-payload decoder.
//!
//! The `transactions` bytes argument is a sequential pack of sub-transactions:
//!
//! ```text
//! [operation: 1B][to: 20B][value: 32B][dataLength: 32B][data: N bytes]  (repeated)
//! ```
//!
//! This is not ABI-encoded — there is no padding or offset header. The caller
//! is responsible for recursing into each sub-tx's `data` via the resolver.

use alloy_primitives::{Address, U256};

/// 4-byte selector for `multiSend(bytes)` (both `MultiSend` and
/// `MultiSendCallOnly`).
pub const MULTISEND_SELECTOR: [u8; 4] = [0x8d, 0x80, 0xff, 0x0a];

/// Minimum bytes required to parse one sub-transaction header
/// (operation + to + value + dataLength, before the variable data).
const HEADER_SIZE: usize = 1 + 20 + 32 + 32;

/// One sub-transaction extracted from a `multiSend` payload.
#[derive(Debug)]
pub struct MultisendTx {
    /// 0 = Call, 1 = DelegateCall.
    pub operation: u8,
    pub to: Address,
    pub value: U256,
    /// Raw calldata for this sub-call. May be empty for plain ETH transfers.
    pub data: Vec<u8>,
}

/// Extract the raw `transactions` bytes from an already-decoded `multiSend`
/// call. Returns `None` when the decoded args don't contain a `bytes` value
/// (e.g. signature matched but ABI shape was unexpected).
pub fn extract_transactions_bytes(decoded: &crate::decode::DecodedCall) -> Option<Vec<u8>> {
    for arg in &decoded.args {
        if let alloy_dyn_abi::DynSolValue::Bytes(b) = &arg.value {
            return Some(b.clone());
        }
    }
    None
}

/// Parse the packed `transactions` bytes into individual sub-transactions.
///
/// Stops early on truncation rather than returning an error so callers always
/// get whatever could be cleanly extracted.
pub fn parse_multisend_transactions(bytes: &[u8]) -> Vec<MultisendTx> {
    let mut out = Vec::new();
    let mut cursor = 0;

    while cursor + HEADER_SIZE <= bytes.len() {
        let operation = bytes[cursor];
        cursor += 1;

        let to = Address::from_slice(&bytes[cursor..cursor + 20]);
        cursor += 20;

        let value = U256::from_be_slice(&bytes[cursor..cursor + 32]);
        cursor += 32;

        let data_length = U256::from_be_slice(&bytes[cursor..cursor + 32]);
        cursor += 32;

        let Ok(data_length) = usize::try_from(data_length) else {
            break;
        };
        if cursor + data_length > bytes.len() {
            break;
        }

        let data = bytes[cursor..cursor + data_length].to_vec();
        cursor += data_length;

        out.push(MultisendTx {
            operation,
            to,
            value,
            data,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_tx(operation: u8, to: [u8; 20], value: u128, data: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(operation);
        buf.extend_from_slice(&to);
        buf.extend_from_slice(&U256::from(value).to_be_bytes::<32>());
        buf.extend_from_slice(&U256::from(data.len()).to_be_bytes::<32>());
        buf.extend_from_slice(data);
        buf
    }

    #[test]
    fn single_call_tx() {
        let calldata = hex::decode("095ea7b3").unwrap();
        let packed = pack_tx(0, [0x11; 20], 0, &calldata);
        let txs = parse_multisend_transactions(&packed);
        assert_eq!(txs.len(), 1);
        assert_eq!(txs[0].operation, 0);
        assert_eq!(txs[0].to, Address::from([0x11; 20]));
        assert_eq!(txs[0].value, U256::ZERO);
        assert_eq!(txs[0].data, calldata);
    }

    #[test]
    fn two_txs_parsed() {
        let data1 = hex::decode("095ea7b3").unwrap();
        let data2 = hex::decode("a9059cbb").unwrap();
        let mut packed = pack_tx(0, [0x11; 20], 0, &data1);
        packed.extend(pack_tx(1, [0x22; 20], 1_000, &data2));
        let txs = parse_multisend_transactions(&packed);
        assert_eq!(txs.len(), 2);
        assert_eq!(txs[1].operation, 1);
        assert_eq!(txs[1].to, Address::from([0x22; 20]));
        assert_eq!(txs[1].value, U256::from(1_000u128));
    }

    #[test]
    fn empty_data_eth_transfer() {
        let packed = pack_tx(0, [0xaa; 20], 1_000_000, &[]);
        let txs = parse_multisend_transactions(&packed);
        assert_eq!(txs.len(), 1);
        assert!(txs[0].data.is_empty());
    }

    #[test]
    fn truncated_payload_stops_early() {
        let packed = pack_tx(0, [0x11; 20], 0, &[0x01, 0x02, 0x03, 0x04]);
        // Chop off the last 2 bytes so the data field is incomplete.
        let truncated = &packed[..packed.len() - 2];
        let txs = parse_multisend_transactions(truncated);
        assert!(txs.is_empty());
    }
}
