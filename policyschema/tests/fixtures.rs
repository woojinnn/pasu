//! 통합 테스트 — `examples/fixtures/NN.expected.json`이 모두 완전한
//! `NormalizedRequestV2`로 deserialize되어야 한다.

use std::fs;
use std::path::PathBuf;

use policyschema::NormalizedRequestV2;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("fixtures")
}

#[test]
fn all_expected_fixtures_deserialize() {
    let dir = fixtures_dir();
    let mut count = 0;
    let mut failed = Vec::new();
    for entry in fs::read_dir(&dir).expect("fixtures 디렉토리 read") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        if !name.ends_with(".expected.json") {
            continue;
        }
        count += 1;
        let bytes = fs::read(&path).expect("fixture read");
        match serde_json::from_slice::<NormalizedRequestV2>(&bytes) {
            Ok(_) => {}
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }
    assert!(count > 0, "expected.json fixture를 찾지 못함");
    assert!(
        failed.is_empty(),
        "fixture deserialize 실패 ({}/{}):\n  {}",
        failed.len(),
        count,
        failed.join("\n  ")
    );
}
