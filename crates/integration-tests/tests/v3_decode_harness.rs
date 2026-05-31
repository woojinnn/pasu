//! Deterministic CI gate for the v3 `ActionBody[]` decode harness.
//!
//! 1. `surface_installs_clean` — every local `registryV2/index` bundle installs
//!    into the WASM v3 state without error.
//! 2. `synthetic_fuzz_single_emit` — fuzzing every `single_emit` callkey with a
//!    fixed seed produces zero hard failures (no panic, no serde-shape break,
//!    no hard engine error).
//!
//! Both install + route on their own test thread (R1: WASM v3 install state is
//! thread-local). Phase 2/3 add `synthetic_fuzz_all_strategies` and
//! `corpus_replay` to this file.

use policy_engine_integration_tests::harness::{self, adapters};

/// Fixed seed for the deterministic sweep (valid hex; arbitrary).
const GLOBAL_SEED: u64 = 0x5C09_EBA1;
/// Iterations per callkey in the gate (kept small for speed; CLI raises it).
const ITERS_PER_CALLKEY: u64 = 16;

#[test]
fn surface_installs_clean() {
    let surface = adapters::load_and_install().expect("load + install local registryV2 index");

    eprintln!(
        "callkeys={} typed_data_keys={} unique_bundles_installed={} install_failures={}",
        surface.total_callkeys,
        surface.total_typed_keys,
        surface.installed_bundle_ids.len(),
        surface.install_failures.len(),
    );
    for (id, err) in &surface.install_failures {
        eprintln!("  INSTALL FAIL {id}: {err}");
    }

    assert!(
        surface.install_failures.is_empty(),
        "{} local bundle(s) failed to install",
        surface.install_failures.len(),
    );
    // Loose staleness guard: the index is committed; an empty/broken index means
    // build-index.ts needs rerunning. (Exact counts churn with the token list.)
    assert!(
        surface.total_callkeys >= 300,
        "index looks stale/empty ({} callkeys) — run `npx tsx registryV2/scripts/build-index.ts`",
        surface.total_callkeys,
    );
    assert!(
        surface.installed_bundle_ids.len() >= 50,
        "too few unique bundles installed ({})",
        surface.installed_bundle_ids.len(),
    );
}

#[test]
fn synthetic_fuzz_single_emit() {
    let report = harness::run_synthetic_single_emit(GLOBAL_SEED, ITERS_PER_CALLKEY)
        .expect("run synthetic single_emit fuzz");

    eprintln!("{}", report.summary());

    assert_eq!(
        report.hard_failures(),
        0,
        "synthetic single_emit fuzz found {} hard failure(s):\n{}",
        report.hard_failures(),
        report.summary(),
    );
}

#[test]
fn corpus_replay() {
    let root = harness::default_corpus_root();
    let outcomes = harness::corpus::run_corpus(&root).expect("run real-tx corpus");

    let mismatches: Vec<_> = outcomes.iter().filter(|o| !o.matched).collect();
    eprintln!(
        "corpus: {}/{} matched",
        outcomes.len() - mismatches.len(),
        outcomes.len()
    );
    for m in &mismatches {
        eprintln!(
            "  MISS [{}] {} expect={} got={}",
            m.source, m.label, m.expect, m.got
        );
    }

    assert!(
        !outcomes.is_empty(),
        "no corpus.json found under {}",
        root.display()
    );
    assert!(
        mismatches.is_empty(),
        "{} corpus entr(ies) did not match their pinned expectation",
        mismatches.len()
    );
}

#[test]
fn synthetic_fuzz_all_strategies() {
    let report = harness::run_synthetic_all(GLOBAL_SEED, ITERS_PER_CALLKEY)
        .expect("run synthetic all-strategy fuzz");

    eprintln!("{}", report.summary());

    assert_eq!(
        report.hard_failures(),
        0,
        "synthetic all-strategy fuzz found {} hard failure(s):\n{}",
        report.hard_failures(),
        report.summary(),
    );
}

/// Recursively find the first `"<field>": "<string>"` entry in a JSON value.
fn find_string_field(v: &serde_json::Value, field: &str) -> Option<String> {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(serde_json::Value::String(s)) = m.get(field) {
                return Some(s.clone());
            }
            m.values().find_map(|x| find_string_field(x, field))
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_string_field(x, field)),
        _ => None,
    }
}

