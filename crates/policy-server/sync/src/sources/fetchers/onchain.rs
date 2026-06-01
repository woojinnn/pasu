use std::sync::Arc;

use serde_json::Value;

use policy_state::{ChainId, DataSource};

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
        self.decode_any(&call.decoder_id, &returndata)
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
            match self.decode_any(&call.decoder_id, &mc_res.return_data) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::{Address, ChainId, DataSource};

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
}
