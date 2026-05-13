use std::str::FromStr as _;

use abi_resolver::{DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::U256;
use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef, Category,
    DecimalString, Validity, ValiditySource,
};

use crate::mapper::{MapContext, Mapper, MapperError, MapperId};

pub const UNISWAP_V3_MAPPER_ID: &str = "uniswap_v3";
pub const EXACT_OUTPUT_SINGLE_MAPPER_ID: &str = "uniswap-v3/exactOutputSingle";
pub const EXACT_OUTPUT_MAPPER_ID: &str = "uniswap-v3/exactOutput";

const EXACT_INPUT_SINGLE_SIGNATURE: &str =
    "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))";
const EXACT_INPUT_SIGNATURE: &str = "exactInput((bytes,address,uint256,uint256,uint256))";
const EXACT_OUTPUT_SINGLE_SIGNATURE: &str =
    "exactOutputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))";
const EXACT_OUTPUT_SIGNATURE: &str = "exactOutput((bytes,address,uint256,uint256,uint256))";

#[derive(Debug, Clone, Copy, Default)]
pub struct UniswapV3Mapper;

impl UniswapV3Mapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UniswapV3Mapper {
    fn id(&self) -> MapperId {
        MapperId::new(UNISWAP_V3_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        match decoded.function_signature.as_str() {
            EXACT_INPUT_SINGLE_SIGNATURE | EXACT_INPUT_SIGNATURE => {
                decoded.decoder_id == DecoderId::new(UNISWAP_V3_MAPPER_ID)
            }
            EXACT_OUTPUT_SINGLE_SIGNATURE => {
                decoded.decoder_id == DecoderId::new(EXACT_OUTPUT_SINGLE_MAPPER_ID)
            }
            EXACT_OUTPUT_SIGNATURE => decoded.decoder_id == DecoderId::new(EXACT_OUTPUT_MAPPER_ID),
            _ => false,
        }
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        match decoded.function_signature.as_str() {
            EXACT_INPUT_SINGLE_SIGNATURE => Ok(vec![map_exact_input_single(ctx, decoded)?]),
            EXACT_INPUT_SIGNATURE => Ok(vec![map_exact_input(ctx, decoded)?]),
            EXACT_OUTPUT_SINGLE_SIGNATURE => Ok(vec![map_exact_output_single(ctx, decoded)?]),
            EXACT_OUTPUT_SIGNATURE => Ok(vec![map_exact_output(ctx, decoded)?]),
            other => Err(argument_mismatch(
                "function_signature",
                format!("unsupported Uniswap V3 function {other}"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExactInputSingleMapper;

impl ExactInputSingleMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for ExactInputSingleMapper {
    fn id(&self) -> MapperId {
        MapperId::new(UNISWAP_V3_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == DecoderId::new(UNISWAP_V3_MAPPER_ID)
            && decoded.function_signature == EXACT_INPUT_SINGLE_SIGNATURE
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        Ok(vec![map_exact_input_single(ctx, decoded)?])
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ExactInputMapper;

impl ExactInputMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for ExactInputMapper {
    fn id(&self) -> MapperId {
        MapperId::new(UNISWAP_V3_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == DecoderId::new(UNISWAP_V3_MAPPER_ID)
            && decoded.function_signature == EXACT_INPUT_SIGNATURE
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        Ok(vec![map_exact_input(ctx, decoded)?])
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UniswapV3ExactOutputSingleMapper;

impl UniswapV3ExactOutputSingleMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UniswapV3ExactOutputSingleMapper {
    fn id(&self) -> MapperId {
        MapperId::new(EXACT_OUTPUT_SINGLE_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == DecoderId::new(EXACT_OUTPUT_SINGLE_MAPPER_ID)
            && decoded.function_signature == EXACT_OUTPUT_SINGLE_SIGNATURE
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        Ok(vec![map_exact_output_single(ctx, decoded)?])
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UniswapV3ExactOutputMapper;

impl UniswapV3ExactOutputMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UniswapV3ExactOutputMapper {
    fn id(&self) -> MapperId {
        MapperId::new(EXACT_OUTPUT_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == DecoderId::new(EXACT_OUTPUT_MAPPER_ID)
            && decoded.function_signature == EXACT_OUTPUT_SIGNATURE
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        Ok(vec![map_exact_output(ctx, decoded)?])
    }
}

fn map_exact_input_single(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
) -> Result<ActionEnvelope, MapperError> {
    let token_in = address_arg(decoded, "tokenIn")?;
    let token_out = address_arg(decoded, "tokenOut")?;
    let fee = uint_arg(decoded, "fee")?;
    let recipient = address_arg(decoded, "recipient")?;
    let deadline = uint_arg(decoded, "deadline")?;
    let amount_in = uint_arg(decoded, "amountIn")?;
    let amount_out_minimum = uint_arg(decoded, "amountOutMinimum")?;

    Ok(swap_envelope(SwapAction {
        mode: SwapMode::ExactIn,
        token_in: asset_ref(ctx, &token_in),
        token_out: asset_ref(ctx, &token_out),
        amount_in: amount_constraint(AmountKind::Exact, amount_in)?,
        amount_out: amount_constraint(AmountKind::Min, amount_out_minimum)?,
        recipient,
        validity: Some(validity(deadline)?),
        fee_bps: Some(fee_bps(fee)?),
        enrichment: SwapEnrichment::default(),
    }))
}

fn map_exact_input(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
) -> Result<ActionEnvelope, MapperError> {
    let path = bytes_arg(decoded, "path")?;
    let recipient = address_arg(decoded, "recipient")?;
    let deadline = uint_arg(decoded, "deadline")?;
    let amount_in = uint_arg(decoded, "amountIn")?;
    let amount_out_minimum = uint_arg(decoded, "amountOutMinimum")?;
    let parsed = parse_path(path)?;

    Ok(swap_envelope(SwapAction {
        mode: SwapMode::ExactIn,
        token_in: asset_ref(ctx, &parsed.token_in),
        token_out: asset_ref(ctx, &parsed.token_out),
        amount_in: amount_constraint(AmountKind::Exact, amount_in)?,
        amount_out: amount_constraint(AmountKind::Min, amount_out_minimum)?,
        recipient,
        validity: Some(validity(deadline)?),
        fee_bps: Some(parsed.first_fee / 100),
        enrichment: SwapEnrichment::default(),
    }))
}

fn map_exact_output_single(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
) -> Result<ActionEnvelope, MapperError> {
    let token_in = address_arg(decoded, "tokenIn")?;
    let token_out = address_arg(decoded, "tokenOut")?;
    let fee = uint_arg(decoded, "fee")?;
    let recipient = address_arg(decoded, "recipient")?;
    let deadline = uint_arg(decoded, "deadline")?;
    let amount_out = uint_arg(decoded, "amountOut")?;
    let amount_in_maximum = uint_arg(decoded, "amountInMaximum")?;

    Ok(swap_envelope(SwapAction {
        mode: SwapMode::ExactOut,
        token_in: asset_ref(ctx, &token_in),
        token_out: asset_ref(ctx, &token_out),
        amount_in: amount_constraint(AmountKind::Max, amount_in_maximum)?,
        amount_out: amount_constraint(AmountKind::Exact, amount_out)?,
        recipient,
        validity: Some(validity(deadline)?),
        fee_bps: Some(fee_bps(fee)?),
        enrichment: SwapEnrichment::default(),
    }))
}

fn map_exact_output(
    ctx: &MapContext<'_>,
    decoded: &DecodedCall,
) -> Result<ActionEnvelope, MapperError> {
    let path = bytes_arg(decoded, "path")?;
    let recipient = address_arg(decoded, "recipient")?;
    let deadline = uint_arg(decoded, "deadline")?;
    let amount_out = uint_arg(decoded, "amountOut")?;
    let amount_in_maximum = uint_arg(decoded, "amountInMaximum")?;
    let parsed = parse_path(path)?;

    Ok(swap_envelope(SwapAction {
        mode: SwapMode::ExactOut,
        token_in: asset_ref(ctx, &parsed.token_out),
        token_out: asset_ref(ctx, &parsed.token_in),
        amount_in: amount_constraint(AmountKind::Max, amount_in_maximum)?,
        amount_out: amount_constraint(AmountKind::Exact, amount_out)?,
        recipient,
        validity: Some(validity(deadline)?),
        fee_bps: Some(parsed.first_fee / 100),
        enrichment: SwapEnrichment::default(),
    }))
}

fn swap_envelope(action: SwapAction) -> ActionEnvelope {
    ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(action),
    }
}

struct ParsedPath {
    token_in: Address,
    token_out: Address,
    first_fee: u32,
}

fn parse_path(path: &[u8]) -> Result<ParsedPath, MapperError> {
    const ADDRESS_LEN: usize = 20;
    const FEE_LEN: usize = 3;
    const HOP_LEN: usize = ADDRESS_LEN + FEE_LEN;

    if path.len() < ADDRESS_LEN + HOP_LEN {
        return Err(argument_mismatch(
            "path",
            format!(
                "expected at least one V3 hop with 43 bytes, got {}",
                path.len()
            ),
        ));
    }
    if !(path.len() - ADDRESS_LEN).is_multiple_of(HOP_LEN) {
        return Err(argument_mismatch(
            "path",
            format!(
                "expected length 20 + 23*N for Uniswap V3 packed path, got {}",
                path.len()
            ),
        ));
    }

    let first_fee = (u32::from(path[20]) << 16) | (u32::from(path[21]) << 8) | u32::from(path[22]);
    let token_in = address_from_bytes(&path[..ADDRESS_LEN])?;
    let token_out = address_from_bytes(&path[path.len() - ADDRESS_LEN..])?;

    Ok(ParsedPath {
        token_in,
        token_out,
        first_fee,
    })
}

fn address_from_bytes(bytes: &[u8]) -> Result<Address, MapperError> {
    Address::from_str(&format!("0x{}", hex::encode(bytes)))
        .map_err(|e| MapperError::Internal(anyhow::anyhow!(e)))
}

fn asset_ref(ctx: &MapContext<'_>, address: &Address) -> AssetRef {
    let metadata = ctx.token_registry.lookup(ctx.chain_id, address);
    AssetRef {
        kind: AssetKind::Erc20,
        chain_id: ctx.chain_id,
        address: Some(address.clone()),
        symbol: metadata.as_ref().map(|m| m.symbol.clone()),
        decimals: metadata.map(|m| m.decimals),
    }
}

fn amount_constraint(kind: AmountKind, value: U256) -> Result<AmountConstraint, MapperError> {
    Ok(AmountConstraint {
        kind,
        value: Some(decimal(value)?),
    })
}

fn validity(deadline: U256) -> Result<Validity, MapperError> {
    Ok(Validity {
        expires_at: decimal(deadline)?,
        source: ValiditySource::TxDeadline,
    })
}

fn decimal(value: U256) -> Result<DecimalString, MapperError> {
    DecimalString::from_str(&value.to_string())
        .map_err(|e| MapperError::Internal(anyhow::anyhow!(e)))
}

fn fee_bps(value: U256) -> Result<u32, MapperError> {
    let fee: u32 = value
        .try_into()
        .map_err(|e| MapperError::Internal(anyhow::anyhow!("fee value out of range: {e}")))?;
    Ok(fee / 100)
}

fn uint_arg(decoded: &DecodedCall, name: &str) -> Result<U256, MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Uint(value) => Ok(*value),
        other => Err(argument_mismatch(
            name,
            format!("expected uint, got {other:?}"),
        )),
    }
}

fn address_arg(decoded: &DecodedCall, name: &str) -> Result<Address, MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Address(value) => Ok(value.clone()),
        other => Err(argument_mismatch(
            name,
            format!("expected address, got {other:?}"),
        )),
    }
}

fn bytes_arg<'a>(decoded: &'a DecodedCall, name: &str) -> Result<&'a [u8], MapperError> {
    match arg(decoded, name)? {
        DecodedValue::Bytes(value) => Ok(value),
        other => Err(argument_mismatch(
            name,
            format!("expected bytes, got {other:?}"),
        )),
    }
}

fn arg<'a>(decoded: &'a DecodedCall, name: &str) -> Result<&'a DecodedValue, MapperError> {
    decoded
        .args
        .iter()
        .find(|arg| arg.name == name)
        .map(|arg| &arg.value)
        .ok_or_else(|| MapperError::MissingArgument(name.to_owned()))
}

fn argument_mismatch(name: &str, message: String) -> MapperError {
    MapperError::ArgumentMismatch {
        name: name.to_owned(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        UniswapV3ExactOutputMapper, UniswapV3ExactOutputSingleMapper, UniswapV3Mapper,
        UNISWAP_V3_MAPPER_ID,
    };
    use abi_resolver::decoders::uniswap_v3::{
        ExactInputDecoder, ExactInputSingleDecoder, EXACT_OUTPUT_DECODER_ID,
        EXACT_OUTPUT_SINGLE_DECODER_ID, UNISWAP_V3_DECODER_ID,
    };
    use abi_resolver::{
        DecodeContext, DecodedArg, DecodedCall, DecodedValue, Decoder as _, DecoderId,
    };
    use alloy_primitives::U256;
    use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
    use policy_engine::action::{
        Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
        Category, DecimalString, Validity, ValiditySource,
    };
    use serde::Deserialize;
    use std::collections::HashMap;
    use std::str::FromStr as _;

    use crate::{EmptyTokenRegistry, MapContext, Mapper as _, TokenMetadata, TokenRegistry};

    struct StaticTokenRegistry {
        tokens: HashMap<(u64, Address), TokenMetadata>,
    }

    impl StaticTokenRegistry {
        fn new(tokens: impl IntoIterator<Item = (u64, Address, TokenMetadata)>) -> Self {
            Self {
                tokens: tokens
                    .into_iter()
                    .map(|(chain_id, address, metadata)| ((chain_id, address), metadata))
                    .collect(),
            }
        }
    }

    impl TokenRegistry for StaticTokenRegistry {
        fn lookup(&self, chain_id: u64, address: &Address) -> Option<TokenMetadata> {
            self.tokens.get(&(chain_id, address.clone())).cloned()
        }
    }

    #[derive(Deserialize)]
    struct Fixture {
        chain_id: u64,
        rpc: Rpc,
    }

    #[derive(Deserialize)]
    struct Rpc {
        params: Vec<TxParam>,
    }

    #[derive(Deserialize)]
    struct TxParam {
        from: String,
        to: String,
        data: String,
    }

    fn address(value: &str) -> Address {
        Address::from_str(value).unwrap()
    }

    fn decimal(value: &str) -> DecimalString {
        DecimalString::from_str(value).unwrap()
    }

    fn amount(kind: AmountKind, value: &str) -> AmountConstraint {
        AmountConstraint {
            kind,
            value: Some(decimal(value)),
        }
    }

    fn erc20(chain_id: u64, address: &str, symbol: Option<&str>, decimals: Option<u8>) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            chain_id,
            address: Some(address.parse().unwrap()),
            symbol: symbol.map(str::to_owned),
            decimals,
        }
    }

    fn metadata_registry() -> StaticTokenRegistry {
        StaticTokenRegistry::new([
            (
                1,
                address("0xdac17f958d2ee523a2206206994597c13d831ec7"),
                TokenMetadata {
                    symbol: "USDT".to_owned(),
                    decimals: 6,
                },
            ),
            (
                1,
                address("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
                TokenMetadata {
                    symbol: "USDC".to_owned(),
                    decimals: 6,
                },
            ),
            (
                1,
                address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
                TokenMetadata {
                    symbol: "WETH".to_owned(),
                    decimals: 18,
                },
            ),
        ])
    }

    fn ctx<'a>(
        token_registry: &'a dyn TokenRegistry,
        from: &'a Address,
        to: &'a Address,
        value_wei: &'a DecimalString,
    ) -> MapContext<'a> {
        MapContext {
            chain_id: 1,
            from,
            to,
            value_wei,
            block_timestamp: Some(1_700_000_000),
            token_registry,
        }
    }

    fn exact_input_single_decoded() -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(UNISWAP_V3_DECODER_ID),
            function_signature:
                "exactInputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))"
                    .to_owned(),
            args: vec![
                DecodedArg {
                    name: "tokenIn".to_owned(),
                    abi_type: "address".to_owned(),
                    value: DecodedValue::Address(address(
                        "0xdac17f958d2ee523a2206206994597c13d831ec7",
                    )),
                },
                DecodedArg {
                    name: "tokenOut".to_owned(),
                    abi_type: "address".to_owned(),
                    value: DecodedValue::Address(address(
                        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                    )),
                },
                DecodedArg {
                    name: "fee".to_owned(),
                    abi_type: "uint24".to_owned(),
                    value: DecodedValue::Uint(U256::from(3000u64)),
                },
                DecodedArg {
                    name: "recipient".to_owned(),
                    abi_type: "address".to_owned(),
                    value: DecodedValue::Address(address(
                        "0x1111111111111111111111111111111111111111",
                    )),
                },
                DecodedArg {
                    name: "deadline".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(9_999_999_999u64)),
                },
                DecodedArg {
                    name: "amountIn".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(200_000_000u64)),
                },
                DecodedArg {
                    name: "amountOutMinimum".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::ZERO),
                },
                DecodedArg {
                    name: "sqrtPriceLimitX96".to_owned(),
                    abi_type: "uint160".to_owned(),
                    value: DecodedValue::Uint(U256::ZERO),
                },
            ],
            nested: vec![],
        }
    }

    fn exact_input_decoded() -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(UNISWAP_V3_DECODER_ID),
            function_signature: "exactInput((bytes,address,uint256,uint256,uint256))".to_owned(),
            args: vec![
                DecodedArg {
                    name: "path".to_owned(),
                    abi_type: "bytes".to_owned(),
                    value: DecodedValue::Bytes(
                        hex::decode(
                            "dac17f958d2ee523a2206206994597c13d831ec70001f4a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48000bb8c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                        )
                        .unwrap(),
                    ),
                },
                DecodedArg {
                    name: "recipient".to_owned(),
                    abi_type: "address".to_owned(),
                    value: DecodedValue::Address(address(
                        "0x1111111111111111111111111111111111111111",
                    )),
                },
                DecodedArg {
                    name: "deadline".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::ONE),
                },
                DecodedArg {
                    name: "amountIn".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(1_000_000u64)),
                },
                DecodedArg {
                    name: "amountOutMinimum".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::ZERO),
                },
            ],
            nested: vec![],
        }
    }

    fn exact_output_single_decoded() -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(EXACT_OUTPUT_SINGLE_DECODER_ID),
            function_signature:
                "exactOutputSingle((address,address,uint24,address,uint256,uint256,uint256,uint160))"
                    .to_owned(),
            args: vec![
                DecodedArg {
                    name: "tokenIn".to_owned(),
                    abi_type: "address".to_owned(),
                    value: DecodedValue::Address(address(
                        "0xdac17f958d2ee523a2206206994597c13d831ec7",
                    )),
                },
                DecodedArg {
                    name: "tokenOut".to_owned(),
                    abi_type: "address".to_owned(),
                    value: DecodedValue::Address(address(
                        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                    )),
                },
                DecodedArg {
                    name: "fee".to_owned(),
                    abi_type: "uint24".to_owned(),
                    value: DecodedValue::Uint(U256::from(3000u64)),
                },
                DecodedArg {
                    name: "recipient".to_owned(),
                    abi_type: "address".to_owned(),
                    value: DecodedValue::Address(address(
                        "0x1111111111111111111111111111111111111111",
                    )),
                },
                DecodedArg {
                    name: "deadline".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(9_999_999_999u64)),
                },
                DecodedArg {
                    name: "amountOut".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(100_000_000u64)),
                },
                DecodedArg {
                    name: "amountInMaximum".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(200_000_000u64)),
                },
                DecodedArg {
                    name: "sqrtPriceLimitX96".to_owned(),
                    abi_type: "uint160".to_owned(),
                    value: DecodedValue::Uint(U256::ZERO),
                },
            ],
            nested: vec![],
        }
    }

    fn exact_output_decoded() -> DecodedCall {
        DecodedCall {
            decoder_id: DecoderId::new(EXACT_OUTPUT_DECODER_ID),
            function_signature: "exactOutput((bytes,address,uint256,uint256,uint256))".to_owned(),
            args: vec![
                DecodedArg {
                    name: "path".to_owned(),
                    abi_type: "bytes".to_owned(),
                    value: DecodedValue::Bytes(
                        hex::decode(
                            "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000bb8dac17f958d2ee523a2206206994597c13d831ec7",
                        )
                        .unwrap(),
                    ),
                },
                DecodedArg {
                    name: "recipient".to_owned(),
                    abi_type: "address".to_owned(),
                    value: DecodedValue::Address(address(
                        "0x1111111111111111111111111111111111111111",
                    )),
                },
                DecodedArg {
                    name: "deadline".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::ONE),
                },
                DecodedArg {
                    name: "amountOut".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(1_000_000u64)),
                },
                DecodedArg {
                    name: "amountInMaximum".to_owned(),
                    abi_type: "uint256".to_owned(),
                    value: DecodedValue::Uint(U256::from(2_000_000u64)),
                },
            ],
            nested: vec![],
        }
    }

    fn expected_exact_input_single_envelope(symbols: bool) -> ActionEnvelope {
        let (in_symbol, in_decimals, out_symbol, out_decimals) = if symbols {
            (Some("USDT"), Some(6), Some("WETH"), Some(18))
        } else {
            (None, None, None, None)
        };
        ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(SwapAction {
                mode: SwapMode::ExactIn,
                token_in: erc20(
                    1,
                    "0xdac17f958d2ee523a2206206994597c13d831ec7",
                    in_symbol,
                    in_decimals,
                ),
                token_out: erc20(
                    1,
                    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                    out_symbol,
                    out_decimals,
                ),
                amount_in: amount(AmountKind::Exact, "200000000"),
                amount_out: amount(AmountKind::Min, "0"),
                recipient: address("0x1111111111111111111111111111111111111111"),
                validity: Some(Validity {
                    expires_at: decimal("9999999999"),
                    source: ValiditySource::TxDeadline,
                }),
                fee_bps: Some(30),
                enrichment: SwapEnrichment::default(),
            }),
        }
    }

    fn expected_exact_input_envelope(symbols: bool) -> ActionEnvelope {
        let (in_symbol, in_decimals, out_symbol, out_decimals) = if symbols {
            (Some("USDT"), Some(6), Some("WETH"), Some(18))
        } else {
            (None, None, None, None)
        };
        ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(SwapAction {
                mode: SwapMode::ExactIn,
                token_in: erc20(
                    1,
                    "0xdac17f958d2ee523a2206206994597c13d831ec7",
                    in_symbol,
                    in_decimals,
                ),
                token_out: erc20(
                    1,
                    "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
                    out_symbol,
                    out_decimals,
                ),
                amount_in: amount(AmountKind::Exact, "1000000"),
                amount_out: amount(AmountKind::Min, "0"),
                recipient: address("0x1111111111111111111111111111111111111111"),
                validity: Some(Validity {
                    expires_at: decimal("1"),
                    source: ValiditySource::TxDeadline,
                }),
                fee_bps: Some(5),
                enrichment: SwapEnrichment::default(),
            }),
        }
    }

    #[test]
    fn test_map_exact_input_single_produces_swap_action() {
        let token_registry = EmptyTokenRegistry;
        let from = address("0x0000000000000000000000000000000000000001");
        let to = address("0xe592427a0aece92de3edee1f18e0157c05861564");
        let value_wei = decimal("0");

        let result = UniswapV3Mapper::new()
            .map(
                &ctx(&token_registry, &from, &to, &value_wei),
                &exact_input_single_decoded(),
            )
            .unwrap();

        assert_eq!(UniswapV3Mapper::new().id().as_str(), UNISWAP_V3_MAPPER_ID);
        assert_eq!(result, vec![expected_exact_input_single_envelope(false)]);
        let Action::Swap(swap) = &result[0].action else {
            panic!("expected swap action");
        };
        assert_eq!(result[0].category, Category::Dex);
        assert_eq!(swap.mode, SwapMode::ExactIn);
        assert_eq!(swap.amount_in.kind, AmountKind::Exact);
        assert_eq!(swap.amount_out.kind, AmountKind::Min);
        assert_eq!(swap.fee_bps, Some(30));
    }

    #[test]
    fn test_map_exact_input_parses_path_and_uses_first_hop_fee() {
        let token_registry = EmptyTokenRegistry;
        let from = address("0x0000000000000000000000000000000000000001");
        let to = address("0xe592427a0aece92de3edee1f18e0157c05861564");
        let value_wei = decimal("0");

        let result = UniswapV3Mapper::new()
            .map(
                &ctx(&token_registry, &from, &to, &value_wei),
                &exact_input_decoded(),
            )
            .unwrap();

        assert_eq!(result, vec![expected_exact_input_envelope(false)]);
        let Action::Swap(swap) = &result[0].action else {
            panic!("expected swap action");
        };
        assert_eq!(
            swap.token_in.address,
            Some(address("0xdac17f958d2ee523a2206206994597c13d831ec7"))
        );
        assert_eq!(
            swap.token_out.address,
            Some(address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"))
        );
        assert_eq!(swap.fee_bps, Some(5));
    }

    #[test]
    fn maps_exact_output_single_to_swap_action() {
        let token_registry = EmptyTokenRegistry;
        let from = address("0x0000000000000000000000000000000000000001");
        let to = address("0xe592427a0aece92de3edee1f18e0157c05861564");
        let value_wei = decimal("0");

        let result = UniswapV3ExactOutputSingleMapper::new()
            .map(
                &ctx(&token_registry, &from, &to, &value_wei),
                &exact_output_single_decoded(),
            )
            .unwrap();

        assert_eq!(result.len(), 1);
        let Action::Swap(swap) = &result[0].action else {
            panic!("expected swap action");
        };
        assert_eq!(result[0].category, Category::Dex);
        assert_eq!(swap.mode, SwapMode::ExactOut);
        assert_eq!(swap.amount_in, amount(AmountKind::Max, "200000000"));
        assert_eq!(swap.amount_out, amount(AmountKind::Exact, "100000000"));
        assert_eq!(
            swap.token_in.address,
            Some(address("0xdac17f958d2ee523a2206206994597c13d831ec7"))
        );
        assert_eq!(
            swap.token_out.address,
            Some(address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"))
        );
        assert_eq!(
            swap.recipient,
            address("0x1111111111111111111111111111111111111111")
        );
        assert_eq!(
            swap.validity,
            Some(Validity {
                expires_at: decimal("9999999999"),
                source: ValiditySource::TxDeadline,
            })
        );
        assert_eq!(swap.fee_bps, Some(30));
        assert_eq!(swap.enrichment, SwapEnrichment::default());
    }

    #[test]
    fn maps_exact_output_path_uses_reverse_direction() {
        let token_registry = EmptyTokenRegistry;
        let from = address("0x0000000000000000000000000000000000000001");
        let to = address("0xe592427a0aece92de3edee1f18e0157c05861564");
        let value_wei = decimal("0");

        let result = UniswapV3ExactOutputMapper::new()
            .map(
                &ctx(&token_registry, &from, &to, &value_wei),
                &exact_output_decoded(),
            )
            .unwrap();

        assert_eq!(result.len(), 1);
        let Action::Swap(swap) = &result[0].action else {
            panic!("expected swap action");
        };
        assert_eq!(swap.mode, SwapMode::ExactOut);
        assert_eq!(swap.amount_in.kind, AmountKind::Max);
        assert_eq!(swap.amount_out.kind, AmountKind::Exact);
        assert_eq!(
            swap.token_in.address,
            Some(address("0xdac17f958d2ee523a2206206994597c13d831ec7"))
        );
        assert_eq!(
            swap.token_out.address,
            Some(address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"))
        );
        assert_eq!(swap.fee_bps, Some(30));
    }

    #[test]
    fn test_map_uses_token_registry_for_symbol() {
        let token_registry = metadata_registry();
        let from = address("0x0000000000000000000000000000000000000001");
        let to = address("0xe592427a0aece92de3edee1f18e0157c05861564");
        let value_wei = decimal("0");

        let result = UniswapV3Mapper::new()
            .map(
                &ctx(&token_registry, &from, &to, &value_wei),
                &exact_input_decoded(),
            )
            .unwrap();

        assert_eq!(result, vec![expected_exact_input_envelope(true)]);
        let Action::Swap(swap) = &result[0].action else {
            panic!("expected swap action");
        };
        assert_eq!(swap.token_in.symbol.as_deref(), Some("USDT"));
        assert_eq!(swap.token_in.decimals, Some(6));
        assert_eq!(swap.token_out.symbol.as_deref(), Some("WETH"));
        assert_eq!(swap.token_out.decimals, Some(18));
    }

    fn fixture(input: &str) -> (Fixture, Vec<u8>) {
        let fixture: Fixture = serde_json::from_str(input).unwrap();
        let data = fixture.rpc.params[0]
            .data
            .strip_prefix("0x")
            .unwrap()
            .to_owned();
        (fixture, hex::decode(data).unwrap())
    }

    fn decode_context<'a>(
        fixture: &'a Fixture,
        to: &'a Address,
        value: &'a DecimalString,
    ) -> DecodeContext<'a> {
        DecodeContext {
            chain_id: fixture.chain_id,
            to,
            value,
            block_timestamp: Some(1_700_000_000),
        }
    }

    fn build_fixture(input: &str, single: bool) -> Vec<ActionEnvelope> {
        let (fixture, calldata) = fixture(input);
        let tx = &fixture.rpc.params[0];
        let token_registry = metadata_registry();
        let from = address(&tx.from);
        let to = address(&tx.to);
        let value_wei = decimal("0");
        let decoded = if single {
            ExactInputSingleDecoder::new()
                .decode(&decode_context(&fixture, &to, &value_wei), &calldata)
                .unwrap()
        } else {
            ExactInputDecoder::new()
                .decode(&decode_context(&fixture, &to, &value_wei), &calldata)
                .unwrap()
        };

        UniswapV3Mapper::new()
            .map(&ctx(&token_registry, &from, &to, &value_wei), &decoded)
            .unwrap()
    }

    #[test]
    fn test_decoder_mapper_chain_produces_expected_exact_input_single_envelope() {
        let result = build_fixture(
            include_str!(
                "../../../../integration-tests/data/golden/inputs/swap_uniswap_v3_exact_input_single.json"
            ),
            true,
        );

        assert_eq!(result, vec![expected_exact_input_single_envelope(true)]);
    }

    #[test]
    fn test_decoder_mapper_chain_produces_expected_exact_input_envelope() {
        let result = build_fixture(
            include_str!(
                "../../../../integration-tests/data/golden/inputs/swap_uniswap_v3_exact_input_multi.json"
            ),
            false,
        );

        assert_eq!(result, vec![expected_exact_input_envelope(true)]);
    }
}