/// Field-level golden (manual oracle layer "A", manual §5c) for the Morpho Blue
/// `market_id`.
///
/// `corpus_replay`'s oracle (`corpus.rs::check_expect`) compares only the
/// verdict + top-level `domain` — it never inspects body field VALUES. So a
/// Morpho `supply` whose `market_id` is wrong (say, naively mapped to a plain
/// `$args.*` field instead of the keccak, or with the Tier B injector removed)
/// would still pass `corpus_replay` as `pass`/`lending` — a SILENT mis-decode.
/// This test is the only thing that pins it: it routes a real mainnet supply tx
/// and asserts the decoded `LendingVenue::MorphoBlue.market_id` equals
/// `keccak256(abi.encode(marketParams))` (= `MarketParamsLib.id`), the value
/// `maybe_inject_morpho_market_id` must produce.
#[test]
fn morpho_supply_market_id_is_keccak_marketparams() {
    // R1: install + route on the same thread.
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real mainnet supply tx 0xf2cdff2b1203…: market (loan=WETH 0xC02a…,
    // collat=0xe1B4…, oracle=0xcb6a…, irm=0x870a…, lltv=91.5%), 2.5157 WETH.
    const TO: &str = "0xbbbbbbbbbb9cc5e90e3b3af64bdaf62c37eeffcb";
    const CALLDATA: &str = "0xa99aad89000000000000000000000000c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2000000000000000000000000e1b4d34e8754600962cd944b535180bd758e6c2e000000000000000000000000cb6a6fdfdb18ec9a004465aef74ff9092fd4f89a000000000000000000000000870ac11d48b15db9a138cf899d20f13f79ba00bc0000000000000000000000000000000000000000000000000cb2bba6f17b800000000000000000000000000000000000000000000000000022e9df45f93190e8000000000000000000000000000000000000000000000000000000000000000000000000000000000000000040534e513df8277870b81e97b5107b3f39de4f1500000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000000";

    // keccak256(abi.encode(marketParams)) = MarketParamsLib.id. Cross-checked
    // independently of the Rust injector via
    //   cast abi-encode "f((address,address,address,address,uint256))" "(…)" | cast keccak
    const EXPECTED_MARKET_ID: &str =
        "0xb7ad412532006bf876534ccae59900ddd9d1d1e394959065cb39b12b22f94ff5";

    let env = harness::route::route_calldata(1, TO, "0xa99aad89", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let market_id =
        find_string_field(&env, "market_id").expect("decoded supply body carries a market_id");
    assert_eq!(
        market_id, EXPECTED_MARKET_ID,
        "Morpho market_id mismatch — Tier B keccak(MarketParams) regressed"
    );
}

/// Recursively find the first `"<field>": <bool>` entry in a JSON value.
fn find_bool_field(v: &serde_json::Value, field: &str) -> Option<bool> {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(serde_json::Value::Bool(b)) = m.get(field) {
                return Some(*b);
            }
            m.values().find_map(|x| find_bool_field(x, field))
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_bool_field(x, field)),
        _ => None,
    }
}

/// Recursively find the first object containing `"<field>": "<expected>"`.
fn find_object_with_string_field<'a>(
    v: &'a serde_json::Value,
    field: &str,
    expected: &str,
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    match v {
        serde_json::Value::Object(m) => {
            if m.get(field).and_then(serde_json::Value::as_str) == Some(expected) {
                return Some(m);
            }
            m.values()
                .find_map(|x| find_object_with_string_field(x, field, expected))
        }
        serde_json::Value::Array(a) => a
            .iter()
            .find_map(|x| find_object_with_string_field(x, field, expected)),
        _ => None,
    }
}

/// Field-level golden for Morpho `setAuthorization` (Tier 3
/// `LendingAction::SetAuthorization`).
///
/// The corpus oracle checks only the verdict + top-level domain — never WHO is
/// being authorized. So a manifest that mis-maps `authorized` (e.g. to the
/// protocol address or a wrong arg) would still pass as `pass`/`lending`. This
/// pins the operator address + grant flag from a real mainnet `setAuthorization`
/// tx — the security-critical fields for a permission-delegation analyzer.
#[test]
fn morpho_set_authorization_decodes_operator_and_flag() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real mainnet setAuthorization tx 0x255f24ea…: grant control to the
    // operator 0x4A6c312e… (newIsAuthorized = true).
    const TO: &str = "0xbbbbbbbbbb9cc5e90e3b3af64bdaf62c37eeffcb";
    const CALLDATA: &str = "0xeecea0000000000000000000000000004a6c312ec70e8747a587ee860a0353cd42be0ae00000000000000000000000000000000000000000000000000000000000000001";

    let env = harness::route::route_calldata(1, TO, "0xeecea000", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let authorized =
        find_string_field(&env, "authorized").expect("set_authorization body carries `authorized`");
    assert_eq!(
        authorized, "0x4a6c312ec70e8747a587ee860a0353cd42be0ae0",
        "operator (authorized) address mis-decoded"
    );
    assert_eq!(
        find_bool_field(&env, "is_authorized"),
        Some(true),
        "grant flag (is_authorized) mis-decoded"
    );
}

