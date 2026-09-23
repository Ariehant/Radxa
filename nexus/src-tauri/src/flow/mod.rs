//! Flows: typed DAGs of nodes. `flow.md` holds semantics (nodes, config,
//! edges); `flow.canvas` holds layout only. Moving a node touches only the
//! canvas; connecting ports touches only flow.md.

pub mod engine;
pub mod model;
pub mod nodes;
pub mod types;
pub mod validate;

use crate::error::{NexusError, Result};
use crate::fs as vfs;
use crate::index::worker::{path_node_id, IndexCtx};
use crate::vault::{slugify, Vault};
use model::{Canvas, FlowEdgeDef, FlowFile, FlowNodeDef, NodeTemplate, PortDef, PortRef};
use rusqlite::{params, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use validate::{EdgeCheck, Validation};

pub const TEMPLATES_DIR: &str = "templates/nodes";

#[derive(Debug, Clone, Serialize)]
pub struct NodeView {
    pub id: String,
    #[serde(rename = "ref")]
    pub template: String,
    pub title: String,
    pub kind: String,
    pub config: Value,
    pub inputs: Vec<PortDef>,
    pub outputs: Vec<PortDef>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub missing_template: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateSummary {
    #[serde(rename = "ref")]
    pub path: String,
    pub title: String,
    pub kind: String,
    pub description: Option<String>,
    pub inputs: Vec<PortDef>,
    pub outputs: Vec<PortDef>,
}

impl From<&NodeTemplate> for TemplateSummary {
    fn from(t: &NodeTemplate) -> Self {
        TemplateSummary {
            path: t.path.clone(),
            title: t.title.clone(),
            kind: t.meta.kind.clone(),
            description: t.meta.description.clone(),
            inputs: t.meta.inputs.clone(),
            outputs: t.meta.outputs.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FlowView {
    pub dir: String,
    pub path: String,
    pub canvas_path: String,
    pub id: Option<String>,
    pub name: String,
    pub entry: Option<String>,
    pub description: String,
    pub nodes: Vec<NodeView>,
    pub edges: Vec<EdgeCheck>,
    pub errors: Vec<String>,
    pub order: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlowSummary {
    pub dir: String,
    pub name: String,
    pub nodes: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Position {
    pub id: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub w: Option<f64>,
    #[serde(default)]
    pub h: Option<f64>,
}

/// `flows/x`, `flows/x/flow.md` or `flows/x/flow.canvas` → `flows/x`.
pub fn flow_dir(rel: &str) -> String {
    let r = vfs::normalize_rel(rel);
    r.strip_suffix("/flow.md").or_else(|| r.strip_suffix("/flow.canvas")).unwrap_or(&r).to_owned()
}

fn md_path(dir: &str) -> String {
    format!("{dir}/flow.md")
}

fn canvas_path(dir: &str) -> String {
    format!("{dir}/flow.canvas")
}

pub fn load_templates(v: &Vault) -> Result<HashMap<String, NodeTemplate>> {
    let mut out = HashMap::new();
    let dir = v.abs(TEMPLATES_DIR)?;
    if !dir.is_dir() {
        return Ok(out);
    }
    for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
        if !entry.file_type().is_file() || entry.path().extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(rel) = v.rel(entry.path()) else { continue };
        match v.read_text(&rel).and_then(|src| NodeTemplate::parse(&rel, &src)) {
            Ok(t) => {
                out.insert(rel, t);
            }
            Err(e) => log::warn!("template {rel}: {e}"),
        }
    }
    Ok(out)
}

pub struct Loaded {
    pub dir: String,
    pub flow: FlowFile,
    pub canvas: Canvas,
    pub templates: HashMap<String, NodeTemplate>,
}

impl Loaded {
    pub fn validation(&self) -> Validation {
        validate::validate(&self.flow, &self.templates)
    }
}

pub fn load(v: &Vault, dir: &str) -> Result<Loaded> {
    let dir = flow_dir(dir);
    let src = v.read_text(&md_path(&dir))?;
    let flow = FlowFile::parse(&src)?;
    let canvas = match v.read_text(&canvas_path(&dir)) {
        Ok(c) => Canvas::parse(&c)?,
        Err(NexusError::NotFound(_)) => Canvas::default(),
        Err(e) => return Err(e),
    };
    Ok(Loaded { dir, flow, canvas, templates: load_templates(v)? })
}

pub fn view(l: &Loaded) -> FlowView {
    let val = l.validation();
    let nodes = l
        .flow
        .meta
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let t = l.templates.get(&n.template);
            let pos = l.canvas.get(&n.id);
            NodeView {
                id: n.id.clone(),
                template: n.template.clone(),
                title: n.title.clone().or_else(|| t.map(|t| t.title.clone())).unwrap_or_else(|| n.id.clone()),
                kind: t.map(|t| t.meta.kind.clone()).unwrap_or_default(),
                config: n.config.clone(),
                inputs: t.map(|t| t.meta.inputs.clone()).unwrap_or_default(),
                outputs: t.map(|t| t.meta.outputs.clone()).unwrap_or_default(),
                // Unplaced nodes get a simple left-to-right default layout.
                x: pos.map(|p| p.x).unwrap_or(100.0 + i as f64 * 380.0),
                y: pos.map(|p| p.y).unwrap_or(100.0),
                w: pos.map(|p| p.width).unwrap_or(model::DEFAULT_W),
                h: pos.map(|p| p.height).unwrap_or(model::DEFAULT_H),
                missing_template: t.is_none(),
            }
        })
        .collect();
    FlowView {
        path: md_path(&l.dir),
        canvas_path: canvas_path(&l.dir),
        dir: l.dir.clone(),
        id: l.flow.meta.id.clone(),
        name: l.flow.meta.name.clone(),
        entry: l.flow.meta.entry.clone(),
        description: l.flow.body.trim().to_owned(),
        nodes,
        edges: val.edges,
        errors: val.errors,
        order: val.order,
    }
}

fn save_flow(v: &Vault, l: &Loaded) -> Result<()> {
    v.write_text(&md_path(&l.dir), &l.flow.to_markdown()?, None)?;
    Ok(())
}

fn save_canvas(v: &Vault, l: &Loaded) -> Result<()> {
    v.write_text(&canvas_path(&l.dir), &l.canvas.to_json()?, None)?;
    Ok(())
}

pub fn list(v: &Vault) -> Result<Vec<FlowSummary>> {
    let c = v.index.read()?;
    let mut stmt = c.prepare_cached(
        "SELECT f.path, COALESCE(json_extract(n.data, '$.frontmatter.name'), json_extract(n.data, '$.title')),
                COALESCE(json_array_length(json_extract(n.data, '$.frontmatter.nodes')), 0)
         FROM nodes n JOIN files f ON f.id = n.file_id WHERE n.kind = 'flow' ORDER BY f.path",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(FlowSummary { dir: flow_dir(&r.get::<_, String>(0)?), name: r.get(1)?, nodes: r.get::<_, i64>(2)? as usize })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn create(v: &Vault, name: &str) -> Result<FlowView> {
    let base = slugify(name);
    let dir = (1..)
        .map(|i| if i == 1 { format!("flows/{base}") } else { format!("flows/{base}-{i}") })
        .find(|d| v.abs(d).map(|p| !p.exists()).unwrap_or(false))
        .expect("infinite");
    let l = Loaded { dir, flow: FlowFile::new(name), canvas: Canvas::default(), templates: load_templates(v)? };
    save_flow(v, &l)?;
    save_canvas(v, &l)?;
    Ok(view(&l))
}

/// Layout-only update: writes flow.canvas, never flow.md.
pub fn save_layout(v: &Vault, dir: &str, positions: &[Position]) -> Result<()> {
    let mut l = load(v, dir)?;
    for p in positions {
        if l.flow.node(&p.id).is_none() {
            return Err(NexusError::NotFound(format!("node {}", p.id)));
        }
        l.canvas.upsert(&p.id, p.x.round(), p.y.round(), p.w.map(f64::round), p.h.map(f64::round));
    }
    save_canvas(v, &l)
}

/// Add a typed edge. Incompatible types are rejected, never written.
pub fn connect(v: &Vault, dir: &str, from: &str, to: &str) -> Result<FlowView> {
    let mut l = load(v, dir)?;
    let (a, b) = (PortRef::parse(from)?, PortRef::parse(to)?);
    if l.flow.meta.edges.iter().any(|e| e.from == from && e.to == to) {
        return Ok(view(&l));
    }
    let (ft, tt) = validate::port_types(&l.flow, &l.templates, &a, &b);
    validate::check_types(ft, tt).map_err(NexusError::Invalid)?;
    l.flow.meta.edges.push(FlowEdgeDef { from: a.to_string(), to: b.to_string(), data_type: ft.map(|t| t.to_string()) });
    let val = l.validation();
    if let Some(bad) = val.edges.last().filter(|e| !e.valid) {
        return Err(NexusError::Invalid(bad.reason.clone().unwrap_or_else(|| "invalid edge".into())));
    }
    if val.errors.iter().any(|e| e.starts_with("cycle")) {
        return Err(NexusError::invalid("edge would create a cycle"));
    }
    save_flow(v, &l)?;
    Ok(view(&l))
}

pub fn disconnect(v: &Vault, dir: &str, from: &str, to: &str) -> Result<FlowView> {
    let mut l = load(v, dir)?;
    let before = l.flow.meta.edges.len();
    l.flow.meta.edges.retain(|e| !(e.from == from && e.to == to));
    if l.flow.meta.edges.len() != before {
        save_flow(v, &l)?;
    }
    Ok(view(&l))
}

pub fn add_node(v: &Vault, dir: &str, template: &str, x: f64, y: f64, config: Option<Value>) -> Result<FlowView> {
    let mut l = load(v, dir)?;
    let template = vfs::normalize_rel(template);
    let t = l.templates.get(&template).ok_or_else(|| NexusError::NotFound(template.clone()))?;
    let id = l.flow.unique_node_id();
    let config = config.unwrap_or_else(|| Value::Object(t.meta.config.clone()));
    l.flow.meta.nodes.push(FlowNodeDef { id: id.clone(), template, config, title: None });
    if l.flow.meta.entry.is_none() {
        l.flow.meta.entry = Some(id.clone());
    }
    l.canvas.upsert(&id, x.round(), y.round(), None, None);
    save_flow(v, &l)?;
    save_canvas(v, &l)?;
    Ok(view(&l))
}

pub fn remove_node(v: &Vault, dir: &str, id: &str) -> Result<FlowView> {
    let mut l = load(v, dir)?;
    l.flow.meta.nodes.retain(|n| n.id != id);
    l.flow.meta.edges.retain(|e| {
        PortRef::parse(&e.from).map(|p| p.node != id).unwrap_or(true) && PortRef::parse(&e.to).map(|p| p.node != id).unwrap_or(true)
    });
    if l.flow.meta.entry.as_deref() == Some(id) {
        l.flow.meta.entry = l.flow.meta.nodes.first().map(|n| n.id.clone());
    }
    l.canvas.remove(id);
    save_flow(v, &l)?;
    save_canvas(v, &l)?;
    Ok(view(&l))
}

pub fn update_node(v: &Vault, dir: &str, id: &str, config: Option<Value>, title: Option<String>) -> Result<FlowView> {
    let mut l = load(v, dir)?;
    let n = l.flow.meta.nodes.iter_mut().find(|n| n.id == id).ok_or_else(|| NexusError::NotFound(format!("node {id}")))?;
    if let Some(c) = config {
        if !c.is_object() {
            return Err(NexusError::invalid("config must be an object"));
        }
        n.config = c;
    }
    if let Some(t) = title {
        n.title = if t.trim().is_empty() { None } else { Some(t) };
    }
    save_flow(v, &l)?;
    Ok(view(&l))
}

// ---------- index hooks ----------

/// Flow steps become `flow-node` rows and typed `flow` edges in SQLite so
/// table views can query them like any other data.
pub fn index_flow(tx: &Transaction, ctx: &IndexCtx) -> rusqlite::Result<()> {
    let Ok(flow) = FlowFile::parse(ctx.src) else { return Ok(()) };
    let dir = flow_dir(ctx.rel);
    let canvas_id = path_node_id(&canvas_path(&dir));
    let mut ins_node = tx.prepare_cached("INSERT OR REPLACE INTO nodes(id, file_id, kind, data) VALUES (?1, ?2, 'flow-node', ?3)")?;
    let mut ins_edge = tx.prepare_cached("INSERT OR IGNORE INTO edges(from_id, to_id, kind, port, data_type) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    for n in &flow.meta.nodes {
        let data = json!({
            "path": ctx.rel,
            "flow": dir,
            "flow_id": ctx.node_id,
            "canvas_id": canvas_id,
            "node_id": n.id,
            "ref": n.template,
            "title": n.title,
            "config": n.config,
        });
        ins_node.execute(params![format!("{}/{}", ctx.node_id, n.id), ctx.file_id, data.to_string()])?;
        ins_edge.execute(params![ctx.node_id, crate::index::parser::link_key(&n.template), "relation", "ref", Option::<String>::None])?;
    }
    for e in &flow.meta.edges {
        let (Ok(a), Ok(b)) = (PortRef::parse(&e.from), PortRef::parse(&e.to)) else { continue };
        ins_edge.execute(params![
            format!("{}/{}", ctx.node_id, a.node),
            format!("{}/{}", ctx.node_id, b.node),
            "flow",
            format!("{}>{}", a.port, b.port),
            e.data_type
        ])?;
    }
    Ok(())
}

/// Canvas positions → `canvas_layout`; `file` cards become link edges.
pub fn index_canvas(tx: &Transaction, ctx: &IndexCtx) -> rusqlite::Result<()> {
    let Ok(canvas) = Canvas::parse(ctx.src) else { return Ok(()) };
    let mut ins = tx.prepare_cached("INSERT OR REPLACE INTO canvas_layout(flow_id, node_id, x, y, w, h) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
    let mut link = tx.prepare_cached("INSERT OR IGNORE INTO edges(from_id, to_id, kind, port, data_type) VALUES (?1, ?2, 'link', '', NULL)")?;
    for n in &canvas.nodes {
        ins.execute(params![ctx.node_id, n.id, n.x, n.y, n.width, n.height])?;
        if n.kind == "file" {
            if let Some(f) = n.extra.get("file").and_then(Value::as_str) {
                link.execute(params![ctx.node_id, crate::index::parser::link_key(f)])?;
            }
        }
    }
    Ok(())
}

/// Merge template defaults under instance config.
pub fn effective_config(t: Option<&NodeTemplate>, n: &FlowNodeDef) -> Map<String, Value> {
    let mut cfg = t.map(|t| t.meta.config.clone()).unwrap_or_default();
    if let Value::Object(m) = &n.config {
        for (k, v) in m {
            cfg.insert(k.clone(), v.clone());
        }
    }
    cfg
}

#[cfg(test)]
mod tests;
