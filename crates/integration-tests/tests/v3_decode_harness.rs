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

/// Recursively find the first value stored under `key` (returns the sub-tree,
/// not a leaf string — used to scope a later `find_string_field` to one branch).
fn find_object_by_key<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    match v {
        serde_json::Value::Object(m) => {
            if let Some(found) = m.get(key) {
                return Some(found);
            }
            m.values().find_map(|x| find_object_by_key(x, key))
        }
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_object_by_key(x, key)),
        _ => None,
    }
}

/// Field-level golden for Curve StableSwap-NG `exchange` (G3: coin-index → token
/// resolution via the value-map).
///
/// Curve's `exchange` calldata carries int128 pool-coin INDICES (`i`, `j`), NOT
/// token addresses. The manifest resolves them with `$match $args.i / $cases`,
/// where the per-pool baked coin list maps index → coin address. The corpus
/// oracle checks only verdict + top-level domain, so a coin mapped to the wrong
/// index (or a value-map that fails to resolve) would still pass as `pass`/`amm`
/// — a SILENT mis-decode of "which token am I selling/buying". This pins it:
/// on the 2btc pool, `i=0` must resolve `token_in` to coin0 (WBTC) and `j=1`
/// must resolve `token_out` to coin1 (tBTC).
#[test]
fn curve_stableswap_ng_exchange_resolves_coin_index_to_token() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // 2btc pool (coins: WBTC=coin0, tBTC=coin1). Synthetic
    // exchange(i=0, j=1, _dx=1e8, _min_dy=99e6) — sell WBTC for tBTC.
    const TO: &str = "0xb7ecb2aa52aa64a717180e030241bc75cd946726";
    const WBTC: &str = "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599";
    const TBTC: &str = "0x18084fba666a33d37592fa2633fd49a74dd93a88";
    const CALLDATA: &str = "0x3df02124000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000005f5e1000000000000000000000000000000000000000000000000000000000005e69ec0";

    let env = harness::route::route_calldata(1, TO, "0x3df02124", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    // Scope the address lookup to each branch so token_in vs token_out can't be
    // confused regardless of map ordering.
    let token_in = find_object_by_key(&env, "token_in").expect("swap body carries token_in");
    let token_out = find_object_by_key(&env, "token_out").expect("swap body carries token_out");
    assert_eq!(
        find_string_field(token_in, "address").as_deref(),
        Some(WBTC),
        "i=0 must resolve token_in to coin0 (WBTC) via the value-map; got {token_in}"
    );
    assert_eq!(
        find_string_field(token_out, "address").as_deref(),
        Some(TBTC),
        "j=1 must resolve token_out to coin1 (tBTC) via the value-map; got {token_out}"
    );
}

/// Field-level golden for Curve StableSwap-NG `remove_liquidity_one_coin` — the
/// coin-index value-map in the `PooledBurnOneCoin` (Tier 3) context. `i=1` on the
/// 2btc pool must resolve `token_out` to coin1 (tBTC), and `lp_token` is the pool
/// itself (NG pool == LP token). A mis-mapped index would silently withdraw the
/// wrong coin.
#[test]
fn curve_stableswap_ng_one_coin_resolves_coin_index_to_token() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // 2btc (coins: WBTC=0, tBTC=1). remove_liquidity_one_coin(_burn=1e18, i=1, _min=5e7).
    const TO: &str = "0xb7ecb2aa52aa64a717180e030241bc75cd946726";
    const TBTC: &str = "0x18084fba666a33d37592fa2633fd49a74dd93a88";
    const CALLDATA: &str = "0x1a4d01d20000000000000000000000000000000000000000000000000de0b6b3a764000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000002faf080";

    let env = harness::route::route_calldata(1, TO, "0x1a4d01d2", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let token_out = find_object_by_key(&env, "token_out").expect("one_coin body carries token_out");
    assert_eq!(
        find_string_field(token_out, "address").as_deref(),
        Some(TBTC),
        "i=1 must resolve token_out to coin1 (tBTC) via the value-map; got {token_out}"
    );
    let lp_token = find_object_by_key(&env, "lp_token").expect("one_coin body carries lp_token");
    assert_eq!(
        find_string_field(lp_token, "address").as_deref(),
        Some(TO),
        "NG pool == LP token: lp_token address must be the pool ($to); got {lp_token}"
    );
}

/// Field-level golden for Curve StableSwap-NG `add_liquidity` — positional baking.
/// Curve's `add_liquidity(uint256[2] _amounts, ...)` carries only amounts; the
/// coins are IMPLICIT (the pool's coin list). The manifest bakes `coins[k]` paired
/// with `$args._amounts[k]` positionally. On 2btc, `tokens[0]` must be coin0
/// (WBTC) and `tokens[1]` coin1 (tBTC) — a swapped order would mis-attribute the
/// deposited amounts.
#[test]
fn curve_stableswap_ng_add_liquidity_bakes_pool_coins_in_order() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // 2btc. add_liquidity(_amounts=[1e8, 2e18], _min_mint_amount=1e8). uint256[2] is
    // fixed-size → encoded inline (no offset).
    const TO: &str = "0xb7ecb2aa52aa64a717180e030241bc75cd946726";
    const WBTC: &str = "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599";
    const TBTC: &str = "0x18084fba666a33d37592fa2633fd49a74dd93a88";
    const CALLDATA: &str = "0x0b4c7e4d0000000000000000000000000000000000000000000000000000000005f5e1000000000000000000000000000000000000000000000000001bc16d674ec800000000000000000000000000000000000000000000000000000000000005f5e100";

    let env = harness::route::route_calldata(1, TO, "0x0b4c7e4d", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let tokens = find_object_by_key(&env, "tokens").expect("add_liquidity body carries tokens");
    let arr = tokens
        .as_array()
        .expect("tokens is an array of [TokenRef, amount]");
    assert_eq!(
        arr.len(),
        2,
        "2-coin pool must bake exactly 2 tokens; got {tokens}"
    );
    assert_eq!(
        find_string_field(&arr[0], "address").as_deref(),
        Some(WBTC),
        "tokens[0] must be coin0 (WBTC); got {}",
        arr[0]
    );
    assert_eq!(
        find_string_field(&arr[1], "address").as_deref(),
        Some(TBTC),
        "tokens[1] must be coin1 (tBTC); got {}",
        arr[1]
    );
}

/// Field-level golden for Curve Router NG `exchange` — the `$fn` core extension.
///
/// router-ng cannot be expressed by a `$match` value-map: `token_out` is the
/// LAST non-zero entry of the variable-hop `_route` array (selected per-hop by
/// `swap_type`), and `route_hash` is a keccak of the route. This pins BOTH new
/// WhitelistedFns — `$fn curve_route_last_token` and `$fn route_hash` — against
/// a REAL mainnet exchange (1-hop CRV→USDC, `swap_params` hop0 `swap_type=1` →
/// coin-producing → `token_out = route[2] = USDC`), plus `recipient =
/// $args._receiver`. `corpus_replay` only compares verdict+domain, so a broken
/// `$fn` (wrong `token_out`) would still pass there — this is the only guard.
#[test]
fn curve_router_ng_exchange_resolves_last_token_via_fn() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real mainnet tx 0x1a464128… on CurveRouter v1.2 (exchange + _receiver):
    //   route[0] = 0xaf5191b0… (in), route[1] = pool 0x3211c6cb…,
    //   route[2] = USDC 0xa0b86991… (out), route[3..] = 0; hop0 swap_type = 1.
    const TO: &str = "0x45312ea0eff7e09c83cbe249fa1d7598c4c8cd4e";
    const TOKEN_IN: &str = "0xaf5191b0de278c7286d6c7cc6ab6bb8a73ba2cd6";
    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const RECEIVER: &str = "0x8b65363a01510490fba03cea97b14c73b7ee8f75";
    const CALLDATA: &str = "0xc872a3c5000000000000000000000000af5191b0de278c7286d6c7cc6ab6bb8a73ba2cd60000000000000000000000003211c6cbef1429da3d0d58494938299c92ad5860000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb4800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000001400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010f0cf064dd592000000000000000000000000000000000000000000000000000000000000048d266430000000000000000000000003211c6cbef1429da3d0d58494938299c92ad586000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008b65363a01510490fba03cea97b14c73b7ee8f75";

    let env = harness::route::route_calldata(1, TO, "0xc872a3c5", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );

    let venue = find_object_by_key(&env, "venue").expect("swap body carries venue");
    assert_eq!(
        find_string_field(venue, "name").as_deref(),
        Some("aggregator_route"),
        "venue must be aggregator_route; got {venue}"
    );
    assert_eq!(
        find_string_field(venue, "router").as_deref(),
        Some(TO),
        "router must be tx.to; got {venue}"
    );
    let route_hash = find_string_field(venue, "route_hash").expect("venue carries route_hash");
    assert!(
        route_hash.starts_with("0x") && route_hash.len() == 66,
        "$fn route_hash must be 32-byte hex; got {route_hash}"
    );

    let token_in = find_object_by_key(&env, "token_in").expect("swap body carries token_in");
    let token_out = find_object_by_key(&env, "token_out").expect("swap body carries token_out");
    assert_eq!(
        find_string_field(token_in, "address").as_deref(),
        Some(TOKEN_IN),
        "token_in must be $args._route[0]; got {token_in}"
    );
    assert_eq!(
        find_string_field(token_out, "address").as_deref(),
        Some(USDC),
        "$fn curve_route_last_token must resolve token_out to route[2]=USDC (swap_type 1); got {token_out}"
    );
    assert_eq!(
        find_string_field(&env, "recipient").as_deref(),
        Some(RECEIVER),
        "recipient must resolve to $args._receiver"
    );
}

