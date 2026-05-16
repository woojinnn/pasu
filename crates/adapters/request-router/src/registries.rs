use std::sync::Arc;

use abi_resolver::ids::{
    ERC20_APPROVE_DECODER_ID, ERC20_TRANSFER_DECODER_ID, ERC20_TRANSFER_FROM_DECODER_ID,
    SET_APPROVAL_FOR_ALL_DECODER_ID,
};
use abi_resolver::ids::{
    EXACT_OUTPUT_DECODER_ID, EXACT_OUTPUT_SINGLE_DECODER_ID, UNISWAP_V3_DECODER_ID,
};
use abi_resolver::ids::{
    SR02_EXACT_INPUT_DECODER_ID, SR02_EXACT_INPUT_SINGLE_DECODER_ID, SR02_EXACT_OUTPUT_DECODER_ID,
    SR02_EXACT_OUTPUT_SINGLE_DECODER_ID,
};
use abi_resolver::ids::{
    SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID, SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID,
    SWAP_EXACT_ETH_FOR_TOKENS_FOT_DECODER_ID, SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID,
    SWAP_EXACT_TOKENS_FOR_ETH_FOT_DECODER_ID, SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID,
    SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_DECODER_ID, SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID,
    SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID,
};
use abi_resolver::ids::{WETH_DEPOSIT_DECODER_ID, WETH_WITHDRAW_DECODER_ID};
use abi_resolver::openchain::{OpenchainIndex, SignatureCandidate};
use abi_resolver::resolver::Resolver;
use abi_resolver::sourcify::SourcifyIndex;
use abi_resolver::DecoderId;
use abi_resolver::InMemoryDecoderRegistry;
use call_adapter::{InMemoryCallAdapterRegistry, MultiRouterCallAdapter, WethWithdrawCallAdapter};
use mappers::protocols::erc20::{
    Erc20ApproveMapper, Erc20TransferFromMapper, Erc20TransferMapper, SetApprovalForAllMapper,
};
use mappers::protocols::swap_router_02::{
    Sr02ExactInputMapper, Sr02ExactInputSingleMapper, Sr02ExactOutputMapper,
    Sr02ExactOutputSingleMapper,
};
use mappers::protocols::uniswap_v2::{
    SwapETHForExactTokensMapper, SwapExactETHForTokensFotMapper, SwapExactETHForTokensMapper,
    SwapExactTokensForETHFotMapper, SwapExactTokensForETHMapper, SwapExactTokensForTokensFotMapper,
    SwapExactTokensForTokensMapper, SwapTokensForExactETHMapper, SwapTokensForExactTokensMapper,
};
use mappers::protocols::uniswap_v3::{
    UniswapV3ExactOutputMapper, UniswapV3ExactOutputSingleMapper, UniswapV3Mapper,
};
use mappers::protocols::weth::{WethDepositMapper, WethWithdrawMapper};
use mappers::{InMemoryMapperRegistry, MapperMatchKey};
use sign_resolver::adapters::eip2612::Eip2612Adapter;
use sign_resolver::adapters::permit2::Permit2Adapter;
use sign_resolver::InMemorySignAdapterRegistry;

/// Built-in Sourcify bundle, embedded at compile time. Mirrors the bundle
/// loaded by `web-server`. Keeps the fallback decode path self-contained: even
/// if no external SQLite DB is attached, we can still resolve calldata for the
/// curated contract set.
const SOURCIFY_BUNDLE: &[u8] = include_bytes!("../../abi-resolver/data/sourcify.json");

