//! DAG execution. Running a node runs its upstream sub-graph in topological
//! order (reusing cached outputs when their inputs are unchanged), then the
//! node itself. Every run writes a Markdown log to `runs/<flow>/<stamp>.md`,
//! which is itself indexed knowledge (it links the flow and created notes).

use super::model::{FlowNodeDef, NodeTemplate, PortRef};
use super::types::PortType;
use super::validate::{self, upstream_order};
use super::{effective_config, load, Loaded};
use crate::agent::tools::ToolCtx;
use crate::error::{NexusError, Result};
use crate::vault::Vault;
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::time::Instant;

pub type Outputs = Map<String, Value>;

/// What an executor gets for one node invocation.
pub struct NodeCtx<'a> {
    pub vault: &'a Vault,
    pub loaded: &'a Loaded,
    pub node: &'a FlowNodeDef,
    pub template: &'a NodeTemplate,
    pub config: &'a Map<String, Value>,
    pub inputs: &'a Map<String, Value>,
    pub tools: &'a ToolCtx<'a>,
    pub log: &'a Mutex<Vec<String>>,
}

impl NodeCtx<'_> {
    pub fn cfg_str(&self, k: &str) -> Option<String> {
        self.config.get(k).and_then(Value::as_str).map(|s| render(s, self))
    }
    pub fn note(&self, line: impl Into<String>) {
        self.log.lock().push(line.into());
    }
    /// First declared output port name (most nodes have exactly one).
    pub fn out_port(&self) -> String {
        self.template.meta.outputs.first().map(|p| p.name.clone()).unwrap_or_else(|| "out".into())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Cached,
    Error,
    Skipped,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Cached => "cached",
            Status::Error => "error",
            Status::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeRun {
    pub node: String,
    pub title: String,
    pub kind: String,
    pub status: Status,
    pub ms: u64,
    pub outputs: Outputs,
    pub error: Option<String>,
    pub log: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunResult {
    pub flow: String,
    pub run_id: String,
    pub target: Option<String>,
    pub ok: bool,
    pub nodes: Vec<NodeRun>,
    pub created: Vec<String>,
    pub log_path: String,
    pub ms: u64,
}

struct CacheEntry {
    key: u64,
    outputs: Outputs,
}

/// Per-vault engine state: last outputs per (flow, node), keyed by a hash of
/// the node's config + inputs so edits invalidate automatically.
#[derive(Default)]
pub struct Engine {
    cache: Mutex<HashMap<(String, String), CacheEntry>>,
    last: Mutex<HashMap<String, RunResult>>,
}

/// `{{name}}` substitution: inputs (rendered), config values, and
/// `flow` / `node` / `date` / `datetime`.
pub fn render(tpl: &str, ctx: &NodeCtx) -> String {
    let mut out = String::with_capacity(tpl.len());
    let mut rest = tpl;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let Some(j) = after.find("}}") else {
            out.push_str(&rest[i..]);
            return out;
        };
        let key = after[..j].trim();
        let val = match key {
            "flow" => Some(ctx.loaded.flow.meta.name.clone()),
            "node" => Some(ctx.node.id.clone()),
            "date" => Some(crate::time::now_rfc3339()[..10].to_owned()),
            "datetime" => Some(crate::time::now_rfc3339()),
            k => ctx.inputs.get(k).or_else(|| ctx.config.get(k)).map(render_value),
        };
        match val {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[i..i + 2 + j + 2]),
        }
        rest = &after[j + 2..];
    }
    out.push_str(rest);
    out
}

/// Render an input value for a prompt: documents become titled sections.
pub fn render_value(v: &Value) -> String {
    fn doc(d: &Map<String, Value>) -> Option<String> {
        let content = d.get("content")?.as_str()?;
        let title = d.get("title").and_then(Value::as_str).unwrap_or("");
        let path = d.get("path").and_then(Value::as_str).unwrap_or("");
        Some(format!("## {title} ({path})\n\n{content}"))
    }
    match v {
        Value::String(s) => s.clone(),
        Value::Object(m) => doc(m).unwrap_or_else(|| v.to_string()),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(render_value).collect();
            parts.join("\n\n---\n\n")
        }
        other => other.to_string(),
    }
}

fn cache_key(config: &Map<String, Value>, inputs: &Map<String, Value>, template: &NodeTemplate) -> u64 {
    let s = json!({ "c": config, "i": inputs, "t": template.body, "k": template.meta.kind }).to_string();
    xxhash_rust::xxh3::xxh3_64(s.as_bytes())
}