/// Field-level golden for Curve CryptoSwap-NG (V2) `exchange` — the coin-index
/// value-map over `uint256` indices (cryptoswap uses `uint256` i/j, NOT the
/// `int128` of stableswap-ng). `uint256` renders as a decimal STRING
/// (`args_json::uint_to_json`, >64 bits → string), so the `$cases` keys "0".."2"
/// match — this pins that the uint256 path resolves like the int128 one. On the
/// 3-coin crvUSDT/WBTC/WETH pool, `i=1` → WBTC, `j=2` → WETH.
#[test]
fn curve_cryptoswap_exchange_resolves_uint256_coin_index() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // crvusdtwbtcweth (coins: USDT=0, WBTC=1, WETH=2). exchange(i=1, j=2, dx=1e8, min_dy=1).
    const TO: &str = "0xf5f5b97624542d72a9e06f04804bf81baa15e2b4";
    const WBTC: &str = "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599";
    const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
    const CALLDATA: &str = "0x5b41b908000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000005f5e1000000000000000000000000000000000000000000000000000000000000000001";

    let env = harness::route::route_calldata(1, TO, "0x5b41b908", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let token_in = find_object_by_key(&env, "token_in").expect("swap body carries token_in");
    let token_out = find_object_by_key(&env, "token_out").expect("swap body carries token_out");
    assert_eq!(
        find_string_field(token_in, "address").as_deref(),
        Some(WBTC),
        "i=1 (uint256) must resolve token_in to coin1 (WBTC); got {token_in}"
    );
    assert_eq!(
        find_string_field(token_out, "address").as_deref(),
        Some(WETH),
        "j=2 (uint256) must resolve token_out to coin2 (WETH); got {token_out}"
    );
}

/// Field-level golden for Curve Twocrypto-NG (`CurveTwocryptoOptimized`, 2-coin)
/// `exchange` — the coin-index value-map reused on a twocrypto pool (`AmmVenue::
/// CurveV2`, 0 core code; same `uint256` i/j as cryptoswap). Pinned against a REAL
/// mainnet tx (0x08d406f5…) on the crvUSD/cbBTC pool (Curve Twocrypto-NG factory
/// 0x98ee851a, deployed/used by Yield Basis): `i=0` → crvUSD (coin0), `j=1` →
/// cbBTC (coin1). `corpus_replay` checks only verdict+domain, so a swapped coin
/// map would silently mis-decode "which token am I selling" — this pins it.
#[test]
fn curve_twocrypto_exchange_resolves_coin_index_to_token() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real tx 0x08d406f5… exchange(i=0, j=1, dx=3.6168e18, min_dy=0) — sell crvUSD for cbBTC.
    const TO: &str = "0x862cb4e988fb66e72f128d1183829f8c05b6c6a0";
    const CRVUSD: &str = "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e";
    const CBBTC: &str = "0xcbb7c0000ab88b473b1f5afd9ef808440eed33bf";
    const CALLDATA: &str = "0x5b41b908000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000003231ee7c5f69365a0000000000000000000000000000000000000000000000000000000000000000";

    let env = harness::route::route_calldata(1, TO, "0x5b41b908", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let token_in = find_object_by_key(&env, "token_in").expect("swap body carries token_in");
    let token_out = find_object_by_key(&env, "token_out").expect("swap body carries token_out");
    assert_eq!(
        find_string_field(token_in, "address").as_deref(),
        Some(CRVUSD),
        "i=0 must resolve token_in to coin0 (crvUSD); got {token_in}"
    );
    assert_eq!(
        find_string_field(token_out, "address").as_deref(),
        Some(CBBTC),
        "j=1 must resolve token_out to coin1 (cbBTC); got {token_out}"
    );
}

/// Field-level golden for Curve CryptoSwap-NG `add_liquidity` — 3-coin positional
/// baking. `add_liquidity(uint256[3] amounts, ...)` carries only amounts; coins
/// are implicit. On crvUSDT/WBTC/WETH, `tokens[0..2]` must bake to USDT/WBTC/WETH
/// in coin order.
#[test]
fn curve_cryptoswap_add_liquidity_bakes_three_coins_in_order() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // crvusdtwbtcweth. add_liquidity(amounts=[1e6, 2e6, 3e6], min_mint_amount=1).
    const TO: &str = "0xf5f5b97624542d72a9e06f04804bf81baa15e2b4";
    const USDT: &str = "0xdac17f958d2ee523a2206206994597c13d831ec7";
    const WBTC: &str = "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599";
    const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
    const CALLDATA: &str = "0x4515cef300000000000000000000000000000000000000000000000000000000000f424000000000000000000000000000000000000000000000000000000000001e848000000000000000000000000000000000000000000000000000000000002dc6c00000000000000000000000000000000000000000000000000000000000000001";

    let env = harness::route::route_calldata(1, TO, "0x4515cef3", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let tokens = find_object_by_key(&env, "tokens").expect("add_liquidity body carries tokens");
    let arr = tokens.as_array().expect("tokens is an array");
    assert_eq!(
        arr.len(),
        3,
        "3-coin pool must bake exactly 3 tokens; got {tokens}"
    );
    for (idx, want) in [USDT, WBTC, WETH].iter().enumerate() {
        assert_eq!(
            find_string_field(&arr[idx], "address").as_deref(),
            Some(*want),
            "tokens[{idx}] must be coin{idx}; got {}",
            arr[idx]
        );
    }
}

/// Field-level golden for Curve StableSwap-NG on BASE (chain 8453) — cross-chain
/// `chain_to_addresses` + the int128 coin-index value-map on the superOETHb/WETH
/// pool. Pins that the SAME curve_v1 venue + decode resolves a Base pool: `i=0` →
/// WETH (coin0), `j=1` → superOETHb (coin1). Demonstrates 0-core-code chain
/// extension (manifest `chain_to_addresses: {"8453": …}`).
#[test]
fn curve_stableswap_ng_base_exchange_resolves_coin_index() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Base superOETHb/WETH pool. exchange(i=0, j=1, _dx=1e18, _min_dy=0.99e18).
    const TO: &str = "0x302a94e3c28c290eaf2a4605fc52e11eb915f378";
    const WETH: &str = "0x4200000000000000000000000000000000000006";
    const SOETH: &str = "0xdbfefd2e8460a6ee4955a68582f85708baea60a3";
    const CALLDATA: &str = "0x3df02124000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000dbd2fc137a30000";

    let env = harness::route::route_calldata(8453, TO, "0x3df02124", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed on chain 8453: {env}"
    );
    let token_in = find_object_by_key(&env, "token_in").expect("swap body carries token_in");
    let token_out = find_object_by_key(&env, "token_out").expect("swap body carries token_out");
    assert_eq!(
        find_string_field(token_in, "address").as_deref(),
        Some(WETH),
        "i=0 must resolve token_in to coin0 (WETH) on Base; got {token_in}"
    );
    assert_eq!(
        find_string_field(token_out, "address").as_deref(),
        Some(SOETH),
        "j=1 must resolve token_out to coin1 (superOETHb) on Base; got {token_out}"
    );
}

