mod common;

#[tokio::test]
async fn publish_creates_files() {
    let server = common::TestServer::start().await;

    let manifest = serde_json::json!({
        "name": "foo",
        "version": "0.0.1",
        "sdk_version": 1,
        "description": "test",
        "capabilities": ["decoder"],
        "applies_to": [{"chain": 1, "address": "0x0000000000000000000000000000000000000001"}],
        "factory_of": [],
        "proxy_of": []
    });
    let wasm = vec![0u8, 0x61, 0x73, 0x6d, 1, 0, 0, 0];

    let res = reqwest::Client::new()
        .post(format!("{}/publish", server.base_url))
        .multipart(
            reqwest::multipart::Form::new()
                .text("manifest", manifest.to_string())
                .part("wasm", reqwest::multipart::Part::bytes(wasm)),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "foo");
    assert_eq!(body["version"], "0.0.1");
}
