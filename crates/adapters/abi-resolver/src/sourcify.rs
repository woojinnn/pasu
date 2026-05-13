//! Sourcify-backed signature lookup.
//!
//! Loads contract ABIs (Sourcify metadata.json shape) and indexes every
//! function by `(chain_id, address_lowercase, selector)` so a transaction can
//! be resolved in one hash lookup.
//!
//! Sourcify itself returns `(chain, address) -> ABI`. The selector for each
//! ABI function is derived from `keccak256(canonical_signature)[0..4]` —
//! `alloy_dyn_abi::DynSolType` + `alloy_primitives::keccak256` give us this
//! for free.

use alloy_json_abi::Function;
use alloy_primitives::Address;
use serde::Deserialize;
use std::collections::HashMap;

/// Per-function info we cache. Argument names come straight from the Sourcify
/// metadata when present; missing names fall back to `arg0, arg1, ...` later
/// at decode time.
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// Function name (e.g. `approve`).
    pub name: String,
    /// Canonical Solidity signature (e.g. `approve(address,uint256)`).
    pub signature: String,
    /// Argument names parallel to the function inputs.
    pub arg_names: Vec<String>,
    /// The full alloy `Function` — `JsonAbiExt::abi_decode_input` consumes it
    /// directly, so the decoder doesn't have to re-parse the signature.
    pub function: Function,
}

/// In-memory index of every function across every loaded contract.
#[derive(Debug, Default)]
pub struct SourcifyIndex {
    by_key: HashMap<(u64, [u8; 20], [u8; 4]), FunctionInfo>,
}

impl SourcifyIndex {
    /// Empty index — useful when Sourcify data hasn't been imported yet.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Insert every function from a Sourcify-style contract entry.
    ///
    /// `chain_id` and `address` are taken verbatim — the address is normalised
    /// to lowercase bytes so callers don't have to think about checksumming.
    pub fn insert_contract(&mut self, chain_id: u64, address: Address, abi: &[Function]) {
        let addr_bytes: [u8; 20] = address.into();
        for function in abi {
            let selector = function.selector().0;
            let arg_names = function
                .inputs
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>();
            let signature = function.signature();
            self.by_key.insert(
                (chain_id, addr_bytes, selector),
                FunctionInfo {
                    name: function.name.clone(),
                    signature,
                    arg_names,
                    function: function.clone(),
                },
            );
        }
    }

    /// Look up a function by `(chain_id, address, selector)`.
    #[must_use]
    pub fn lookup(
        &self,
        chain_id: u64,
        address: &Address,
        selector: [u8; 4],
    ) -> Option<&FunctionInfo> {
        let addr_bytes: [u8; 20] = (*address).into();
        self.by_key.get(&(chain_id, addr_bytes, selector))
    }

    /// Number of functions currently indexed (across every contract).
    #[must_use]
    pub fn function_count(&self) -> usize {
        self.by_key.len()
    }
}

/// On-disk format used to ship a curated batch of Sourcify ABIs together with
/// the binary. Decoupled from the network layout so we can change either side
/// without touching the other.
///
/// Loaded by `SourcifyIndex::load_bundle`. The bundled file lives at
/// `crates/abi-resolver/data/sourcify.json` once step 2's curation script runs.
#[derive(Debug, Deserialize)]
pub struct SourcifyBundle {
    pub contracts: Vec<SourcifyContract>,
}

#[derive(Debug, Deserialize)]
pub struct SourcifyContract {
    pub chain_id: u64,
    pub address: Address,
    /// The contract's ABI in alloy's JSON shape.
    pub abi: Vec<Function>,
}

impl SourcifyIndex {
    /// Load + index a bundle from JSON bytes.
    ///
    /// # Errors
    /// Returns the underlying `serde_json::Error` if the bytes don't deserialize.
    pub fn load_bundle(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        let bundle: SourcifyBundle = serde_json::from_slice(bytes)?;
        let mut index = Self::empty();
        for contract in bundle.contracts {
            index.insert_contract(contract.chain_id, contract.address, &contract.abi);
        }
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_json_abi::{InternalType, Param, StateMutability};

    fn approve_function() -> Function {
        Function {
            name: "approve".into(),
            inputs: vec![
                Param {
                    name: "spender".into(),
                    ty: "address".into(),
                    components: vec![],
                    internal_type: Some(InternalType::AddressPayable("address".into())),
                },
                Param {
                    name: "amount".into(),
                    ty: "uint256".into(),
                    components: vec![],
                    internal_type: None,
                },
            ],
            outputs: vec![Param {
                name: String::new(),
                ty: "bool".into(),
                components: vec![],
                internal_type: None,
            }],
            state_mutability: StateMutability::NonPayable,
        }
    }

    #[test]
    fn insert_and_lookup_round_trip() {
        let mut index = SourcifyIndex::empty();
        let address = Address::from([0x11u8; 20]);
        index.insert_contract(1, address, &[approve_function()]);

        let info = index
            .lookup(1, &address, [0x09, 0x5e, 0xa7, 0xb3])
            .expect("approve should be indexed");
        assert_eq!(info.name, "approve");
        assert_eq!(info.signature, "approve(address,uint256)");
        assert_eq!(info.arg_names, vec!["spender", "amount"]);
    }

    #[test]
    fn lookup_misses_on_wrong_chain() {
        let mut index = SourcifyIndex::empty();
        let address = Address::from([0x11u8; 20]);
        index.insert_contract(1, address, &[approve_function()]);

        assert!(index
            .lookup(137, &address, [0x09, 0x5e, 0xa7, 0xb3])
            .is_none());
    }

    #[test]
    fn lookup_misses_on_wrong_selector() {
        let mut index = SourcifyIndex::empty();
        let address = Address::from([0x11u8; 20]);
        index.insert_contract(1, address, &[approve_function()]);

        assert!(index
            .lookup(1, &address, [0xde, 0xad, 0xbe, 0xef])
            .is_none());
    }

    #[test]
    fn empty_index_has_zero_functions() {
        let index = SourcifyIndex::empty();
        assert_eq!(index.function_count(), 0);
    }

    #[test]
    fn load_bundle_round_trip() {
        let bundle = serde_json::json!({
            "contracts": [{
                "chain_id": 1,
                "address": "0x1111111111111111111111111111111111111111",
                "abi": [{
                    "name": "approve",
                    "type": "function",
                    "inputs": [
                        { "name": "spender", "type": "address" },
                        { "name": "amount",  "type": "uint256" }
                    ],
                    "outputs": [{ "name": "", "type": "bool" }],
                    "stateMutability": "nonpayable"
                }]
            }]
        });
        let bytes = serde_json::to_vec(&bundle).unwrap();
        let index = SourcifyIndex::load_bundle(&bytes).expect("bundle should load");
        assert_eq!(index.function_count(), 1);
    }
}