/// Field-level golden for Curve StableSwap-NG on Base `add_liquidity` — the DYNAMIC
/// `uint256[]` array variant (the Base blueprint uses `uint256[]`, NOT the mainnet
/// pools' fixed `uint256[2]`). Pins that `$args._amounts[k]` positional baking
/// resolves over a dynamically-encoded array: `tokens[0]` = WETH paired with
/// `_amounts[0]`, `tokens[1]` = superOETHb paired with `_amounts[1]`.
#[test]
fn curve_stableswap_ng_base_add_liquidity_dynamic_array_bakes_coins() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Base superOETHb/WETH. add_liquidity(_amounts=[1e18, 2e18], _min_mint_amount=1).
    // uint256[] is dynamic → head offset 0x40, then [len=2, 1e18, 2e18].
    const TO: &str = "0x302a94e3c28c290eaf2a4605fc52e11eb915f378";
    const WETH: &str = "0x4200000000000000000000000000000000000006";
    const SOETH: &str = "0xdbfefd2e8460a6ee4955a68582f85708baea60a3";
    const CALLDATA: &str = "0xb72df5de0000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000001bc16d674ec80000";

    let env = harness::route::route_calldata(8453, TO, "0xb72df5de", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let tokens = find_object_by_key(&env, "tokens").expect("add_liquidity body carries tokens");
    let arr = tokens
        .as_array()
        .expect("tokens is an array of [TokenRef, amount]");
    assert_eq!(
        arr.len(),
        2,
        "2-coin pool must bake exactly 2 tokens; got {tokens}"
    );
    assert_eq!(
        find_string_field(&arr[0], "address").as_deref(),
        Some(WETH),
        "tokens[0] must be coin0 (WETH) via dynamic-array baking; got {}",
        arr[0]
    );
    assert_eq!(
        find_string_field(&arr[1], "address").as_deref(),
        Some(SOETH),
        "tokens[1] must be coin1 (superOETHb) via dynamic-array baking; got {}",
        arr[1]
    );
}

/// Field-level golden for Curve crvUSD `create_loan` — the new `LendingVenue::CrvUsd`
/// venue + the create_loan → `borrow` mapping. On the wstETH market, `create_loan`
/// must decode to a lending `borrow` whose `asset` is crvUSD (the DEBT token) and
/// whose `venue.collateral` is wstETH (the market's collateral). Baking the
/// collateral as the borrowed asset (or vice-versa) would silently misreport what
/// the user is borrowing vs depositing — exactly what a scope analyzer must get right.
#[test]
fn curve_crvusd_create_loan_borrows_crvusd_against_collateral() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // wstETH market Controller. create_loan(collateral=1e18, debt=1000e18, N=10).
    const TO: &str = "0x100daa78fc509db39ef7d04de0c1abd299f4c6ce";
    const CRVUSD: &str = "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e";
    const WSTETH: &str = "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0";
    const CALLDATA: &str = "0x23cfed030000000000000000000000000000000000000000000000000de0b6b3a764000000000000000000000000000000000000000000000000003635c9adc5dea00000000000000000000000000000000000000000000000000000000000000000000a";

    let env = harness::route::route_calldata(1, TO, "0x23cfed03", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    // The borrowed asset is crvUSD (debt), NOT the collateral.
    let asset = find_object_by_key(&env, "asset").expect("borrow body carries asset");
    assert_eq!(
        find_string_field(asset, "address").as_deref(),
        Some(CRVUSD),
        "create_loan must borrow crvUSD; got {asset}"
    );
    // The crv_usd venue names the market's collateral (wstETH).
    let collateral =
        find_object_by_key(&env, "collateral").expect("crv_usd venue carries collateral");
    assert_eq!(
        find_string_field(collateral, "address").as_deref(),
        Some(WSTETH),
        "wstETH market venue.collateral must be wstETH; got {collateral}"
    );
}

/// Field-level golden for Curve LlamaLend `create_loan` — the new
/// `LendingVenue::LlamaLend` venue (distinct from crvUSD). On the WBTC LlamaLend
/// market, `create_loan` must decode to a lending `borrow` with `venue.name =
/// "llama_lend"`, `asset` = crvUSD (the borrowed token), and `venue.collateral` =
/// WBTC. Pins both the new venue tag and the borrow/collateral split.
#[test]
fn curve_llamalend_create_loan_borrows_crvusd_against_collateral() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // WBTC LlamaLend market Controller. create_loan(collateral=1e18, debt=1000e18, N=10).
    const TO: &str = "0xcad85b7fe52b1939dceebee9bcf0b2a5aa0ce617";
    const CRVUSD: &str = "0xf939e0a03fb07f59a73314e73794be0e57ac1b4e";
    const WBTC: &str = "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599";
    const CALLDATA: &str = "0x23cfed030000000000000000000000000000000000000000000000000de0b6b3a764000000000000000000000000000000000000000000000000003635c9adc5dea00000000000000000000000000000000000000000000000000000000000000000000a";

    let env = harness::route::route_calldata(1, TO, "0x23cfed03", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let venue = find_object_by_key(&env, "venue").expect("borrow body carries venue");
    assert_eq!(
        find_string_field(venue, "name").as_deref(),
        Some("llama_lend"),
        "LlamaLend market venue.name must be llama_lend (not crv_usd); got {venue}"
    );
    let asset = find_object_by_key(&env, "asset").expect("borrow body carries asset");
    assert_eq!(
        find_string_field(asset, "address").as_deref(),
        Some(CRVUSD),
        "LlamaLend create_loan must borrow crvUSD; got {asset}"
    );
    let collateral =
        find_object_by_key(venue, "collateral").expect("llama_lend venue carries collateral");
    assert_eq!(
        find_string_field(collateral, "address").as_deref(),
        Some(WBTC),
        "WBTC market venue.collateral must be WBTC; got {collateral}"
    );
}

/// Field-level golden for Curve LlamaLend `approve(address,bool)` — the permission-
/// delegation primitive (ScopeBall's raison d'être). On a newgen LlamaLend
/// Controller (sreUSD market), `approve(_spender, _allow)` must decode to a
/// `permission` / `protocol_authorization` granting the OPERATOR role to
/// `_spender`, with `is_authorized` reflecting the `_allow` bool. A silent drop
/// (as "admin") would hide a loan-management authority grant.
#[test]
fn curve_llamalend_approve_decodes_operator_authorization() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // sreUSD market Controller (newgen). approve(_spender=0x1111…1111, _allow=true).
    const TO: &str = "0x4f79fe450a2baf833e8f50340bd230f5a3ecafe9";
    const SPENDER: &str = "0x1111111111111111111111111111111111111111";
    const CALLDATA: &str = "0x3d140d2100000000000000000000000011111111111111111111111111111111111111110000000000000000000000000000000000000000000000000000000000000001";

    let env = harness::route::route_calldata(1, TO, "0x3d140d21", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    assert_eq!(
        find_string_field(&env, "authorized").as_deref(),
        Some(SPENDER),
        "approve must authorize _spender as operator; got {env}"
    );
    assert_eq!(
        find_string_field(&env, "permission").as_deref(),
        Some("operator"),
        "LlamaLend approve grants the operator permission; got {env}"
    );
    assert_eq!(
        find_object_by_key(&env, "is_authorized").and_then(serde_json::Value::as_bool),
        Some(true),
        "_allow=1 must decode is_authorized=true (grant, not revoke); got {env}"
    );
}

/// Field-level golden: Curve veCRV `create_lock` must decode to a `staking`
/// `lock` whose locked `token` is CRV (BAKED by the manifest — not in calldata),
/// `amount` = `_value`, and `unlock_time` = `_unlock_time`. The corpus
/// (verdict + domain only) cannot verify the baked token or the arg plumbing —
/// a wrong-token bake or dropped arg would still pass as `pass`/`staking`.
#[test]
fn curve_vecrv_create_lock_locks_crv_for_unlock_time() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // veCRV VotingEscrow. create_lock(_value=1000e18, _unlock_time=1900000000).
    const TO: &str = "0x5f3b5dfeb7b28cdbd7faba78963ee202a494e2a2";
    const CRV: &str = "0xd533a949740bb3306d119cc777fa900ba034cd52";
    const CALLDATA: &str = "0x65fc387300000000000000000000000000000000000000000000003635c9adc5dea0000000000000000000000000000000000000000000000000000000000000713fb300";

    let env = harness::route::route_calldata(1, TO, "0x65fc3873", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let token = find_object_by_key(&env, "token").expect("lock body carries token");
    assert_eq!(
        find_string_field(token, "address").as_deref(),
        Some(CRV),
        "veCRV lock token must be CRV (baked); got {token}"
    );
    assert_eq!(
        find_string_field(&env, "amount").as_deref(),
        Some("0x3635c9adc5dea00000"),
        "lock amount must equal _value (1000e18)"
    );
    assert_eq!(
        find_string_field(&env, "unlock_time").as_deref(),
        Some("0x713fb300"),
        "lock unlock_time must equal _unlock_time (1900000000)"
    );
}

