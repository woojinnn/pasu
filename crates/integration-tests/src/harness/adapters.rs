//! Local adapter loader.
//!
//! Reads the committed `registryV2/index/{by-callkey,by-typed-data}/*.json`
//! entries (each embeds a fully-resolved v3 bundle), installs every unique
//! bundle into the WASM v3 thread-local state via
//! [`declarative_install_v3_json`](policy_engine_wasm::declarative_install_v3_json),
//! and enumerates the routable surface: one [`RoutableCall`] per callkey and
//! one [`RoutableTypedData`] per typed-data key.
//!
//! Many callkeys share a single `bundle_id` (ERC-standard token
//! auto-enumerate), so installs are de-duplicated by `bundle_id` while the
//! routable surface is built per-callkey for honest coverage accounting.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;

/// The `emit.strategy` of a bundle — selects the WASM builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// One `ActionBody` from a flat templated body.
    SingleEmit,
    /// `Multicall` from an opcode stream (Uniswap Universal Router, V4).
    OpcodeStreamDispatch,
    /// `Multicall` from an array argument (Permit2 batch, Balancer batchSwap).
    ArrayEmit,
    /// A single tagged `ActionBody` from a `version‖tag‖abi.encode` byte string
    /// (HyperLiquid CoreWriter).
    TaggedDispatch,
    /// Any strategy the harness does not model.
    Other,
}

impl Strategy {
    fn parse(s: &str) -> Self {
        match s {
            "single_emit" => Self::SingleEmit,
            "opcode_stream_dispatch" => Self::OpcodeStreamDispatch,
            "array_emit" => Self::ArrayEmit,
            "tagged_dispatch" => Self::TaggedDispatch,
            _ => Self::Other,
        }
    }

    /// Stable label for reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SingleEmit => "single_emit",
            Self::OpcodeStreamDispatch => "opcode_stream_dispatch",
            Self::ArrayEmit => "array_emit",
            Self::TaggedDispatch => "tagged_dispatch",
            Self::Other => "other",
        }
    }
}

/// One ABI input parameter (recursive for tuples).
#[derive(Clone, Debug)]
pub struct AbiInput {
    /// Parameter name (may be empty for unnamed components).
    pub name: String,
    /// Canonical Solidity type string (e.g. `uint256`, `address[]`, `bytes`).
    pub ty: String,
    /// Tuple components, when `ty` is a tuple/tuple-array.
    pub components: Option<Vec<AbiInput>>,
}

/// A routable calldata adapter, derived from one `by-callkey` index entry.
#[derive(Clone, Debug)]
pub struct RoutableCall {
    /// EIP-155 chain id (parsed from the callkey filename).
    pub chain_id: u64,
    /// Target contract, lowercased `0x` + 40 hex.
    pub to: String,
    /// Function selector, lowercased `0x` + 8 hex.
    pub selector: String,
    /// Owning bundle id.
    pub bundle_id: String,
    /// Decode strategy.
    pub strategy: Strategy,
    /// Top-level ABI inputs of `abi_fragment.abi`.
    pub abi_inputs: Vec<AbiInput>,
    /// Full `emit` block (per-opcode / per-action / array source descriptors).
    pub emit: Value,
    /// Whether the bundle also carries `match.typed_data` (a sign-primary flow).
    /// Such entries are routed via the typed-data fuzzer, not the calldata one:
    /// their callkey selector is often a synthetic sentinel and their body uses
    /// named EIP-712 message paths that only resolve on the typed-data route.
    pub has_typed_data: bool,
    /// Index filename stem — coverage accounting + repro label.
    pub source_callkey: String,
}

/// A routable EIP-712 typed-data adapter, from one `by-typed-data` index entry.
#[derive(Clone, Debug)]
pub struct RoutableTypedData {
    /// EIP-155 chain id.
    pub chain_id: u64,
    /// `domain.verifyingContract`, lowercased.
    pub verifying_contract: String,
    /// EIP-712 `primaryType`.
    pub primary_type: String,
    /// Optional witness struct type (Permit2 witness orders).
    pub witness_type: Option<String>,
    /// `domain.name`, if present.
    pub domain_name: Option<String>,
    /// Owning bundle id.
    pub bundle_id: String,
    /// Decode strategy (only `single_emit`/`array_emit` are routable here).
    pub strategy: Strategy,
    /// `match.typed_data.types` (EIP-712 struct definitions).
    pub types: Value,
    /// `abi_fragment.abi` — drives the message → args wrap rule.
    pub abi: Value,
    /// Full `emit` block.
    pub emit: Value,
    /// Index filename stem.
    pub source_key: String,
}

