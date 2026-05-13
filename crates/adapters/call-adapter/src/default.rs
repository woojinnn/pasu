//! `DefaultCallAdapter` - the standard composite: look up a Decoder, decode,
//! look up a Mapper, map. 99% of protocols use this. Special cases (e.g.
//! Universal Router's nested command stream that must be handled atomically)
//! can implement CallAdapter directly.

use crate::call_adapter::{AdapterError, CallAdapter, CallAdapterId, CallContext};

pub struct DefaultCallAdapter {
    id: CallAdapterId,
    match_keys: Vec<abi_resolver::CallMatchKey>,
}

impl DefaultCallAdapter {
    pub fn new(id: CallAdapterId, match_keys: Vec<abi_resolver::CallMatchKey>) -> Self {
        Self { id, match_keys }
    }
}

impl CallAdapter for DefaultCallAdapter {
    fn id(&self) -> CallAdapterId {
        self.id.clone()
    }

    fn match_keys(&self) -> Vec<abi_resolver::CallMatchKey> {
        self.match_keys.clone()
    }

    fn build(
        &self,
        ctx: &CallContext<'_>,
        calldata: &[u8],
    ) -> Result<Vec<policy_engine::ActionEnvelope>, AdapterError> {
        if calldata.len() < 4 {
            return Err(AdapterError::CalldataTooShort(calldata.len()));
        }

        let selector: [u8; 4] = calldata[0..4].try_into().unwrap();
        let key = abi_resolver::CallMatchKey {
            chain_id: ctx.chain_id,
            to: ctx.to.clone(),
            selector,
        };
        let decoder = ctx
            .decoder_registry
            .resolve(&key)
            .ok_or(AdapterError::NoDecoder)?;
        let dec_ctx = abi_resolver::DecodeContext {
            chain_id: ctx.chain_id,
            to: ctx.to,
            value: ctx.value_wei,
            block_timestamp: ctx.block_timestamp,
        };
        let decoded = decoder.decode(&dec_ctx, calldata)?;

        let mapper_key = mappers::MapperMatchKey {
            decoder_id: decoded.decoder_id.clone(),
        };
        let mapper = ctx
            .mapper_registry
            .resolve(&mapper_key)
            .ok_or_else(|| AdapterError::NoMapper(decoded.decoder_id.as_str().to_string()))?;
        let map_ctx = mappers::MapContext {
            chain_id: ctx.chain_id,
            from: ctx.from,
            to: ctx.to,
            value_wei: ctx.value_wei,
            block_timestamp: ctx.block_timestamp,
            token_registry: ctx.token_registry,
        };

        Ok(mapper.map(&map_ctx, &decoded)?)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use abi_resolver::{
        CallMatchKey, DecodeContext, DecodedCall, Decoder, DecoderError, DecoderId,
        InMemoryDecoderRegistry,
    };
    use mappers::{
        EmptyTokenRegistry, InMemoryMapperRegistry, MapContext, Mapper, MapperError, MapperId,
        MapperMatchKey, TokenMetadata, TokenRegistry,
    };
    use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
    use policy_engine::action::misc::WrapAction;
    use policy_engine::action::{
        Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
        Category, DecimalString, Validity, ValiditySource,
    };
    use serde::Deserialize;

    use super::DefaultCallAdapter;
    use crate::{AdapterError, CallAdapter, CallAdapterId, CallContext};

    struct MockDecoder {
        id: DecoderId,
        keys: Vec<CallMatchKey>,
    }

    impl MockDecoder {
        fn new(id: &str, keys: Vec<CallMatchKey>) -> Self {
            Self {
                id: DecoderId::new(id),
                keys,
            }
        }
    }

    impl Decoder for MockDecoder {
        fn id(&self) -> DecoderId {
            self.id.clone()
        }

        fn match_keys(&self) -> Vec<CallMatchKey> {
            self.keys.clone()
        }

        fn decode(
            &self,
            _ctx: &DecodeContext<'_>,
            _calldata: &[u8],
        ) -> Result<DecodedCall, DecoderError> {
            Ok(DecodedCall {
                decoder_id: self.id.clone(),
                function_signature: "deposit()".to_owned(),
                args: Vec::new(),
                nested: Vec::new(),
            })
        }
    }

    struct MockMapper {
        id: MapperId,
    }

    impl MockMapper {
        fn new(id: &str) -> Self {
            Self {
                id: MapperId::new(id),
            }
        }
    }

    impl Mapper for MockMapper {
        fn id(&self) -> MapperId {
            self.id.clone()
        }

        fn accepts(&self, _decoded: &DecodedCall) -> bool {
            true
        }

        fn map(
            &self,
            ctx: &MapContext<'_>,
            _decoded: &DecodedCall,
        ) -> Result<Vec<ActionEnvelope>, MapperError> {
            Ok(vec![wrap_envelope(ctx.chain_id, ctx.to.clone())])
        }
    }

    fn address(value: &str) -> Address {
        value.parse().unwrap()
    }

    fn decimal(value: &str) -> DecimalString {
        value.parse().unwrap()
    }

    fn key(selector: [u8; 4]) -> CallMatchKey {
        CallMatchKey {
            chain_id: 1,
            to: address("0x2222222222222222222222222222222222222222"),
            selector,
        }
    }

    fn wrap_envelope(chain_id: u64, recipient: Address) -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Misc,
            action: Action::Wrap(WrapAction {
                native_asset: AssetRef {
                    kind: AssetKind::Native,
                    chain_id,
                    address: None,
                    symbol: Some("ETH".to_owned()),
                    decimals: Some(18),
                },
                wrapped_asset: AssetRef {
                    kind: AssetKind::Erc20,
                    chain_id,
                    address: Some(address("0x3333333333333333333333333333333333333333")),
                    symbol: Some("WETH".to_owned()),
                    decimals: Some(18),
                },
                amount: AmountConstraint {
                    kind: AmountKind::Exact,
                    value: Some(decimal("100")),
                },
                recipient,
            }),
        }
    }

    fn ctx<'a>(
        from: &'a Address,
        to: &'a Address,
        value_wei: &'a DecimalString,
        token_registry: &'a EmptyTokenRegistry,
        decoder_registry: &'a InMemoryDecoderRegistry,
        mapper_registry: &'a InMemoryMapperRegistry,
    ) -> CallContext<'a> {
        CallContext {
            chain_id: 1,
            from,
            to,
            value_wei,
            block_timestamp: Some(123),
            token_registry,
            decoder_registry,
            mapper_registry,
        }
    }

    #[test]
    fn test_default_call_adapter_happy_path() {
        let selector = [0xde, 0xad, 0xbe, 0xef];
        let key = key(selector);
        let decoder = Arc::new(MockDecoder::new("mock-decoder", vec![key.clone()]));
        let decoder_registry = InMemoryDecoderRegistry::builder().register(decoder).build();
        let mapper_registry = InMemoryMapperRegistry::builder()
            .register(
                MapperMatchKey {
                    decoder_id: DecoderId::new("mock-decoder"),
                },
                Arc::new(MockMapper::new("mock-mapper")),
            )
            .build();
        let token_registry = EmptyTokenRegistry;
        let from = address("0x1111111111111111111111111111111111111111");
        let to = key.to.clone();
        let value_wei = decimal("0");
        let adapter = DefaultCallAdapter::new(CallAdapterId::new("default"), vec![key]);

        let result = adapter
            .build(
                &ctx(
                    &from,
                    &to,
                    &value_wei,
                    &token_registry,
                    &decoder_registry,
                    &mapper_registry,
                ),
                &[0xde, 0xad, 0xbe, 0xef, 0x00],
            )
            .unwrap();

        assert_eq!(result, vec![wrap_envelope(1, to)]);
    }

    #[test]
    fn test_default_call_adapter_no_decoder() {
        let decoder_registry = InMemoryDecoderRegistry::empty();
        let mapper_registry = InMemoryMapperRegistry::empty();
        let token_registry = EmptyTokenRegistry;
        let from = address("0x1111111111111111111111111111111111111111");
        let to = address("0x2222222222222222222222222222222222222222");
        let value_wei = decimal("0");
        let adapter =
            DefaultCallAdapter::new(CallAdapterId::new("default"), vec![key([1, 2, 3, 4])]);

        let err = adapter
            .build(
                &ctx(
                    &from,
                    &to,
                    &value_wei,
                    &token_registry,
                    &decoder_registry,
                    &mapper_registry,
                ),
                &[0xde, 0xad, 0xbe, 0xef],
            )
            .unwrap_err();

        assert!(matches!(err, AdapterError::NoDecoder));
    }

    #[test]
    fn test_default_call_adapter_no_mapper() {
        let selector = [0xde, 0xad, 0xbe, 0xef];
        let key = key(selector);
        let decoder_registry = InMemoryDecoderRegistry::builder()
            .register(Arc::new(MockDecoder::new(
                "mock-decoder",
                vec![key.clone()],
            )))
            .build();
        let mapper_registry = InMemoryMapperRegistry::empty();
        let token_registry = EmptyTokenRegistry;
        let from = address("0x1111111111111111111111111111111111111111");
        let to = key.to.clone();
        let value_wei = decimal("0");
        let adapter = DefaultCallAdapter::new(CallAdapterId::new("default"), vec![key]);

        let err = adapter
            .build(
                &ctx(
                    &from,
                    &to,
                    &value_wei,
                    &token_registry,
                    &decoder_registry,
                    &mapper_registry,
                ),
                &[0xde, 0xad, 0xbe, 0xef],
            )
            .unwrap_err();

        match err {
            AdapterError::NoMapper(id) => assert_eq!(id, "mock-decoder"),
            other => panic!("expected NoMapper, got {other:?}"),
        }
    }

    #[test]
    fn test_default_call_adapter_calldata_too_short() {
        let decoder_registry = InMemoryDecoderRegistry::empty();
        let mapper_registry = InMemoryMapperRegistry::empty();
        let token_registry = EmptyTokenRegistry;
        let from = address("0x1111111111111111111111111111111111111111");
        let to = address("0x2222222222222222222222222222222222222222");
        let value_wei = decimal("0");
        let adapter =
            DefaultCallAdapter::new(CallAdapterId::new("default"), vec![key([1, 2, 3, 4])]);

        let err = adapter
            .build(
                &ctx(
                    &from,
                    &to,
                    &value_wei,
                    &token_registry,
                    &decoder_registry,
                    &mapper_registry,
                ),
                &[0xde, 0xad, 0xbe],
            )
            .unwrap_err();

        assert!(matches!(err, AdapterError::CalldataTooShort(3)));
    }

    #[derive(Deserialize)]
    struct V2Fixture {
        chain_id: u64,
        rpc: V2Rpc,
    }

    #[derive(Deserialize)]
    struct V2Rpc {
        params: Vec<V2TxParam>,
    }

    #[derive(Deserialize)]
    struct V2TxParam {
        from: String,
        to: String,
        data: String,
    }

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

    fn v2_fixture(input: &str) -> (V2Fixture, Vec<u8>) {
        let fixture: V2Fixture = serde_json::from_str(input).unwrap();
        let data = fixture.rpc.params[0]
            .data
            .strip_prefix("0x")
            .unwrap()
            .to_owned();
        (fixture, hex::decode(data).unwrap())
    }

    fn v2_token_registry() -> StaticTokenRegistry {
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
                address("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
                TokenMetadata {
                    symbol: "WETH".to_owned(),
                    decimals: 18,
                },
            ),
        ])
    }

    fn v2_mapper_registry() -> InMemoryMapperRegistry {
        use mappers::protocols::uniswap_v2::{
            SwapExactTokensForTokensMapper, SwapTokensForExactTokensMapper,
        };

        InMemoryMapperRegistry::builder()
            .register(
                MapperMatchKey {
                    decoder_id: DecoderId::new("uniswap-v2/swapExactTokensForTokens"),
                },
                Arc::new(SwapExactTokensForTokensMapper::new()),
            )
            .register(
                MapperMatchKey {
                    decoder_id: DecoderId::new("uniswap-v2/swapTokensForExactTokens"),
                },
                Arc::new(SwapTokensForExactTokensMapper::new()),
            )
            .build()
    }

    fn erc20_asset(address_value: &str, symbol: &str, decimals: u8) -> AssetRef {
        AssetRef {
            kind: AssetKind::Erc20,
            chain_id: 1,
            address: Some(address(address_value)),
            symbol: Some(symbol.to_owned()),
            decimals: Some(decimals),
        }
    }

    fn expected_v2_exact_in_envelope() -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(SwapAction {
                mode: SwapMode::ExactIn,
                token_in: erc20_asset("0xdac17f958d2ee523a2206206994597c13d831ec7", "USDT", 6),
                token_out: erc20_asset("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "WETH", 18),
                amount_in: AmountConstraint {
                    kind: AmountKind::Exact,
                    value: Some(decimal("200000000")),
                },
                amount_out: AmountConstraint {
                    kind: AmountKind::Min,
                    value: Some(decimal("0")),
                },
                recipient: address("0x1111111111111111111111111111111111111111"),
                slippage_bps: None,
                validity: Some(Validity {
                    expires_at: decimal("9999999999"),
                    source: ValiditySource::TxDeadline,
                }),
                fee_bps: Some(30),
                enrichment: SwapEnrichment::default(),
            }),
        }
    }

    fn expected_v2_exact_out_envelope() -> ActionEnvelope {
        ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(SwapAction {
                mode: SwapMode::ExactOut,
                token_in: erc20_asset("0xdac17f958d2ee523a2206206994597c13d831ec7", "USDT", 6),
                token_out: erc20_asset("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "WETH", 18),
                amount_in: AmountConstraint {
                    kind: AmountKind::Max,
                    value: Some(decimal("4000000000")),
                },
                amount_out: AmountConstraint {
                    kind: AmountKind::Exact,
                    value: Some(decimal("1000000000000000000")),
                },
                recipient: address("0x1111111111111111111111111111111111111111"),
                slippage_bps: None,
                validity: Some(Validity {
                    expires_at: decimal("9999999999"),
                    source: ValiditySource::TxDeadline,
                }),
                fee_bps: Some(30),
                enrichment: SwapEnrichment::default(),
            }),
        }
    }

    fn v2_decoder_registry() -> InMemoryDecoderRegistry {
        use abi_resolver::decoders::uniswap_v2::{
            SwapExactTokensForTokensDecoder, SwapTokensForExactTokensDecoder,
        };

        InMemoryDecoderRegistry::builder()
            .register(Arc::new(SwapExactTokensForTokensDecoder::new()))
            .register(Arc::new(SwapTokensForExactTokensDecoder::new()))
            .build()
    }

    fn build_v2_fixture(input: &str) -> Vec<ActionEnvelope> {
        use abi_resolver::DecoderRegistry as _;

        let (fixture, calldata) = v2_fixture(input);
        let tx = &fixture.rpc.params[0];
        let token_registry = v2_token_registry();
        let decoder_registry = v2_decoder_registry();
        let mapper_registry = v2_mapper_registry();
        let adapter = DefaultCallAdapter::new(
            CallAdapterId::new("default/uniswap-v2"),
            decoder_registry.match_keys(),
        );
        let from = address(&tx.from);
        let to = address(&tx.to);
        let value_wei = decimal("0");

        adapter
            .build(
                &CallContext {
                    chain_id: fixture.chain_id,
                    from: &from,
                    to: &to,
                    value_wei: &value_wei,
                    block_timestamp: Some(1_700_000_000),
                    token_registry: &token_registry,
                    decoder_registry: &decoder_registry,
                    mapper_registry: &mapper_registry,
                },
                &calldata,
            )
            .unwrap()
    }

    #[test]
    fn test_default_call_adapter_build_produces_expected_v2_swap_exact_in_envelope() {
        let result = build_v2_fixture(include_str!(
            "../../../integration-tests/data/golden/inputs/swap_uniswap_v2_exact_in.json"
        ));

        assert_eq!(result, vec![expected_v2_exact_in_envelope()]);
    }

    #[test]
    fn test_default_call_adapter_build_produces_expected_v2_swap_exact_out_envelope() {
        let result = build_v2_fixture(include_str!(
            "../../../integration-tests/data/golden/inputs/swap_uniswap_v2_exact_out.json"
        ));

        assert_eq!(result, vec![expected_v2_exact_out_envelope()]);
    }
}
