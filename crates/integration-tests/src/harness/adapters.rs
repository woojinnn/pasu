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

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{Map, Value};

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
    /// `Arc`-shared across every callkey of the same bundle: the surface holds
    /// ~53k callkeys but only ~890 distinct bundles, so sharing the (often large)
    /// emit template keeps the cached surface small.
    pub emit: Arc<Value>,
    /// Whether the bundle also carries `match.typed_data` (a sign-primary flow).
    /// Such entries are routed via the typed-data fuzzer, not the calldata one:
    /// their callkey selector is often a synthetic sentinel and their body uses
    /// named EIP-712 message paths that only resolve on the typed-data route.
    pub has_typed_data: bool,
    /// Index filename stem — coverage accounting + repro label.
    pub source_callkey: String,
    /// Stable representative key for `3-ref` index entries. CI-safe author-time
    /// validation can check one callkey per source-generated bundle template
    /// instead of every materialized address.
    pub source_ref_key: Option<String>,
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

/// Loader controls for callers that need a bounded subset of the local surface.
#[derive(Clone, Debug)]
pub struct LoadOptions {
    /// Optional case-sensitive substring matched against callkey stems or bundle ids.
    pub filter: Option<String>,
    /// For `3-ref` source-generated surfaces, load one callkey per source bundle
    /// template instead of every materialized address.
    pub representative_source_refs: bool,
    /// Whether to include the typed-data index. Author-time calldata-only gates
    /// can skip it to avoid unrelated installs.
    pub include_typed_data: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            filter: None,
            representative_source_refs: false,
            include_typed_data: true,
        }
    }
}

impl LoadOptions {
    /// Build loader options from process environment.
    ///
    /// CI can keep source-generated protocol surfaces bounded by setting
    /// `PASU_V3_HARNESS_REPRESENTATIVE_SOURCE_REFS=1`. Local/manual runs
    /// stay exhaustive unless the variable is explicitly enabled.
    #[must_use]
    pub fn from_env() -> Self {
        let mut options = Self::default();
        if env_flag("PASU_V3_HARNESS_REPRESENTATIVE_SOURCE_REFS") {
            options.representative_source_refs = true;
        }
        options
    }

    fn matches_callkey_entry(&self, stem: &str, bundle_id: &str) -> bool {
        self.filter
            .as_deref()
            .is_none_or(|filter| stem.contains(filter) || bundle_id.contains(filter))
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            let value = value.trim();
            !value.is_empty()
                && !matches!(
                    value.to_ascii_lowercase().as_str(),
                    "0" | "false" | "off" | "no"
                )
        })
        .unwrap_or(false)
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

fn read_registry_json(registry_root: &Path, object_ref: &str) -> Result<Value> {
    let rel = object_ref.trim_start_matches('/');
    if rel.contains("..") {
        bail!("generated registry ref contains traversal: {object_ref}");
    }
    let path = registry_root.join(rel);
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

fn lookup_source_path(context: &Value, path: &str) -> Option<Value> {
    let mut current = context;
    for segment in path.split('.') {
        if let Some(array) = current.as_array() {
            let index = segment.parse::<usize>().ok()?;
            current = array.get(index)?;
        } else if let Some(object) = current.as_object() {
            current = object.get(segment)?;
        } else {
            return None;
        }
    }
    Some(current.clone())
}

fn substitute_source_placeholders(value: &Value, context: &Value) -> Result<Value> {
    match value {
        Value::String(s) if s.starts_with("$source.") => {
            lookup_source_path(context, s.trim_start_matches("$source."))
                .ok_or_else(|| anyhow!("unknown source placeholder {s:?}"))
        }
        Value::Array(items) => items
            .iter()
            .map(|item| substitute_source_placeholders(item, context))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        Value::Object(object) => {
            let mut out = Map::new();
            for (key, nested) in object {
                out.insert(
                    key.clone(),
                    substitute_source_placeholders(nested, context)?,
                );
            }
            Ok(Value::Object(out))
        }
        _ => Ok(value.clone()),
    }
}

fn sanitize_id_suffix(value: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = false;
    let mut last_was_slash = false;
    for ch in value.to_ascii_lowercase().chars() {
        let mapped = if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            ch
        } else if ch == '/' {
            '/'
        } else {
            '-'
        };
        if mapped == '-' {
            if last_was_dash {
                continue;
            }
            last_was_dash = true;
            last_was_slash = false;
        } else if mapped == '/' {
            if last_was_slash {
                continue;
            }
            last_was_slash = true;
            last_was_dash = false;
        } else {
            last_was_dash = false;
            last_was_slash = false;
        }
        out.push(mapped);
    }
    out.trim_matches('-').to_owned()
}

