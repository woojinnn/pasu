//! [`ActionMeta`] / [`ActionNature`] / [`Eip712Domain`] lowering.
//!
//! Every per-action `*Context` type embeds a `meta: Core::ActionMeta`, so this
//! lowering is shared by all domains (reached via [`LowerCtx::meta`]).
//!
//! [`LowerCtx::meta`]: super::super::dispatch::LowerCtx::meta

use serde_json::{Map, Value};

use simulation_reducer::action::{ActionMeta, ActionNature, Eip712Domain};

use super::cedar::{addr, u256_hex};

/// Lower an [`ActionMeta`] → `{ submittedAt, submitter, nature }`
/// (`Core::ActionMeta`).
pub(crate) fn lower_action_meta(meta: &ActionMeta) -> Value {
    let mut m = Map::new();
    // `submittedAt` is a unix-seconds Long (JSON number).
    m.insert(
        "submittedAt".into(),
        Value::from(meta.submitted_at.as_unix()),
    );
    m.insert("submitter".into(), Value::String(addr(&meta.submitter)));
    m.insert("nature".into(), lower_nature(&meta.nature));
    Value::Object(m)
}

/// Lower an [`ActionNature`] → discriminated `{ kind, … }` (`Core::ActionNature`).
pub(crate) fn lower_nature(nature: &ActionNature) -> Value {
    let mut m = Map::new();
    match nature {
        ActionNature::OnchainTx {
            chain,
            nonce,
            gas_limit,
            gas_price,
            value,
        } => {
            m.insert("kind".into(), Value::String("onchain_tx".into()));
            m.insert("chain".into(), Value::String(chain.to_string()));
            m.insert("nonce".into(), Value::from(*nonce));
            m.insert("gasLimit".into(), Value::String(u256_hex(*gas_limit)));
            // gas_price is a LiveField<U256>: inline its inner value.
            m.insert("gasPrice".into(), Value::String(u256_hex(gas_price.value)));
            m.insert("value".into(), Value::String(u256_hex(*value)));
        }
        ActionNature::OffchainSig {
            domain,
            deadline,
            nonce_key,
        } => {
            m.insert("kind".into(), Value::String("offchain_sig".into()));
            m.insert("domain".into(), lower_eip712(domain));
            m.insert("deadline".into(), Value::from(deadline.as_unix()));
            if let Some(nonce_key) = nonce_key {
                // `nonceKey` is a String slot for an app-specific key; the Rust
                // `NonceKey` is an enum, so serialize it and stringify (compact
                // JSON) into the slot. Omitted entirely when absent.
                if let Ok(serialized) = serde_json::to_string(nonce_key) {
                    m.insert("nonceKey".into(), Value::String(serialized));
                }
            }
        }
    }
    Value::Object(m)
}

/// Lower an [`Eip712Domain`] → `{ name, version?, chainId?, verifyingContract?,
/// salt? }` (`Core::Eip712Domain`). Absent optionals are omitted.
pub(crate) fn lower_eip712(domain: &Eip712Domain) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String(domain.name.clone()));
    if let Some(version) = &domain.version {
        m.insert("version".into(), Value::String(version.clone()));
    }
    if let Some(chain_id) = domain.chain_id {
        m.insert("chainId".into(), Value::from(chain_id));
    }
    if let Some(verifying_contract) = &domain.verifying_contract {
        m.insert(
            "verifyingContract".into(),
            Value::String(addr(verifying_contract)),
        );
    }
    if let Some(salt) = &domain.salt {
        m.insert("salt".into(), Value::String(salt.clone()));
    }
    Value::Object(m)
}
