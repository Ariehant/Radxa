use crate::error::NexusError;
use crate::rpc::Router;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct QueryParams {
    id: String,
    #[serde(default)]
    params: Value,
}

pub fn register(r: &mut Router) {
    // Views never store data: they are registered queries over the index.
    r.add("view.list", |_s, _p: Value| Ok(crate::registry::get().views.list().into_iter().cloned().collect::<Vec<_>>()));
    r.add("view.query", |s, p: QueryParams| {
        let v = s.vault()?;
        let def = crate::registry::get().views.get(&p.id).ok_or_else(|| NexusError::NotFound(format!("view {}", p.id)))?;
        (def.query)(&v, &p.params)
    });
}