/// Field-level golden: Curve FeeDistributor `claim(address)` must decode to a
/// `staking` `claim_rewards` on the new `curve_fee_distributor` venue, with
/// `distributor = tx.to` and `on_behalf_of = $args._addr` (the beneficiary).
#[test]
fn curve_fee_distributor_claim_for_decodes_venue_and_beneficiary() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real mainnet tx 0x4452dd94… on the crvUSD FeeDistributor:
    // claim(_addr = 0x4986d3b5…).
    const TO: &str = "0xd16d5ec345dd86fb63c6a9c43c517210f1027914";
    const BENEFICIARY: &str = "0x4986d3b5160032ab7df0fac9503f6a2360f3f888";
    const CALLDATA: &str =
        "0x1e83409a0000000000000000000000004986d3b5160032ab7df0fac9503f6a2360f3f888";

    let env = harness::route::route_calldata(1, TO, "0x1e83409a", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let venue = find_object_by_key(&env, "venue").expect("claim_rewards carries venue");
    assert_eq!(
        find_string_field(venue, "name").as_deref(),
        Some("curve_fee_distributor"),
        "venue must be curve_fee_distributor; got {venue}"
    );
    assert_eq!(
        find_string_field(venue, "distributor").as_deref(),
        Some(TO),
        "distributor must be tx.to; got {venue}"
    );
    assert_eq!(
        find_string_field(&env, "on_behalf_of").as_deref(),
        Some(BENEFICIARY),
        "on_behalf_of must resolve to $args._addr"
    );
}

/// Field-level golden: Curve Minter `mint_for` must decode to a `staking`
/// `claim_rewards` whose `gauges` carries the calldata gauge, `on_behalf_of`
/// is `_for`, and `reward_token` is CRV (baked).
#[test]
fn curve_minter_mint_for_claims_crv_from_gauge_on_behalf() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Minter. mint_for(gauge_addr=0xbfcf..(3pool gauge), _for=0x..b02d).
    const TO: &str = "0xd061d61a4d941c39e5453435b6345dc261c2fce0";
    const CRV: &str = "0xd533a949740bb3306d119cc777fa900ba034cd52";
    const GAUGE: &str = "0xbfcf63294ad7105dea65aa58f8ae5be2d9d0952a";
    const FOR: &str = "0x000000000000000000000000000000000000b02d";
    const CALLDATA: &str = "0x27f18ae3000000000000000000000000bfcf63294ad7105dea65aa58f8ae5be2d9d0952a000000000000000000000000000000000000000000000000000000000000b02d";

    let env = harness::route::route_calldata(1, TO, "0x27f18ae3", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let reward =
        find_object_by_key(&env, "reward_token").expect("claim_rewards carries reward_token");
    assert_eq!(
        find_string_field(reward, "address").as_deref(),
        Some(CRV),
        "reward token must be CRV (baked); got {reward}"
    );
    let gauges = find_object_by_key(&env, "gauges").expect("claim_rewards carries gauges");
    let arr = gauges.as_array().expect("gauges must be an array");
    assert!(
        arr.iter().any(|g| g.as_str() == Some(GAUGE)),
        "gauges must contain the calldata gauge; got {gauges}"
    );
    assert_eq!(
        find_string_field(&env, "on_behalf_of").as_deref(),
        Some(FOR),
        "mint_for on_behalf_of must equal _for"
    );
}

/// Field-level golden: Curve GaugeController `vote_for_gauge_weights` must decode
/// to a `staking` `vote_for_gauge` carrying the calldata `gauge` and `weight_bp`
/// (basis points). Moves no funds; pins the gauge + weight plumbing.
#[test]
fn curve_gauge_controller_vote_decodes_gauge_and_weight() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // GaugeController. vote_for_gauge_weights(_gauge_addr=0xbfcf.., _user_weight=10000).
    const TO: &str = "0x2f50d538606fa9edd2b11e2446beb18c9d5846bb";
    const GAUGE: &str = "0xbfcf63294ad7105dea65aa58f8ae5be2d9d0952a";
    const CALLDATA: &str = "0xd7136328000000000000000000000000bfcf63294ad7105dea65aa58f8ae5be2d9d0952a0000000000000000000000000000000000000000000000000000000000002710";

    let env = harness::route::route_calldata(1, TO, "0xd7136328", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    assert_eq!(
        find_string_field(&env, "gauge").as_deref(),
        Some(GAUGE),
        "vote gauge must equal _gauge_addr"
    );
    assert_eq!(
        find_string_field(&env, "weight_bp").as_deref(),
        Some("0x2710"),
        "vote weight_bp must equal _user_weight (10000 bp)"
    );
}

/// Field-level golden: Curve Minter `toggle_approve_mint` is NOT a staking action
/// — it grants/revokes mint authority, so it must cross-route to the `permission`
/// domain (`protocol_authorization`) with `authorized` = the minting user and
/// `protocol_name` = "curve_minter". Two selectors on ONE contract fan out to two
/// domains; pins that the grant is not silently swallowed as a reward mint.
#[test]
fn curve_minter_toggle_approve_decodes_permission_grant() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Minter. toggle_approve_mint(minting_user=0x..b02d).
    const TO: &str = "0xd061d61a4d941c39e5453435b6345dc261c2fce0";
    const USER: &str = "0x000000000000000000000000000000000000b02d";
    const CALLDATA: &str =
        "0xdd289d60000000000000000000000000000000000000000000000000000000000000b02d";

    let env = harness::route::route_calldata(1, TO, "0xdd289d60", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    assert_eq!(
        find_string_field(&env, "domain").as_deref(),
        Some("permission"),
        "toggle_approve_mint must route to the permission domain; got {env}"
    );
    assert_eq!(
        find_string_field(&env, "authorized").as_deref(),
        Some(USER),
        "authorized must equal minting_user"
    );
    assert_eq!(
        find_string_field(&env, "protocol_name").as_deref(),
        Some("curve_minter"),
        "protocol_name must be curve_minter"
    );
}

/// Field-level golden: Curve gauge `deposit(uint256, address)` must decode to a
/// `staking` `gauge_deposit` whose venue gauge is the routed contract, `amount`
/// = `_value`, and `on_behalf_of` = `_addr`. The staked LP is identified by the
/// gauge venue (no separate token field). Decoded from a multi-address manifest
/// (one manifest covering all 8 onboarded pool gauges).
#[test]
fn curve_gauge_deposit_for_credits_addr() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // WBTC+tBTC (2btc) pool gauge. deposit(_value=2e18, _addr=0x..b02d).
    const TO: &str = "0x5010263ac1978297f56048c7d2b02316a3435404";
    const ADDR: &str = "0x000000000000000000000000000000000000b02d";
    const CALLDATA: &str = "0x6e553f650000000000000000000000000000000000000000000000001bc16d674ec80000000000000000000000000000000000000000000000000000000000000000b02d";

    let env = harness::route::route_calldata(1, TO, "0x6e553f65", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    assert_eq!(
        find_string_field(&env, "gauge").as_deref(),
        Some(TO),
        "gauge venue must be the routed gauge (multi-address $to)"
    );
    assert_eq!(
        find_string_field(&env, "amount").as_deref(),
        Some("0x1bc16d674ec80000"),
        "deposit amount must equal _value (2e18)"
    );
    assert_eq!(
        find_string_field(&env, "on_behalf_of").as_deref(),
        Some(ADDR),
        "deposit on_behalf_of must equal _addr"
    );
}

/// Field-level golden: Curve gauge `claim_rewards(address, address)` must decode
/// to a `staking` `claim_rewards` with NO `reward_token` (a gauge pays a
/// configured multi-reward set, not statically known) and empty `gauges` (the
/// gauge IS the venue), carrying `on_behalf_of` = `_addr` and `recipient` =
/// `_receiver`. This is the gauge claim path, distinct from a Minter CRV mint.
#[test]
fn curve_gauge_claim_rewards_to_recipient() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const TO: &str = "0x5010263ac1978297f56048c7d2b02316a3435404";
    const ADDR: &str = "0x000000000000000000000000000000000000b02d";
    const RECV: &str = "0x000000000000000000000000000000000000c0de";
    const CALLDATA: &str = "0x9faceb1b000000000000000000000000000000000000000000000000000000000000b02d000000000000000000000000000000000000000000000000000000000000c0de";

    let env = harness::route::route_calldata(1, TO, "0x9faceb1b", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    assert_eq!(
        find_string_field(&env, "on_behalf_of").as_deref(),
        Some(ADDR),
        "claim on_behalf_of must equal _addr"
    );
    assert_eq!(
        find_string_field(&env, "recipient").as_deref(),
        Some(RECV),
        "claim recipient must equal _receiver"
    );
    // A gauge's own claim carries no reward_token (unlike the Minter's CRV mint).
    assert!(
        find_object_by_key(&env, "reward_token").is_none(),
        "gauge claim_rewards must omit reward_token; got {env}"
    );
}

