//! Etherscan V2 unified API client.
//!
//! Single endpoint (`api.etherscan.io/v2/api`) covers every chain via
//! the `chainid` parameter — Ethereum, Arbitrum, Base, Polygon, etc.
//! Free tier needs a key from <https://etherscan.io/myapikey>.
//!
//! We only consume one action today: `addresstokenbalance` (list of
//! ERC-20 balances for an address). The response gives us contract
//! address + symbol + decimals + balance — enough to build a
//! `TokenHolding` without an extra RPC roundtrip.

use std::str::FromStr;

use serde::Deserialize;

use policy_state::primitives::{Address, ChainId, U256};
use policy_state::token::TokenKey;

use crate::error::SyncError;

use super::DiscoveredToken;

const ETHERSCAN_V2_BASE: &str = "https://api.etherscan.io/v2/api";

/// Etherscan V2 API client. Cheap to clone (`reqwest::Client` is `Arc`
/// inside).
#[derive(Clone, Debug)]
pub struct EtherscanClient {
    api_key: String,
    http: reqwest::Client,
}

impl EtherscanClient {
    /// Build from a string key. Use [`Self::from_env`] for the standard
    /// `ETHERSCAN_API_KEY` env-var lookup.
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            http: reqwest::Client::new(),
        }
    }

    /// Read `ETHERSCAN_API_KEY` from the environment. Returns `None`
    /// when unset OR empty — `dotenv` files frequently leave the key
    /// declared but blank in dev environments.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        match std::env::var("ETHERSCAN_API_KEY") {
            Ok(k) if !k.trim().is_empty() => Some(Self::new(k.trim().to_string())),
            _ => None,
        }
    }

    /// `account.addresstokenbalance` — every ERC-20 balance the address
    /// currently holds on `chain`. Empty list when the address has
    /// never interacted with an ERC-20.
    pub async fn list_erc20_balances(
        &self,
        chain: &ChainId,
        address: Address,
    ) -> Result<Vec<DiscoveredToken>, SyncError> {
        let chainid = caip2_to_chain_id(chain)?;
        let addr_lower = format!("{address:#x}");

        let url = format!(
            "{ETHERSCAN_V2_BASE}?chainid={chainid}&module=account&action=addresstokenbalance\
             &address={addr_lower}&page=1&offset=500&apikey={key}",
            key = self.api_key,
        );

        let resp: EsResponse = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: "etherscan".into(),
                reason: format!("http: {e}"),
            })?
            .json()
            .await
            .map_err(|e| SyncError::FetchFailed {
                source_id: "etherscan".into(),
                reason: format!("json: {e}"),
            })?;

        match resp {
            EsResponse::Ok { result } => {
                let mut out = Vec::with_capacity(result.len());
                for row in result {
                    match row.into_discovered(chain) {
                        Ok(t) => out.push(t),
                        Err(_err) => {
                            // Skip malformed rows rather than fail the
                            // whole discovery — Etherscan occasionally
                            // returns rows with missing decimals etc.
                        }
                    }
                }
                Ok(out)
            }
            EsResponse::EmptyOk => Ok(Vec::new()),
            EsResponse::Err { result, message } => Err(SyncError::FetchFailed {
                source_id: "etherscan".into(),
                reason: format!("{message}: {result}"),
            }),
        }
    }
}

/// CAIP-2 (`eip155:<n>`) → integer chain id for the `chainid` query param.
fn caip2_to_chain_id(chain: &ChainId) -> Result<u64, SyncError> {
    let s = chain.as_str();
    let n = s
        .strip_prefix("eip155:")
        .ok_or_else(|| SyncError::FetchFailed {
            source_id: "etherscan".into(),
            reason: format!("chain {s} is not an EVM chain (CAIP-2 `eip155:N` expected)"),
        })?;
    n.parse::<u64>().map_err(|_| SyncError::FetchFailed {
        source_id: "etherscan".into(),
        reason: format!("chain {s} has invalid eip155 id"),
    })
}

// ---------- wire types ----------

