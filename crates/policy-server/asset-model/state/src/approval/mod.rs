//! `ApprovalSet` — wallet-level approval collections split by scope. spec §4.4.
//!
//! Only ERC721 *per-token* `approve(tokenId, spender)` is nested under
//! `TokenHolding.approved_to` (it is 1:1 with that holding, so this is natural).
//! Everything else lives here as flat collections.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tsify_next::Tsify;

pub mod erc20;
pub mod permit2;

pub use erc20::AllowanceSpec;
pub use permit2::Permit2Allowance;

use crate::primitives::{Address, ChainId, Spender};

/// Identifies a single contract by `(chain, contract address)`.
pub type ContractAddrKey = (ChainId, Address);

/// Identifies a single `(chain, contract, spender)` triple.
pub type SpenderKey = (ChainId, Address, Spender);

/// Wallet-level set of token approvals, split into one flat collection per scope.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ApprovalSet {
    /// ERC20 allowances.
    /// `(chain, token contract)` → per-spender limit.
    /// (Tuple key, so serialized as JSON pairs.)
    #[serde(default, with = "crate::serde_helpers::map_as_pairs")]
    #[tsify(type = "Array<[[ChainId, string], Array<[string, AllowanceSpec]>]>")]
    pub erc20: BTreeMap<ContractAddrKey, BTreeMap<Spender, AllowanceSpec>>,

    /// ERC721/ERC1155 `setApprovalForAll`.
    /// `(chain, NFT/1155 contract)` → spenders granted set-for-all approval.
    #[serde(default, with = "crate::serde_helpers::map_as_pairs")]
    #[tsify(type = "Array<[[ChainId, string], Array<string>]>")]
    pub set_for_all: BTreeMap<ContractAddrKey, BTreeSet<Spender>>,

    /// Allowances as recorded by the Permit2 contract.
    /// `(chain, token contract, spender)` → limit.
    #[serde(default, with = "crate::serde_helpers::map_as_pairs")]
    #[tsify(type = "Array<[[ChainId, string, string], Permit2Allowance]>")]
    pub permit2: BTreeMap<SpenderKey, Permit2Allowance>,
}

impl ApprovalSet {
    /// Creates an empty `ApprovalSet` with no recorded approvals.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks up the ERC20 allowance for `spender` on the `(chain, token)` contract, if any.
    #[must_use]
    pub fn allowance(&self, key: &ContractAddrKey, spender: &Spender) -> Option<&AllowanceSpec> {
        self.erc20.get(key).and_then(|m| m.get(spender))
    }

    /// Returns whether `spender` holds set-for-all approval on the `(chain, contract)` collection.
    #[must_use]
    pub fn has_set_for_all(&self, key: &ContractAddrKey, spender: &Spender) -> bool {
        self.set_for_all
            .get(key)
            .is_some_and(|s| s.contains(spender))
    }

    /// Looks up the Permit2 allowance for the `(chain, token, spender)` triple, if any.
    #[must_use]
    pub fn permit2_of(&self, key: &SpenderKey) -> Option<&Permit2Allowance> {
        self.permit2.get(key)
    }
}
