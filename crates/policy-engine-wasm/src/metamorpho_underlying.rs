//! GENERATED (do not hand-edit) — MetaMorpho mainnet vault -> underlying asset map.
//! Source: blue-api.morpho.org/graphql vaults(listed:true, chainId 1) snapshot 2026-06-02
//! (same 1st-party data as the Q1 vault universe). Re-generate when the listed set changes.
//! Used by maybe_inject_metamorpho_underlying so a GeneralAdapter1 erc4626* leg
//! (vault is a runtime arg, underlying NOT in calldata) can fill the required asset.
//!
//! Re-gen: query `blue-api.morpho.org/graphql` `vaults(where:{chainId_in:[1]})` for
//! `{address, asset{address}}`, lowercase, sort by vault, emit the pairs below.
//! Coverage contract (D-A): a Bundler3 erc4626* leg whose vault is OUTSIDE this set
//! resolves no underlying, so `build_multicall_call_array_body` REFUSES the whole
//! bundle (warn-closed) rather than emit a 0x0-asset action. Re-gen to cover a
//! newly-listed vault.

/// (vault_lc, underlying_lc), SORTED by vault for binary search. 73 mainnet vaults.
pub const METAMORPHO_UNDERLYING: &[(&str, &str)] = &[
    ("0x0b2d98bbf3e38df1d1b7be7343732e32e8b1f818", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x19b3cd7032b8c062e8d44eacad661a0970dd8c55", "0x6c3ea9036406852006290770bedfcaba0e23a0e8"),
    ("0x2371e134e3455e0593363cbf89d3b6cf53740618", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0x2c25f6c25770ffec5959d34b94bf898865e5d6b1", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0x2c793f5cb25b35a99648783c01e6cccc200d2096", "0x6c3ea9036406852006290770bedfcaba0e23a0e8"),
    ("0x2ed10624315b74a78f11fabedaa1a228c198aefb", "0x1abaea1f7c830bd89acc67ec4af516284b1bc33c"),
    ("0x2f1abb81ed86be95bcf8178ba62c8e72d6834775", "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599"),
    ("0x31a5684983eee865d943a696aac155363ba024f9", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0x34ece536d2ae03192b06c0a67030d1faf4c0ba43", "0x5f7827fdeb7c20b443265fc2f40845b715385ff2"),
    ("0x443df5eee3196e9b2dd77cabd3ea76c3dee8f9b2", "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599"),
    ("0x47fe8ab9ee47dd65c24df52324181790b9f47efc", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0x4881ef0bf6d2365d3dd6499ccd7532bcdbce0658", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0x4d52545235a3df246a8928c583e47ae7eec4acfe", "0xfa2b947eec368f42195f24f36d2af29f7c24cec2"),
    ("0x4f460bb11cf958606c69a963b4a17f9daeeea8b6", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x4ff4186188f8406917293a9e01a1ca16d3cf9e59", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x500331c9ff24d9d11aee6b07734aa72343ea74a5", "0x6b175474e89094c44da98b954eedeac495271d0f"),
    ("0x56a76b428244a50513ec81e225a293d128fd581d", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x62fe596d59fb077c2df736df212e0affb522dc78", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x64964e162aa18d32f91ea5b24a09529f811aeb8e", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x6859b34a9379122d25a9fa46f0882d434fee36c3", "0xab5eb14c09d416f0ac63661e57edb7aecdb9befa"),
    ("0x68aea7b82df6ccdf76235d46445ed83f85f845a3", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x6c26793c7f1e2785c09b460676e797b716f0bc8e", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x6d6d386c4855d9b604d7e14c70526407f6272394", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0x701907283a57ff77e255c3f1aad790466b8ce4ef", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0x711a68a82dd80cb0435b281af76b0b80804efab9", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x71cb2f8038b2c5d65ddc740b2f3268890cd2a89c", "0x8292bb45bf1ee4d140127049757c2e0ff06317ed"),
    ("0x739d8a60ed4b14e4cb6dcaeaf79d2ec0ca092237", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0x75741a12b36d181f44f389e0c6b1e0210311e3ff", "0x5f7827fdeb7c20b443265fc2f40845b715385ff2"),
    ("0x777791c4d6dc2ce140d00d2828a7c93503c67777", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x78b18e07dc43017fceaabad0751d6464c0f56b25", "0x64351fc9810adad17a690e4e1717df5e7e085160"),
    ("0x78fc2c2ed1a4cdb5402365934ae5648adad094d0", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0x79fd640000f8563a866322483524a4b48f1ed702", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0x812b2c6ab3f4471c0e43d4bb61098a9211017427", "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599"),
    ("0x833adaef212c5cd3f78906b44bbfb18258f238f0", "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0"),
    ("0x888883f0eddf69ca4bfd00af93714ff97f188888", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0x8cb3649114051ca5119141a34c200d65dc0faa73", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0x8eb67a509616cd6a7c1b3c8c21d48ff57df3d458", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x95eef579155cd2c5510f312c8fa39208c3be01a8", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0x965ec3552427b8258bd0a0c7baa234618fc98d01", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0x974c8fbf4fd795f66b85b73ebc988a51f1a040a9", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0x9a8bc3b04b7f3d87cfc09ba407dced575f2d61d8", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0x9b7cca326004967f9d2b7cf5f2328d82cf65b302", "0x00000000efe302beaa2b3e6e1b18d08d69a9012a"),
    ("0xa02f5e93f783baf150aa1f8b341ae90fe0a772f7", "0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf"),
    ("0xa0804346780b4c2e3be118ac957d1db82f9d7484", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0xa1ff9c28ebc160c1dcde4b9aa9551f617880c6fb", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xa3fc33543beee52bc60babc80af3d29789637b6d", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0xa71d08a159258553a5ac190d60fa919425ff02ea", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0xa8875aaebc4f830524e35d57f9772ffacbdd6c45", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xb0f05e4de970a1aaf77f8c2f823953a367504ba9", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xbdd4859050468fbc11dec07113a6e633608a1372", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0xbeef01735c132ada46aa9aa4c54623caa92a64cb", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xbeef02e5e13584ab96848af90261f0c8ee04722a", "0x6c3ea9036406852006290770bedfcaba0e23a0e8"),
    ("0xbeef047a543e45807105e51a8bbefcc5950fcfba", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0xbeef050ecd6a16c4e7bffbb52ebba7846c4b8cd4", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0xbeef1f5bd88285e5b239b6aacb991d38cca23ac9", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xbeef2b5fd3d94469b7782aebe6364e6e6fb1b709", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xbeef346d7099865208ff331e4f648f4154ddaa05", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xbeef96b330ef1fe7ebe41ece0bd4a41a94bc03dc", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xbeefb9f61cc44895d8aec381373555a64191a9c4", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xbeefc1cdafc5b4a649b54d07afc6bf0f75c6f4e2", "0xc139190f447e929f090edeb554d95abb8b18ac1c"),
    ("0xbeefff209270748ddd194831b3fa287a5386f5bc", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xbeefff68cc520d68f82641eff84330c631e2490e", "0x6b175474e89094c44da98b954eedeac495271d0f"),
    ("0xbeeffff0629438ad198c47f80d66fa4be5c0caf6", "0x00000000efe302beaa2b3e6e1b18d08d69a9012a"),
    ("0xc080f56504e0278828a403269db945f6c6d6e014", "0xa0d69e286b938e21cbf7e51d71f6a4c8918f482f"),
    ("0xc54b4e08c1dcc199fdd35c6b5ab589ffd3428a8d", "0xdac17f958d2ee523a2206206994597c13d831ec7"),
    ("0xcdaea3dde6ce5969aa1414a82a3a681ced51ce72", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xd41830d88dfd08678b0b886e0122193d54b02acc", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xd63070114470f685b75b74d60eec7c1113d33a3d", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xdc94785959b73f7a168452b3654e44fec6a750e4", "0x8236a87084f8b84306f72007f36f2618a5634494"),
    ("0xdd0f28e19c1780eb6396170735d45153d261490d", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
    ("0xe0c98605f279e4d7946d25b75869c69802823763", "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599"),
    ("0xe89371eaaac6d46d4c3ed23453241987916224fc", "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ("0xf9bddd4a9b3a45f980e11fdde96e16364ddbec49", "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"),
];

/// Lowercased-vault -> lowercased-underlying lookup (binary search; map is sorted).
#[must_use]
pub fn underlying_of(vault_lc: &str) -> Option<&'static str> {
    METAMORPHO_UNDERLYING
        .binary_search_by(|(v, _)| (*v).cmp(vault_lc))
        .ok()
        .map(|i| METAMORPHO_UNDERLYING[i].1)
}