/// Field-level golden for Permit2 `invalidateNonces`.
///
/// `invalidateNonces(token,spender,newNonce)` is an ordered nonce revocation for
/// a Permit2 allowance pair. The current ActionBody does not carry the nonce
/// floor, but it must still surface the token+spender permission being revoked.
#[test]
fn permit2_invalidate_nonces_decodes_revoke_scope() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const TO: &str = "0x000000000022d473030f116ddee9f6b43ac78ba3";
    const TOKEN: &str = "2222222222222222222222222222222222222222";
    const SPENDER: &str = "3333333333333333333333333333333333333333";
    const CALLDATA: &str = concat!(
        "0x65d9723c",
        "000000000000000000000000",
        "2222222222222222222222222222222222222222",
        "000000000000000000000000",
        "3333333333333333333333333333333333333333",
        "0000000000000000000000000000000000000000000000000000000000000007"
    );

    let env = harness::route::route_calldata(1, TO, "0x65d9723c", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let scope = find_object_with_string_field(&env, "kind", "permit2_lockdown")
        .expect("revoke_approval carries permit2_lockdown scope");
    let expected_spender = format!("0x{SPENDER}");
    assert_eq!(
        scope.get("spender").and_then(serde_json::Value::as_str),
        Some(expected_spender.as_str()),
        "spender mis-decoded"
    );
    let token = scope
        .get("token")
        .and_then(|v| find_string_field(v, "address"))
        .expect("permit2 revoke scope carries token address");
    assert_eq!(token, format!("0x{TOKEN}"), "token mis-decoded");
}

/// Field-level golden for Permit2 `invalidateUnorderedNonces`.
///
/// Unordered nonce invalidation is bitmap-scoped, not token/spender-scoped.
/// It should surface as a Permit2 nonce revoke scope rather than an AMM order
/// cancel with `wordPos` pretending to be an order hash.
#[test]
fn permit2_invalidate_unordered_nonces_decodes_bitmap_scope() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const TO: &str = "0x000000000022d473030f116ddee9f6b43ac78ba3";
    const CALLDATA: &str = concat!(
        "0x3ff9dcb1",
        "0000000000000000000000000000000000000000000000000000000000000007",
        "000000000000000000000000000000000000000000000000000000000000000a"
    );

    let env = harness::route::route_calldata(1, TO, "0x3ff9dcb1", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let scope = find_object_with_string_field(&env, "kind", "permit2_unordered_nonce")
        .expect("revoke_approval carries permit2_unordered_nonce scope");
    assert_eq!(
        scope.get("chain").and_then(serde_json::Value::as_str),
        Some("eip155:1"),
        "chain mis-decoded"
    );
    assert_eq!(
        scope.get("word_pos").and_then(serde_json::Value::as_str),
        Some("0x7"),
        "word_pos mis-decoded"
    );
    assert_eq!(
        scope.get("mask").and_then(serde_json::Value::as_str),
        Some("0xa"),
        "mask mis-decoded"
    );
}

/// Field-level golden for Aave V3 Gateway `withdrawETHWithPermit`.
///
/// Current verified Gateway deployments ignore the legacy calldata `pool` arg
/// and call immutable POOL. Existing Gateway manifests use `$resolved.pool`, so
/// this pins both the new selector and the route-context injection that keeps
/// the venue/live-input target equal to the canonical Pool instead of trusting
/// calldata or falling back to zero.
#[test]
fn aave_v3_withdraw_eth_with_permit_decodes_pool_and_recipient() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const GATEWAY: &str = "0xd01607c3c5ecaba394d8be377a08590149325722";
    const EXPECTED_POOL: &str = "87870bca3f3fd6335c3f4ce8392d69350b4fa4e2";
    const RECIPIENT: &str = "1111111111111111111111111111111111111111";
    const CALLDATA: &str = concat!(
        "0xd4c40b6c",
        "000000000000000000000000",
        "2222222222222222222222222222222222222222",
        "0000000000000000000000000000000000000000000000000000000000000005",
        "000000000000000000000000",
        "1111111111111111111111111111111111111111",
        "0000000000000000000000000000000000000000000000000000000000000007",
        "000000000000000000000000000000000000000000000000000000000000001b",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000000"
    );

    let env = harness::route::route_calldata(1, GATEWAY, "0xd4c40b6c", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );

    let venue =
        find_object_with_string_field(&env, "name", "aave_v3").expect("Aave venue is present");
    assert_eq!(
        venue.get("pool").and_then(serde_json::Value::as_str),
        Some(format!("0x{EXPECTED_POOL}").as_str()),
        "Gateway pool resolver did not use the canonical immutable Pool"
    );
    assert_eq!(
        find_string_field(&env, "recipient"),
        Some(format!("0x{RECIPIENT}")),
        "withdraw recipient mis-decoded"
    );
}

