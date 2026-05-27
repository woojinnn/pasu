//! WalletState — 한 지갑의 온체인 사실 스냅샷. spec §3.
//!
//! Sync Orchestrator 가 LiveField 를 갱신하고, Reducer 가 action 적용 시
//! in-place 로 수정한다.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::approval::ApprovalSet;
use crate::pending::PendingTx;
use crate::position::Position;
use crate::primitives::{Address, BlockHeight, ChainId};
use crate::token::{TokenHolding, TokenKey};

/// (account address, 추적 chain set).
/// EVM 은 address 가 chain 간 공통이라 single Address.
/// 비-EVM 추가 시 (예: Solana) confederate identity 가 필요 — 후속.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WalletId {
    pub address: Address,
    pub chains: BTreeSet<ChainId>,
}

impl WalletId {
    pub fn new(address: Address, chains: impl IntoIterator<Item = ChainId>) -> Self {
        Self {
            address,
            chains: chains.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletState {
    pub wallet_id: WalletId,

    /// per-instance fungibility 단위로 holding 1개.
    /// (TokenKey 가 enum 이라 JSON object key 로 못 쓰므로 pairs 로 직렬화.)
    #[serde(default, with = "crate::serde_helpers::map_as_pairs")]
    pub tokens: BTreeMap<TokenKey, TokenHolding>,

    /// scope 별로 분리된 wallet-level 권한 컬렉션.
    #[serde(default)]
    pub approvals: ApprovalSet,

    /// 토큰 형태가 아닌 protocol-tracked 권리/상태.
    #[serde(default)]
    pub positions: Vec<Position>,

    /// 서명-only / 미체결 entries.
    #[serde(default)]
    pub pending: Vec<PendingTx>,

    /// 마지막 sync 시점의 체인별 블록.
    #[serde(default)]
    pub block_heights: BTreeMap<ChainId, BlockHeight>,
}

impl WalletState {
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

    /// 정책 view 헬퍼 — 특정 토큰의 사용 가능 잔액 (balance - committed).
    /// Owned NFT 같은 경우 None.
    pub fn available_balance(&self, key: &TokenKey) -> Option<crate::primitives::U256> {
        self.tokens.get(key).and_then(|h| h.available())
    }

    /// 한 spender 에 부여된 모든 approval 을 평탄하게 walk (cross-chain 정책용).
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

/// `all_approvals_to` walker 결과.
#[derive(Debug)]
pub enum ApprovalEntry<'a> {
    Erc20 {
        contract: crate::approval::ContractAddrKey,
        spec: &'a crate::approval::AllowanceSpec,
    },
    SetForAll {
        contract: crate::approval::ContractAddrKey,
    },
    Permit2 {
        key: crate::approval::SpenderKey,
        allowance: &'a crate::approval::Permit2Allowance,
    },
}
