//! Node kind `query_graph`.

use crate::error::{NexusError, Result};
use crate::flow::engine::{single, NodeCtx, Outputs};
use crate::registry::nodes::{NodeKind, NodeRegistry};
use crate::index::{query, sql};
use serde_json::{json, Value};

pub fn register(r: &mut NodeRegistry) {
    r.add(NodeKind {
        id: "query_graph",
        label: "Query Graph",
        description: "Backlinks of `config.target` or FTS `config.search` as note refs.",
        cacheable: false,
        run,
    });
}

/// `query_graph`: backlinks of `config.target`, or FTS `config.search` → note refs.
fn run(ctx: &NodeCtx) -> Result<Outputs> {
    let c = ctx.vault.index.read()?;
    let refs: Vec<Value> = if let Some(search) = ctx.cfg_str("search") {
        drop(c);
        ctx.vault.index.flush_fts()?;
        let c = ctx.vault.index.read()?;
        query::search(&c, &search, 50).map_err(sql)?.into_iter().map(|h| json!({ "path": h.path, "title": h.title })).collect()
    } else {
        let target = ctx.cfg_str("target").ok_or_else(|| NexusError::invalid("query_graph: set config.target or config.search"))?;
        let path = query::resolve(&c, &target).map_err(sql)?.ok_or_else(|| NexusError::NotFound(target.clone()))?;
        query::backlinks(&c, &path).map_err(sql)?.into_iter().map(|b| json!({ "path": b.path, "title": b.title })).collect()
    };
    ctx.note(format!("{} note(s)", refs.len()));
    Ok(single(ctx.out_port(), Value::Array(refs)))
}