/// Field-level golden for non-mainnet Aave V3 Gateway static resolution.
///
/// Base uses a different Pool and wrapped-native address from mainnet. This
/// pins that the route context injects both values locally and still ignores
/// the legacy calldata `pool` argument.
#[test]
fn aave_v3_base_deposit_eth_decodes_gateway_pool_and_weth() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const GATEWAY: &str = "0xa0d9c1e9e48ca30c8d8c3b5d69ff5dc1f6dffc24";
    const EXPECTED_POOL: &str = "0xa238dd80c259a72e81d7e4664a9801593f98d1c5";
    const EXPECTED_WETH: &str = "0x4200000000000000000000000000000000000006";
    const ON_BEHALF_OF: &str = "0x1111111111111111111111111111111111111111";
    const CALLDATA: &str = concat!(
        "0x474cf53d",
        "000000000000000000000000",
        "2222222222222222222222222222222222222222",
        "000000000000000000000000",
        "1111111111111111111111111111111111111111",
        "0000000000000000000000000000000000000000000000000000000000000000"
    );

    let env = harness::route::route_calldata(8453, GATEWAY, "0x474cf53d", CALLDATA, "1");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );

    let venue =
        find_object_with_string_field(&env, "name", "aave_v3").expect("Aave venue is present");
    assert_eq!(
        venue.get("pool").and_then(serde_json::Value::as_str),
        Some(EXPECTED_POOL),
        "Base Gateway pool resolver did not use the canonical immutable Pool"
    );
    assert_eq!(
        find_string_field(&env, "on_behalf_of"),
        Some(ON_BEHALF_OF.to_owned()),
        "depositETH on_behalf_of mis-decoded"
    );

    let asset =
        find_object_with_string_field(&env, "standard", "erc20").expect("supply asset present");
    assert_eq!(
        asset.get("address").and_then(serde_json::Value::as_str),
        Some(EXPECTED_WETH),
        "Base Gateway should decode wrapped-native as Base WETH"
    );
}

/// Field-level golden for Aave V3.4+ `approvePositionManager` and
/// `renouncePositionManagerRole`.
///
/// These are permission primitives, not ordinary lending balance changes.
/// The surface gate keeps them COVER; this test pins the security-critical
/// authorized manager, grant/revoke flag, and explicit authorizer where present.
#[test]
fn aave_v3_position_manager_permission_decodes_manager_and_flag() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const POOL: &str = "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2";
    const MANAGER: &str = "1111111111111111111111111111111111111111";
    const USER: &str = "2222222222222222222222222222222222222222";
    const FUZZ_SUBMITTER: &str = "0x000000000000000000000000000000000000aaaa";
    const APPROVE_CALLDATA: &str = concat!(
        "0xb8caa7c5",
        "000000000000000000000000",
        "1111111111111111111111111111111111111111",
        "0000000000000000000000000000000000000000000000000000000000000001"
    );
    const RENOUNCE_CALLDATA: &str = concat!(
        "0xfea149a6",
        "000000000000000000000000",
        "2222222222222222222222222222222222222222"
    );

    let approve = harness::route::route_calldata(1, POOL, "0xb8caa7c5", APPROVE_CALLDATA, "0");
    assert_eq!(
        approve.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "approvePositionManager route did not succeed: {approve}"
    );
    assert_eq!(
        find_string_field(&approve, "authorized"),
        Some(format!("0x{MANAGER}"))
    );
    assert_eq!(find_bool_field(&approve, "is_authorized"), Some(true));

    let renounce = harness::route::route_calldata(1, POOL, "0xfea149a6", RENOUNCE_CALLDATA, "0");
    assert_eq!(
        renounce.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "renouncePositionManagerRole route did not succeed: {renounce}"
    );
    assert_eq!(
        find_string_field(&renounce, "authorizer"),
        Some(format!("0x{USER}"))
    );
    assert_eq!(
        find_string_field(&renounce, "authorized"),
        Some(FUZZ_SUBMITTER.into())
    );
    assert_eq!(find_bool_field(&renounce, "is_authorized"), Some(false));
}

