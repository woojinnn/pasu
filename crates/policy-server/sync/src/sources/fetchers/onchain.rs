use std::sync::Arc;

use serde_json::Value;

use policy_state::{ChainId, DataSource, TokenKey, TokenRef, U256};

use super::decoder::DecoderRegistry;
use super::rpc::multicall::{Call3, Multicall};
use super::rpc::{BlockTag, EthCallRequest, RpcRouter};
use crate::error::SyncError;

#[derive(Clone, Debug)]
pub struct OnchainCall {
    pub chain: ChainId,
    pub contract: alloy_primitives::Address,
    pub calldata: Vec<u8>,
    pub decoder_id: String,
}

impl OnchainCall {
    pub fn from_source(source: &DataSource, args_encoded: Vec<u8>) -> Result<Self, SyncError> {
        match source {
            DataSource::OnchainView {
                chain,
                contract,
                function,
                decoder_id,
            } => {
                let selector = super::decoder::function_selector(function);
                let mut calldata = Vec::with_capacity(4 + args_encoded.len());
                calldata.extend_from_slice(&selector);
                calldata.extend_from_slice(&args_encoded);
                Ok(Self {
                    chain: chain.clone(),
                    contract: *contract,
                    calldata,
                    decoder_id: decoder_id.clone(),
                })
            }
            other => Err(SyncError::FetchFailed {
                source_id: "onchain_fetcher".into(),
                reason: format!("expected OnchainView, got {other:?}"),
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OnchainOutcome {
    pub success: bool,
    pub value: Option<Value>,
    pub error: Option<String>,
}

pub struct OnchainViewFetcher {
    router: Arc<RpcRouter>,
    decoders: DecoderRegistry,
    abi_decoder: super::abi_decoder::AbiDecoder,
    multicall: Multicall,
}

impl OnchainViewFetcher {
    #[must_use]
    pub fn new(router: Arc<RpcRouter>) -> Self {
        let multicall = Multicall::new(router.clone());
        Self {
            router,
            decoders: DecoderRegistry::with_builtins(),
            abi_decoder: super::abi_decoder::AbiDecoder::default(),
            multicall,
        }
    }

    #[must_use]
    pub fn with_decoders(router: Arc<RpcRouter>, decoders: DecoderRegistry) -> Self {
        let multicall = Multicall::new(router.clone());
        Self {
            router,
            decoders,
            abi_decoder: super::abi_decoder::AbiDecoder::default(),
            multicall,
        }
    }

    pub const fn decoders_mut(&mut self) -> &mut DecoderRegistry {
        &mut self.decoders
    }

    pub const fn abi_decoder_mut(&mut self) -> &mut super::abi_decoder::AbiDecoder {
        &mut self.abi_decoder
    }

    /// `decoder_id` 를 풀 디코더 결정: 먼저 손코딩 `DecoderRegistry`,
    /// 모르면 `AbiDecoder` (alloy-dyn-abi) 로 fallback.
    fn decode_call_value(&self, call: &OnchainCall, data: &[u8]) -> Result<Value, SyncError> {
        if call.decoder_id == "uniswap_v3_position_fees_owed" {
            return decode_uniswap_v3_position_fees_owed(&call.chain, data);
        }
        self.decode_any(&call.decoder_id, data)
    }

    fn decode_any(&self, decoder_id: &str, data: &[u8]) -> Result<Value, SyncError> {
        if let Ok(v) = self.decoders.decode(decoder_id, data) {
            return Ok(v);
        }
        if self.abi_decoder.knows(decoder_id) {
            return self.abi_decoder.decode(decoder_id, data);
        }
        Err(SyncError::UnknownDecoder(decoder_id.to_string()))
    }

    pub async fn fetch_one(&self, call: &OnchainCall) -> Result<Value, SyncError> {
        let req = EthCallRequest::new(call.contract, call.calldata.clone());
        let returndata = self.router.eth_call(&call.chain, req).await?;
        self.decode_call_value(call, &returndata)
    }

    pub async fn fetch_batch(
        &self,
        chain: &ChainId,
        calls: &[OnchainCall],
    ) -> Result<Vec<OnchainOutcome>, SyncError> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }

        for c in calls {
            if &c.chain != chain {
                return Err(SyncError::FetchFailed {
                    source_id: "onchain_fetcher".into(),
                    reason: format!("batch chain mismatch: expected {}, got {}", chain, c.chain),
                });
            }
        }

        let mc_calls: Vec<Call3> = calls
            .iter()
            .map(|c| Call3 {
                target: c.contract,
                allow_failure: true,
                call_data: c.calldata.clone(),
            })
            .collect();

        let results = self
            .multicall
            .aggregate3(chain, mc_calls, BlockTag::Latest)
            .await?;

        let mut out = Vec::with_capacity(results.len());
        for (call, mc_res) in calls.iter().zip(results.iter()) {
            if !mc_res.success {
                out.push(OnchainOutcome {
                    success: false,
                    value: None,
                    error: Some("multicall returned !success".into()),
                });
                continue;
            }
            match self.decode_call_value(call, &mc_res.return_data) {
                Ok(v) => out.push(OnchainOutcome {
                    success: true,
                    value: Some(v),
                    error: None,
                }),
                Err(e) => out.push(OnchainOutcome {
                    success: false,
                    value: None,
                    error: Some(format!("{e}")),
                }),
            }
        }
        Ok(out)
    }
}

fn decode_uniswap_v3_position_fees_owed(chain: &ChainId, data: &[u8]) -> Result<Value, SyncError> {
    let token0 = address_word(data, 2)?;
    let token1 = address_word(data, 3)?;
    let owed0 = u256_word(data, 10)?;
    let owed1 = u256_word(data, 11)?;

    serde_json::to_value(vec![
        (
            TokenRef::new(TokenKey::Erc20 {
                chain: chain.clone(),
                address: token0,
            }),
            owed0,
        ),
        (
            TokenRef::new(TokenKey::Erc20 {
                chain: chain.clone(),
                address: token1,
            }),
            owed1,
        ),
    ])
    .map_err(|e| SyncError::FetchFailed {
        source_id: "onchain_fetcher".into(),
        reason: format!("serialize uniswap_v3_position_fees_owed: {e}"),
    })
}

fn word_at(data: &[u8], index: usize) -> Result<&[u8], SyncError> {
    let start = index
        .checked_mul(32)
        .ok_or_else(|| SyncError::FetchFailed {
            source_id: "onchain_fetcher".into(),
            reason: "ABI word index overflow".into(),
        })?;
    let end = start + 32;
    data.get(start..end).ok_or_else(|| SyncError::FetchFailed {
        source_id: "onchain_fetcher".into(),
        reason: format!(
            "uniswap_v3_position_fees_owed needs >=384 bytes, got {}",
            data.len()
        ),
    })
}

fn u256_word(data: &[u8], index: usize) -> Result<U256, SyncError> {
    Ok(U256::from_be_slice(word_at(data, index)?))
}

fn address_word(data: &[u8], index: usize) -> Result<alloy_primitives::Address, SyncError> {
    let word = word_at(data, index)?;
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&word[12..32]);
    Ok(alloy_primitives::Address::from(addr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::{Address, ChainId, DataSource};
    use std::str::FromStr;

    #[test]
    fn from_source_encodes_selector() {
        let source = DataSource::OnchainView {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::ZERO,
            function: "totalSupply()".into(),
            decoder_id: "u256".into(),
        };

        let call = OnchainCall::from_source(&source, vec![]).unwrap();
        // totalSupply() selector = 0x18160ddd
        assert_eq!(&call.calldata[..4], &[0x18, 0x16, 0x0d, 0xdd]);
        assert_eq!(call.calldata.len(), 4); // selector only
        assert_eq!(call.decoder_id, "u256");
    }

    #[test]
    fn from_source_with_args() {
        let source = DataSource::OnchainView {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::ZERO,
            function: "balanceOf(address)".into(),
            decoder_id: "erc20_balance".into(),
        };
        let user = Address::ZERO;
        let args = super::super::decoder::encode_address(user);
        let call = OnchainCall::from_source(&source, args.to_vec()).unwrap();
        // balanceOf(address) selector = 0x70a08231
        assert_eq!(&call.calldata[..4], &[0x70, 0xa0, 0x82, 0x31]);
        assert_eq!(call.calldata.len(), 4 + 32);
    }

    #[test]
    fn decodes_uniswap_v3_position_fees_owed_with_chain_qualified_tokens() {
        let chain = ChainId::base();
        let token0 = Address::from_str("0x4200000000000000000000000000000000000006").unwrap();
        let token1 = Address::from_str("0xd9aaec86b65d86f6a7b5b1b0c42ffa531710b6ca").unwrap();
        let owed0 = U256::from(123u64);
        let owed1 = U256::from(456u64);
        let mut data = vec![0u8; 12 * 32];
        data[(2 * 32 + 12)..(3 * 32)].copy_from_slice(token0.as_slice());
        data[(3 * 32 + 12)..(4 * 32)].copy_from_slice(token1.as_slice());
        data[(10 * 32)..(11 * 32)].copy_from_slice(&owed0.to_be_bytes::<32>());
        data[(11 * 32)..(12 * 32)].copy_from_slice(&owed1.to_be_bytes::<32>());

        let value = decode_uniswap_v3_position_fees_owed(&chain, &data).unwrap();
        let decoded: Vec<(TokenRef, U256)> = serde_json::from_value(value).unwrap();

        assert_eq!(decoded.len(), 2);
        assert_eq!(
            decoded[0].0,
            TokenRef::new(TokenKey::Erc20 {
                chain: chain.clone(),
                address: token0,
            })
        );
        assert_eq!(decoded[0].1, owed0);
        assert_eq!(
            decoded[1].0,
            TokenRef::new(TokenKey::Erc20 {
                chain,
                address: token1,
            })
        );
        assert_eq!(decoded[1].1, owed1);
    }
}
