use policy_engine::host::oracle::SnapshotOracle;
use policy_engine::{
    Address, Eip712TypedData, HostCapabilities, Pipeline, PolicyEngine, Request, SignatureRequest,
    TransactionRequest,
};
use policy_engine_adapters_bundle::{default_registry, default_signature_registry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::PathBuf;

const CURRENT_API: &str = "policy_engine::Pipeline::build_action_for(&policy_engine::Request) -> Result<policy_engine::LegacyAction, policy_engine::PipelineError>";

#[derive(Debug, Deserialize, Serialize)]
struct Fixture {
    label: String,
    rpc: Rpc,
    chain_id: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct Rpc {
    method: String,
    params: Value,
}

#[derive(Debug, Serialize)]
struct BaselineOutput {
    label: String,
    chain_id: u64,
    rpc: Rpc,
    current_api: &'static str,
    request: Value,
    result: CaptureResult,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum CaptureResult {
    Ok { action: Value },
    Error { error: String },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let (input_path, output_path) = parse_args()?;
    let raw = fs::read_to_string(&input_path)
        .map_err(|error| format!("failed to read {}: {error}", input_path.display()))?;
    let fixture: Fixture = serde_json::from_str(&raw)
        .map_err(|error| format!("failed to parse {}: {error}", input_path.display()))?;

    let request = request_from_fixture(&fixture)?;
    let request_json = serde_json::to_value(&request)
        .map_err(|error| format!("failed to serialize request: {error}"))?;

    let registry = default_registry();
    let signature_registry = default_signature_registry();
    let oracle = SnapshotOracle::new();
    let policies = PolicyEngine::builder()
        .build()
        .map_err(|error| format!("failed to build empty policy engine: {error}"))?;
    let host = HostCapabilities::new(&oracle);
    let pipeline =
        Pipeline::new(&registry, host, &policies).with_signature_registry(&signature_registry);

    let result = match pipeline.build_action_for(&request) {
        Ok(action) => CaptureResult::Ok {
            action: serde_json::to_value(action)
                .map_err(|error| format!("failed to serialize action: {error}"))?,
        },
        Err(error) => CaptureResult::Error {
            error: error.to_string(),
        },
    };

    let output = BaselineOutput {
        label: fixture.label,
        chain_id: fixture.chain_id,
        rpc: fixture.rpc,
        current_api: CURRENT_API,
        request: request_json,
        result,
    };
    let pretty = serde_json::to_string_pretty(&output)
        .map_err(|error| format!("failed to serialize output: {error}"))?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(&output_path, format!("{pretty}\n"))
        .map_err(|error| format!("failed to write {}: {error}", output_path.display()))?;

    Ok(())
}

fn parse_args() -> Result<(PathBuf, PathBuf), String> {
    let mut args = env::args().skip(1);
    let mut input = None;
    let mut output = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => {
                input = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--input requires a path".to_string())?,
                ));
            }
            "--output" => {
                output = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--output requires a path".to_string())?,
                ));
            }
            "-h" | "--help" => return Err(usage()),
            other => return Err(format!("unexpected argument `{other}`\n{}", usage())),
        }
    }

    let input = input.ok_or_else(usage)?;
    let output = output.ok_or_else(usage)?;
    Ok((input, output))
}

fn usage() -> String {
    "usage: capture_baseline --input <path> --output <path>".to_string()
}

fn request_from_fixture(fixture: &Fixture) -> Result<Request, String> {
    match fixture.rpc.method.as_str() {
        "eth_sendTransaction" => tx_request_from_params(&fixture.rpc.params, fixture.chain_id),
        "eth_signTypedData_v4" => sig_request_from_params(&fixture.rpc.params, fixture.chain_id),
        "personal_sign" | "eth_sign" => Err(format!(
            "{} is parsed by sign-resolver today, but policy_engine::Request has no raw-message variant",
            fixture.rpc.method
        )),
        method => Err(format!("unsupported fixture RPC method `{method}`")),
    }
}

fn tx_request_from_params(params: &Value, fallback_chain_id: u64) -> Result<Request, String> {
    let tx = params
        .as_array()
        .and_then(|items| items.first())
        .ok_or_else(|| {
            "eth_sendTransaction params must contain a transaction object".to_string()
        })?;

    let chain_id = tx
        .get("chainId")
        .and_then(chain_id_from_value)
        .unwrap_or(fallback_chain_id);
    let from = string_field(tx, "from").and_then(|raw| Address::new(&raw))?;
    let to = string_field(tx, "to").and_then(|raw| Address::new(&raw))?;
    let value_wei = tx
        .get("value")
        .and_then(Value::as_str)
        .map(value_to_decimal)
        .transpose()?
        .unwrap_or_else(|| "0".to_string());
    let data_hex = tx
        .get("data")
        .or_else(|| tx.get("input"))
        .and_then(Value::as_str)
        .unwrap_or("0x");
    let data = hex_to_bytes(data_hex)?;
    let gas = tx.get("gas").and_then(u64_from_value);
    let nonce = tx.get("nonce").and_then(u64_from_value);

    Ok(Request::Tx(TransactionRequest {
        chain_id,
        from,
        to,
        value_wei,
        data,
        gas,
        nonce,
    }))
}

fn sig_request_from_params(params: &Value, fallback_chain_id: u64) -> Result<Request, String> {
    let items = params
        .as_array()
        .ok_or_else(|| "eth_signTypedData_v4 params must be an array".to_string())?;
    let signer = items
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| "eth_signTypedData_v4 params[0] must be signer address".to_string())
        .and_then(Address::new)?;
    let typed_data_value = items
        .get(1)
        .ok_or_else(|| "eth_signTypedData_v4 params[1] must be typed data".to_string())?;
    let typed_data_value = if let Some(raw) = typed_data_value.as_str() {
        serde_json::from_str(raw)
            .map_err(|error| format!("typed data string is not valid JSON: {error}"))?
    } else {
        typed_data_value.clone()
    };
    let typed_data: Eip712TypedData = serde_json::from_value(typed_data_value)
        .map_err(|error| format!("typed data does not match policy_engine shape: {error}"))?;

    let chain_id = if typed_data.domain.chain_id == 0 {
        fallback_chain_id
    } else {
        typed_data.domain.chain_id
    };

    Ok(Request::Sig(SignatureRequest {
        chain_id,
        signer,
        typed_data,
    }))
}

fn string_field(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("missing string field `{field}`"))
}

fn hex_to_bytes(raw: &str) -> Result<Vec<u8>, String> {
    let clean = strip_0x(raw);
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    hex::decode(clean).map_err(|error| format!("invalid hex bytes `{raw}`: {error}"))
}

fn value_to_decimal(raw: &str) -> Result<String, String> {
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u128::from_str_radix(hex, 16)
            .map(|value| value.to_string())
            .map_err(|error| format!("invalid hex value `{raw}`: {error}"))
    } else {
        raw.parse::<u128>()
            .map(|value| value.to_string())
            .map_err(|error| format!("invalid decimal value `{raw}`: {error}"))
    }
}

fn chain_id_from_value(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(u64_from_str))
}

fn u64_from_value(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(u64_from_str))
}

fn u64_from_str(raw: &str) -> Option<u64> {
    if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        raw.parse::<u64>().ok()
    }
}

fn strip_0x(raw: &str) -> &str {
    raw.strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw)
}