/// Field-level golden for Aave V3 variable debt `renounceDelegation`.
///
/// `renounceDelegation(delegator)` is a credit-delegation revoke path: the
/// caller is the delegatee and the resulting borrow allowance becomes zero.
/// This pins the security-relevant revoke shape instead of treating the debt
/// token as a disabled ERC20 transfer/approval surface.
#[test]
fn aave_v3_variable_debt_renounce_delegation_decodes_zero_allowance() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const VARIABLE_DEBT_USDC: &str = "0x72e95b8931767c79ba4eee721354d6e99a61d004";
    const FUZZ_SUBMITTER: &str = "0x000000000000000000000000000000000000aaaa";
    const CALLDATA: &str =
        "0x91fb372d000000000000000000000000000000000000000000000000000000000000b0b0";

    let env = harness::route::route_calldata(1, VARIABLE_DEBT_USDC, "0x91fb372d", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "renounceDelegation route did not succeed: {env}"
    );
    assert_eq!(
        find_string_field(&env, "delegatee"),
        Some(FUZZ_SUBMITTER.into()),
        "delegatee should be the caller revoking delegated borrow allowance"
    );
    assert_eq!(
        find_string_field(&env, "amount"),
        Some("0x0".into()),
        "renounceDelegation should decode as a zero borrow allowance"
    );
}

/// Field-level golden for Aave V3 variable-debt market expansion.
///
/// The extra debt-token manifests are per-market because the delegated borrow
/// asset changes with the debt token. This pins one non-USDC market so a
/// copy/paste mistake cannot silently keep decoding every delegation as USDC.
#[test]
fn aave_v3_variable_debt_dai_approve_delegation_decodes_dai_asset() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const VARIABLE_DEBT_DAI: &str = "0xcf8d0c70c850859266f5c338b38f9d663181c314";
    const DAI: &str = "0x6b175474e89094c44da98b954eedeac495271d0f";
    const DELEGATEE: &str = "0x000000000000000000000000000000000000d1e9";
    const CALLDATA: &str = concat!(
        "0xc04a8a10",
        "000000000000000000000000000000000000000000000000000000000000d1e9",
        "00000000000000000000000000000000000000000000000000000000002625a0"
    );

    let env = harness::route::route_calldata(1, VARIABLE_DEBT_DAI, "0xc04a8a10", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "approveDelegation route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "delegatee"), Some(DELEGATEE.into()));
    assert_eq!(find_string_field(&env, "amount"), Some("0x2625a0".into()));

    let asset = find_object_with_string_field(&env, "standard", "erc20")
        .expect("delegate_borrow asset token is present");
    assert_eq!(
        asset.get("address").and_then(serde_json::Value::as_str),
        Some(DAI),
        "variableDebtDAI should decode the delegated borrow asset as DAI"
    );
}

/// Field-level golden for Aave V3.4+ position-manager execution paths.
///
/// `setUserEModeOnBehalfOf` and
/// `setUserUseReserveAsCollateralOnBehalfOf` are the same user-position actions
/// as their direct variants, but the affected user is explicit. The optional
/// `on_behalf_of` field prevents the decoder from silently attributing the
/// manager's action to the manager's own account.
#[test]
fn aave_v3_on_behalf_position_actions_decode_target_account() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const POOL: &str = "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2";
    const USER: &str = "2222222222222222222222222222222222222222";
    const SET_EMODE_CALLDATA: &str = concat!(
        "0x4ba06814",
        "0000000000000000000000000000000000000000000000000000000000000001",
        "000000000000000000000000",
        "2222222222222222222222222222222222222222"
    );
    const DISABLE_COLLATERAL_CALLDATA: &str = concat!(
        "0x972b35fa",
        "000000000000000000000000",
        "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "000000000000000000000000",
        "2222222222222222222222222222222222222222"
    );

    let set_emode = harness::route::route_calldata(1, POOL, "0x4ba06814", SET_EMODE_CALLDATA, "0");
    assert_eq!(
        set_emode.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "setUserEModeOnBehalfOf route did not succeed: {set_emode}"
    );
    assert_eq!(
        find_string_field(&set_emode, "on_behalf_of"),
        Some(format!("0x{USER}"))
    );

    let disable_collateral =
        harness::route::route_calldata(1, POOL, "0x972b35fa", DISABLE_COLLATERAL_CALLDATA, "0");
    assert_eq!(
        disable_collateral
            .get("ok")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "setUserUseReserveAsCollateralOnBehalfOf route did not succeed: {disable_collateral}"
    );
    let body = find_object_with_string_field(&disable_collateral, "action", "disable_collateral")
        .expect("collateral off action is decoded");
    assert!(
        body.get("asset").is_some(),
        "expected flattened disable_collateral body: {disable_collateral}"
    );
    assert_eq!(
        find_string_field(&disable_collateral, "on_behalf_of"),
        Some(format!("0x{USER}"))
    );
}