/// Field-level golden: a real Lido `submit` must decode to a `liquid_staking`
/// `stake` whose `amount` equals `msg.value` (manifest `amount = $tx.value`) —
/// a value the corpus (verdict + domain only) cannot verify. Pins the
/// `$tx.value` → Stake.amount plumbing and the referral arg decode.
#[test]
fn lido_submit_amount_is_msg_value_and_referral() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real mainnet submit tx 0x361f764d…: msg.value 9.8 ETH, referral 0x6dc9…3e43.
    const TO: &str = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84";
    const CALLDATA: &str =
        "0xa1903eab0000000000000000000000006dc9657c2d90d57cadffb64239242d06e6103e43";

    let env = harness::route::route_calldata(1, TO, "0xa1903eab", CALLDATA, "9800000000000000000");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    // U256 renders as lower-hex; 0x88009813ced40000 == 9_800_000_000_000_000_000
    // == the msg.value passed to route. Pins the $tx.value → Stake.amount plumbing.
    assert_eq!(
        find_string_field(&env, "amount").as_deref(),
        Some("0x88009813ced40000"),
        "Stake.amount must equal msg.value ($tx.value): {env}"
    );
    assert_eq!(
        find_string_field(&env, "referral")
            .map(|s| s.to_lowercase())
            .as_deref(),
        Some("0x6dc9657c2d90d57cadffb64239242d06e6103e43"),
        "Stake.referral mis-decoded: {env}"
    );
}

/// Field-level golden: a real Lido `requestWithdrawals` must decode to a
/// `liquid_staking` `request_withdrawal` whose `owner` is the NFT beneficiary
/// and whose `token` is stETH (the burned asset). Pins arg decode + the baked
/// stETH-vs-wstETH token discriminator.
#[test]
fn lido_request_withdrawals_decodes_owner_and_steth_token() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real mainnet requestWithdrawals tx 0xf9cdfd1c…: amounts=[2360955914336215],
    // owner 0xafc9…9a21.
    const TO: &str = "0x889edc2edab5f40e902b864ad4d7ade8e412f9b1";
    const CALLDATA: &str = "0xd66810420000000000000000000000000000000000000000000000000000000000000040000000000000000000000000afc98cfd72b91c5da3a35dbd387bdc40dd289a21000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000086346e29ab7d7";

    let env = harness::route::route_calldata(1, TO, "0xd6681042", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    assert_eq!(
        find_string_field(&env, "owner")
            .map(|s| s.to_lowercase())
            .as_deref(),
        Some("0xafc98cfd72b91c5da3a35dbd387bdc40dd289a21"),
        "RequestWithdrawal.owner mis-decoded: {env}"
    );
    let token = find_object_by_key(&env, "token").expect("request_withdrawal body carries token");
    assert_eq!(
        find_string_field(token, "address")
            .map(|s| s.to_lowercase())
            .as_deref(),
        Some("0xae7ab96520de3a18e5e111b5eaab095312d7fe84"),
        "requestWithdrawals burns stETH; token must be stETH: {token}"
    );
}

/// Field-level golden: a real wstETH `wrap` must decode to a `liquid_staking`
/// `wrap` whose `amount` is the supplied stETH amount (`$args._stETHAmount`).
#[test]
fn lido_wrap_amount_decodes() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real mainnet wrap tx 0x3422ad3f…: _stETHAmount = 12948522489883869033.
    const TO: &str = "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0";
    const CALLDATA: &str =
        "0xea598cb0000000000000000000000000000000000000000000000000b3b26499afaf3f69";

    let env = harness::route::route_calldata(1, TO, "0xea598cb0", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    // U256 lower-hex: 0xb3b26499afaf3f69 == 12_948_522_489_883_869_033.
    assert_eq!(
        find_string_field(&env, "amount").as_deref(),
        Some("0xb3b26499afaf3f69"),
        "Wrap.amount mis-decoded: {env}"
    );
}

/// Field-level golden: a real wstETH `wrap` must decode with the `expected_wsteth`
/// live-input plumbing wired end-to-end — the manifest `onchain_view` source
/// (`getWstETHByStETH(uint256)`) survives the action_builder → reducer-struct →
/// env serialization so the host can later fill the concrete wstETH amount.
///
/// The VALUE is host-populated (skeleton `0` here), so this pins the SOURCE view
/// fn, not the value: a manifest that drops the live_inputs block, mis-names the
/// view, or whose `WrapLiveInputs` field stops round-tripping would fail here
/// while `corpus_replay` (verdict + domain only) stays green.
#[test]
fn lido_wrap_expected_wsteth_live_input_is_wired() {
    let _surface = adapters::load_and_install().expect("install local surface");

    // Same real mainnet wrap tx 0x3422ad3f… as `lido_wrap_amount_decodes`.
    const TO: &str = "0x7f39c581f595b53c5cb19bd0b3f8da6c935e2ca0";
    const CALLDATA: &str =
        "0xea598cb0000000000000000000000000000000000000000000000000b3b26499afaf3f69";

    let env = harness::route::route_calldata(1, TO, "0xea598cb0", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    let live = find_object_by_key(&env, "expected_wsteth")
        .expect("wrap body carries the expected_wsteth live field");
    assert_eq!(
        find_string_field(live, "function").as_deref(),
        Some("getWstETHByStETH(uint256)"),
        "expected_wsteth onchain_view source fn mis-wired: {env}"
    );
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

/// Field-level golden for Pendle `swapExactTokenForPt` (yield-domain positional
/// mapping).
///
/// `corpus_replay`'s oracle checks only the verdict + top-level `domain` — never
/// the body field VALUES. A positional mis-map (market vs receiver swapped, the
/// `direction` constant wrong, or `external_token` pointing at the wrong
/// `TokenInput` slot) would still pass corpus as `pass`/`yield` — a silent
/// mis-decode (the §9.4 lesson). This pins, from a real mainnet swap, the
/// security-relevant fields: which market, which direction, who receives, and
/// that `external_token` is the calldata `TokenInput.tokenIn` (`$args.input[0]`).
#[test]
fn pendle_swap_exact_token_for_pt_decodes_market_direction_token_recipient() {
    // R1: install + route on the same thread.
    let _surface = adapters::load_and_install().expect("install local surface");

    // Real mainnet tx 0x008bb93a4cd627b2f6f93ca5f4ac4415208ca09e0db82999638ff1a747a50dad
    // on Router V4: swapExactTokenForPt(receiver=0xf1bfd60e…, market=0x3c53fae2…,
    // minPtOut, guessPtOut, TokenInput{tokenIn=0x38eeb52f…, …}, limit).
    const TO: &str = "0x888888888889758f76e7103c6cbf23abbf58f946";
    const CALLDATA: &str = "0xc81f847a000000000000000000000000f1bfd60ece3b5b4d1472a3b00543c5912111b07a0000000000000000000000003c53fae231ad3c0408a8b6d33138bbff1caec3300000000000000000000000000000000000000000000001b5acd5dca234e455070000000000000000000000000000000000000000000000db0e7f06a4b959ef9e0000000000000000000000000000000000000000000001b84daa4a3b916c1da80000000000000000000000000000000000000000000001b61cfe0d4972b3df3d000000000000000000000000000000000000000000000000000000000000001e000000000000000000000000000000000000000000000000000000e8d4a510000000000000000000000000000000000000000000000000000000000000000140000000000000000000000000000000000000000000000000000000000000028000000000000000000000000038eeb52f0771140d10c4e9a9a72349a329fe8a6a00000000000000000000000000000000000000000000013c469edbe3eedbe28900000000000000000000000038eeb52f0771140d10c4e9a9a72349a329fe8a6a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    let env = harness::route::route_calldata(1, TO, "0xc81f847a", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    assert_eq!(
        find_string_field(&env, "market").as_deref(),
        Some("0x3c53fae231ad3c0408a8b6d33138bbff1caec330"),
        "market mis-decoded (must be calldata arg[1])"
    );
    assert_eq!(
        find_string_field(&env, "direction").as_deref(),
        Some("token_for_pt"),
        "PT swap direction constant mis-set"
    );
    assert_eq!(
        find_string_field(&env, "recipient").as_deref(),
        Some("0xf1bfd60ece3b5b4d1472a3b00543c5912111b07a"),
        "recipient mis-decoded (must be calldata arg[0])"
    );
    // external_token = TokenInput.tokenIn (input[0]) — the only nested AssetRef.
    let ext = find_object_by_key(&env, "external_token")
        .expect("token_for_pt swap carries external_token");
    assert_eq!(
        find_string_field(ext, "address").as_deref(),
        Some("0x38eeb52f0771140d10c4e9a9a72349a329fe8a6a"),
        "external_token must be the calldata TokenInput.tokenIn"
    );
}

/// Field-level golden for the Pendle market enrichment (P1c, §4d): the same real
/// `swapExactTokenForPt` must decode with the market→(SY,PT,YT)+maturity live
/// inputs wired end-to-end.
///
/// The VALUEs are host-populated (skeleton zero here), so this pins the SOURCE
/// views, not the values: SY/PT/YT come from `IPMarket.readTokens()` and maturity
/// from `IPMarket.expiry()`, and — crucially — the source `contract` is the
/// calldata `market` (resolved from `$args.market` at decode time), NOT the
/// router `$to`. That last check is what proves the manifest-only enrichment
/// (CASE A) actually wired: a block dropped, a view mis-named, the `contract`
/// left at the router, or a `MarketTokensLiveInputs` round-trip break all fail
/// here while `corpus_replay` (verdict + domain only) stays green.
#[test]
fn pendle_swap_market_enrichment_live_inputs_wired() {
    // R1: install + route on the same thread.
    let _surface = adapters::load_and_install().expect("install local surface");

    // Same real mainnet tx 0x008bb93a… as the positional-mapping golden above.
    const TO: &str = "0x888888888889758f76e7103c6cbf23abbf58f946";
    const CALLDATA: &str = "0xc81f847a000000000000000000000000f1bfd60ece3b5b4d1472a3b00543c5912111b07a0000000000000000000000003c53fae231ad3c0408a8b6d33138bbff1caec3300000000000000000000000000000000000000000000001b5acd5dca234e455070000000000000000000000000000000000000000000000db0e7f06a4b959ef9e0000000000000000000000000000000000000000000001b84daa4a3b916c1da80000000000000000000000000000000000000000000001b61cfe0d4972b3df3d000000000000000000000000000000000000000000000000000000000000001e000000000000000000000000000000000000000000000000000000e8d4a510000000000000000000000000000000000000000000000000000000000000000140000000000000000000000000000000000000000000000000000000000000028000000000000000000000000038eeb52f0771140d10c4e9a9a72349a329fe8a6a00000000000000000000000000000000000000000000013c469edbe3eedbe28900000000000000000000000038eeb52f0771140d10c4e9a9a72349a329fe8a6a000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000e0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    let env = harness::route::route_calldata(1, TO, "0xc81f847a", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "route did not succeed: {env}"
    );
    // SY/PT/YT enrichment sources read IPMarket.readTokens() …
    let read = find_object_with_string_field(&env, "function", "readTokens()")
        .expect("market enrichment carries a readTokens() onchain_view source");
    // … on the calldata MARKET (resolved from $args.market), NOT the router $to.
    assert_eq!(
        read.get("contract").and_then(serde_json::Value::as_str),
        Some("0x3c53fae231ad3c0408a8b6d33138bbff1caec330"),
        "readTokens() source must target the calldata market, not the router"
    );
    // maturity is sourced from IPMarket.expiry().
    find_object_with_string_field(&env, "function", "expiry()")
        .expect("market enrichment carries an expiry() onchain_view source");
}