/// Selector → human-readable signature pairs used to seed the openchain
/// fallback tier. Mirrors the seed list in `web-server`'s build_resolver().
/// Keeping it here lets the `request-router` test/standalone wasm path resolve
/// common DeFi entrypoints without depending on the web-server binary.
fn openchain_seed() -> &'static [([u8; 4], &'static str)] {
    &[
        ([0x09, 0x5e, 0xa7, 0xb3], "approve(address spender, uint256 amount)"),
        ([0xa9, 0x05, 0x9c, 0xbb], "transfer(address to, uint256 amount)"),
        (
            [0x23, 0xb8, 0x72, 0xdd],
            "transferFrom(address from, address to, uint256 amount)",
        ),
        (
            [0xa2, 0x2c, 0xb4, 0x65],
            "setApprovalForAll(address operator, bool approved)",
        ),
        ([0x70, 0xa0, 0x82, 0x31], "balanceOf(address account)"),
        (
            [0x41, 0x4b, 0xf3, 0x89],
            "exactInputSingle((address tokenIn, address tokenOut, uint24 fee, address recipient, uint256 deadline, uint256 amountIn, uint256 amountOutMinimum, uint160 sqrtPriceLimitX96) params)",
        ),
        (
            [0xc0, 0x4b, 0x8d, 0x59],
            "exactInput((bytes path, address recipient, uint256 deadline, uint256 amountIn, uint256 amountOutMinimum) params)",
        ),
        ([0xac, 0x96, 0x50, 0xd8], "multicall(bytes[] data)"),
        (
            [0x5a, 0xe4, 0x01, 0xdc],
            "multicall(uint256 deadline, bytes[] data)",
        ),
        (
            [0x38, 0xed, 0x17, 0x39],
            "swapExactTokensForTokens(uint256 amountIn, uint256 amountOutMin, address[] path, address to, uint256 deadline)",
        ),
        (
            [0x7f, 0xf3, 0x6a, 0xb5],
            "swapExactETHForTokens(uint256 amountOutMin, address[] path, address to, uint256 deadline)",
        ),
        (
            [0xb6, 0xf9, 0xde, 0x95],
            "swapExactETHForTokensSupportingFeeOnTransferTokens(uint256 amountOutMin, address[] path, address to, uint256 deadline)",
        ),
        (
            [0x79, 0x1a, 0xc9, 0x47],
            "swapExactTokensForETHSupportingFeeOnTransferTokens(uint256 amountIn, uint256 amountOutMin, address[] path, address to, uint256 deadline)",
        ),
        (
            [0x24, 0x85, 0x6b, 0xc3],
            "execute(bytes commands, bytes[] inputs)",
        ),
        (
            [0x35, 0x93, 0x56, 0x4c],
            "execute(bytes commands, bytes[] inputs, uint256 deadline)",
        ),
        (
            [0x61, 0x7b, 0xa0, 0x37],
            "supply(address asset, uint256 amount, address onBehalfOf, uint16 referralCode)",
        ),
        (
            [0x69, 0x32, 0x8d, 0xec],
            "withdraw(address asset, uint256 amount, address to)",
        ),
        (
            [0xa4, 0x15, 0xbc, 0xad],
            "borrow(address asset, uint256 amount, uint256 interestRateMode, uint16 referralCode, address onBehalfOf)",
        ),
        (
            [0x57, 0x3a, 0xde, 0x81],
            "repay(address asset, uint256 amount, uint256 interestRateMode, address onBehalfOf)",
        ),
        ([0xd0, 0xe3, 0x0d, 0xb0], "deposit()"),
        ([0x2e, 0x1a, 0x7d, 0x4d], "withdraw(uint256 wad)"),
    ]
}

/// Build the legacy `Resolver` that powers the second-tier Sourcify fallback.
/// Always loads the embedded curated bundle + openchain seed. When the `sqlite`
/// feature is on, also tries to attach `/tmp/sourcify_dump/sourcify.sqlite`
/// (or `$SOURCIFY_SQLITE_PATH`) for full Mainnet coverage.
fn build_fallback_resolver() -> Resolver {
    let sourcify = SourcifyIndex::load_bundle(SOURCIFY_BUNDLE)
        .expect("embedded sourcify bundle must deserialize");
    let mut openchain = OpenchainIndex::empty();
    for (selector, signature) in openchain_seed() {
        openchain.insert(
            *selector,
            SignatureCandidate {
                signature: (*signature).into(),
                verified: true,
            },
        );
    }
    let resolver = Resolver::new(sourcify, openchain);

    #[cfg(feature = "sqlite")]
    {
        use abi_resolver::sqlite_index::SqliteSourcifyIndex;
        use std::path::{Path, PathBuf};

        let sqlite_path = std::env::var("SOURCIFY_SQLITE_PATH")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                let default = Path::new("/tmp/sourcify_dump/sourcify.sqlite");
                default.exists().then(|| default.to_path_buf())
            });
        if let Some(path) = sqlite_path {
            match SqliteSourcifyIndex::open_read_only(&path) {
                Ok(db) => {
                    tracing::info!("attached SQLite Sourcify dump at {}", path.display());
                    return resolver.with_sqlite(db);
                }
                Err(e) => {
                    tracing::warn!(
                        "could not attach SQLite dump at {} ({e}); using curated bundle only",
                        path.display()
                    );
                }
            }
        } else {
            tracing::info!(
                "no SQLite Sourcify dump configured (set SOURCIFY_SQLITE_PATH); using curated bundle only"
            );
        }
    }

    resolver
}

pub struct DefaultRegistries {
    pub decoders: Arc<InMemoryDecoderRegistry>,
    pub mappers: Arc<InMemoryMapperRegistry>,
    pub call_adapters: Arc<InMemoryCallAdapterRegistry>,
    pub sign_adapters: Arc<InMemorySignAdapterRegistry>,
    /// Legacy `Resolver` used as the fallback decode tier when no per-function
    /// `Decoder` matches in `route_request`. The bridge module converts its
    /// `decode::DecodedCall` output to the new shape so existing mappers can
    /// consume it unchanged.
    pub resolver: Arc<Resolver>,
}

