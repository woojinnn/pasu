use crate::action::dex::PoolRef;
use crate::context_keys::{ADDRESS, ID, LABEL};
use serde_json::{Map, Value};

pub(crate) fn pool_json(pool: &PoolRef) -> Value {
    let mut out = Map::new();
    if let Some(address) = &pool.address {
        out.insert(ADDRESS.into(), Value::from(address.to_string()));
    }
    if let Some(id) = &pool.id {
        out.insert(ID.into(), Value::from(id.as_str()));
    }
    if let Some(label) = &pool.label {
        out.insert(LABEL.into(), Value::from(label.as_str()));
    }
    Value::Object(out)
}