/// The full routable surface plus install bookkeeping.
#[derive(Clone, Debug)]
pub struct RoutableSurface {
    /// One per `by-callkey` entry.
    pub calls: Vec<RoutableCall>,
    /// One per `by-typed-data` entry.
    pub typed: Vec<RoutableTypedData>,
    /// Distinct bundle ids installed into the WASM thread-local (~80).
    pub installed_bundle_ids: Vec<String>,
    /// Total `by-callkey` entries seen.
    pub total_callkeys: usize,
    /// Total `by-typed-data` entries seen.
    pub total_typed_keys: usize,
    /// Bundles that failed to install (`bundle_id` → error message).
    pub install_failures: Vec<(String, String)>,
}

/// Locate `registryV2/index` by walking up from this crate's manifest dir.
pub fn registry_index_root() -> Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut dir: &Path = manifest.as_path();
    loop {
        let candidate = dir.join("registryV2").join("index");
        if candidate.join("by-callkey").is_dir() {
            return Ok(candidate);
        }
        dir = dir.parent().ok_or_else(|| {
            anyhow!(
                "registryV2/index/by-callkey not found walking up from {}",
                manifest.display()
            )
        })?;
    }
}

/// Read a directory's `*.json` entries, sorted by filename for determinism.
fn sorted_json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("read_dir {}", dir.display()))?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    Ok(files)
}

fn parse_abi_input(v: &Value) -> AbiInput {
    AbiInput {
        name: v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        ty: v
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        components: v
            .get("components")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().map(parse_abi_input).collect()),
    }
}

fn parse_abi_inputs(bundle: &Value) -> Vec<AbiInput> {
    bundle
        .get("abi_fragment")
        .and_then(|f| f.get("abi"))
        .and_then(|a| a.get("inputs"))
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(parse_abi_input).collect())
        .unwrap_or_default()
}

fn strategy_of(bundle: &Value) -> Strategy {
    bundle
        .get("emit")
        .and_then(|e| e.get("strategy"))
        .and_then(Value::as_str)
        .map_or(Strategy::Other, Strategy::parse)
}

/// Parse `<chain>__<addr>__<selector>` from a callkey filename stem.
fn parse_callkey_stem(stem: &str) -> Option<(u64, String, String)> {
    let mut parts = stem.split("__");
    let chain = parts.next()?.parse::<u64>().ok()?;
    let to = parts.next()?.to_ascii_lowercase();
    let selector = parts.next()?.to_ascii_lowercase();
    if parts.next().is_some() {
        return None; // callkeys have exactly 3 components
    }
    Some((chain, to, selector))
}