fn append_id_suffix(id: &str, suffix: &str) -> Result<String> {
    let clean = sanitize_id_suffix(suffix);
    if clean.is_empty() {
        bail!("source materialization produced empty id suffix for {id}");
    }
    if let Some(at) = id.rfind('@') {
        Ok(format!("{}/{clean}{}", &id[..at], &id[at..]))
    } else {
        Ok(format!("{id}/{clean}"))
    }
}

fn materialize_ref_bundle(registry_root: &Path, entry: &Value) -> Result<Value> {
    let bundle_ref = entry
        .get("bundle_ref")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("ref entry missing bundle_ref"))?;
    let template = read_registry_json(registry_root, bundle_ref)?;
    let Some(context_ref) = entry.get("context_ref").and_then(Value::as_str) else {
        return Ok(template);
    };
    let context_doc = read_registry_json(registry_root, context_ref)?;
    let context = context_doc
        .get("context")
        .ok_or_else(|| anyhow!("source context document missing context"))?;
    let substituted = substitute_source_placeholders(&template, context)?;
    let object = substituted
        .as_object()
        .ok_or_else(|| anyhow!("source-substituted bundle is not an object"))?;
    let selector = object
        .get("match")
        .and_then(|m| m.get("selector"))
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("source-substituted bundle missing match.selector"))?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("source-substituted bundle missing id"))?;
    let id_suffix = context
        .get("id_suffix")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("source context missing id_suffix"))?;
    let chain_id = context_doc
        .get("chain_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("source context missing chain_id"))?;
    let address = context_doc
        .get("address")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("source context missing address"))?
        .to_ascii_lowercase();

    let mut materialized = object.clone();
    materialized.remove("source_materialize");
    materialized.insert(
        "id".to_owned(),
        Value::String(append_id_suffix(id, id_suffix)?),
    );
    let mut chain_map = Map::new();
    chain_map.insert(
        chain_id.to_string(),
        Value::Array(vec![Value::String(address)]),
    );
    materialized.insert(
        "match".to_owned(),
        serde_json::json!({
            "selector": selector,
            "chain_to_addresses": Value::Object(chain_map),
        }),
    );
    Ok(Value::Object(materialized))
}

fn source_ref_key(entry: &Value) -> Option<String> {
    if entry.get("schema_version").and_then(Value::as_str) != Some("3-ref") {
        return None;
    }
    let bundle_ref = entry.get("bundle_ref").and_then(Value::as_str)?;
    let source = entry
        .get("materialization")
        .and_then(|m| m.get("source"))
        .and_then(Value::as_str)
        .unwrap_or("resolved-source");
    Some(format!("{source}|{bundle_ref}"))
}

fn bundle_from_index_entry(index_root: &Path, entry: &Value, path: &Path) -> Result<Value> {
    if let Some(bundle) = entry.get("bundle") {
        return Ok(bundle.clone());
    }
    if entry.get("schema_version").and_then(Value::as_str) == Some("3-ref") {
        let registry_root = index_root
            .parent()
            .ok_or_else(|| anyhow!("registry index root has no parent"))?;
        return materialize_ref_bundle(registry_root, entry);
    }
    bail!("no `bundle` field in {}", path.display())
}

// ───────────────────────────────────────────────────────────────────────────
// Surface cache — build once, share an `Arc`
// ───────────────────────────────────────────────────────────────────────────
//
// Building the routable surface reads the whole committed index (~53k
// `by-callkey` files) and is byte-identical across every test. libtest runs each
// `#[test]` on its own thread, so the previous "rebuild on every call" made up to
// ~`num_cpus` of these multi-GB builds run concurrently → ~49 GB RSS. We now
// build once per `LoadOptions`, cache an `Arc<RoutableSurface>`, and only replay
// the (cheap, pre-serialized) bundle install into each thread's WASM thread-local.

/// Cache key = the `LoadOptions` fields that change the built surface.
type LoadKey = (Option<String>, bool, bool);

#[derive(Clone)]
struct CachedSurface {
    surface: Arc<RoutableSurface>,
    /// `(bundle_id, bundle_json)` for every unique bundle, replayed into each
    /// thread's `DECLARATIVE_V3_STATE`. libtest gives every test a fresh thread,
    /// so the thread-local install can't be shared — but this is ~890 cheap
    /// inserts, next to the file load which is now done once and cached.
    install_list: Arc<Vec<(String, String)>>,
}

