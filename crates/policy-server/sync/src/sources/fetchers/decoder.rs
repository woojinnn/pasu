use std::collections::HashMap;

use alloy_primitives::{keccak256, Address, U256};
use serde_json::{json, Value};

use crate::error::SyncError;

// ============ Selector encoding ============

/// "balanceOf(address)" → [0x70, 0xa0, 0x82, 0x31]
#[must_use]
pub fn function_selector(signature: &str) -> [u8; 4] {
    let hash = keccak256(signature.as_bytes());
    let mut out = [0u8; 4];
    out.copy_from_slice(&hash[..4]);
    out
}

#[must_use]
pub fn encode_call(signature: &str, args_encoded: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + args_encoded.len());
    out.extend_from_slice(&function_selector(signature));
    out.extend_from_slice(args_encoded);
    out
}

#[must_use]
pub fn encode_address(addr: Address) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(addr.as_slice());
    out
}

#[must_use]
pub const fn encode_u256(v: U256) -> [u8; 32] {
    v.to_be_bytes::<32>()
}

// ============ Decoder registry ============

pub type DecodeFn = fn(&[u8]) -> Result<Value, SyncError>;

#[derive(Default)]
pub struct DecoderRegistry {
    by_id: HashMap<String, DecodeFn>,
}

impl DecoderRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.register("u256", decode_u256_as_string);
        r.register("erc20_balance", decode_u256_as_string);
        r.register("erc20_allowance", decode_u256_as_string);
        r.register("erc20_total_supply", decode_u256_as_string);
        r.register("permit2_allowance", decode_permit2_allowance);
        r.register("aave_user_data", decode_aave_user_data);
        r.register("bool", decode_bool);
        r.register("address", decode_address);
        r
    }

    pub fn register(&mut self, id: &str, f: DecodeFn) {
        self.by_id.insert(id.to_string(), f);
    }

    pub fn decode(&self, decoder_id: &str, data: &[u8]) -> Result<Value, SyncError> {
        let f = self
            .by_id
            .get(decoder_id)
            .copied()
            .ok_or_else(|| SyncError::UnknownDecoder(decoder_id.to_string()))?;
        f(data)
    }
}

// ============ Built-in decoders ============

pub fn decode_u256_as_string(data: &[u8]) -> Result<Value, SyncError> {
    if data.len() < 32 {
        return Err(SyncError::FetchFailed {
            source_id: "decoder".into(),
            reason: format!("u256 decoder needs >=32 bytes, got {}", data.len()),
        });
    }
    let v = U256::from_be_slice(&data[..32]);
    Ok(Value::String(v.to_string()))
}

pub fn decode_bool(data: &[u8]) -> Result<Value, SyncError> {
    if data.len() < 32 {
        return Err(SyncError::FetchFailed {
            source_id: "decoder".into(),
            reason: "bool decoder needs >=32 bytes".into(),
        });
    }
    Ok(Value::Bool(data[31] != 0))
}

pub fn decode_address(data: &[u8]) -> Result<Value, SyncError> {
    if data.len() < 32 {
        return Err(SyncError::FetchFailed {
            source_id: "decoder".into(),
            reason: "address decoder needs >=32 bytes".into(),
        });
    }
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&data[12..32]);
    Ok(Value::String(format!("0x{}", hex::encode(addr))))
}

/// Permit2 `allowance(owner, token, spender)` → (amount: uint160, expiration: uint48, nonce: uint48)
pub fn decode_permit2_allowance(data: &[u8]) -> Result<Value, SyncError> {
    if data.len() < 96 {
        return Err(SyncError::FetchFailed {
            source_id: "decoder".into(),
            reason: format!("permit2 needs >=96 bytes, got {}", data.len()),
        });
    }
    let amount = U256::from_be_slice(&data[0..32]);
    let expiration = U256::from_be_slice(&data[32..64]);
    let nonce = U256::from_be_slice(&data[64..96]);
    Ok(json!({
        "amount": amount.to_string(),
        "expiration": expiration.to_string(),
        "nonce": nonce.to_string(),
    }))
}

/// Aave V3 `Pool.getUserAccountData(user)`:
/// (totalCollateralBase, totalDebtBase, availableBorrowsBase,
///  currentLiquidationThreshold, ltv, healthFactor) — 6 × uint256
pub fn decode_aave_user_data(data: &[u8]) -> Result<Value, SyncError> {
    if data.len() < 192 {
        return Err(SyncError::FetchFailed {
            source_id: "decoder".into(),
            reason: format!("aave_user_data needs >=192 bytes, got {}", data.len()),
        });
    }
    let total_collateral = U256::from_be_slice(&data[0..32]);
    let total_debt = U256::from_be_slice(&data[32..64]);
    let available_borrows = U256::from_be_slice(&data[64..96]);
    let liq_threshold = U256::from_be_slice(&data[96..128]);
    let ltv = U256::from_be_slice(&data[128..160]);
    let health_factor = U256::from_be_slice(&data[160..192]);
    Ok(json!({
        "totalCollateralBase": total_collateral.to_string(),
        "totalDebtBase": total_debt.to_string(),
        "availableBorrowsBase": available_borrows.to_string(),
        "currentLiquidationThreshold": liq_threshold.to_string(),
        "ltv": ltv.to_string(),
        "healthFactor": health_factor.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_match_known() {
        // balanceOf(address) = 0x70a08231
        assert_eq!(
            function_selector("balanceOf(address)"),
            [0x70, 0xa0, 0x82, 0x31]
        );
        // allowance(address,address) = 0xdd62ed3e
        assert_eq!(
            function_selector("allowance(address,address)"),
            [0xdd, 0x62, 0xed, 0x3e]
        );
        // totalSupply() = 0x18160ddd
        assert_eq!(function_selector("totalSupply()"), [0x18, 0x16, 0x0d, 0xdd]);
    }

    #[test]
    fn decodes_u256() {
        let mut data = [0u8; 32];
        data[31] = 42;
        let r = decode_u256_as_string(&data).unwrap();
        assert_eq!(r, Value::String("42".to_string()));
    }

    #[test]
    fn registry_with_builtins() {
        let r = DecoderRegistry::with_builtins();
        let mut data = [0u8; 32];
        data[31] = 7;
        let v = r.decode("erc20_balance", &data).unwrap();
        assert_eq!(v, Value::String("7".to_string()));
    }

    #[test]
    fn unknown_decoder_errors() {
        let r = DecoderRegistry::with_builtins();
        let err = r.decode("nonexistent", &[]).unwrap_err();
        assert!(matches!(err, SyncError::UnknownDecoder(_)));
    }
}
