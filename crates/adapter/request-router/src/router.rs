use std::str::FromStr as _;

use abi_resolver::bridge::convert_legacy_call;
use abi_resolver::resolver::ResolveOutcome;
use abi_resolver::CallMatchKey;
use alloy_primitives::Address as AlloyAddress;
use alloy_primitives::U256;
use call_adapter::{CallAdapterRegistry, CallContext};
use mappers::{MapContext, MapperMatchKey, MapperRegistry as _};
use policy_engine::action::{Address, DecimalString};
use serde_json::Value;
use sign_resolver::{
    parse_sign_request, SignAdapterRegistry, SignContext, SignMatchKey, SignPayload,
};

use crate::registries::DefaultRegistries;

const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

pub struct RouterContext<'a> {
    pub registries: &'a DefaultRegistries,
    pub token_registry: &'a dyn mappers::TokenRegistry,
    pub block_timestamp: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    #[error("unsupported method: {0}")]
    Unsupported(String),
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("call adapter error: {0}")]
    Call(#[source] anyhow::Error),
    #[error("sign adapter error: {0}")]
    Sign(#[source] anyhow::Error),
    #[error("no adapter matched request")]
    NoMatch,
    #[error("internal error: {0}")]
    Internal(#[source] anyhow::Error),
}

pub fn route_request(
    ctx: &RouterContext<'_>,
    method: &str,
    params: &Value,
    chain_id: u64,
) -> Result<Vec<policy_engine::ActionEnvelope>, RouterError> {
    match method {
        "eth_sendTransaction" | "eth_call" => route_call(ctx, params, chain_id),
        "eth_signTypedData_v4" => route_sign_typed_data(ctx, method, params, chain_id),
        _ => Err(RouterError::Unsupported(method.to_owned())),
    }
}

fn route_call(
    ctx: &RouterContext<'_>,
    params: &Value,
    chain_id: u64,
) -> Result<Vec<policy_engine::ActionEnvelope>, RouterError> {
    let tx = params
        .as_array()
        .and_then(|params| params.first())
        .ok_or_else(|| RouterError::InvalidParams("params[0] transaction object missing".into()))?;

    let to = required_address(tx, "to")?;
    let from = optional_address(tx, "from")?.unwrap_or_else(zero_address);
    let value = tx
        .get("value")
        .map(value_to_decimal_string)
        .transpose()?
        .unwrap_or_else(zero_decimal);
    let calldata = tx
        .get("data")
        .or_else(|| tx.get("input"))
        .and_then(Value::as_str)
        .ok_or_else(|| RouterError::InvalidParams("missing tx data/input".into()))
        .and_then(hex_to_bytes)?;

    if calldata.len() < 4 {
        return Err(RouterError::InvalidParams(format!(
            "calldata too short: {} bytes",
            calldata.len()
        )));
    }

    let selector = selector(&calldata)?;
    let key = CallMatchKey {
        chain_id,
        to: to.clone(),
        selector,
    };

    // Tier 1: a registered `CallAdapter` (new pipeline). Covers protocols where
    // we ship a per-function `sol!`-backed decoder + mapper.
    if let Some(adapter) = ctx.registries.call_adapters.resolve(&key) {
        let call_ctx = CallContext {
            chain_id,
            from: &from,
            to: &to,
            value_wei: &value,
            block_timestamp: ctx.block_timestamp,
            token_registry: ctx.token_registry,
            decoder_registry: ctx.registries.decoders.as_ref(),
            mapper_registry: ctx.registries.mappers.as_ref(),
        };
        return adapter
            .build(&call_ctx, &calldata)
            .map_err(|err| RouterError::Call(err.into()));
    }

    // Tier 2: legacy `Resolver` fallback (Sourcify bundle + openchain seed + optional
    // SQLite). We decode dynamically, then convert the result into the new
    // `DecodedCall` shape and dispatch through the new `MapperRegistry` using
    // the canonical decoder_id derived from the selector. This lets us pick up
    // any function whose ABI Sourcify knows about, as long as a mapper exists.
    route_call_fallback(ctx, chain_id, &from, &to, &value, &calldata, selector)
}

/// Sourcify-backed fallback decode + mapper dispatch. Invoked from
/// [`route_call`] when no per-function `CallAdapter` is registered for
/// `(chain, to, selector)`.
fn route_call_fallback(
    ctx: &RouterContext<'_>,
    chain_id: u64,
    from: &Address,
    to: &Address,
    value: &DecimalString,
    calldata: &[u8],
    selector: [u8; 4],
) -> Result<Vec<policy_engine::ActionEnvelope>, RouterError> {
    let alloy_addr = AlloyAddress::from_str(&to.to_string())
        .map_err(|e| RouterError::Internal(anyhow::anyhow!("address conversion: {e}")))?;
    let outcome = ctx
        .registries
        .resolver
        .resolve(chain_id, &alloy_addr, calldata);
    let legacy_call = match outcome {
        ResolveOutcome::Resolved(r) => r.decoded,
        ResolveOutcome::NotFound => return Err(RouterError::NoMatch),
    };

    let decoded = convert_legacy_call(legacy_call, selector)
        .map_err(|e| RouterError::Internal(anyhow::anyhow!(e)))?;

    let mapper_key = MapperMatchKey {
        decoder_id: decoded.decoder_id.clone(),
    };
    let mapper = ctx
        .registries
        .mappers
        .resolve(&mapper_key)
        .ok_or(RouterError::NoMatch)?;

    let map_ctx = MapContext {
        chain_id,
        from,
        to,
        value_wei: value,
        block_timestamp: ctx.block_timestamp,
        token_registry: ctx.token_registry,
    };

    mapper
        .map(&map_ctx, &decoded)
        .map_err(|e| RouterError::Call(e.into()))
}

fn route_sign_typed_data(
    ctx: &RouterContext<'_>,
    method: &str,
    params: &Value,
    chain_id: u64,
) -> Result<Vec<policy_engine::ActionEnvelope>, RouterError> {
    let request = parse_sign_request(method, params, chain_id)
        .map_err(|err| RouterError::InvalidParams(err.to_string()))?;
    let signer = Address::from_str(&request.signer)
        .map_err(|err| RouterError::InvalidParams(format!("invalid signer: {err}")))?;
    let SignPayload::TypedData(typed_data) = &request.payload else {
        return Err(RouterError::Unsupported(method.to_owned()));
    };

    let primary_type = typed_data
        .get("primaryType")
        .and_then(Value::as_str)
        .ok_or_else(|| RouterError::InvalidParams("missing typedData.primaryType".into()))?;
    let verifying_contract = typed_data
        .get("domain")
        .and_then(|domain| domain.get("verifyingContract"))
        .and_then(Value::as_str)
        .map(|value| {
            Address::from_str(value).map_err(|err| {
                RouterError::InvalidParams(format!("invalid domain.verifyingContract: {err}"))
            })
        })
        .transpose()?;

    let exact = verifying_contract.clone().and_then(|verifying_contract| {
        ctx.registries.sign_adapters.resolve(&SignMatchKey {
            chain_id: request.chain_id,
            verifying_contract: Some(verifying_contract),
            primary_type: primary_type.to_owned(),
        })
    });
    let adapter = exact
        .or_else(|| {
            ctx.registries.sign_adapters.resolve(&SignMatchKey {
                chain_id: request.chain_id,
                verifying_contract: None,
                primary_type: primary_type.to_owned(),
            })
        })
        .ok_or(RouterError::NoMatch)?;
    let sign_ctx = SignContext {
        chain_id: request.chain_id,
        signer: &signer,
        block_timestamp: ctx.block_timestamp,
        token_registry: ctx.token_registry,
    };

    adapter
        .build(&sign_ctx, &request)
        .map_err(|err| RouterError::Sign(err.into()))
}

fn selector(calldata: &[u8]) -> Result<[u8; 4], RouterError> {
    calldata
        .get(..4)
        .ok_or_else(|| RouterError::InvalidParams("calldata shorter than selector".into()))?
        .try_into()
        .map_err(|_| RouterError::Internal(anyhow::anyhow!("selector slice length mismatch")))
}

fn required_address(tx: &Value, field: &str) -> Result<Address, RouterError> {
    let value = tx
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| RouterError::InvalidParams(format!("missing tx {field}")))?;
    Address::from_str(value)
        .map_err(|err| RouterError::InvalidParams(format!("invalid tx {field}: {err}")))
}

fn optional_address(tx: &Value, field: &str) -> Result<Option<Address>, RouterError> {
    tx.get(field)
        .and_then(Value::as_str)
        .map(|value| {
            Address::from_str(value)
                .map_err(|err| RouterError::InvalidParams(format!("invalid tx {field}: {err}")))
        })
        .transpose()
}

fn value_to_decimal_string(value: &Value) -> Result<DecimalString, RouterError> {
    if let Some(raw) = value.as_str() {
        if raw.starts_with("0x") || raw.starts_with("0X") {
            return hex_to_decimal_string(raw);
        }
        return DecimalString::from_str(raw)
            .map_err(|err| RouterError::InvalidParams(format!("invalid tx value: {err}")));
    }

    if let Some(raw) = value.as_u64() {
        return DecimalString::from_str(&raw.to_string())
            .map_err(|err| RouterError::Internal(anyhow::anyhow!(err)));
    }

    Err(RouterError::InvalidParams(
        "tx value must be a hex string, decimal string, or u64".into(),
    ))
}

fn hex_to_decimal_string(value: &str) -> Result<DecimalString, RouterError> {
    let clean = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let parsed = if clean.is_empty() {
        U256::from(0)
    } else {
        U256::from_str_radix(clean, 16)
            .map_err(|err| RouterError::InvalidParams(format!("invalid hex value: {err}")))?
    };

    DecimalString::from_str(&parsed.to_string())
        .map_err(|err| RouterError::Internal(anyhow::anyhow!(err)))
}

fn hex_to_bytes(value: &str) -> Result<Vec<u8>, RouterError> {
    let clean = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    if clean.is_empty() {
        return Ok(Vec::new());
    }

    hex::decode(clean)
        .map_err(|err| RouterError::InvalidParams(format!("invalid calldata hex: {err}")))
}

fn zero_address() -> Address {
    Address::from_str(ZERO_ADDRESS).expect("static zero address must be valid")
}

fn zero_decimal() -> DecimalString {
    DecimalString::from_str("0").expect("static zero decimal must be valid")
}

#[cfg(test)]
mod tests {
    use mappers::EmptyTokenRegistry;
    use policy_engine::Action;
    use serde_json::{json, Value};

    use crate::{route_request, DefaultRegistries, RouterContext, RouterError};

    fn ctx<'a>(
        registries: &'a DefaultRegistries,
        token_registry: &'a EmptyTokenRegistry,
    ) -> RouterContext<'a> {
        RouterContext {
            registries,
            token_registry,
            block_timestamp: None,
        }
    }

    fn route(method: &str, params: Value, chain_id: u64) -> Vec<policy_engine::ActionEnvelope> {
        let registries = DefaultRegistries::standard();
        let token_registry = EmptyTokenRegistry;
        route_request(
            &ctx(&registries, &token_registry),
            method,
            &params,
            chain_id,
        )
        .unwrap()
    }

    #[test]
    fn test_route_request_dispatches_call_for_uniswap_v2() {
        let params = json!([{
            "from": "0x0000000000000000000000000000000000000001",
            "to": "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
            "value": "0x0",
            "data": "0x38ed1739000000000000000000000000000000000000000000000000000000000bebc200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000002540be3ff0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000dac17f958d2ee523a2206206994597c13d831ec7000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
        }]);

        let envelopes = route("eth_sendTransaction", params, 1);

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].category, policy_engine::Category::Dex);
        assert_eq!(envelopes[0].action.kind(), "swap");
        assert!(matches!(envelopes[0].action, Action::Swap(_)));
    }

    #[test]
    fn test_route_request_dispatches_call_for_uniswap_v3_single() {
        let params = json!([{
            "from": "0x0000000000000000000000000000000000000001",
            "to": "0xE592427A0AEce92De3Edee1F18E0157C05861564",
            "value": "0x0",
            "data": "0x414bf389000000000000000000000000dac17f958d2ee523a2206206994597c13d831ec7000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc20000000000000000000000000000000000000000000000000000000000000bb8000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000002540be3ff000000000000000000000000000000000000000000000000000000000bebc20000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
        }]);

        let envelopes = route("eth_sendTransaction", params, 1);

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].category, policy_engine::Category::Dex);
        assert_eq!(envelopes[0].action.kind(), "swap");
        assert!(matches!(envelopes[0].action, Action::Swap(_)));
    }

    #[test]
    fn test_route_request_dispatches_sign_for_eip2612() {
        let params = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            {
                "domain": {
                    "name": "USD Coin",
                    "version": "2",
                    "chainId": 1,
                    "verifyingContract": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                },
                "types": {
                    "EIP712Domain": [
                        { "name": "name", "type": "string" },
                        { "name": "version", "type": "string" },
                        { "name": "chainId", "type": "uint256" },
                        { "name": "verifyingContract", "type": "address" }
                    ],
                    "Permit": [
                        { "name": "owner", "type": "address" },
                        { "name": "spender", "type": "address" },
                        { "name": "value", "type": "uint256" },
                        { "name": "nonce", "type": "uint256" },
                        { "name": "deadline", "type": "uint256" }
                    ]
                },
                "primaryType": "Permit",
                "message": {
                    "owner": "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "spender": "0x1111111111111111111111111111111111111111",
                    "value": "50000000",
                    "nonce": 1,
                    "deadline": 1600
                }
            }
        ]);

        let envelopes = route("eth_signTypedData_v4", params, 1);

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].category, policy_engine::Category::Misc);
        assert_eq!(envelopes[0].action.kind(), "permit");
        assert!(matches!(envelopes[0].action, Action::Permit(_)));
    }

    #[test]
    fn test_route_request_dispatches_sign_for_permit2_single() {
        let params = json!([
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            {
                "domain": {
                    "name": "Permit2",
                    "chainId": 1,
                    "verifyingContract": "0x000000000022d473030f116ddee9f6b43ac78ba3"
                },
                "types": {
                    "EIP712Domain": [
                        { "name": "name", "type": "string" },
                        { "name": "chainId", "type": "uint256" },
                        { "name": "verifyingContract", "type": "address" }
                    ],
                    "PermitSingle": [
                        { "name": "details", "type": "PermitDetails" },
                        { "name": "spender", "type": "address" },
                        { "name": "sigDeadline", "type": "uint256" }
                    ],
                    "PermitDetails": [
                        { "name": "token", "type": "address" },
                        { "name": "amount", "type": "uint160" },
                        { "name": "expiration", "type": "uint48" },
                        { "name": "nonce", "type": "uint48" }
                    ]
                },
                "primaryType": "PermitSingle",
                "message": {
                    "details": {
                        "token": "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                        "amount": "10000000000000000",
                        "expiration": 4600,
                        "nonce": 1
                    },
                    "spender": "0x1111111111111111111111111111111111111111",
                    "sigDeadline": 1600
                }
            }
        ]);

        let envelopes = route("eth_signTypedData_v4", params, 1);

        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].category, policy_engine::Category::Misc);
        assert_eq!(envelopes[0].action.kind(), "permit");
        assert!(matches!(envelopes[0].action, Action::Permit(_)));
    }

    #[test]
    fn test_route_request_unsupported_method() {
        let registries = DefaultRegistries::standard();
        let token_registry = EmptyTokenRegistry;
        let err = route_request(
            &ctx(&registries, &token_registry),
            "personal_sign",
            &json!(["0xdeadbeef", "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]),
            1,
        )
        .unwrap_err();

        assert!(matches!(err, RouterError::Unsupported(method) if method == "personal_sign"));
    }

    #[test]
    fn test_route_request_unknown_selector() {
        let registries = DefaultRegistries::standard();
        let token_registry = EmptyTokenRegistry;
        let err = route_request(
            &ctx(&registries, &token_registry),
            "eth_sendTransaction",
            &json!([{
                "from": "0x0000000000000000000000000000000000000001",
                "to": "0x000000000000000000000000000000000000dEaD",
                "value": "0x0",
                "data": "0xdeadbeef0000000000000000000000000000000000000000000000000000000000000000"
            }]),
            1,
        )
        .unwrap_err();

        assert!(matches!(err, RouterError::NoMatch));
    }
}