impl DefaultRegistries {
    #[must_use]
    pub fn standard() -> Self {
        // Option A: only the Universal Router uses a per-function `Decoder` +
        // `CallAdapter` because UR also runs an opcode dispatcher that goes
        // beyond plain ABI decoding (V3_SWAP, WRAP_ETH, …). Everything else
        // (ERC20, V2, V3, SR02, WETH, …) is routed through the Sourcify
        // fallback in `request_router::route_call_fallback` and dispatched to
        // a `Mapper` via the bridge's `selector → decoder_id` table. As a
        // result the `decoders` registry is intentionally empty here — kept
        // for type compatibility with downstream consumers.
        let decoders = Arc::new(InMemoryDecoderRegistry::builder().build());
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
                // V2 fee-on-transfer variants (3 — exact-IN only).
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SWAP_EXACT_TOKENS_FOR_TOKENS_FOT_DECODER_ID),
                    },
                    Arc::new(SwapExactTokensForTokensFotMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SWAP_EXACT_ETH_FOR_TOKENS_FOT_DECODER_ID),
                    },
                    Arc::new(SwapExactETHForTokensFotMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SWAP_EXACT_TOKENS_FOR_ETH_FOT_DECODER_ID),
                    },
                    Arc::new(SwapExactTokensForETHFotMapper::new()),
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
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SET_APPROVAL_FOR_ALL_DECODER_ID),
                    },
                    Arc::new(SetApprovalForAllMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(WETH_DEPOSIT_DECODER_ID),
                    },
                    Arc::new(WethDepositMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(WETH_WITHDRAW_DECODER_ID),
                    },
                    Arc::new(WethWithdrawMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SR02_EXACT_INPUT_SINGLE_DECODER_ID),
                    },
                    Arc::new(Sr02ExactInputSingleMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SR02_EXACT_INPUT_DECODER_ID),
                    },
                    Arc::new(Sr02ExactInputMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SR02_EXACT_OUTPUT_SINGLE_DECODER_ID),
                    },
                    Arc::new(Sr02ExactOutputSingleMapper::new()),
                )
                .register(
                    MapperMatchKey {
                        decoder_id: DecoderId::new(SR02_EXACT_OUTPUT_DECODER_ID),
                    },
                    Arc::new(Sr02ExactOutputMapper::new()),
                )
                .build(),
        );
        // Only the Universal Router needs a dedicated `CallAdapter` because its
        // `execute()` calldata wraps an opcode stream that goes beyond plain
        // ABI decoding. Every other selector falls through to the Sourcify
        // fallback path which lowers via `bridge` → `MapperRegistry`.
        let call_adapters = Arc::new(
            InMemoryCallAdapterRegistry::builder()
                .register(Arc::new(MultiRouterCallAdapter::uniswap_ur()))
                .register(Arc::new(MultiRouterCallAdapter::pancake_ur()))
                .register(Arc::new(WethWithdrawCallAdapter::new()))
                .build(),
        );
        let sign_adapters = Arc::new(
            InMemorySignAdapterRegistry::builder()
                .register(Arc::new(Eip2612Adapter::new()))
                .register(Arc::new(Permit2Adapter::new()))
                .build(),
        );

        let resolver = Arc::new(build_fallback_resolver());

        Self {
            decoders,
            mappers,
            call_adapters,
            sign_adapters,
            resolver,
        }
    }
}

#[cfg(test)]
mod tests {
    use abi_resolver::ids::{
        SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID, SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID,
        SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID, SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID,
        SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID, SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID,
    };
    use abi_resolver::DecoderId;
    use mappers::{MapperMatchKey, MapperRegistry};
    use sign_resolver::{SignAdapterRegistry, SignMatchKey};

    use super::DefaultRegistries;

    #[test]
    fn test_standard_registries_built_no_panic() {
        let _registries = DefaultRegistries::standard();
    }

    #[test]
    fn test_standard_registries_resolve_v2_mapper_keys() {
        // Option A: decoders registry is empty; all dispatch happens via the
        // mapper registry keyed by `decoder_id` (looked up from selector by
        // the bridge module during the Sourcify fallback path).
        let registries = DefaultRegistries::standard();

        for decoder_id in [
            SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID,
            SWAP_TOKENS_FOR_EXACT_TOKENS_DECODER_ID,
            SWAP_EXACT_ETH_FOR_TOKENS_DECODER_ID,
            SWAP_TOKENS_FOR_EXACT_ETH_DECODER_ID,
            SWAP_EXACT_TOKENS_FOR_ETH_DECODER_ID,
            SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID,
        ] {
            assert!(
                registries
                    .mappers
                    .resolve(&MapperMatchKey {
                        decoder_id: DecoderId::new(decoder_id),
                    })
                    .is_some(),
                "mapper for {decoder_id} should be registered"
            );
        }
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