/// Field-level golden for Compound V3 Comet `allow`.
///
/// Comet `allow(manager,isAllowed)` is account-wide manager authorization, so it
/// must decode to `LendingAction::SetAuthorization`, not a generic ERC20
/// approval. Pinning the manager + grant flag prevents the red-flag permission
/// primitive from silently becoming an opaque or token-only action.
#[test]
fn compound_v3_allow_decodes_manager_and_flag() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const TO: &str = "0xc3d688b66703497daa19211eedff47f25384cdc3";
    const MANAGER: &str = "1111111111111111111111111111111111111111";
    const CALLDATA: &str = concat!(
        "0x110496e5",
        "000000000000000000000000",
        "1111111111111111111111111111111111111111",
        "0000000000000000000000000000000000000000000000000000000000000001"
    );

    let env = harness::route::route_calldata(1, TO, "0x110496e5", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let authorized =
        find_string_field(&env, "authorized").expect("set_authorization carries authorized");
    assert_eq!(authorized, format!("0x{MANAGER}"));
    assert_eq!(find_bool_field(&env, "is_authorized"), Some(true));
}

/// Field-level golden for Compound V3 Comet `allowBySig`.
///
/// The relayed calldata contains both the signatory (`owner`) and manager. The
/// ActionBody extension keeps `authorizer` optional so direct calls can omit it,
/// but signature relay paths must preserve it for policy display/evaluation.
#[test]
fn compound_v3_allow_by_sig_decodes_authorizer_manager_and_flag() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const TO: &str = "0xc3d688b66703497daa19211eedff47f25384cdc3";
    const OWNER: &str = "2222222222222222222222222222222222222222";
    const MANAGER: &str = "3333333333333333333333333333333333333333";
    const CALLDATA: &str = concat!(
        "0xbb24d994",
        "000000000000000000000000",
        "2222222222222222222222222222222222222222",
        "000000000000000000000000",
        "3333333333333333333333333333333333333333",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000007",
        "00000000000000000000000000000000000000000000000000000002540be3ff",
        "000000000000000000000000000000000000000000000000000000000000001b",
        "0000000000000000000000000000000000000000000000000000000000000000",
        "0000000000000000000000000000000000000000000000000000000000000000"
    );

    let env = harness::route::route_calldata(1, TO, "0xbb24d994", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    assert_eq!(
        find_string_field(&env, "authorizer"),
        Some(format!("0x{OWNER}"))
    );
    assert_eq!(
        find_string_field(&env, "authorized"),
        Some(format!("0x{MANAGER}"))
    );
    assert_eq!(find_bool_field(&env, "is_authorized"), Some(false));
}

/// Field-level golden for Compound V3 Comet off-chain `Authorization`.
#[test]
fn compound_v3_authorization_typed_data_decodes_permission_fields() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const TO: &str = "0xc3d688b66703497daa19211eedff47f25384cdc3";
    const OWNER: &str = "0x4444444444444444444444444444444444444444";
    const MANAGER: &str = "0x5555555555555555555555555555555555555555";
    let message = serde_json::json!({
        "owner": OWNER,
        "manager": MANAGER,
        "isAllowed": true,
        "nonce": "9",
        "expiry": "9999999999"
    });

    let env = harness::route::route_typed_data(
        1,
        TO,
        "Authorization",
        None,
        Some("Compound USDC"),
        &message,
    );
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "typed-data route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "authorizer"), Some(OWNER.into()));
    assert_eq!(find_string_field(&env, "authorized"), Some(MANAGER.into()));
    assert_eq!(find_bool_field(&env, "is_authorized"), Some(true));
}

