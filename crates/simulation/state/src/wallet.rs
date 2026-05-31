//! `WalletState` — on-chain fact snapshot of a single wallet. spec §3.
//!
//! The Sync Orchestrator refreshes the `LiveField`s, and the Reducer mutates
//! the state in place when applying an action.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tsify_next::Tsify;

use crate::approval::ApprovalSet;
use crate::pending::PendingTx;
use crate::position::Position;
use crate::primitives::{Address, BlockHeight, ChainId};
use crate::token::{TokenHolding, TokenKey};

/// Wallet identity: an account address plus the set of tracked chains.
///
/// On EVM the address is shared across chains, so a single `Address` suffices.
/// Adding non-EVM chains (e.g. Solana) would require a federated identity — future work.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WalletId {
    /// Account address (shared across all EVM chains).
    #[tsify(type = "string")]
    pub address: Address,
    /// Set of chains being tracked for this account.
    #[tsify(type = "Array<ChainId>")]
    pub chains: BTreeSet<ChainId>,
}

impl WalletId {
    /// Builds a `WalletId` from an address and a set of tracked chains.
    pub fn new(address: Address, chains: impl IntoIterator<Item = ChainId>) -> Self {
        Self {
            address,
            chains: chains.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
/// On-chain fact snapshot for a single wallet (spec §3).
pub struct WalletState {
    /// Identity (address + tracked chains) this snapshot belongs to.
    pub wallet_id: WalletId,

    /// One holding per fungibility instance.
    /// (`TokenKey` is an enum, so it can't be a JSON object key; serialized as pairs.)
    #[serde(default, with = "crate::serde_helpers::map_as_pairs")]
    #[tsify(type = "Array<[TokenKey, TokenHolding]>")]
    pub tokens: BTreeMap<TokenKey, TokenHolding>,

    /// Wallet-level approvals, partitioned by scope.
    #[serde(default)]
    pub approvals: ApprovalSet,

    /// Protocol-tracked rights/state that are not held as tokens.
    #[serde(default)]
    pub positions: Vec<Position>,

    /// Signature-only / unsettled entries.
    #[serde(default)]
    pub pending: Vec<PendingTx>,

    /// Per-chain block at the last sync point.
    #[serde(default)]
    #[tsify(type = "Array<[ChainId, BlockHeight]>")]
    pub block_heights: BTreeMap<ChainId, BlockHeight>,
}

impl WalletState {
    /// Creates an empty `WalletState` for the given wallet identity.
    #[must_use]
    pub fn new(wallet_id: WalletId) -> Self {
        Self {
            wallet_id,
            tokens: BTreeMap::new(),
            approvals: ApprovalSet::default(),
            positions: Vec::new(),
            pending: Vec::new(),
            block_heights: BTreeMap::new(),
        }
    }

    /// Policy-view helper: spendable balance of a token (balance - committed).
    /// Returns `None` for holdings without a spendable amount, e.g. owned NFTs.
    #[must_use]
    pub fn available_balance(&self, key: &TokenKey) -> Option<crate::primitives::U256> {
        self.tokens
            .get(key)
            .and_then(super::token::holding::TokenHolding::available)
    }

    /// Flatly walks every approval granted to a single spender (for cross-chain policy).
    pub fn all_approvals_to<'a>(
        &'a self,
        spender: &'a crate::primitives::Spender,
    ) -> impl Iterator<Item = ApprovalEntry<'a>> + 'a {
        let erc20 = self.approvals.erc20.iter().flat_map(move |(key, m)| {
            m.iter().filter_map(move |(s, alw)| {
                if s == spender {
                    Some(ApprovalEntry::Erc20 {
                        contract: key.clone(),
                        spec: alw,
                    })
                } else {
                    None
                }
            })
        });
        let sfa = self.approvals.set_for_all.iter().filter_map(move |(k, s)| {
            if s.contains(spender) {
                Some(ApprovalEntry::SetForAll {
                    contract: k.clone(),
                })
            } else {
                None
            }
        });
        let p2 = self.approvals.permit2.iter().filter_map(move |(k, a)| {
            if &k.2 == spender {
                Some(ApprovalEntry::Permit2 {
                    key: k.clone(),
                    allowance: a,
                })
            } else {
                None
            }
        });
        erc20.chain(sfa).chain(p2)
    }
}

/// One result yielded by the `all_approvals_to` walker.
#[derive(Debug)]
pub enum ApprovalEntry<'a> {
    /// An ERC-20 token allowance granted to the spender.
    Erc20 {
        /// Token contract the allowance applies to.
        contract: crate::approval::ContractAddrKey,
        /// The allowance amount/spec granted on that contract.
        spec: &'a crate::approval::AllowanceSpec,
    },
    /// A collection-wide (set-for-all) operator approval.
    SetForAll {
        /// Token contract the operator is approved for.
        contract: crate::approval::ContractAddrKey,
    },
    /// A Permit2 allowance entry.
    Permit2 {
        /// Composite key (token, spender, ...) identifying the Permit2 grant.
        key: crate::approval::SpenderKey,
        /// The Permit2 allowance (amount + expiration + nonce) for that key.
        allowance: &'a crate::approval::Permit2Allowance,
    },
}