/// Etherscan returns either `{status: "1", message: "OK", result: [...]}`
/// on success, or `{status: "0", message: "NOTOK", result: "<reason>"}`
/// on failure. The result type swap forces us into a manual enum.
#[derive(Debug)]
enum EsResponse {
    Ok { result: Vec<EsTokenRow> },
    EmptyOk,
    Err { result: String, message: String },
}

impl<'de> Deserialize<'de> for EsResponse {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            status: String,
            message: String,
            #[serde(default)]
            result: serde_json::Value,
        }
        let raw = Raw::deserialize(d)?;
        if raw.status == "1" {
            // Successful — `result` is an array of rows. Empty array
            // legitimate (= no tokens).
            let rows: Vec<EsTokenRow> =
                serde_json::from_value(raw.result).map_err(serde::de::Error::custom)?;
            return Ok(if rows.is_empty() {
                Self::EmptyOk
            } else {
                Self::Ok { result: rows }
            });
        }
        // Some empty-balance responses come back as status="0",
        // message="No transactions found" with result as a string. Treat
        // that as empty list rather than error.
        if raw.message.to_lowercase().contains("no transactions")
            || raw.message.to_lowercase().contains("no token transfers")
        {
            return Ok(Self::EmptyOk);
        }
        let result_str = match raw.result {
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        };
        Ok(Self::Err {
            result: result_str,
            message: raw.message,
        })
    }
}

/// One row in the `addresstokenbalance` response.
#[derive(Debug, Deserialize)]
struct EsTokenRow {
    #[serde(alias = "TokenAddress")]
    contract_address: String,
    #[serde(alias = "TokenSymbol")]
    symbol: String,
    #[serde(default, alias = "TokenName")]
    _name: Option<String>,
    #[serde(alias = "TokenQuantity")]
    quantity: String,
    #[serde(default, alias = "TokenDivisor")]
    divisor: Option<String>,
}

impl EsTokenRow {
    fn into_discovered(self, chain: &ChainId) -> Result<DiscoveredToken, String> {
        let address = Address::from_str(&self.contract_address)
            .map_err(|e| format!("bad contract address `{}`: {e}", self.contract_address))?;
        let balance = U256::from_str_radix(self.quantity.trim(), 10)
            .map_err(|e| format!("bad quantity `{}`: {e}", self.quantity))?;
        let decimals = self
            .divisor
            .as_deref()
            .map_or(18, |s| divisor_to_decimals(s).unwrap_or(18));
        Ok(DiscoveredToken {
            key: TokenKey::Erc20 {
                chain: chain.clone(),
                address,
            },
            symbol: self.symbol,
            decimals,
            balance,
        })
    }
}

/// Etherscan returns "divisor" as `10^decimals` (e.g. "1000000" for
/// USDC). Recover the exponent; default to 18 on parse failure.
fn divisor_to_decimals(divisor: &str) -> Option<u8> {
    let v = divisor.trim();
    if v.is_empty() {
        return None;
    }
    // Count trailing zeros that follow a leading '1'.
    if !v.starts_with('1') {
        return None;
    }
    let zeros = v[1..].chars().take_while(|c| *c == '0').count();
    if 1 + zeros == v.len() {
        u8::try_from(zeros).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divisor_decimals_common_values() {
        assert_eq!(divisor_to_decimals("1000000"), Some(6)); // USDC
        assert_eq!(divisor_to_decimals("1000000000000000000"), Some(18)); // ETH
        assert_eq!(divisor_to_decimals("100"), Some(2));
        assert_eq!(divisor_to_decimals("1"), Some(0));
    }

    #[test]
    fn divisor_decimals_invalid() {
        assert_eq!(divisor_to_decimals(""), None);
        assert_eq!(divisor_to_decimals("12345"), None);
        assert_eq!(divisor_to_decimals("1000100"), None);
    }

    #[test]
    fn caip2_parser() {
        assert_eq!(caip2_to_chain_id(&ChainId::new("eip155:1")).unwrap(), 1);
        assert_eq!(
            caip2_to_chain_id(&ChainId::new("eip155:42161")).unwrap(),
            42_161
        );
        assert!(caip2_to_chain_id(&ChainId::new("solana:mainnet")).is_err());
    }
}
