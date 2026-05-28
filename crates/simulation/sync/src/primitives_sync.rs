//! Primitives sync — LiveField 가 아닌 "사실" 필드들의 RPC 갱신.
//!
//! `TokenHolding.balance`, `approvals_erc20`, `block_heights` 는 LiveField 가
//! 아니라 plain 필드라 walker 가 못 잡는다. 이들은 별도 경로로 sync 한다:
//!
//! - block_heights : eth_blockNumber (chain 별)
//! - native balance: eth_getBalance (chain 별, Native holding)
//! - ERC20 balance : balanceOf(owner) (Multicall3 로 chain 별 묶음)
//! - approvals     : allowance(owner, spender) (이미 알고있는 entry 만 refresh)
//!
//! 새 approval / 새 토큰 발견 (event log indexing) 은 본 모듈 범위 밖 — subscription
//! 또는 indexer 의 일.

use alloy_primitives::U256;

use simulation_state::{Balance, BlockHeight, Time, TokenKey, WalletState};

use crate::error::SyncError;
use crate::fetchers::decoder::{encode_address, function_selector};
use crate::fetchers::rpc::multicall::{Call3, Multicall};
use crate::fetchers::rpc::{BlockTag, EthCallRequest};
use crate::orchestrator::Orchestrator;

#[derive(Debug, Default, Clone)]
pub struct PrimitivesReport {
    pub block_heights_updated: usize,
    pub native_balances_updated: usize,
    pub erc20_balances_updated: usize,
    pub approvals_updated: usize,
    pub errors: Vec<String>,
}

impl Orchestrator {
    /// LiveField 가 아닌 primitive 필드들을 RPC 로 갱신.
    pub async fn sync_primitives(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<PrimitivesReport, SyncError> {
        let router = match self.router_ref() {
            Some(r) => r,
            None => {
                return Ok(PrimitivesReport {
                    errors: vec!["no router configured".into()],
                    ..Default::default()
                });
            }
        };

        let mut report = PrimitivesReport::default();
        let owner = state.wallet_id.address;
        let chains: Vec<_> = state.wallet_id.chains.iter().cloned().collect();

        // 1. block heights
        for chain in &chains {
            match router.eth_block_number(chain).await {
                Ok(n) => {
                    state.block_heights.insert(
                        chain.clone(),
                        BlockHeight {
                            number: n,
                            time: now.as_unix(),
                        },
                    );
                    report.block_heights_updated += 1;
                }
                Err(e) => report.errors.push(format!("blockNumber {}: {}", chain, e)),
            }
        }

        // 2. native balances (chain 별)
        for chain in &chains {
            let native_key = TokenKey::Native {
                chain: chain.clone(),
            };
            if state.tokens.contains_key(&native_key) {
                match router.eth_balance(chain, owner, BlockTag::Latest).await {
                    Ok(bal) => {
                        if let Some(h) = state.tokens.get_mut(&native_key) {
                            h.balance = Balance::Fungible { amount: bal };
                            h.last_synced_at = now;
                            report.native_balances_updated += 1;
                        }
                    }
                    Err(e) => report.errors.push(format!("balance native {}: {}", chain, e)),
                }
            }
        }

        // 3. ERC20 balances — chain 별 multicall
        for chain in &chains {
            let erc20_keys: Vec<TokenKey> = state
                .tokens
                .keys()
                .filter(|k| matches!(k, TokenKey::Erc20 { chain: c, .. } if c == chain))
                .cloned()
                .collect();

            if erc20_keys.is_empty() {
                continue;
            }

            let selector = function_selector("balanceOf(address)");
            let args = encode_address(owner);
            let calls: Vec<Call3> = erc20_keys
                .iter()
                .map(|k| {
                    let contract = match k {
                        TokenKey::Erc20 { address, .. } => *address,
                        _ => unreachable!(),
                    };
                    let mut data = Vec::with_capacity(36);
                    data.extend_from_slice(&selector);
                    data.extend_from_slice(&args);
                    Call3 {
                        target: contract,
                        allow_failure: true,
                        call_data: data,
                    }
                })
                .collect();

            let multicall = Multicall::new(router.clone());
            match multicall.aggregate3(chain, calls, BlockTag::Latest).await {
                Ok(results) => {
                    for (key, res) in erc20_keys.iter().zip(results.iter()) {
                        if res.success && res.return_data.len() >= 32 {
                            let bal = U256::from_be_slice(&res.return_data[..32]);
                            if let Some(h) = state.tokens.get_mut(key) {
                                h.balance = Balance::Fungible { amount: bal };
                                h.last_synced_at = now;
                                report.erc20_balances_updated += 1;
                            }
                        }
                    }
                }
                Err(e) => report.errors.push(format!("erc20 multicall {}: {}", chain, e)),
            }
        }

        // 4. 기존 approvals_erc20 refresh — allowance(owner, spender)
        report.approvals_updated += self.refresh_known_approvals(state, owner, now).await;

        Ok(report)
    }

    async fn refresh_known_approvals(
        &self,
        state: &mut WalletState,
        owner: alloy_primitives::Address,
        now: Time,
    ) -> usize {
        let router = match self.router_ref() {
            Some(r) => r,
            None => return 0,
        };

        // 갱신할 (chain, token, spender) 목록을 먼저 수집 (immutable borrow 끝낸 뒤 write).
        let mut to_refresh: Vec<(simulation_state::ChainId, alloy_primitives::Address, alloy_primitives::Address)> =
            Vec::new();
        for ((chain, token), spenders) in &state.approvals.erc20 {
            for spender in spenders.keys() {
                to_refresh.push((chain.clone(), *token, *spender));
            }
        }

        let selector = function_selector("allowance(address,address)");
        let mut updated = 0;
        for (chain, token, spender) in to_refresh {
            let mut data = Vec::with_capacity(68);
            data.extend_from_slice(&selector);
            data.extend_from_slice(&encode_address(owner));
            data.extend_from_slice(&encode_address(spender));
            let req = EthCallRequest::new(token, data);
            if let Ok(ret) = router.eth_call(&chain, req).await {
                if ret.len() >= 32 {
                    let amount = U256::from_be_slice(&ret[..32]);
                    if let Some(map) = state.approvals.erc20.get_mut(&(chain.clone(), token)) {
                        if let Some(spec) = map.get_mut(&spender) {
                            spec.amount = amount;
                            spec.is_unlimited = amount == U256::MAX;
                            spec.last_set_at = now;
                            updated += 1;
                        }
                    }
                }
            }
        }
        updated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::{Address, ChainId, WalletId, WalletState};

    #[tokio::test]
    async fn sync_primitives_no_router_reports_error() {
        // router 없는 orchestrator → graceful error
        let onchain_router = {
            let toml = r#"
[chains."eip155:1"]
[[chains."eip155:1".providers]]
name = "publicnode"
kind = "public"
url = "https://ethereum-rpc.publicnode.com"
priority = 1
"#;
            let cfg = crate::RpcConfig::load_str(toml).unwrap();
            std::sync::Arc::new(crate::RpcRouter::from_config(cfg).unwrap())
        };
        // new() 는 router None → sync_primitives 가 error report
        let orch = Orchestrator::new(crate::fetchers::OnchainViewFetcher::new(onchain_router));
        let mut state =
            WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        let report = orch.sync_primitives(&mut state, Time::from_unix(0)).await.unwrap();
        assert!(!report.errors.is_empty());
    }
}