/// Field-level golden for the Pendle off-chain limit-order sign (P1d).
///
/// The maker signs an EIP-712 `Order` against `PendleLimitRouter` (domain
/// `"Pendle Limit Order Protocol"` v1, primary_type `Order`). The typed-data
/// route reshapes the message into `args.order.*` (wrap rule) and runs the same
/// emit-rule the calldata path uses. This pins the security-relevant fields: the
/// `orderType` uint8 → discriminant value-map (`0` → `"sy_for_pt"`), who the
/// maker is, which YT (market identity) is traded, and the SY token side.
/// `corpus_replay` cannot see these (verdict + domain only) — a value-map miss or
/// a positional mis-map would pass corpus as `pass`/`yield`.
#[test]
fn pendle_sign_limit_order_typed_data_decodes_order_fields() {
    let _surface = adapters::load_and_install().expect("install local surface");

    const VC: &str = "0x000000000000c9b3e2c3ec88b1b4c0cd853f4321";
    const TOKEN: &str = "0x1111111111111111111111111111111111111111"; // SY token side
    const YT: &str = "0x2222222222222222222222222222222222222222";
    const MAKER: &str = "0x3333333333333333333333333333333333333333";
    const RECEIVER: &str = "0x4444444444444444444444444444444444444444";
    let message = serde_json::json!({
        "salt": "1",
        "expiry": "9999999999",
        "nonce": "0",
        "orderType": 0,
        "token": TOKEN,
        "YT": YT,
        "maker": MAKER,
        "receiver": RECEIVER,
        "makingAmount": "1000000000",
        "lnImpliedRate": "0",
        "failSafeRate": "0",
        "permit": "0x"
    });

    let env = harness::route::route_typed_data(
        1,
        VC,
        "Order",
        None,
        Some("Pendle Limit Order Protocol"),
        &message,
    );
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "typed-data route did not succeed: {env}"
    );
    // orderType uint8 `0` → "sy_for_pt" via the discriminant value-map.
    assert_eq!(
        find_string_field(&env, "order_type"),
        Some("sy_for_pt".into()),
        "orderType value-map mis-decoded: {env}"
    );
    assert_eq!(
        find_string_field(&env, "maker"),
        Some(MAKER.into()),
        "Order.maker mis-decoded"
    );
    assert_eq!(
        find_string_field(&env, "yt"),
        Some(YT.into()),
        "Order.YT mis-decoded"
    );
    // SY token side is wrapped as an ERC20 TokenRef.
    let tok = find_object_by_key(&env, "token").expect("sign carries the SY token side");
    assert_eq!(
        find_string_field(tok, "address"),
        Some(TOKEN.into()),
        "Order.token (SY side) mis-decoded"
    );
}

// ---------------------------------------------------------------------------
// EigenLayer (restaking) field-level goldens. The corpus oracle checks only
// verdict + top-level domain; these pin the decoded field VALUES (operator,
// strategies, withdrawer, permission grant) that a wrong emit would silently
// mis-decode — including the array_emit (queueWithdrawals) and nested-tuple
// (completeQueuedWithdrawal `$args.withdrawal[i]`) cases the single_emit
// `check:manifest` validate does NOT cover.
// ---------------------------------------------------------------------------

const EL_DM: &str = "0x39053d51b77dc0d36036fc1fcc8cb819df8ef37a";
const EL_SM: &str = "0x858646372cc42e1a627fce94aa7a7033e7cf075a";
const EL_PC: &str = "0x25e5f8b1e7adf44518d35d5b2271f114e081f0e5";

/// Real mainnet `delegateTo` 0x6defd3f6…: delegate to operator 0x8c81d590….
#[test]
fn eigenlayer_delegate_to_decodes_operator() {
    let _surface = adapters::load_and_install().expect("install local surface");
    const CALLDATA: &str = "0xeea9064b0000000000000000000000008c81d590cc94ca2451c4bde24c598193da74a57500000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    let env = harness::route::route_calldata(1, EL_DM, "0xeea9064b", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "delegateTo route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "domain"), Some("restaking".into()));
    assert_eq!(
        find_string_field(&env, "action"),
        Some("delegate_to".into())
    );
    assert_eq!(
        find_string_field(&env, "operator"),
        Some("0x8c81d590cc94ca2451c4bde24c598193da74a575".into()),
        "delegateTo operator mis-decoded"
    );
}

