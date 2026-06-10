use alloy_primitives::U256;

use policy_state::{Address, Balance, BlockHeight, ChainId, Time, TokenKey, WalletState};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimitiveFetchPlan {
    pub owner: Address,
    pub requests: Vec<PrimitiveFetchRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimitiveFetchRequest {
    BlockHeight {
        chain: ChainId,
    },
    NativeBalance {
        chain: ChainId,
    },
    Erc20Balances {
        chain: ChainId,
        tokens: Vec<Address>,
    },
    Erc20Approval {
        chain: ChainId,
        token: Address,
        spender: Address,
    },
}

enum PrimitiveFetchUpdate {
    BlockHeight {
        chain: ChainId,
        number: u64,
    },
    NativeBalance {
        chain: ChainId,
        amount: U256,
    },
    Erc20Balance {
        chain: ChainId,
        token: Address,
        amount: U256,
    },
    Erc20Approval {
        chain: ChainId,
        token: Address,
        spender: Address,
        amount: U256,
    },
}

#[must_use]
pub fn plan_primitive_fetches(state: &WalletState) -> PrimitiveFetchPlan {
    let owner = state.wallet_id.address;
    let chains: Vec<_> = state.wallet_id.chains.iter().cloned().collect();
    let mut requests = Vec::new();

    for chain in &chains {
        requests.push(PrimitiveFetchRequest::BlockHeight {
            chain: chain.clone(),
        });

        let native_key = TokenKey::Native {
            chain: chain.clone(),
        };
        if state.tokens.contains_key(&native_key) {
            requests.push(PrimitiveFetchRequest::NativeBalance {
                chain: chain.clone(),
            });
        }

        let tokens: Vec<_> = state
            .tokens
            .keys()
            .filter_map(|key| match key {
                TokenKey::Erc20 { chain: c, address } if c == chain => Some(*address),
                _ => None,
            })
            .collect();
        if !tokens.is_empty() {
            requests.push(PrimitiveFetchRequest::Erc20Balances {
                chain: chain.clone(),
                tokens,
            });
        }
    }

    for ((chain, token), spenders) in &state.approvals.erc20 {
        for spender in spenders.keys() {
            requests.push(PrimitiveFetchRequest::Erc20Approval {
                chain: chain.clone(),
                token: *token,
                spender: *spender,
            });
        }
    }

    PrimitiveFetchPlan { owner, requests }
}

impl Orchestrator {
    pub async fn sync_primitives(
        &self,
        state: &mut WalletState,
        now: Time,
    ) -> Result<PrimitivesReport, SyncError> {
        if self.router_ref().is_none() {
            return Ok(PrimitivesReport {
                errors: vec!["no router configured".into()],
                ..Default::default()
            });
        }

        let mut report = PrimitivesReport::default();
        let plan = plan_primitive_fetches(state);

        for request in plan.requests {
            match self.fetch_primitive_update(&request, plan.owner).await {
                Ok(updates) => {
                    for update in updates {
                        apply_primitive_update(state, &mut report, update, now);
                    }
                }
                Err(e) => {
                    report
                        .errors
                        .push(format!("{}: {e}", primitive_request_label(&request)));
                }
            }
        }

        Ok(report)
    }

    async fn fetch_primitive_update(
        &self,
        request: &PrimitiveFetchRequest,
        owner: Address,
    ) -> Result<Vec<PrimitiveFetchUpdate>, SyncError> {
        let Some(router) = self.router_ref() else {
            return Err(SyncError::FetchFailed {
                source_id: primitive_request_label(request),
                reason: "primitive fetch requires a configured RPC router".into(),
            });
        };

        match request {
            PrimitiveFetchRequest::BlockHeight { chain } => {
                let number = router.eth_block_number(chain).await?;
                Ok(vec![PrimitiveFetchUpdate::BlockHeight {
                    chain: chain.clone(),
                    number,
                }])
            }
            PrimitiveFetchRequest::NativeBalance { chain } => {
                let amount = router.eth_balance(chain, owner, BlockTag::Latest).await?;
                Ok(vec![PrimitiveFetchUpdate::NativeBalance {
                    chain: chain.clone(),
                    amount,
                }])
            }
            PrimitiveFetchRequest::Erc20Balances { chain, tokens } => {
                let selector = function_selector("balanceOf(address)");
                let args = encode_address(owner);
                let calls: Vec<Call3> = tokens
                    .iter()
                    .map(|token| {
                        let mut data = Vec::with_capacity(36);
                        data.extend_from_slice(&selector);
                        data.extend_from_slice(&args);
                        Call3 {
                            target: *token,
                            allow_failure: true,
                            call_data: data,
                        }
                    })
                    .collect();
                let multicall = Multicall::new(router.clone());
                let results = multicall.aggregate3(chain, calls, BlockTag::Latest).await?;
                let updates = tokens
                    .iter()
                    .zip(results.iter())
                    .filter_map(|(token, result)| {
                        if result.success && result.return_data.len() >= 32 {
                            Some(PrimitiveFetchUpdate::Erc20Balance {
                                chain: chain.clone(),
                                token: *token,
                                amount: U256::from_be_slice(&result.return_data[..32]),
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
                Ok(updates)
            }
            PrimitiveFetchRequest::Erc20Approval {
                chain,
                token,
                spender,
            } => {
                let selector = function_selector("allowance(address,address)");
                let mut data = Vec::with_capacity(68);
                data.extend_from_slice(&selector);
                data.extend_from_slice(&encode_address(owner));
                data.extend_from_slice(&encode_address(*spender));
                let req = EthCallRequest::new(*token, data);
                let ret = router.eth_call(chain, req).await?;
                if ret.len() >= 32 {
                    Ok(vec![PrimitiveFetchUpdate::Erc20Approval {
                        chain: chain.clone(),
                        token: *token,
                        spender: *spender,
                        amount: U256::from_be_slice(&ret[..32]),
                    }])
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }
}

fn apply_primitive_update(
    state: &mut WalletState,
    report: &mut PrimitivesReport,
    update: PrimitiveFetchUpdate,
    now: Time,
) {
    match update {
        PrimitiveFetchUpdate::BlockHeight { chain, number } => {
            state.block_heights.insert(
                chain,
                BlockHeight {
                    number,
                    time: now.as_unix(),
                },
            );
            report.block_heights_updated += 1;
        }
        PrimitiveFetchUpdate::NativeBalance { chain, amount } => {
            let key = TokenKey::Native { chain };
            if let Some(holding) = state.tokens.get_mut(&key) {
                holding.balance = Balance::Fungible { amount };
                holding.last_synced_at = now;
                report.native_balances_updated += 1;
            }
        }
        PrimitiveFetchUpdate::Erc20Balance {
            chain,
            token,
            amount,
        } => {
            let key = TokenKey::Erc20 {
                chain,
                address: token,
            };
            if let Some(holding) = state.tokens.get_mut(&key) {
                holding.balance = Balance::Fungible { amount };
                holding.last_synced_at = now;
                report.erc20_balances_updated += 1;
            }
        }
        PrimitiveFetchUpdate::Erc20Approval {
            chain,
            token,
            spender,
            amount,
        } => {
            if let Some(spenders) = state.approvals.erc20.get_mut(&(chain, token)) {
                if let Some(spec) = spenders.get_mut(&spender) {
                    spec.amount = amount;
                    spec.is_unlimited = amount == U256::MAX;
                    spec.last_set_at = now;
                    report.approvals_updated += 1;
                }
            }
        }
    }
}

fn primitive_request_label(request: &PrimitiveFetchRequest) -> String {
    match request {
        PrimitiveFetchRequest::BlockHeight { chain } => format!("blockNumber {chain}"),
        PrimitiveFetchRequest::NativeBalance { chain } => format!("balance native {chain}"),
        PrimitiveFetchRequest::Erc20Balances { chain, .. } => format!("erc20 multicall {chain}"),
        PrimitiveFetchRequest::Erc20Approval {
            chain,
            token,
            spender,
        } => format!("allowance {chain} token={token:#x} spender={spender:#x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    use policy_state::{
        Address, BaseCategory, ChainId, DataSource, FiatCurrency, LiveField, OracleProvider,
        PegTarget, Price, TokenHolding, TokenKind, WalletId, WalletState,
    };

    fn addr(hex: &str) -> Address {
        Address::from_str(hex).unwrap()
    }

    fn native_holding(chain: ChainId) -> TokenHolding {
        TokenHolding {
            key: TokenKey::Native {
                chain: chain.clone(),
            },
            kind: TokenKind::NativeGas,
            symbol: "ETH".into(),
            decimals: 18,
            balance: Balance::zero_fungible(),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: Some(LiveField::new(
                Price::new("1"),
                DataSource::OracleFeed {
                    provider: OracleProvider::Chainlink,
                    feed_id: "ETH/USD".into(),
                },
                Time::from_unix(0),
            )),
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(0),
            primitives_source: DataSource::OnchainView {
                chain,
                contract: Address::ZERO,
                function: "eth_getBalance".into(),
                decoder_id: "u256".into(),
            },
        }
    }

    fn erc20_holding(chain: ChainId, token: Address, symbol: &str) -> TokenHolding {
        TokenHolding {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address: token,
            },
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: symbol.into(),
            decimals: 6,
            balance: Balance::zero_fungible(),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: Time::from_unix(0),
            primitives_source: DataSource::OnchainView {
                chain,
                contract: token,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        }
    }

    #[test]
    fn plan_primitive_fetches_groups_plain_state_into_fetch_requests() {
        let chain = ChainId::ethereum_mainnet();
        let owner = addr("0x1111111111111111111111111111111111111111");
        let token_a = addr("0x2222222222222222222222222222222222222222");
        let token_b = addr("0x3333333333333333333333333333333333333333");
        let spender = addr("0x4444444444444444444444444444444444444444");
        let mut state = WalletState::new(WalletId::new(owner, [chain.clone()]));
        state.tokens.insert(
            TokenKey::Native {
                chain: chain.clone(),
            },
            native_holding(chain.clone()),
        );
        state.tokens.insert(
            TokenKey::Erc20 {
                chain: chain.clone(),
                address: token_a,
            },
            erc20_holding(chain.clone(), token_a, "A"),
        );
        state.tokens.insert(
            TokenKey::Erc20 {
                chain: chain.clone(),
                address: token_b,
            },
            erc20_holding(chain.clone(), token_b, "B"),
        );
        state
            .approvals
            .erc20
            .entry((chain.clone(), token_a))
            .or_default()
            .insert(
                spender,
                policy_state::AllowanceSpec::new(U256::from(1), Time::from_unix(0)),
            );

        let plan = plan_primitive_fetches(&state);

        assert_eq!(plan.owner, owner);
        assert_eq!(
            plan.requests,
            vec![
                PrimitiveFetchRequest::BlockHeight {
                    chain: chain.clone()
                },
                PrimitiveFetchRequest::NativeBalance {
                    chain: chain.clone()
                },
                PrimitiveFetchRequest::Erc20Balances {
                    chain: chain.clone(),
                    tokens: vec![token_a, token_b],
                },
                PrimitiveFetchRequest::Erc20Approval {
                    chain,
                    token: token_a,
                    spender,
                },
            ]
        );
    }

    #[tokio::test]
    async fn sync_primitives_no_router_reports_error() {
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
        let orch = Orchestrator::new(crate::fetchers::OnchainViewFetcher::new(onchain_router));
        let mut state =
            WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]));
        let report = orch
            .sync_primitives(&mut state, Time::from_unix(0))
            .await
            .unwrap();
        assert!(!report.errors.is_empty());
    }
}