/// Field-level golden for Compound V3 cWETH market expansion.
///
/// cUSDC and cWETH share the Comet user-facing ABI, but the policy context must
/// not keep the cUSDC base asset when routing cWETH. This pins the static
/// `$resolved.compound_v3_base_asset` injection for the cWETH Comet.
#[test]
fn compound_v3_cweth_supply_decodes_weth_base_asset() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const COMET_WETH: &str = "0xa17581a9e3356d9a858b789d68b4d866e593ae94";
    const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
    const CALLDATA: &str = concat!(
        "0xf2b9fdb8",
        "000000000000000000000000",
        "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        "00000000000000000000000000000000000000000000000000000000000003e8"
    );

    let env = harness::route::route_calldata(1, COMET_WETH, "0xf2b9fdb8", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let venue = find_object_with_string_field(&env, "name", "compound_v3")
        .expect("Compound V3 venue is present");
    assert_eq!(
        venue.get("comet").and_then(serde_json::Value::as_str),
        Some(COMET_WETH)
    );
    assert_eq!(
        venue
            .get("base_asset")
            .and_then(|v| v.pointer("/key/address"))
            .and_then(serde_json::Value::as_str),
        Some(WETH)
    );
}

/// Field-level golden for Compound V3 cWETH off-chain `Authorization`.
#[test]
fn compound_v3_cweth_authorization_typed_data_decodes_permission_fields() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const TO: &str = "0xa17581a9e3356d9a858b789d68b4d866e593ae94";
    const OWNER: &str = "0x4444444444444444444444444444444444444444";
    const MANAGER: &str = "0x5555555555555555555555555555555555555555";
    let message = serde_json::json!({
        "owner": OWNER,
        "manager": MANAGER,
        "isAllowed": true,
        "nonce": "9",
        "expiry": "9999999999"
    });

    let env = harness::route::route_typed_data(
        1,
        TO,
        "Authorization",
        None,
        Some("Compound WETH"),
        &message,
    );
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "typed-data route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "authorizer"), Some(OWNER.into()));
    assert_eq!(find_string_field(&env, "authorized"), Some(MANAGER.into()));
    assert_eq!(find_bool_field(&env, "is_authorized"), Some(true));
}

/// Field-level golden for Compound V3 Base market expansion.
///
/// Base has its own cUSDCv3 Comet address and native USDC base token. This pins
/// the chain-aware `$resolved.compound_v3_base_asset` mapping for L2 markets.
#[test]
fn compound_v3_base_usdc_supply_decodes_base_usdc_asset() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const BASE_COMET_USDC: &str = "0xb125e6687d4313864e53df431d5425969c15eb2f";
    const BASE_USDC: &str = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913";
    const CALLDATA: &str = concat!(
        "0xf2b9fdb8",
        "000000000000000000000000",
        "833589fcd6edb6e08f4c7c32d4f71b54bda02913",
        "00000000000000000000000000000000000000000000000000000000000003e8"
    );

    let env = harness::route::route_calldata(8453, BASE_COMET_USDC, "0xf2b9fdb8", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "Base cUSDCv3 supply route did not succeed: {env}"
    );
    let venue = find_object_with_string_field(&env, "name", "compound_v3")
        .expect("Compound V3 venue is present");
    assert_eq!(
        venue.get("comet").and_then(serde_json::Value::as_str),
        Some(BASE_COMET_USDC)
    );
    assert_eq!(
        venue
            .get("base_asset")
            .and_then(|v| v.pointer("/key/address"))
            .and_then(serde_json::Value::as_str),
        Some(BASE_USDC)
    );
}

/// Field-level golden for Compound V3 Base off-chain `Authorization`.
#[test]
fn compound_v3_base_aero_authorization_typed_data_decodes_permission_fields() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const TO: &str = "0x784efeb622244d2348d4f2522f8860b96fbece89";
    const OWNER: &str = "0x4444444444444444444444444444444444444444";
    const MANAGER: &str = "0x5555555555555555555555555555555555555555";
    let message = serde_json::json!({
        "owner": OWNER,
        "manager": MANAGER,
        "isAllowed": false,
        "nonce": "9",
        "expiry": "9999999999"
    });

    let env = harness::route::route_typed_data(
        8453,
        TO,
        "Authorization",
        None,
        Some("Compound AERO"),
        &message,
    );
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "Base cAEROv3 typed-data route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "authorizer"), Some(OWNER.into()));
    assert_eq!(find_string_field(&env, "authorized"), Some(MANAGER.into()));
    assert_eq!(find_bool_field(&env, "is_authorized"), Some(false));
}