static SURFACE_CACHE: OnceLock<Mutex<HashMap<LoadKey, CachedSurface>>> = OnceLock::new();

thread_local! {
    /// Load keys whose bundles are already installed into THIS thread's WASM
    /// thread-local, so repeated calls on one thread don't reinstall.
    static INSTALLED_KEYS: RefCell<HashSet<LoadKey>> = RefCell::new(HashSet::new());
}

fn load_key(options: &LoadOptions) -> LoadKey {
    (
        options.filter.clone(),
        options.representative_source_refs,
        options.include_typed_data,
    )
}

/// Load + install the full local surface with default options.
pub fn load_and_install() -> Result<Arc<RoutableSurface>> {
    load_and_install_with_options(LoadOptions::from_env())
}

/// Load + install a filtered/representative surface subset.
///
/// The expensive surface build is memoized process-wide. The first caller builds
/// it (holding the cache lock so concurrent first-callers serialize on one build
/// rather than each allocating a multi-GB copy); every later call — and every
/// other test thread — gets the cached `Arc` and only replays the bundle install
/// into its own WASM thread-local.
pub fn load_and_install_with_options(options: LoadOptions) -> Result<Arc<RoutableSurface>> {
    let key = load_key(&options);
    let cache = SURFACE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let mut built_here = false;
    let cached = {
        // Recover the guard even if a prior build panicked while holding it — the
        // cached data is still valid, and a poisoned harness mutex should not turn
        // every later test into a panic.
        let mut guard = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) = guard.get(&key) {
            cached.clone()
        } else {
            let cached = build_surface(options)?;
            guard.insert(key.clone(), cached.clone());
            built_here = true;
            cached
        }
    };

    // `build_surface` already installed into the building thread; any *other*
    // thread that gets a cache hit must replay the install into its own
    // thread-local once (libtest spawns a fresh thread per test).
    INSTALLED_KEYS.with(|installed| {
        let first_on_thread = installed.borrow_mut().insert(key.clone());
        if first_on_thread && !built_here {
            replay_install(&cached.install_list);
        }
    });

    Ok(cached.surface)
}

/// (Re)install every cached bundle into the calling thread's `DECLARATIVE_V3_STATE`.
/// Installs are idempotent (replace by `bundle_id`); failures were already
/// recorded on the building thread, so they are ignored here.
fn replay_install(install_list: &[(String, String)]) {
    for (_bundle_id, bundle_json) in install_list {
        let _ = policy_engine_wasm::declarative_install_v3_json(bundle_json.clone());
    }
}

/// Per-bundle decode fields, shared by `Arc`/clone across every callkey of that
/// bundle so the surface does not carry one copy per materialized callkey.
struct BundleFields {
    strategy: Strategy,
    abi_inputs: Vec<AbiInput>,
    emit: Arc<Value>,
    has_typed_data: bool,
}

