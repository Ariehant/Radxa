//! Vault tools an agent node may call. Every tool goes through the vault
//! (so writes are atomic, indexed and git-committed like any other save).

use super::provider::ToolSpec;
use crate::error::{NexusError, Result};
use crate::index::{query, sql};
use crate::vault::Vault;
use parking_lot::Mutex;
use serde_json::{json, Value};

pub const MAX_NOTE_CHARS: usize = 20_000;

pub struct ToolCtx<'a> {
    pub vault: &'a Vault,
    /// Provenance stamped into notes an agent creates.
    pub flow: &'a str,
    pub node: &'a str,
    pub run: &'a str,
    /// Paths created during this run (linked from the run log).
    pub created: Mutex<Vec<String>>,
}

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn call(&self, ctx: &ToolCtx, args: &Value) -> Result<Value>;
}

fn arg<'v>(args: &'v Value, k: &str) -> Option<&'v str> {
    args.get(k).and_then(Value::as_str).filter(|s| !s.trim().is_empty())
}

/// `path` or a wikilink-style `title` → vault path.
fn locate(ctx: &ToolCtx, args: &Value) -> Result<String> {
    if let Some(p) = arg(args, "path") {
        return Ok(crate::fs::normalize_rel(p));
    }
    let target = arg(args, "title").or_else(|| arg(args, "note")).ok_or_else(|| NexusError::invalid("give `path` or `title`"))?;
    let c = ctx.vault.index.read()?;
    query::resolve(&c, target).map_err(sql)?.ok_or_else(|| NexusError::NotFound(target.to_owned()))
}

pub struct ReadNote;
impl Tool for ReadNote {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "read_note".into(),
            description: "Read a note from the knowledge base by vault path (e.g. notes/a.md) or by title.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Vault-relative path" },
                    "title": { "type": "string", "description": "Note title or wikilink target" }
                }
            }),
        }
    }
    fn call(&self, ctx: &ToolCtx, args: &Value) -> Result<Value> {
        let path = locate(ctx, args)?;
        let text = ctx.vault.read_text(&path)?;
        let truncated = text.chars().count() > MAX_NOTE_CHARS;
        let content: String = text.chars().take(MAX_NOTE_CHARS).collect();
        Ok(json!({ "path": path, "content": content, "truncated": truncated }))
    }
}

pub struct QueryGraph;
impl Tool for QueryGraph {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "query_graph".into(),
            description: "Query the knowledge graph: notes linking to a note (`backlinks_of`), links out of a note (`links_of`), or full-text `search`.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "backlinks_of": { "type": "string" },
                    "links_of": { "type": "string" },
                    "search": { "type": "string" },
                    "limit": { "type": "integer" }
                }
            }),
        }
    }
    fn call(&self, ctx: &ToolCtx, args: &Value) -> Result<Value> {
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20).min(100) as usize;
        if let Some(t) = arg(args, "backlinks_of") {
            let path = locate(ctx, &json!({ "title": t, "path": if t.ends_with(".md") { t } else { "" } }))?;
            let c = ctx.vault.index.read()?;
            let mut v = query::backlinks(&c, &path).map_err(sql)?;
            v.truncate(limit);
            return Ok(json!({ "note": path, "backlinks": v }));
        }
        if let Some(t) = arg(args, "links_of") {
            let path = locate(ctx, &json!({ "title": t, "path": if t.ends_with(".md") { t } else { "" } }))?;
            let c = ctx.vault.index.read()?;
            let mut v = query::outlinks(&c, &path).map_err(sql)?;
            v.truncate(limit);
            return Ok(json!({ "note": path, "links": v }));
        }
        if let Some(q) = arg(args, "search") {
            ctx.vault.index.flush_fts()?;
            let c = ctx.vault.index.read()?;
            return Ok(json!({ "query": q, "hits": query::search(&c, q, limit).map_err(sql)? }));
        }
        Err(NexusError::invalid("give one of backlinks_of, links_of, search"))
    }
}

pub struct CreateNote;
impl Tool for CreateNote {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: "create_note".into(),
            description: "Create a new Markdown note in the knowledge base. Use [[wikilinks]] to connect it to existing notes.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "body": { "type": "string", "description": "Markdown body" },
                    "dir": { "type": "string", "description": "Folder, default notes/generated" }
                },
                "required": ["title", "body"]
            }),
        }
    }
    fn call(&self, ctx: &ToolCtx, args: &Value) -> Result<Value> {
        let title = arg(args, "title").ok_or_else(|| NexusError::invalid("title is required"))?;
        let body = args.get("body").and_then(Value::as_str).unwrap_or("");
        let dir = arg(args, "dir").unwrap_or("notes/generated");
        let saved = write_generated_note(ctx, dir, title, body)?;
        Ok(json!({ "path": saved }))
    }
}

/// Create a note with provenance frontmatter. Shared by the create_note tool
/// and the write_note node.
pub fn write_generated_note(ctx: &ToolCtx, dir: &str, title: &str, body: &str) -> Result<String> {
    let dir = crate::fs::normalize_rel(dir);
    if !(dir == "notes" || dir.starts_with("notes/")) {
        return Err(NexusError::invalid("agents may only create notes under notes/"));
    }
    let fm = json!({
        "type": "note",
        "title": title,
        "generated_by": format!("[[{}]]", ctx.flow),
        "node": ctx.node,
        "run": ctx.run,
        "created": crate::time::now_rfc3339(),
    });
    let yaml = serde_yaml::to_string(&fm)?;
    let saved = ctx.vault.create_note(&dir, title, &format!("---\n{yaml}---\n\n{}\n", body.trim_end()))?;
    ctx.created.lock().push(saved.path.clone());
    Ok(saved.path)
}

pub fn builtin() -> Vec<Box<dyn Tool>> {
    vec![Box::new(ReadNote), Box::new(QueryGraph), Box::new(CreateNote)]
}