/// Field-level golden for Compound V3 Optimism market expansion.
#[test]
fn compound_v3_optimism_usdt_supply_decodes_optimism_usdt_asset() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const OP_COMET_USDT: &str = "0x995e394b8b2437ac8ce61ee0bc610d617962b214";
    const OP_USDT: &str = "0x94b008aa00579c1307b0ef2c499ad98a8ce58e58";
    const CALLDATA: &str = concat!(
        "0xf2b9fdb8",
        "000000000000000000000000",
        "94b008aa00579c1307b0ef2c499ad98a8ce58e58",
        "00000000000000000000000000000000000000000000000000000000000003e8"
    );

    let env = harness::route::route_calldata(10, OP_COMET_USDT, "0xf2b9fdb8", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "Optimism cUSDTv3 supply route did not succeed: {env}"
    );
    let venue = find_object_with_string_field(&env, "name", "compound_v3")
        .expect("Compound V3 venue is present");
    assert_eq!(
        venue.get("comet").and_then(serde_json::Value::as_str),
        Some(OP_COMET_USDT)
    );
    assert_eq!(
        venue
            .get("base_asset")
            .and_then(|v| v.pointer("/key/address"))
            .and_then(serde_json::Value::as_str),
        Some(OP_USDT)
    );
}

/// Field-level golden for Compound V3 Arbitrum off-chain `Authorization`.
#[test]
fn compound_v3_arbitrum_weth_authorization_typed_data_decodes_permission_fields() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const TO: &str = "0x6f7d514bbd4aff3bcd1140b7344b32f063dee486";
    const OWNER: &str = "0x4444444444444444444444444444444444444444";
    const MANAGER: &str = "0x5555555555555555555555555555555555555555";
    let message = serde_json::json!({
        "owner": OWNER,
        "manager": MANAGER,
        "isAllowed": false,
        "nonce": "9",
        "expiry": "9999999999"
    });

    let env = harness::route::route_typed_data(
        42161,
        TO,
        "Authorization",
        None,
        Some("Compound WETH"),
        &message,
    );
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "Arbitrum cWETHv3 typed-data route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "authorizer"), Some(OWNER.into()));
    assert_eq!(find_string_field(&env, "authorized"), Some(MANAGER.into()));
    assert_eq!(find_bool_field(&env, "is_authorized"), Some(false));
}

/// Field-level golden for the remaining Compound V3 mainnet market expansion.
#[test]
fn compound_v3_mainnet_usds_supply_decodes_usds_asset() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const MAINNET_COMET_USDS: &str = "0x5d409e56d886231adaf00c8775665ad0f9897b56";
    const MAINNET_USDS: &str = "0xdc035d45d973e3ec169d2276ddab16f1e407384f";
    const CALLDATA: &str = concat!(
        "0xf2b9fdb8",
        "000000000000000000000000",
        "dc035d45d973e3ec169d2276ddab16f1e407384f",
        "00000000000000000000000000000000000000000000000000000000000003e8"
    );

    let env = harness::route::route_calldata(1, MAINNET_COMET_USDS, "0xf2b9fdb8", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "mainnet cUSDSv3 supply route did not succeed: {env}"
    );
    let venue = find_object_with_string_field(&env, "name", "compound_v3")
        .expect("Compound V3 venue is present");
    assert_eq!(
        venue.get("comet").and_then(serde_json::Value::as_str),
        Some(MAINNET_COMET_USDS)
    );
    assert_eq!(
        venue
            .get("base_asset")
            .and_then(|v| v.pointer("/key/address"))
            .and_then(serde_json::Value::as_str),
        Some(MAINNET_USDS)
    );
}

/// Field-level golden for the remaining Compound V3 alt-chain typed-data
/// expansion.
#[test]
fn compound_v3_unichain_weth_authorization_typed_data_decodes_permission_fields() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const TO: &str = "0x6c987dde50db1dcdd32cd4175778c2a291978e2a";
    const OWNER: &str = "0x4444444444444444444444444444444444444444";
    const MANAGER: &str = "0x5555555555555555555555555555555555555555";
    let message = serde_json::json!({
        "owner": OWNER,
        "manager": MANAGER,
        "isAllowed": false,
        "nonce": "9",
        "expiry": "9999999999"
    });

    let env = harness::route::route_typed_data(
        130,
        TO,
        "Authorization",
        None,
        Some("Compound WETH"),
        &message,
    );
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "Unichain cWETHv3 typed-data route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "authorizer"), Some(OWNER.into()));
    assert_eq!(find_string_field(&env, "authorized"), Some(MANAGER.into()));
    assert_eq!(find_bool_field(&env, "is_authorized"), Some(false));
}
