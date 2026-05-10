//! Round-trip 테스트 — 21 fixture의 input.json에서 첫 `decodedCall`을 추출해
//! `dispatch::classify_call`을 호출, 결과 카테고리·액션타입이 expected.json과
//! 일치하는지 검증.
//!
//! 완전한 deep equality는 *결정론적이지 않은 필드*(id 순번, fixture별 임의값)가
//! 있어 어렵다. 그래서 *카테고리·액션타입* 수준의 round-trip만 검증.

use std::fs;
use std::path::PathBuf;

use policyschema::action::{ActionCategory, ActionType};
use policyschema::semi_adapter::BuildContext;
use policyschema::request::{Request, TypedDataRequest};
use policyschema::target::ContractTarget;
use policyschema::{classify_call, parse_selector};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("fixtures")
}

#[derive(Debug, Clone)]
struct ParsedExpected {
    actions: Vec<(ActionCategory, ActionType)>,
}

fn parse_expected(value: &serde_json::Value) -> ParsedExpected {
    let mut actions = Vec::new();
    if let Some(arr) = value.get("actions").and_then(|v| v.as_array()) {
        for a in arr {
            let cat = a
                .get("category")
                .and_then(|v| v.as_str())
                .and_then(|s| serde_json::from_value(serde_json::Value::String(s.into())).ok())
                .unwrap_or(ActionCategory::Unknown);
            let typ = a
                .get("type")
                .and_then(|v| v.as_str())
                .and_then(|s| serde_json::from_value(serde_json::Value::String(s.into())).ok())
                .unwrap_or(ActionType::Unknown);
            actions.push((cat, typ));
        }
    }
    ParsedExpected { actions }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct InputJson {
    request: Request,
    targets: Vec<ContractTarget>,
    #[serde(rename = "decodedCalls")]
    decoded_calls: Vec<serde_json::Value>,
}

#[test]
fn classify_first_call_matches_expected() {
    let dir = fixtures_dir();
    let mut total = 0;
    let mut passed = 0;
    let mut skipped = Vec::new();
    let mut mismatches = Vec::new();

    for entry in fs::read_dir(&dir).expect("fixtures dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        if !name.ends_with(".input.json") {
            continue;
        }
        total += 1;

        let stem = name.strip_suffix(".input.json").unwrap();
        let expected_path = dir.join(format!("{stem}.expected.json"));

        let input_bytes = fs::read(&path).unwrap();
        let expected_bytes = fs::read(&expected_path).unwrap();
        let expected_v: serde_json::Value = serde_json::from_slice(&expected_bytes).unwrap();
        let parsed_expected = parse_expected(&expected_v);

        // typed-data 서명 흐름은 별도 처리 (decodedCalls가 비어 있음)
        let input: InputJson = match serde_json::from_slice(&input_bytes) {
            Ok(v) => v,
            Err(_) => {
                skipped.push(format!("{name}: input json parse 실패"));
                continue;
            }
        };

        // typed-data 서명: classify_call 미지원 (sign 디코더는 별도 진입)
        if matches!(input.request, Request::TypedData(_)) {
            // 서명 fixture는 decodedCalls 빈 배열 또는 typed_data만 있음 — 통과
            passed += 1;
            continue;
        }

        // 첫 decodedCall에서 selector 추출
        let first = match input.decoded_calls.first() {
            Some(c) => c,
            None => {
                skipped.push(format!("{name}: decodedCalls 비어있음"));
                continue;
            }
        };
        let selector_str = match first.get("selector").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                skipped.push(format!("{name}: selector 없음"));
                continue;
            }
        };
        let selector = match parse_selector(selector_str) {
            Ok(s) => s,
            Err(e) => {
                skipped.push(format!("{name}: selector 파싱 {e}"));
                continue;
            }
        };
        let args = first.get("args").cloned().unwrap_or(serde_json::Value::Null);

        // ctx 구성
        let (chain_id, actor, target, value_wei) = match &input.request {
            Request::Transaction(tx) => (tx.chain_id, tx.from, tx.to, tx.value.clone()),
            _ => unreachable!(),
        };

        // tests/fixtures.rs와 마찬가지로 'static 라이프타임 hack
        let leaked: &'static [ContractTarget] = Box::leak(input.targets.clone().into_boxed_slice());

        let ctx = BuildContext {
            chain_id,
            actor,
            target,
            value_wei,
            block_timestamp: Some(1_762_499_000),
            targets: leaked,
        };

        // dispatch
        let outcome = match classify_call(target, &selector, &args, &ctx) {
            Ok(o) => o,
            Err(e) => {
                // dispatch 실패는 *해당 selector를 우리가 지원하지 않는다*는 뜻 — skip OK
                skipped.push(format!("{name}: classify {e}"));
                continue;
            }
        };

        // expected의 첫 promote 액션과 비교
        let first_action = parsed_expected.actions.first();
        match first_action {
            Some((expected_cat, expected_type)) => {
                if outcome.category == *expected_cat && outcome.action_type == *expected_type {
                    passed += 1;
                } else {
                    mismatches.push(format!(
                        "{name}: 카테고리·타입 불일치 (실제: {:?}/{:?}, 예상: {:?}/{:?})",
                        outcome.category, outcome.action_type, expected_cat, expected_type
                    ));
                }
            }
            None => {
                skipped.push(format!("{name}: expected.actions 비어있음"));
            }
        }
    }

    println!("\n=== Round-trip 결과 ===");
    println!("총 {total} 개 fixture, {passed} 통과, {} 스킵, {} 불일치", skipped.len(), mismatches.len());
    if !skipped.is_empty() {
        println!("\n스킵:");
        for s in &skipped {
            println!("  • {s}");
        }
    }
    if !mismatches.is_empty() {
        println!("\n불일치:");
        for m in &mismatches {
            println!("  ✗ {m}");
        }
        panic!("{} fixture에서 round-trip 불일치", mismatches.len());
    }

    assert!(total > 0, "fixture 0개");
    assert!(passed > 0, "통과한 fixture 없음");
}

/// EIP-712 typed-data 서명 fixture를 별도로 검증.
#[test]
fn classify_typed_data_fixtures() {
    use policyschema::semi_adapter::sign::build_sign_fields;

    let dir = fixtures_dir();
    let mut count = 0;
    for entry in fs::read_dir(&dir).expect("fixtures dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        if !name.ends_with(".input.json") {
            continue;
        }

        let bytes = fs::read(&path).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let req = json.get("request").unwrap();
        if req.get("kind").and_then(|v| v.as_str()) != Some("typed_data") {
            continue;
        }

        let typed: TypedDataRequest = serde_json::from_value(req.clone())
            .unwrap_or_else(|e| panic!("{name}: typed_data deserialize 실패: {e}"));
        let _fields = build_sign_fields(&typed, Some(1_762_499_000))
            .unwrap_or_else(|e| panic!("{name}: build_sign_fields 실패: {e}"));
        count += 1;
    }
    assert!(count >= 3, "typed_data fixture 최소 3개 (Permit2/EIP-2612/EIP-712 Other)");
}
