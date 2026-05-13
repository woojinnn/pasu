use std::sync::Arc;

use abi_resolver::decoders::uniswap_v2::{
    SwapExactTokensForTokensDecoder, SwapTokensForExactTokensDecoder,
    SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID, SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID,
};
use abi_resolver::decoders::uniswap_v3::{
    ExactInputDecoder, ExactInputSingleDecoder, UNISWAP_V3_DECODER_ID,
};
use abi_resolver::DecoderId;
use abi_resolver::InMemoryDecoderRegistry;
use call_adapter::InMemoryCallAdapterRegistry;
use mappers::protocols::uniswap_v2::{
    SwapExactTokensForTokensMapper, SwapTokensForExactTokensMapper,
};
use mappers::protocols::uniswap_v3::UniswapV3Mapper;
use mappers::{InMemoryMapperRegistry, MapperMatchKey};
use sign_resolver::adapters::eip2612::Eip2612Adapter;
use sign_resolver::adapters::permit2::Permit2Adapter;
use sign_resolver::InMemorySignAdapterRegistry;

pub struct DefaultRegistries {
    pub decoders: Arc<InMemoryDecoderRegistry>,
    pub mappers: Arc<InMemoryMapperRegistry>,
    pub call_adapters: Arc<InMemoryCallAdapterRegistry>,
    pub sign_adapters: Arc<InMemorySignAdapterRegistry>,
}

impl DefaultRegistries {
    #[must_use]
    pub fn standard() -> Self {
        let decoders = Arc::new(
            InMemoryDecoderRegistry::builder()
                .register(Arc::new(SwapExactTokensForTokensDecoder::new()))
                .register(Arc::new(SwapTokensForExactTokensDecoder::new()))
                .register(Arc::new(ExactInputSingleDecoder::new()))
                .register(Arc::new(ExactInputDecoder::new()))
                .build(),
        );
        let mappers = Arc::new(
            InMemoryMapperRegistry::builder()
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID),
                    },
                    Arc::new(SwapExactTokensForTokensMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID),
                    },
                    Arc::new(SwapTokensForExactTokensMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(UNISWAP_V3_DECODER_ID),
                    },
                    Arc::new(UniswapV3Mapper::new()),
                )
                .build(),
        );
        let call_adapters = Arc::new(InMemoryCallAdapterRegistry::from_decoder_registry(
            decoders.as_ref(),
        ));
        let sign_adapters = Arc::new(
            InMemorySignAdapterRegistry::builder()
                .register(Arc::new(Eip2612Adapter::new()))
                .register(Arc::new(Permit2Adapter::new()))
                .build(),
        );

        Self {
            decoders,
            mappers,
            call_adapters,
            sign_adapters,
        }
    }
}

#[cfg(test)]
mod tests {
    use abi_resolver::decoders::uniswap_v2::{
        SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID, SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR,
        SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID, SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR,
        UNISWAP_V2_ROUTER_MAINNET,
    };
    use abi_resolver::{CallMatchKey, DecoderId, DecoderRegistry};
    use mappers::{MapperMatchKey, MapperRegistry};
    use policy_engine::action::Address;
    use sign_resolver::{SignAdapterRegistry, SignMatchKey};
    use std::str::FromStr as _;

    use super::DefaultRegistries;

    fn address(value: &str) -> Address {
        Address::from_str(value).unwrap()
    }

    fn v2_key(selector: [u8; 4]) -> CallMatchKey {
        CallMatchKey {
            chain_id: 1,
            to: address(UNISWAP_V2_ROUTER_MAINNET),
            selector,
        }
    }

    #[test]
    fn test_standard_registries_built_no_panic() {
        let _registries = DefaultRegistries::standard();
    }

    #[test]
    fn test_standard_registries_resolve_v2_keys() {
        let registries = DefaultRegistries::standard();

        assert!(registries
            .decoders
            .resolve(&v2_key(SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR))
            .is_some());
        assert!(registries
            .decoders
            .resolve(&v2_key(SWAP_TOKENS_FOR_EXACT_TOKENS_SELECTOR))
            .is_some());
        assert!(registries
            .mappers
            .resolve(&MapperMatchKey {
                decoder_id: DecoderId::new(SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID),
            })
            .is_some());
        assert!(registries
            .mappers
            .resolve(&MapperMatchKey {
                decoder_id: DecoderId::new(SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID),
            })
            .is_some());
    }

    #[test]
    fn test_standard_registries_resolve_eip2612_wildcard() {
        let registries = DefaultRegistries::standard();

        let adapter = registries.sign_adapters.resolve(&SignMatchKey {
            chain_id: 1,
            verifying_contract: None,
            primary_type: "Permit".to_owned(),
        });

        assert!(adapter.is_some());
    }
}