/// Load every local index entry, install unique bundles into the WASM
/// thread-local, and build the routable surface.
///
/// **Must run on the same OS thread that subsequently routes** (the WASM v3
/// install state is thread-local).
pub fn load_and_install() -> Result<RoutableSurface> {
    let index_root = registry_index_root()?;
    let by_callkey = index_root.join("by-callkey");
    let by_typed = index_root.join("by-typed-data");

    // Pass 1: collect unique bundles (first-wins) for de-duplicated install.
    let callkey_files = sorted_json_files(&by_callkey)?;
    let mut bundles: BTreeMap<String, Value> = BTreeMap::new();
    let mut parsed_calls: Vec<(String, u64, String, String, Value)> = Vec::new();

    for path in &callkey_files {
        let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let entry: Value =
            serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
        let bundle = entry
            .get("bundle")
            .ok_or_else(|| anyhow!("no `bundle` field in {}", path.display()))?;
        let bundle_id = bundle
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("no `bundle.id` in {}", path.display()))?
            .to_owned();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("bad filename {}", path.display()))?;
        let Some((chain, to, selector)) = parse_callkey_stem(stem) else {
            bail!("callkey filename not <chain>__<addr>__<selector>: {stem}");
        };
        bundles
            .entry(bundle_id.clone())
            .or_insert_with(|| bundle.clone());
        parsed_calls.push((bundle_id, chain, to, selector, bundle.clone()));
    }

    // Pass 2: install each unique bundle exactly once.
    let mut installed_bundle_ids = Vec::new();
    let mut install_failures = Vec::new();
    for (bundle_id, bundle) in &bundles {
        let out = policy_engine_wasm::declarative_install_v3_json(bundle.to_string());
        let env: Value = serde_json::from_str(&out)
            .with_context(|| format!("install envelope parse for {bundle_id}"))?;
        if env.get("ok").and_then(Value::as_bool) == Some(true) {
            installed_bundle_ids.push(bundle_id.clone());
        } else {
            install_failures.push((
                bundle_id.clone(),
                env.get("error")
                    .map_or_else(|| "unknown".to_owned(), std::string::ToString::to_string),
            ));
        }
    }

    // Build the per-callkey routable surface.
    let total_callkeys = parsed_calls.len();
    let calls = parsed_calls
        .into_iter()
        .map(|(bundle_id, chain, to, selector, bundle)| {
            let emit = bundle.get("emit").cloned().unwrap_or(Value::Null);
            let has_typed_data = bundle
                .get("match")
                .and_then(|m| m.get("typed_data"))
                .is_some();
            let source_callkey = format!("{chain}__{to}__{selector}");
            RoutableCall {
                chain_id: chain,
                to,
                selector,
                bundle_id,
                strategy: strategy_of(&bundle),
                abi_inputs: parse_abi_inputs(&bundle),
                emit,
                has_typed_data,
                source_callkey,
            }
        })
        .collect();

    // Typed-data surface (bundles are already installed above when they also
    // appear under by-callkey; install any typed-only bundles here too).
    let mut typed = Vec::new();
    let mut total_typed_keys = 0;
    if by_typed.is_dir() {
        for path in sorted_json_files(&by_typed)? {
            total_typed_keys += 1;
            let raw =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let entry: Value =
                serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
            let Some(bundle) = entry.get("bundle") else {
                continue;
            };
            let bundle_id = bundle
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if !bundles.contains_key(&bundle_id) {
                let out = policy_engine_wasm::declarative_install_v3_json(bundle.to_string());
                let env: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
                if env.get("ok").and_then(Value::as_bool) == Some(true) {
                    installed_bundle_ids.push(bundle_id.clone());
                } else {
                    install_failures.push((
                        bundle_id.clone(),
                        env.get("error")
                            .map_or_else(|| "unknown".to_owned(), std::string::ToString::to_string),
                    ));
                }
            }
            let td = bundle.get("match").and_then(|m| m.get("typed_data"));
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            typed.push(RoutableTypedData {
                chain_id: td
                    .and_then(|t| t.get("chain_id"))
                    .and_then(Value::as_u64)
                    .or_else(|| stem.split("__").next().and_then(|c| c.parse().ok()))
                    .unwrap_or(0),
                verifying_contract: td
                    .and_then(|t| t.get("verifying_contract"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                primary_type: td
                    .and_then(|t| t.get("primary_type"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                witness_type: td
                    .and_then(|t| t.get("witness_type"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                domain_name: td
                    .and_then(|t| t.get("domain_name"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                bundle_id,
                strategy: strategy_of(bundle),
                types: td
                    .and_then(|t| t.get("types"))
                    .cloned()
                    .unwrap_or(Value::Null),
                abi: bundle
                    .get("abi_fragment")
                    .and_then(|f| f.get("abi"))
                    .cloned()
                    .unwrap_or(Value::Null),
                emit: bundle.get("emit").cloned().unwrap_or(Value::Null),
                source_key: stem.to_owned(),
            });
        }
    }

    installed_bundle_ids.sort();
    installed_bundle_ids.dedup();

    Ok(RoutableSurface {
        calls,
        typed,
        installed_bundle_ids,
        total_callkeys,
        total_typed_keys,
        install_failures,
    })
}