/// Read the local index and build the routable surface once, installing every
/// unique bundle into the calling thread's WASM thread-local.
fn build_surface(options: LoadOptions) -> Result<CachedSurface> {
    let index_root = registry_index_root()?;
    let by_callkey = index_root.join("by-callkey");
    let by_typed = index_root.join("by-typed-data");

    // Pass 1: collect unique bundles (first-wins) + per-callkey coordinates.
    // Only the `bundle_id` is kept per callkey (NOT a full bundle clone — that
    // clone over ~53k callkeys was the dominant allocation); the per-bundle
    // decode fields are built once below and shared.
    let callkey_files = sorted_json_files(&by_callkey)?;
    let mut bundles: BTreeMap<String, Value> = BTreeMap::new();
    let mut parsed_calls: Vec<(String, u64, String, String, Option<String>)> = Vec::new();
    let mut seen_source_ref_keys = HashSet::new();

    for path in &callkey_files {
        let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let entry: Value =
            serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("bad filename {}", path.display()))?;
        let entry_bundle_id = entry
            .get("bundle_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !options.matches_callkey_entry(stem, entry_bundle_id) {
            continue;
        }
        let source_ref_key = source_ref_key(&entry);
        if options.representative_source_refs {
            if let Some(key) = &source_ref_key {
                if !seen_source_ref_keys.insert(key.clone()) {
                    continue;
                }
            }
        }
        let bundle = bundle_from_index_entry(&index_root, &entry, path)?;
        let bundle_id = bundle
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("no `bundle.id` in {}", path.display()))?
            .to_owned();
        let Some((chain, to, selector)) = parse_callkey_stem(stem) else {
            bail!("callkey filename not <chain>__<addr>__<selector>: {stem}");
        };
        bundles.entry(bundle_id.clone()).or_insert(bundle);
        parsed_calls.push((bundle_id, chain, to, selector, source_ref_key));
    }

    // Typed-data surface — collect entries and fold any typed-only bundles into
    // the unique-bundle set so they are installed (and replayed) too.
    let mut typed = Vec::new();
    let mut total_typed_keys = 0;
    if options.include_typed_data && by_typed.is_dir() {
        for path in sorted_json_files(&by_typed)? {
            total_typed_keys += 1;
            let raw =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let entry: Value =
                serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
            let bundle = bundle_from_index_entry(&index_root, &entry, &path)?;
            let bundle_id = bundle
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
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
                bundle_id: bundle_id.clone(),
                strategy: strategy_of(&bundle),
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
            bundles.entry(bundle_id).or_insert(bundle);
        }
    }

    // by-selector surface — address-agnostic (selector-only) bundles (standard
    // NFT setApprovalForAll). They carry NO per-address callkey, so fold them
    // into the unique-bundle set here purely so they get installed (registering
    // the WASM `selector_bridge`) and replayed onto every thread. The corpus/
    // route path reaches them via the route's selector-only fallback after a
    // per-address miss — no entry in the per-callkey routable surface is needed.
    let by_selector = index_root.join("by-selector");
    if by_selector.is_dir() {
        for path in sorted_json_files(&by_selector)? {
            let raw =
                fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            let entry: Value =
                serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
            let bundle = bundle_from_index_entry(&index_root, &entry, &path)?;
            let bundle_id = bundle
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            bundles.entry(bundle_id).or_insert(bundle);
        }
    }

    // Build per-bundle decode fields once (shared by every callkey of the bundle).
    let bundle_fields: HashMap<String, BundleFields> = bundles
        .iter()
        .map(|(id, bundle)| {
            (
                id.clone(),
                BundleFields {
                    strategy: strategy_of(bundle),
                    abi_inputs: parse_abi_inputs(bundle),
                    emit: Arc::new(bundle.get("emit").cloned().unwrap_or(Value::Null)),
                    has_typed_data: bundle
                        .get("match")
                        .and_then(|m| m.get("typed_data"))
                        .is_some(),
                },
            )
        })
        .collect();

    // Install every unique bundle into this (building) thread, recording the
    // install list (for per-thread replay) and the install outcomes.
    let mut installed_bundle_ids = Vec::new();
    let mut install_failures = Vec::new();
    let mut install_list: Vec<(String, String)> = Vec::with_capacity(bundles.len());
    for (bundle_id, bundle) in &bundles {
        let json = bundle.to_string();
        let out = policy_engine_wasm::declarative_install_v3_json(json.clone());
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
        install_list.push((bundle_id.clone(), json));
    }

    // Build the per-callkey routable surface, sharing each bundle's decode fields.
    let total_callkeys = parsed_calls.len();
    let calls = parsed_calls
        .into_iter()
        .map(|(bundle_id, chain, to, selector, source_ref_key)| {
            // Every callkey's bundle_id came from `bundles`, which `bundle_fields`
            // is built from, so the lookup cannot miss — propagate instead of
            // panicking if that invariant is ever broken.
            let fields = bundle_fields.get(&bundle_id).ok_or_else(|| {
                anyhow!("callkey bundle_id {bundle_id} missing from bundle_fields")
            })?;
            let source_callkey = format!("{chain}__{to}__{selector}");
            Ok(RoutableCall {
                chain_id: chain,
                to,
                selector,
                bundle_id,
                strategy: fields.strategy,
                abi_inputs: fields.abi_inputs.clone(),
                emit: Arc::clone(&fields.emit),
                has_typed_data: fields.has_typed_data,
                source_callkey,
                source_ref_key,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    installed_bundle_ids.sort();
    installed_bundle_ids.dedup();

    let surface = RoutableSurface {
        calls,
        typed,
        installed_bundle_ids,
        total_callkeys,
        total_typed_keys,
        install_failures,
    };

    Ok(CachedSurface {
        surface: Arc::new(surface),
        install_list: Arc::new(install_list),
    })
}