/// Collect a node's inputs from upstream outputs, coercing types
/// (document → document[]) and concatenating fan-in on list inputs.
fn gather_inputs(l: &Loaded, node: &FlowNodeDef, template: &NodeTemplate, done: &HashMap<String, Outputs>) -> Result<Map<String, Value>> {
    let mut inputs = Map::new();
    for e in &l.flow.meta.edges {
        let (Ok(from), Ok(to)) = (PortRef::parse(&e.from), PortRef::parse(&e.to)) else { continue };
        if to.node != node.id {
            continue;
        }
        let Some(value) = done.get(&from.node).and_then(|o| o.get(&from.port)).cloned() else {
            return Err(NexusError::Other(format!("input `{}` has no value from `{}`", to.port, e.from)));
        };
        let from_t = l.flow.node(&from.node).and_then(|n| l.templates.get(&n.template)).and_then(|t| t.output(&from.port)).map(|p| p.ty).unwrap_or(PortType::Any);
        let to_t = template.input(&to.port).map(|p| p.ty).unwrap_or(PortType::Any);
        let value = from_t.coerce(to_t, value);
        match inputs.get_mut(&to.port) {
            Some(Value::Array(existing)) if validate::is_list(to_t) => match value {
                Value::Array(more) => existing.extend(more),
                v => existing.push(v),
            },
            _ => {
                inputs.insert(to.port.clone(), value);
            }
        }
    }
    Ok(inputs)
}

impl Engine {
    pub fn last_run(&self, dir: &str) -> Option<RunResult> {
        self.last.lock().get(dir).cloned()
    }

    pub fn clear(&self, dir: &str) {
        self.cache.lock().retain(|(d, _), _| d != dir);
        self.last.lock().remove(dir);
    }

    /// Run `target` and everything upstream of it (or the whole flow).
    pub fn run(&self, v: &Vault, dir: &str, target: Option<&str>, use_cache: bool) -> Result<RunResult> {
        let started = Instant::now();
        let l = load(v, dir)?;
        let val = l.validation();
        let order = val.order.clone().ok_or_else(|| NexusError::invalid(val.errors.join("; ")))?;
        let dag: Vec<(String, String)> = l
            .flow
            .meta
            .edges
            .iter()
            .filter_map(|e| Some((PortRef::parse(&e.from).ok()?.node, PortRef::parse(&e.to).ok()?.node)))
            .collect();
        let plan = match target {
            Some(t) => {
                if l.flow.node(t).is_none() {
                    return Err(NexusError::NotFound(format!("node {t}")));
                }
                upstream_order(&order, &dag, t)
            }
            None => order,
        };
        // Refuse to run a sub-graph with broken edges.
        for e in val.edges.iter().filter(|e| !e.valid) {
            let to = PortRef::parse(&e.to).map(|p| p.node).unwrap_or_default();
            if plan.contains(&to) {
                return Err(NexusError::invalid(format!("edge {} → {}: {}", e.from, e.to, e.reason.clone().unwrap_or_default())));
            }
        }

        let stamp = crate::time::file_stamp(crate::time::now_secs());
        let run_id = format!("{stamp}-{}", &crate::ids::new_id()[20..]);
        let flow_link = format!("{}/flow", l.dir);
        let tool_ctx = ToolCtx { vault: v, flow: &flow_link, node: "", run: &run_id, created: Mutex::new(vec![]) };
        let mut done: HashMap<String, Outputs> = HashMap::new();
        let mut runs = Vec::new();
        let mut failed = false;

        for id in &plan {
            let node = l.flow.node(id).expect("planned nodes exist");
            let template = l.templates.get(&node.template).ok_or_else(|| NexusError::NotFound(node.template.clone()))?;
            let title = node.title.clone().unwrap_or_else(|| template.title.clone());
            let kind = template.meta.kind.clone();
            if failed {
                runs.push(NodeRun { node: id.clone(), title, kind, status: Status::Skipped, ms: 0, outputs: Map::new(), error: None, log: vec![] });
                continue;
            }
            v.events.emit("run:progress", json!({ "flow": l.dir, "run": run_id, "node": id, "status": "running" }));
            let t0 = Instant::now();
            let config = effective_config(Some(template), node);
            let log = Mutex::new(Vec::new());
            let result = gather_inputs(&l, node, template, &done).and_then(|inputs| {
                let key = cache_key(&config, &inputs, template);
                let is_target = target == Some(id.as_str());
                if use_cache && !is_target && cacheable(&template.meta.kind) {
                    if let Some(hit) = self.cache.lock().get(&(l.dir.clone(), id.clone())).filter(|c| c.key == key) {
                        return Ok((hit.outputs.clone(), true));
                    }
                }
                let node_tools = ToolCtx { vault: v, flow: &flow_link, node: id, run: &run_id, created: Mutex::new(vec![]) };
                let ctx = NodeCtx { vault: v, loaded: &l, node, template, config: &config, inputs: &inputs, tools: &node_tools, log: &log };
                let out = execute(&ctx);
                tool_ctx.created.lock().extend(node_tools.created.into_inner());
                let out = out?;
                self.cache.lock().insert((l.dir.clone(), id.clone()), CacheEntry { key, outputs: out.clone() });
                Ok((out, false))
            });
            let ms = t0.elapsed().as_millis() as u64;
            let run = match result {
                Ok((outputs, cached)) => {
                    done.insert(id.clone(), outputs.clone());
                    NodeRun { node: id.clone(), title, kind, status: if cached { Status::Cached } else { Status::Ok }, ms, outputs, error: None, log: log.into_inner() }
                }
                Err(e) => {
                    failed = true;
                    NodeRun { node: id.clone(), title, kind, status: Status::Error, ms, outputs: Map::new(), error: Some(e.to_string()), log: log.into_inner() }
                }
            };
            v.events.emit("run:progress", json!({ "flow": l.dir, "run": run_id, "node": id, "status": run.status, "ms": ms, "error": run.error }));
            runs.push(run);
        }

        let slug = crate::vault::slugify(if l.flow.meta.name.is_empty() { &l.dir } else { &l.flow.meta.name });
        let log_path = format!("runs/{slug}/{run_id}.md");
        let created = tool_ctx.created.into_inner();
        let mut result = RunResult {
            flow: l.dir.clone(),
            run_id,
            target: target.map(str::to_owned),
            ok: !failed,
            nodes: runs,
            created,
            log_path: log_path.clone(),
            ms: started.elapsed().as_millis() as u64,
        };
        v.write_text(&log_path, &run_log(&l, &result)?, None)?;
        result.ms = started.elapsed().as_millis() as u64;
        self.last.lock().insert(l.dir.clone(), result.clone());
        v.events.emit("run:finished", json!({ "flow": result.flow, "run": result.run_id, "ok": result.ok, "log": result.log_path }));
        Ok(result)
    }
}

