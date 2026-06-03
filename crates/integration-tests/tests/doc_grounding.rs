//! Doc-grounding gate — methodology docs must not point at vanished code trees.
//!
//! The onboarding framework docs (this crate's top-level `*.md`) embed concrete
//! repo paths (`crates/...`, `registryV2/...`). When the codebase moves or
//! renames a crate those refs rot silently: the entire `crates/simulation/*`
//! tree was renamed to `crates/policy-server/*`, leaving ~40 dead path refs that
//! an onboarding agent's very first `grep` would hit. This test catches that
//! class of drift.
//!
//! What it checks, and why it checks it this way:
//!   * Only `crates/` and `registryV2/` roots — `schema/` is dropped because the
//!     docs also use `schema/X` as prose ("schema/Cedar/lowering") and as a
//!     relative module ref (`schema/mod.rs` = `policy-engine/src/schema/`).
//!   * For a **file** token (known source extension) we assert its **parent
//!     directory** exists, not the file. How-to docs legitimately cite
//!     illustrative *new* files (`.../lending/flash_loan.rs`, an invented
//!     example); their containing dir is real, but a renamed/moved crate makes
//!     the whole dir vanish — which is exactly the rot we want to catch.
//!   * For a **directory** token we require >=3 segments (kills prose like
//!     `crates/packages`) and assert it exists.
//!   * Placeholders/globs (`<domain>`, `**`, `{a,b}`, `$x`), and generated /
//!     gitignored / run-created trees (`registryV2/index`, `target/`, `.env`,
//!     `node_modules`, `data/golden/`) are skipped.
//!   * A belt-and-suspenders substring guard fails on any `crates/simulation`
//!     mention (the specific retired tree), even in glob/placeholder form.
//!
//! Known limit: a single file renamed *within* a surviving directory is not
//! caught (the docs hedge file refs with "grep 재확인"). The crate/dir
//! move-or-rename class — the one that actually bit us — is.

use std::path::{Path, PathBuf};

const ROOTS: &[&str] = &["crates/", "registryV2/"];
const META: &[char] = &['<', '>', '*', '{', '}', '$', '(', ')', '…', '?'];
const EXCLUDE: &[&str] = &[
    "registryV2/index",
    "target/",
    ".env",
    "node_modules",
    "/.git",
    "data/golden/",
];
const SRC_EXT: &[&str] = &[
    ".rs",
    ".ts",
    ".tsx",
    ".mjs",
    ".cjs",
    ".json",
    ".sh",
    ".toml",
    ".cedarschema",
    ".md",
    ".lock",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root from CARGO_MANIFEST_DIR")
        .to_path_buf()
}

fn is_path_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':' | '@') || META.contains(&c)
}

/// Strip trailing markdown/sentence punctuation and a `:line(-range)` suffix.
fn normalize(tok: &str) -> &str {
    let t = tok.trim_end_matches(|c: char| {
        matches!(c, '.' | ',' | ';' | ')' | ']' | '`' | '"' | '·' | ':')
    });
    if let Some(i) = t.find(':') {
        if t[i + 1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
        {
            return &t[..i];
        }
    }
    t
}

#[test]
fn methodology_docs_reference_live_paths() {
    let root = repo_root();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut mds: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read crate dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    mds.sort();
    assert!(!mds.is_empty(), "no methodology *.md found in {dir:?}");

    let mut misses: Vec<String> = Vec::new();
    let mut sim_hits: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for md in &mds {
        let name = md.file_name().unwrap().to_string_lossy().into_owned();
        let content = std::fs::read_to_string(md).expect("read md");

        // (1) Specific regression guard: the retired crate tree must not reappear.
        if content.contains("crates/simulation") {
            sim_hits.push(name.clone());
        }

        // (2) General: referenced code trees must resolve.
        for raw in content.split(|c: char| !is_path_char(c)) {
            if raw.len() < 6 || !ROOTS.iter().any(|r| raw.starts_with(r)) {
                continue;
            }
            let tok = normalize(raw);
            if tok.contains(META) || EXCLUDE.iter().any(|e| tok.contains(e)) {
                continue;
            }

            // File token -> verify its parent dir; dir token (>=3 segments) -> itself.
            let target = if SRC_EXT.iter().any(|e| tok.ends_with(e)) {
                match tok.rsplit_once('/') {
                    Some((parent, _)) => parent,
                    None => continue,
                }
            } else if tok.split('/').filter(|s| !s.is_empty()).count() >= 3 {
                tok
            } else {
                continue;
            };

            checked += 1;
            if !root.join(target).exists() {
                misses.push(format!("  [{name}] {raw}  (resolves via: {target})"));
            }
        }
    }

    misses.sort();
    misses.dedup();
    assert!(
        checked > 20,
        "extracted only {checked} concrete paths — extractor likely regressed"
    );

    let mut err = String::new();
    if !sim_hits.is_empty() {
        err.push_str(&format!(
            "retired `crates/simulation/*` tree referenced in: {} \
             (renamed to crates/policy-server/*)\n",
            sim_hits.join(", ")
        ));
    }
    if !misses.is_empty() {
        err.push_str(&format!(
            "{} doc path(s) do not resolve under {}:\n{}\n",
            misses.len(),
            root.display(),
            misses.join("\n")
        ));
    }
    assert!(
        err.is_empty(),
        "\n{err}\n(checked {checked} concrete paths)"
    );
}
