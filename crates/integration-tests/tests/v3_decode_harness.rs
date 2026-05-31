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