/// Readers of live vault state must re-run; everything else (LLM calls,
/// side-effecting writes) reuses its last output while inputs are unchanged.
fn cacheable(kind: &str) -> bool {
    !matches!(kind, "fetch" | "query_graph")
}

/// Dispatch on the template's `kind`.
fn execute(ctx: &NodeCtx) -> Result<Outputs> {
    match ctx.template.meta.kind.as_str() {
        "fetch" => super::nodes::fetch(ctx),
        "agent" => super::nodes::agent(ctx),
        "write_note" => super::nodes::write_note(ctx),
        "git_commit" => super::nodes::git_commit(ctx),
        "query_graph" => super::nodes::query_graph(ctx),
        "" => Err(NexusError::invalid(format!("template {} has no `kind`", ctx.template.path))),
        k => Err(NexusError::invalid(format!("unknown node kind `{k}`"))),
    }
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

fn run_log(l: &Loaded, r: &RunResult) -> Result<String> {
    let fm = json!({
        "type": "run",
        "flow": format!("[[{}/flow]]", l.dir),
        "run": r.run_id,
        "target": r.target,
        "status": if r.ok { "ok" } else { "error" },
        "created": crate::time::now_rfc3339(),
        "duration_ms": r.ms,
        "outputs": r.created.iter().map(|p| format!("[[{}]]", p.trim_end_matches(".md"))).collect::<Vec<_>>(),
    });
    let mut md = format!("---\n{}---\n\n# Run of {} — {}\n\n", serde_yaml::to_string(&fm)?, if l.flow.meta.name.is_empty() { &l.dir } else { &l.flow.meta.name }, r.run_id);
    md.push_str("| node | kind | status | ms |\n|---|---|---|---|\n");
    for n in &r.nodes {
        md.push_str(&format!("| {} ({}) | {} | {} | {} |\n", n.title.replace('|', "\\|"), n.node, n.kind, n.status.as_str(), n.ms));
    }
    for n in &r.nodes {
        md.push_str(&format!("\n## {} `{}`\n\n", n.title, n.node));
        if let Some(e) = &n.error {
            md.push_str(&format!("**Error:** {e}\n\n"));
        }
        for line in &n.log {
            md.push_str(&format!("- {line}\n"));
        }
        for (port, v) in &n.outputs {
            let text = match v {
                Value::String(s) => s.clone(),
                other => serde_json::to_string_pretty(other).unwrap_or_default(),
            };
            md.push_str(&format!("\n**{port}**\n\n````text\n{}\n````\n", clip(&text, 4000)));
        }
    }
    if !r.created.is_empty() {
        md.push_str("\n## Created\n\n");
        for p in &r.created {
            md.push_str(&format!("- [[{}]]\n", p.trim_end_matches(".md")));
        }
    }
    Ok(md)
}
