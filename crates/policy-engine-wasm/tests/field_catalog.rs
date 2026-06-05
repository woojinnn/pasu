use policy_engine_wasm::field_catalog::{build, FieldDto};

const SCHEMA: &str = r#"
namespace Demo {
  type Meta = { submittedAt: Long, kind: String };
  entity Wallet = { address: String };
  entity Protocol;
  action "Swap" appliesTo {
    principal: Wallet,
    resource: Protocol,
    context: { meta: Meta, slippageBp: Long, tokens: Set<String>, custom: { riskScore?: Long } },
  };
}
"#;

fn find<'a>(fields: &'a [FieldDto], path: &str) -> Option<&'a FieldDto> {
    fields.iter().find(|f| f.path == path)
}

#[test]
fn walks_context_to_typed_dotted_leaves() {
    let cat = build(SCHEMA).expect("build");
    let swap = cat.get("Swap").expect("Swap action present");

    // primitive leaf under context
    let slip = find(swap, "context.slippageBp").expect("slippageBp");
    assert_eq!(slip.cedar_type, "Long");
    assert_eq!(slip.field_kind, "primitive");
    assert_eq!(slip.source, "base");

    // nested record (common type resolved) → deep leaves
    assert_eq!(
        find(swap, "context.meta.submittedAt").unwrap().cedar_type,
        "Long"
    );
    assert_eq!(
        find(swap, "context.meta.kind").unwrap().cedar_type,
        "String"
    );

    // set leaf (collection; no descent into elements)
    let toks = find(swap, "context.tokens").expect("tokens");
    assert_eq!(toks.field_kind, "collection");
    assert!(toks.cedar_type.starts_with("Set<"));

    // custom subtree is tagged source=custom
    assert_eq!(
        find(swap, "context.custom.riskScore").unwrap().source,
        "custom"
    );

    // principal entity attribute
    assert_eq!(
        find(swap, "principal.address").unwrap().cedar_type,
        "String"
    );
}

#[test]
fn covers_all_cedar_type_forms() {
    // One action whose context exercises every value type form.
    let schema = r#"
    namespace T {
      entity P;
      action "All" appliesTo {
        principal: P, resource: P,
        context: {
          b: Bool, n: Long, s: String,
          setOfStr: Set<String>,
          rec: { inner: Long },
          dec: decimal, ip: ipaddr, dt: datetime, dur: duration,
        },
      };
    }"#;
    let cat = build(schema).expect("build");
    let f = cat.get("All").expect("All");
    let ty = |p: &str| {
        f.iter()
            .find(|x| x.path == p)
            .map(|x| (x.cedar_type.as_str(), x.field_kind.as_str()))
    };

    assert_eq!(ty("context.b"), Some(("Boolean", "primitive")));
    assert_eq!(ty("context.n"), Some(("Long", "primitive")));
    assert_eq!(ty("context.s"), Some(("String", "primitive")));
    assert_eq!(ty("context.setOfStr").map(|t| t.1), Some("collection"));
    assert_eq!(ty("context.rec.inner"), Some(("Long", "primitive")));
    for (p, name) in [
        ("context.dec", "decimal"),
        ("context.ip", "ipaddr"),
        ("context.dt", "datetime"),
        ("context.dur", "duration"),
    ] {
        assert_eq!(ty(p), Some((name, "extension")), "extension {name}");
    }
}

#[test]
fn resolves_named_custom_context_like_the_shipped_schema() {
    // Mirrors the real schema shape: `custom` is a NAMED common type (not an
    // inline record), so the resolved JSON references it as
    // `{"type":"Amm::SwapCustomContext"}` — the walker must resolve it against
    // commonTypes and recurse. Inline-record tests never exercise this ref path,
    // yet it is exactly how the shipped enriched schema carries custom fields.
    let schema = r#"
    namespace Amm {
      type SwapCustomContext = { totalInputUsd: Long, riskScore: Long };
      entity Wallet = { address: String };
      entity Protocol;
      action "Swap" appliesTo {
        principal: Wallet,
        resource: Protocol,
        context: { slippageBp: Long, custom: SwapCustomContext },
      };
    }"#;
    let cat = build(schema).expect("build");
    let swap = cat.get("Swap").expect("Swap present");

    // base field
    assert_eq!(find(swap, "context.slippageBp").unwrap().cedar_type, "Long");
    assert_eq!(find(swap, "context.slippageBp").unwrap().source, "base");

    // custom fields resolved THROUGH the named common-type reference
    let tiu = find(swap, "context.custom.totalInputUsd").expect("custom.totalInputUsd");
    assert_eq!(tiu.cedar_type, "Long");
    assert_eq!(tiu.source, "custom");
    assert_eq!(
        find(swap, "context.custom.riskScore").unwrap().source,
        "custom"
    );
}