/// Real mainnet `queueWithdrawals` 0xd00d0ca0…: array_emit over one
/// QueuedWithdrawalParams → a Multicall wrapping a `restaking.queue_withdrawal`
/// whose `withdrawer`, `strategies[]` come from the inner tuple via `$inputs[i]`.
#[test]
fn eigenlayer_queue_withdrawals_array_emit_decodes_multicall() {
    let _surface = adapters::load_and_install().expect("install local surface");
    const CALLDATA: &str = "0x0dd8dd02000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000ba3053d3e075a8037d4c01b1ca08aa1cbe508e840000000000000000000000000000000000000000000000000000000000000001000000000000000000000000beac0eeeeeeeeeeeeeeeeeeeeeeeeeeeeeebeac000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000b339ee5c9cff800";
    let env = harness::route::route_calldata(1, EL_DM, "0x0dd8dd02", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "queueWithdrawals route did not succeed: {env}"
    );
    // array_emit expands params[] into a Multicall; inner is restaking.queue_withdrawal.
    assert_eq!(find_string_field(&env, "domain"), Some("multicall".into()));
    assert_eq!(
        find_string_field(&env, "action"),
        Some("queue_withdrawal".into())
    );
    assert_eq!(
        find_string_field(&env, "withdrawer"),
        Some("0xba3053d3e075a8037d4c01b1ca08aa1cbe508e84".into()),
        "queue_withdrawal withdrawer mis-decoded"
    );
    let strategies =
        find_object_by_key(&env, "strategies").expect("queue_withdrawal has strategies");
    assert_eq!(
        strategies
            .as_array()
            .and_then(|a| a.first())
            .and_then(serde_json::Value::as_str),
        Some("0xbeac0eeeeeeeeeeeeeeeeeeeeeeeeeeeeeebeac0"),
        "strategies[0] (native beacon-ETH strategy sentinel) mis-decoded"
    );
}

