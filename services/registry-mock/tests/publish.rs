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

#[tokio::test]
async fn fetches_published_wasm() {
    let server = common::TestServer::start().await;

    let manifest = serde_json::json!({
        "name": "bar",
        "version": "0.1.0",
        "sdk_version": 1,
        "description": "x",
        "capabilities": ["decoder"],
        "applies_to": [{"chain": 1, "address": "0x0000000000000000000000000000000000000002"}],
        "factory_of": [],
        "proxy_of": []
    });
    let wasm = b"\0asm\x01\0\0\0xxxxxxxx".to_vec();

    reqwest::Client::new()
        .post(format!("{}/publish", server.base_url))
        .multipart(
            reqwest::multipart::Form::new()
                .text("manifest", manifest.to_string())
                .part("wasm", reqwest::multipart::Part::bytes(wasm.clone())),
        )
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let got = reqwest::get(format!("{}/packages/bar/v0.1.0/adapter.wasm", server.base_url))
        .await
        .unwrap();
    assert_eq!(got.status(), 200);
    assert_eq!(
        got.headers().get("cache-control").unwrap(),
        "public, max-age=31536000, immutable"
    );
    let body = got.bytes().await.unwrap().to_vec();
    assert_eq!(body, wasm);

    let mj = reqwest::get(format!("{}/packages/bar/v0.1.0/manifest.json", server.base_url))
        .await
        .unwrap();
    assert_eq!(mj.status(), 200);
    let parsed: serde_json::Value = mj.json().await.unwrap();
    assert_eq!(parsed["name"], "bar");
}

#[tokio::test]
async fn chain_endpoint_resolves_explicit_address() {
    let server = common::TestServer::start().await;

    let manifest = serde_json::json!({
        "name": "baz",
        "version": "0.2.0",
        "sdk_version": 1,
        "description": "x",
        "capabilities": ["decoder"],
        "applies_to": [{"chain": 1, "address": "0x0000000000000000000000000000000000000099"}],
        "factory_of": [],
        "proxy_of": []
    });
    reqwest::Client::new()
        .post(format!("{}/publish", server.base_url))
        .multipart(
            reqwest::multipart::Form::new()
                .text("manifest", manifest.to_string())
                .part("wasm", reqwest::multipart::Part::bytes(b"\0asm\x01\0\0\0".to_vec())),
        )
        .send().await.unwrap().error_for_status().unwrap();

    let r = reqwest::get(format!(
        "{}/chains/1/0x0000000000000000000000000000000000000099",
        server.base_url
    ))
    .await
    .unwrap();
    assert_eq!(r.status(), 200);
    let v: serde_json::Value = r.json().await.unwrap();
    assert_eq!(v["version"], "0.2.0");
    assert_eq!(v["wasm_url"], "/packages/baz/v0.2.0/adapter.wasm");
}

#[tokio::test]
async fn chain_endpoint_returns_404_for_unknown() {
    let server = common::TestServer::start().await;
    let r = reqwest::get(format!(
        "{}/chains/1/0x0000000000000000000000000000000000000abc",
        server.base_url
    ))
    .await
    .unwrap();
    assert_eq!(r.status(), 404);
}
