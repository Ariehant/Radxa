//! Node kind `fetch`.

use crate::error::{NexusError, Result};
use crate::flow::engine::{single, NodeCtx, Outputs};
use crate::registry::nodes::{NodeKind, NodeRegistry};
use serde_json::{json, Value};

const MAX_DOCS: usize = 200;
const MAX_DOC_CHARS: usize = 12_000;

pub fn register(r: &mut NodeRegistry) {
    r.add(NodeKind {
        id: "fetch",
        label: "Fetch Data",
        description: "Load notes matching `config.source` (path or glob) as documents.",
        cacheable: false,
        run,
    });
}

/// `fetch`: notes matching `config.source` (a path or SQLite GLOB) → documents.
fn run(ctx: &NodeCtx) -> Result<Outputs> {
    let source = ctx.cfg_str("source").ok_or_else(|| NexusError::invalid("fetch: config.source is required"))?;
    let source = crate::fs::normalize_rel(&source);
    let paths: Vec<String> = if source.contains(['*', '?', '[']) {
        ctx.vault.index.flush()?;
        let c = ctx.vault.index.read()?;
        let mut stmt = c.prepare_cached("SELECT path FROM files WHERE path GLOB ?1 AND kind NOT IN ('attachment', 'canvas') ORDER BY path LIMIT ?2")?;
        let rows = stmt.query_map(rusqlite::params![source, MAX_DOCS as i64], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    } else {
        vec![source.clone()]
    };
    let mut docs = Vec::with_capacity(paths.len());
    for p in &paths {
        let text = ctx.vault.read_text(p)?;
        let (fm, body) = crate::index::parser::split_frontmatter(&text);
        let fm = fm.and_then(|y| crate::index::parser::parse_frontmatter(y).ok()).unwrap_or_default();
        let title = fm.get("title").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(|| p.rsplit('/').next().unwrap_or(p).trim_end_matches(".md").to_owned());
        docs.push(json!({ "path": p, "title": title, "content": body.trim().chars().take(MAX_DOC_CHARS).collect::<String>(), "frontmatter": fm }));
    }
    ctx.note(format!("fetched {} document(s) from `{source}`", docs.len()));
    Ok(single(ctx.out_port(), Value::Array(docs)))
}
