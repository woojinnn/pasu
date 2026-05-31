//! 단순 JSON-RPC HTTP provider — publicnode, cloudflare-eth 같은 무인증 endpoint.
//!
//! 인증이나 특수 헤더가 필요한 provider (alchemy, infura) 도 거의 같은 구조라
//! 추후 같은 패턴으로 추가하면 된다.

use alloy_primitives::{Address, U256};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use simulation_state::ChainId;

use super::super::{BlockTag, EthCallRequest, RpcProvider};
use crate::error::SyncError;

/// 인증 없는 HTTP JSON-RPC provider.
#[derive(Debug, Clone)]
pub struct PublicRpcProvider {
    name: String,
    chain: ChainId,
    url: String,
    client: reqwest::Client,
}

impl PublicRpcProvider {
    pub fn new(name: impl Into<String>, chain: ChainId, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            chain,
            url: url.into(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("reqwest client init"),
        }
    }

    async fn call_method<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, SyncError> {
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            method,
            params,
            id: 1,
        };

        let resp = self
            .client
            .post(&self.url)
            .json(&req)
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: self.name.clone(),
                reason: format!("http: {e}"),
            })?;

        let status = resp.status();
        if !status.is_success() {
            return Err(SyncError::FetchFailed {
                source_id: self.name.clone(),
                reason: format!("http status: {status}"),
            });
        }

        let envelope: JsonRpcResponse<T> =
            resp.json().await.map_err(|e| SyncError::FetchFailed {
                source_id: self.name.clone(),
                reason: format!("json decode: {e}"),
            })?;

        if let Some(err) = envelope.error {
            return Err(SyncError::FetchFailed {
                source_id: self.name.clone(),
                reason: format!("rpc error {}: {}", err.code, err.message),
            });
        }

        envelope.result.ok_or_else(|| SyncError::FetchFailed {
            source_id: self.name.clone(),
            reason: "missing result in jsonrpc response".into(),
        })
    }
}

#[async_trait]
impl RpcProvider for PublicRpcProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn chain(&self) -> &ChainId {
        &self.chain
    }

    async fn health_check(&self) -> Result<(), SyncError> {
        self.eth_block_number().await.map(|_| ())
    }

    async fn eth_call(&self, req: EthCallRequest) -> Result<Vec<u8>, SyncError> {
        let mut tx = serde_json::Map::new();
        tx.insert("to".into(), Value::String(format!("{:#x}", req.to)));
        tx.insert(
            "data".into(),
            Value::String(format!("0x{}", hex::encode(&req.data))),
        );
        if let Some(from) = req.from {
            tx.insert("from".into(), Value::String(format!("{from:#x}")));
        }
        if let Some(value) = req.value {
            tx.insert("value".into(), Value::String(format!("{value:#x}")));
        }

        let hex_string: String = self
            .call_method("eth_call", json!([Value::Object(tx), req.block.as_param()]))
            .await?;
        decode_hex_bytes(&hex_string, &self.name)
    }

    async fn eth_balance(&self, address: Address, block: BlockTag) -> Result<U256, SyncError> {
        let hex_string: String = self
            .call_method(
                "eth_getBalance",
                json!([format!("{:#x}", address), block.as_param()]),
            )
            .await?;
        decode_hex_u256(&hex_string, &self.name)
    }

    async fn eth_block_number(&self) -> Result<u64, SyncError> {
        let hex_string: String = self.call_method("eth_blockNumber", json!([])).await?;
        let n = u64::from_str_radix(hex_string.trim_start_matches("0x"), 16).map_err(|e| {
            SyncError::FetchFailed {
                source_id: self.name.clone(),
                reason: format!("blockNumber parse: {e}"),
            }
        })?;
        Ok(n)
    }

    async fn eth_gas_price(&self) -> Result<U256, SyncError> {
        let hex_string: String = self.call_method("eth_gasPrice", json!([])).await?;
        decode_hex_u256(&hex_string, &self.name)
    }

    async fn eth_get_transaction_receipt(
        &self,
        tx_hash: &str,
    ) -> Result<Option<super::super::TxReceipt>, SyncError> {
        // call_method 는 result: null 을 에러로 본다. 멤풀 tx 의 receipt 는
        // 정상적으로 null 이므로 raw 파싱.
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            method: "eth_getTransactionReceipt",
            params: json!([tx_hash]),
            id: 1,
        };
        let resp_value: Value = self
            .client
            .post(&self.url)
            .json(&req)
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: self.name.clone(),
                reason: format!("http: {e}"),
            })?
            .json()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: self.name.clone(),
                reason: format!("json: {e}"),
            })?;
        if let Some(err) = resp_value.get("error") {
            return Err(SyncError::FetchFailed {
                source_id: self.name.clone(),
                reason: format!("rpc error: {err}"),
            });
        }
        let result_val = resp_value.get("result");
        let Some(result_val) = result_val else {
            return Ok(None);
        };
        if result_val.is_null() {
            return Ok(None);
        }
        let r: RawReceipt =
            serde_json::from_value(result_val.clone()).map_err(|e| SyncError::FetchFailed {
                source_id: self.name.clone(),
                reason: format!("receipt parse: {e}"),
            })?;
        let status_hex = r.status.unwrap_or_else(|| "0x0".to_string());
        let status = status_hex == "0x1" || status_hex == "0x01";
        let block_number = u64::from_str_radix(r.block_number.trim_start_matches("0x"), 16)
            .map_err(|e| SyncError::FetchFailed {
                source_id: self.name.clone(),
                reason: format!("receipt blockNumber: {e}"),
            })?;
        let gas_used = u64::from_str_radix(r.gas_used.trim_start_matches("0x"), 16).unwrap_or(0);
        Ok(Some(super::super::TxReceipt {
            status,
            block_number,
            block_hash: r.block_hash,
            gas_used,
            tx_hash: r.transaction_hash,
        }))
    }
}

/// `eth_getTransactionReceipt` 의 raw response — 우리가 필요한 필드만.
#[derive(Deserialize)]
struct RawReceipt {
    #[serde(rename = "blockNumber")]
    block_number: String,
    #[serde(rename = "blockHash")]
    block_hash: String,
    #[serde(rename = "transactionHash")]
    transaction_hash: String,
    #[serde(rename = "gasUsed")]
    gas_used: String,
    status: Option<String>,
}

// ============ JSON-RPC envelope ============

#[derive(Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'static str,
    method: &'a str,
    params: Value,
    id: u32,
}

#[derive(Deserialize)]
struct JsonRpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    #[allow(dead_code)]
    id: Option<Value>,
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize, Debug)]
struct JsonRpcError {
    code: i64,
    message: String,
}

// ============ helpers ============

fn decode_hex_bytes(s: &str, source: &str) -> Result<Vec<u8>, SyncError> {
    let stripped = s.trim_start_matches("0x");
    hex::decode(stripped).map_err(|e| SyncError::FetchFailed {
        source_id: source.into(),
        reason: format!("hex decode: {e}"),
    })
}

fn decode_hex_u256(s: &str, source: &str) -> Result<U256, SyncError> {
    let stripped = s.trim_start_matches("0x");
    U256::from_str_radix(stripped, 16).map_err(|e| SyncError::FetchFailed {
        source_id: source.into(),
        reason: format!("u256 parse: {e}"),
    })
}
