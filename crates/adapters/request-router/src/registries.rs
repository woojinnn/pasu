use std::collections::HashSet;
use std::sync::Arc;

use abi_resolver::decoders::erc20::{
    Erc20ApproveDecoder, Erc20TransferDecoder, Erc20TransferFromDecoder, ERC20_APPROVE_DECODER_ID,
    ERC20_TRANSFER_DECODER_ID, ERC20_TRANSFER_FROM_DECODER_ID,
};
use abi_resolver::decoders::uniswap_v2::{
    SwapETHForExactTokensDecoder, SwapExactETHForTokensDecoder, SwapExactTokensForETHDecoder,
    SwapExactTokensForTokensDecoder, SwapTokensForExactETHDecoder, SwapTokensForExactTokensDecoder,
    SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID, SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID,
    SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID, SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID,
    SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID, SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID,
};
use abi_resolver::decoders::uniswap_v3::{
    ExactInputDecoder, ExactInputSingleDecoder, ExactOutputDecoder, ExactOutputSingleDecoder,
    EXACT_OUTPUT_DECODER_ID, EXACT_OUTPUT_SINGLE_DECODER_ID, UNISWAP_V3_DECODER_ID,
};
use abi_resolver::InMemoryDecoderRegistry;
use abi_resolver::{DecoderId, DecoderRegistry};
use call_adapter::{
    CallAdapter as _, CallAdapterId, DefaultCallAdapter, InMemoryCallAdapterRegistry,
    UniversalRouterCallAdapter,
};
use mappers::protocols::erc20::{Erc20ApproveMapper, Erc20TransferFromMapper, Erc20TransferMapper};
use mappers::protocols::uniswap_v2::{
    SwapETHForExactTokensMapper, SwapExactETHForTokensMapper, SwapExactTokensForETHMapper,
    SwapExactTokensForTokensMapper, SwapTokensForExactETHMapper, SwapTokensForExactTokensMapper,
};
use mappers::protocols::uniswap_v3::{
    UniswapV3ExactOutputMapper, UniswapV3ExactOutputSingleMapper, UniswapV3Mapper,
};
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
                .register(Arc::new(SwapExactETHForTokensDecoder::new()))
                .register(Arc::new(SwapTokensForExactETHDecoder::new()))
                .register(Arc::new(SwapExactTokensForETHDecoder::new()))
                .register(Arc::new(SwapETHForExactTokensDecoder::new()))
                .register(Arc::new(ExactInputSingleDecoder::new()))
                .register(Arc::new(ExactInputDecoder::new()))
                .register(Arc::new(ExactOutputSingleDecoder::new()))
                .register(Arc::new(ExactOutputDecoder::new()))
                .register(Arc::new(Erc20ApproveDecoder::new()))
                .register(Arc::new(Erc20TransferDecoder::new()))
                .register(Arc::new(Erc20TransferFromDecoder::new()))
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
                        decoder_id: DecoderId::new(SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID),
                    },
                    Arc::new(SwapExactETHForTokensMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID),
                    },
                    Arc::new(SwapTokensForExactETHMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID),
                    },
                    Arc::new(SwapExactTokensForETHMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID),
                    },
                    Arc::new(SwapETHForExactTokensMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(UNISWAP_V3_DECODER_ID),
                    },
                    Arc::new(UniswapV3Mapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(EXACT_OUTPUT_SINGLE_DECODER_ID),
                    },
                    Arc::new(UniswapV3ExactOutputSingleMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(EXACT_OUTPUT_DECODER_ID),
                    },
                    Arc::new(UniswapV3ExactOutputMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(ERC20_APPROVE_DECODER_ID),
                    },
                    Arc::new(Erc20ApproveMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(ERC20_TRANSFER_DECODER_ID),
                    },
                    Arc::new(Erc20TransferMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(ERC20_TRANSFER_FROM_DECODER_ID),
                    },
                    Arc::new(Erc20TransferFromMapper::new()),
                )
                .build(),
        );
        let universal_router_adapter = Arc::new(UniversalRouterCallAdapter::new());
        let universal_router_targets = universal_router_adapter
            .match_keys()
            .into_iter()
            .map(|key| (key.chain_id, key.to))
            .collect::<HashSet<_>>();
        let mut call_adapter_builder =
            InMemoryCallAdapterRegistry::builder().register(universal_router_adapter);
        for key in decoders.match_keys() {
            if universal_router_targets.contains(&(key.chain_id, key.to.clone())) {
                continue;
            }
            let id = CallAdapterId::new(format!(
                "default/chain={}/to={}/sel=0x{}",
                key.chain_id,
                key.to,
                hex::encode(key.selector)
            ));
            call_adapter_builder =
                call_adapter_builder.register(Arc::new(DefaultCallAdapter::new(id, vec![key])));
        }
        let call_adapters = Arc::new(call_adapter_builder.build());
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
        SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID, SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR,
        SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID, SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR,
        SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID, SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR,
        SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID, SWAP_EXACT_TOKENS_FOR_TOKENS_SELECTOR,
        SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID, SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR,
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
            .decoders
            .resolve(&v2_key(SWAP_EXACT_ETH_FOR_TOKENS_SELECTOR))
            .is_some());
        assert!(registries
            .decoders
            .resolve(&v2_key(SWAP_TOKENS_FOR_EXACT_ETH_SELECTOR))
            .is_some());
        assert!(registries
            .decoders
            .resolve(&v2_key(SWAP_EXACT_TOKENS_FOR_ETH_SELECTOR))
            .is_some());
        assert!(registries
            .decoders
            .resolve(&v2_key(SWAP_ETH_FOR_EXACT_TOKENS_SELECTOR))
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
        assert!(registries
            .mappers
            .resolve(&MapperMatchKey {
                decoder_id: DecoderId::new(SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID),
            })
            .is_some());
        assert!(registries
            .mappers
            .resolve(&MapperMatchKey {
                decoder_id: DecoderId::new(SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID),
            })
            .is_some());
        assert!(registries
            .mappers
            .resolve(&MapperMatchKey {
                decoder_id: DecoderId::new(SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID),
            })
            .is_some());
        assert!(registries
            .mappers
            .resolve(&MapperMatchKey {
                decoder_id: DecoderId::new(SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID),
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

    #[test]
    fn route_universal_router_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../integration-tests/data/golden/inputs/swap_universal_router.json"
        ))
        .unwrap();
        let registries = DefaultRegistries::standard();
        let token_registry = mappers::EmptyTokenRegistry;
        let ctx = crate::RouterContext {
            registries: &registries,
            token_registry: &token_registry,
            block_timestamp: None,
        };
        let envelopes = crate::route_request(
            &ctx,
            fixture["rpc"]["method"].as_str().unwrap(),
            &fixture["rpc"]["params"],
            fixture["chain_id"].as_u64().unwrap(),
        )
        .unwrap();

        assert_eq!(envelopes.len(), 1);
    }
}