/// Real mainnet `completeQueuedWithdrawal` 0x3b386866…: the nested `Withdrawal`
/// tuple's `staker` (component 0) and `strategies` (component 5) are pulled via
/// chained-numeric `$args.withdrawal[i]`.
#[test]
fn eigenlayer_complete_queued_withdrawal_decodes_nested_tuple() {
    let _surface = adapters::load_and_install().expect("install local surface");
    const CALLDATA: &str = "0xe4cc3f90000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000001c00000000000000000000000000000000000000000000000000000000000000001000000000000000000000000aa0cbae2dd290f8aed1b672ebe2e197fd969628b0000000000000000000000003601bda2b72628da309ab9df7d310ada38cae44c000000000000000000000000aa0cbae2dd290f8aed1b672ebe2e197fd969628b00000000000000000000000000000000000000000000000000000000000000ab00000000000000000000000000000000000000000000000000000000017f270e00000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000001000000000000000000000000beac0eeeeeeeeeeeeeeeeeeeeeeeeeeeeeebeac000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000003fedc53618828c0000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000";
    let env = harness::route::route_calldata(1, EL_DM, "0xe4cc3f90", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "completeQueuedWithdrawal route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "domain"), Some("restaking".into()));
    assert_eq!(
        find_string_field(&env, "action"),
        Some("complete_withdrawal".into())
    );
    assert_eq!(
        find_string_field(&env, "staker"),
        Some("0xaa0cbae2dd290f8aed1b672ebe2e197fd969628b".into()),
        "Withdrawal.staker (component 0) mis-decoded"
    );
    assert_eq!(find_bool_field(&env, "receive_as_tokens"), Some(true));
    let strategies =
        find_object_by_key(&env, "strategies").expect("complete_withdrawal has strategies");
    assert_eq!(
        strategies
            .as_array()
            .and_then(|a| a.first())
            .and_then(serde_json::Value::as_str),
        Some("0xbeac0eeeeeeeeeeeeeeeeeeeeeeeeeeeeeebeac0"),
        "Withdrawal.strategies[0] (component 5) mis-decoded"
    );
}

/// Real mainnet `depositIntoStrategy` 0xb175325c…: stETH strategy + stETH token.
#[test]
fn eigenlayer_deposit_into_strategy_decodes_strategy_and_token() {
    let _surface = adapters::load_and_install().expect("install local surface");
    const STETH_STRATEGY: &str = "0x93c4b944d05dfe6df7645a86cd2206016c51564d";
    const STETH: &str = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84";
    const CALLDATA: &str = "0xe7a050aa00000000000000000000000093c4b944d05dfe6df7645a86cd2206016c51564d000000000000000000000000ae7ab96520de3a18e5e111b5eaab095312d7fe8400000000000000000000000000000000000000000000000000af87f5a3404400";
    let env = harness::route::route_calldata(1, EL_SM, "0xe7a050aa", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "depositIntoStrategy route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "domain"), Some("restaking".into()));
    assert_eq!(find_string_field(&env, "action"), Some("deposit".into()));
    assert_eq!(
        find_string_field(&env, "strategy"),
        Some(STETH_STRATEGY.into())
    );
    let token = find_object_by_key(&env, "token").expect("deposit has token");
    assert_eq!(
        token
            .pointer("/key/address")
            .and_then(serde_json::Value::as_str),
        Some(STETH),
        "deposit token (stETH) mis-decoded"
    );
}

/// Real mainnet `setAppointee` 0x9739f464…: account grants appointee a
/// selector-scoped call right → permission.protocol_authorization grant.
#[test]
fn eigenlayer_set_appointee_decodes_grant() {
    let _surface = adapters::load_and_install().expect("install local surface");
    const ACCOUNT: &str = "0xf07f83ff977dd004060f00ecefb80a9f92775098";
    const APPOINTEE: &str = "0x54bb392508d458cbf1e48c59d44ffbc93f912329";
    const CALLDATA: &str = "0x950d806e000000000000000000000000f07f83ff977dd004060f00ecefb80a9f9277509800000000000000000000000054bb392508d458cbf1e48c59d44ffbc93f912329000000000000000000000000948a420b8cc1d6bfd0b6087c2e7c344a2cd0bc393635205700000000000000000000000000000000000000000000000000000000";
    let env = harness::route::route_calldata(1, EL_PC, "0x950d806e", CALLDATA, "0");
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "setAppointee route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "domain"), Some("permission".into()));
    assert_eq!(find_string_field(&env, "authorizer"), Some(ACCOUNT.into()));
    assert_eq!(
        find_string_field(&env, "authorized"),
        Some(APPOINTEE.into())
    );
    assert_eq!(find_bool_field(&env, "is_authorized"), Some(true));
}

/// Off-chain `Deposit` EIP-712 (StrategyManager): the staker authorizes a
/// deposit on their behalf → restaking.deposit with the signed strategy/token.
#[test]
fn eigenlayer_deposit_typed_data_decodes_strategy_token_staker() {
    let _surface = adapters::load_and_install().expect("install local surface");
    const STAKER: &str = "0x1111111111111111111111111111111111111111";
    const STRATEGY: &str = "0x93c4b944d05dfe6df7645a86cd2206016c51564d";
    const TOKEN: &str = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84";
    let message = serde_json::json!({
        "staker": STAKER,
        "strategy": STRATEGY,
        "token": TOKEN,
        "amount": "1000000000000000000",
        "nonce": "0",
        "expiry": "9999999999"
    });
    let env =
        harness::route::route_typed_data(1, EL_SM, "Deposit", None, Some("EigenLayer"), &message);
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "Deposit typed-data route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "domain"), Some("restaking".into()));
    assert_eq!(find_string_field(&env, "action"), Some("deposit".into()));
    assert_eq!(find_string_field(&env, "strategy"), Some(STRATEGY.into()));
    assert_eq!(find_string_field(&env, "staker"), Some(STAKER.into()));
}

/// Off-chain `DelegationApproval` EIP-712 (DelegationManager): the operator's
/// delegationApprover authorizes a specific staker→operator delegation →
/// permission.protocol_authorization (authorizer = approver, authorized = staker).
#[test]
fn eigenlayer_delegation_approval_typed_data_decodes_grant() {
    let _surface = adapters::load_and_install().expect("install local surface");
    const APPROVER: &str = "0x2222222222222222222222222222222222222222";
    const STAKER: &str = "0x3333333333333333333333333333333333333333";
    let message = serde_json::json!({
        "delegationApprover": APPROVER,
        "staker": STAKER,
        "operator": "0x4444444444444444444444444444444444444444",
        "salt": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "expiry": "9999999999"
    });
    let env = harness::route::route_typed_data(
        1,
        EL_DM,
        "DelegationApproval",
        None,
        Some("EigenLayer"),
        &message,
    );
    assert_eq!(
        env.get("ok").and_then(serde_json::Value::as_bool),
        Some(true),
        "DelegationApproval typed-data route did not succeed: {env}"
    );
    assert_eq!(find_string_field(&env, "domain"), Some("permission".into()));
    assert_eq!(find_string_field(&env, "authorizer"), Some(APPROVER.into()));
    assert_eq!(find_string_field(&env, "authorized"), Some(STAKER.into()));
    assert_eq!(find_bool_field(&env, "is_authorized"), Some(true));
}

/// End-to-end decode→verdict golden for the EIP-712 typed-data signature wiring
/// (mock-free). Every other typed-data test in this file stops at the decoded
/// body; this one is the only test that carries a real Permit2 `PermitSingle`
/// signature all the way through the PRODUCTION decoder AND the real Cedar
/// evaluation, exactly as the orchestrator's `typedSignatureLifecycle` does:
///
///   route_typed_data (production WASM decode)
///     → ActionBody { domain: token, action: permit2_sign_allowance, spender, … }
///   → evaluate_action_v2_json (real Cedar, no mock)
///     → Verdict::Warn { matched: [permit2-sign-allowance-confirm] }
///
/// The policy bundle is the shipped default-policy fixture
/// (`default_policies_v2/permit2-sign-allowance-confirm`) — the same
/// confirm-before-sign policy the extension installs — so this pins that signing
/// a Permit2 token allowance actually surfaces a warn to the user. A regression
/// in the decoder (wrong action tag), the trigger (`action.tag` match), or the
/// Cedar severity would all break it; the synthetic-fuzz / corpus gates above
/// would not (they never run `evaluate_action_v2_json`).
#[test]
fn permit2_sign_allowance_typed_data_yields_warn_verdict() {
    use serde_json::Value;

    // R1: install + route + evaluate on the same OS thread (WASM v3 install
    // state is thread-local).
    let _surface = adapters::load_and_install().expect("install local surface");

    // The shipped raw `eth_signTypedData_v4` Permit2 PermitSingle golden input.
    const GOLDEN: &str = include_str!("../data/golden/inputs/permit2_permit_single.json");
    let golden: Value = serde_json::from_str(GOLDEN).expect("parse permit2 golden input");
    let typed = &golden["rpc"]["params"][1];
    let verifying_contract = typed["domain"]["verifyingContract"]
        .as_str()
        .expect("golden carries domain.verifyingContract");
    let primary_type = typed["primaryType"]
        .as_str()
        .expect("golden carries primaryType");
    let domain_name = typed["domain"]["name"].as_str();
    let message = &typed["message"];

    // The signed message's spender — the security-critical field a user must see.
    const SPENDER: &str = "0x1111111111111111111111111111111111111111";

    // ── decode: production typed-data route ──────────────────────────────────
    let env = harness::route::route_typed_data(
        1,
        verifying_contract,
        primary_type,
        None,
        domain_name,
        message,
    );
    assert_eq!(
        env.get("ok").and_then(Value::as_bool),
        Some(true),
        "Permit2 PermitSingle typed-data route did not succeed: {env}"
    );
    let actions = env["data"]["actions"]
        .as_array()
        .expect("route env carries data.actions[]");
    assert_eq!(
        actions.len(),
        1,
        "expected exactly one decoded action; got {actions:?}"
    );
    let action = actions[0]
        .get("body")
        .expect("decoded action carries a body");
    let meta = actions[0].get("meta").expect("decoded action carries meta");
    eprintln!("decoded Permit2 ActionBody = {action}");

    assert_eq!(
        action.get("domain").and_then(Value::as_str),
        Some("token"),
        "Permit2 sign must decode to the token domain; got {action}"
    );
    assert_eq!(
        action.get("action").and_then(Value::as_str),
        Some("permit2_sign_allowance"),
        "must decode to the permit2_sign_allowance tag (the policy trigger); got {action}"
    );
    assert_eq!(
        action.get("spender").and_then(Value::as_str),
        Some(SPENDER),
        "decoded spender must equal the signed message spender; got {action}"
    );

    // ── verdict: real Cedar over the shipped default policy ──────────────────
    const POLICY: &str = include_str!(
        "../../policy-engine/tests/fixtures/default_policies_v2/permit2-sign-allowance-confirm/policy.cedar"
    );
    const MANIFEST: &str = include_str!(
        "../../policy-engine/tests/fixtures/default_policies_v2/permit2-sign-allowance-confirm/manifest.json"
    );
    let manifest: Value = serde_json::from_str(MANIFEST).expect("parse permit2 confirm manifest");

    // tx context mirrors `typedSignatureLifecycle`: from = signer, to =
    // verifyingContract. `to` is not a trigger-match field (TriggerField has no
    // `tx.to`); the policy keys solely on `action.tag`.
    let eval_input = serde_json::json!({
        "action": action,
        "meta": meta,
        "tx": {
            "chain_id": "eip155:1",
            "from": "0x000000000000000000000000000000000000aaaa",
            "to": verifying_contract,
        },
        "bundles": [{ "policy": POLICY, "manifest": manifest }],
        "results": {},
    });
    let verdict_env = harness::route::evaluate_action(&eval_input);
    assert_eq!(
        verdict_env.get("ok").and_then(Value::as_bool),
        Some(true),
        "evaluate_action_v2_json did not return an ok envelope: {verdict_env}"
    );
    let verdict = &verdict_env["data"]["verdict"];
    assert_eq!(
        verdict.get("kind").and_then(Value::as_str),
        Some("warn"),
        "signing a Permit2 allowance must warn (confirm-before-sign default policy); got {verdict}"
    );
    let matched = verdict["matched"]
        .as_array()
        .expect("warn verdict carries matched[]");
    assert!(
        matched.iter().any(|m| {
            m.get("policy_id").and_then(Value::as_str) == Some("permit2-sign-allowance-confirm")
        }),
        "warn must be attributed to permit2-sign-allowance-confirm; got {verdict}"
    );
}

/// Companion to the PermitSingle verdict golden — locks the `tuple[]` arm of the
/// named→positional reshape. A Permit2 `PermitBatch` signature carries
/// `details` as an ARRAY of `PermitDetails`; the manifest fans it out via
/// `array_emit` (`array_source: $args.permitBatch[0]`) into a `Multicall` of
/// per-token `permit2_sign_allowance` bodies. A wallet sends `details` as named
/// objects, so without reshaping the nested `tuple[]` the positional per-item
/// paths (`$inputs[0]` = token) would not resolve. This pins that BOTH batch
/// entries decode with their own token in coin order (WETH then USDC) — the
/// `reshape_named_to_positional` tuple[] branch the PermitSingle test never hits.
#[test]
fn permit2_permit_batch_typed_data_reshapes_tuple_array() {
    use serde_json::Value;

    let _surface = adapters::load_and_install().expect("install local surface");

    const VC: &str = "0x000000000022d473030f116ddee9f6b43ac78ba3";
    const WETH: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
    const USDC: &str = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
    const SPENDER: &str = "0x1111111111111111111111111111111111111111";

    // NAMED PermitBatch message — `details` is an array of named PermitDetails.
    let message = serde_json::json!({
        "details": [
            { "token": WETH, "amount": "10000000000000000", "expiration": 4600, "nonce": 1 },
            { "token": USDC, "amount": "5000000", "expiration": 4600, "nonce": 2 }
        ],
        "spender": SPENDER,
        "sigDeadline": 1600
    });

    let env =
        harness::route::route_typed_data(1, VC, "PermitBatch", None, Some("Permit2"), &message);
    assert_eq!(
        env.get("ok").and_then(Value::as_bool),
        Some(true),
        "PermitBatch typed-data route did not succeed: {env}"
    );
    let body = env
        .pointer("/data/actions/0/body")
        .expect("route env carries data.actions[0].body");
    assert_eq!(
        body.get("domain").and_then(Value::as_str),
        Some("multicall"),
        "PermitBatch must fan out to a multicall body; got {body}"
    );
    let inner = body
        .get("actions")
        .and_then(Value::as_array)
        .expect("multicall body carries actions[]");
    assert_eq!(inner.len(), 2, "two batch entries expected; got {inner:?}");

    // Each entry is a permit2_sign_allowance carrying ITS OWN token, in order.
    for (idx, want_token) in [WETH, USDC].iter().enumerate() {
        assert_eq!(
            inner[idx].get("action").and_then(Value::as_str),
            Some("permit2_sign_allowance"),
            "batch[{idx}] must be permit2_sign_allowance; got {}",
            inner[idx]
        );
        assert_eq!(
            inner[idx]
                .pointer("/token/key/address")
                .and_then(Value::as_str),
            Some(*want_token),
            "batch[{idx}] token must reshape from details[{idx}] (tuple[] arm); got {}",
            inner[idx]
        );
        assert_eq!(
            inner[idx].get("spender").and_then(Value::as_str),
            Some(SPENDER),
            "batch[{idx}] spender must equal the signed message spender; got {}",
            inner[idx]
        );
    }
}
