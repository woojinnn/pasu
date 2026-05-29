//! `v3-harness` — CLI for the v3 `ActionBody[]` decode harness.
//!
//! Subcommands (filled in Phase 4): `fuzz | corpus | coverage | replay |
//! import-dune`. This entrypoint is a thin shell over
//! [`policy_engine_integration_tests::harness`].

fn main() {
    eprintln!("v3-harness: subcommands fuzz | corpus | coverage | replay | import-dune (Phase 4)");
    std::process::exit(2);
}
