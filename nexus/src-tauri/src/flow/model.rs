//! On-disk flow model. Semantics live in `flow.md` (frontmatter nodes +
//! edges), layout lives in `flow.canvas` (JSON Canvas 1.0). Node ids match
//! across both files.

use super::types::PortType;
use crate::error::{NexusError, Result};
use crate::index::parser::{parse_frontmatter, split_frontmatter};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

// ---------- flow.md ----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowNodeDef {
    pub id: String,
    #[serde(rename = "ref")]
    pub template: String,
    #[serde(default, skip_serializing_if = "is_empty_obj")]
    pub config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

fn is_empty_obj(v: &Value) -> bool {
    v.is_null() || v.as_object().is_some_and(|m| m.is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowEdgeDef {
    /// `node.port`
    pub from: String,
    /// `node.port`
    pub to: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortRef {
    pub node: String,
    pub port: String,
}

impl PortRef {
    pub fn parse(s: &str) -> Result<PortRef> {
        let (node, port) = s
            .rsplit_once('.')
            .ok_or_else(|| NexusError::invalid(format!("edge endpoint `{s}` must be `node.port`")))?;
        if node.is_empty() || port.is_empty() {
            return Err(NexusError::invalid(format!("edge endpoint `{s}` must be `node.port`")));
        }
        Ok(PortRef { node: node.to_owned(), port: port.to_owned() })
    }
}

impl std::fmt::Display for PortRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.node, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", default = "flow_type")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub nodes: Vec<FlowNodeDef>,
    #[serde(default, deserialize_with = "null_default")]
    pub edges: Vec<FlowEdgeDef>,
    /// Unknown keys survive a load/save cycle.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `inputs:` with nothing after it is YAML null; treat it as empty.
fn null_default<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

fn flow_type() -> String {
    "flow".into()
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowFile {
    pub meta: FlowMeta,
    pub body: String,
}

impl FlowFile {
    pub fn new(name: &str) -> FlowFile {
        FlowFile {
            meta: FlowMeta {
                id: Some(crate::ids::new_id()),
                kind: flow_type(),
                name: name.to_owned(),
                entry: None,
                nodes: vec![],
                edges: vec![],
                extra: Map::new(),
            },
            body: format!("\n# {name}\n\nDescribe what this flow does.\n"),
        }
    }

    pub fn parse(src: &str) -> Result<FlowFile> {
        let (yaml, body) = split_frontmatter(src);
        let yaml = yaml.ok_or_else(|| NexusError::invalid("flow.md has no frontmatter"))?;
        let map = parse_frontmatter(yaml).map_err(NexusError::Invalid)?;
        let meta: FlowMeta = serde_json::from_value(Value::Object(map)).map_err(|e| NexusError::invalid(format!("flow.md: {e}")))?;
        Ok(FlowFile { meta, body: body.to_owned() })
    }

    pub fn to_markdown(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(&self.meta)?;
        Ok(format!("---\n{yaml}---\n{}", self.body))
    }

    pub fn node(&self, id: &str) -> Option<&FlowNodeDef> {
        self.meta.nodes.iter().find(|n| n.id == id)
    }

    pub fn unique_node_id(&self) -> String {
        (1..).map(|i| format!("n{i}")).find(|id| self.node(id).is_none()).expect("infinite")
    }
}

// ---------- flow.canvas (JSON Canvas 1.0) ----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// text / file / url / label / color … preserved verbatim.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Canvas {
    #[serde(default)]
    pub nodes: Vec<CanvasNode>,
    #[serde(default)]
    pub edges: Vec<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

pub const DEFAULT_W: f64 = 240.0;
pub const DEFAULT_H: f64 = 120.0;

impl Canvas {
    pub fn parse(src: &str) -> Result<Canvas> {
        if src.trim().is_empty() {
            return Ok(Canvas::default());
        }
        Ok(serde_json::from_str(src)?)
    }

    pub fn to_json(&self) -> Result<String> {
        let mut s = serde_json::to_string_pretty(self)?;
        s.push('\n');
        Ok(s)
    }

    pub fn get(&self, id: &str) -> Option<&CanvasNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn upsert(&mut self, id: &str, x: f64, y: f64, w: Option<f64>, h: Option<f64>) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
            n.x = x;
            n.y = y;
            if let Some(w) = w {
                n.width = w;
            }
            if let Some(h) = h {
                n.height = h;
            }
        } else {
            let mut extra = Map::new();
            extra.insert("text".into(), Value::String(String::new()));
            self.nodes.push(CanvasNode {
                id: id.to_owned(),
                kind: "text".into(),
                x,
                y,
                width: w.unwrap_or(DEFAULT_W),
                height: h.unwrap_or(DEFAULT_H),
                extra,
            });
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.nodes.retain(|n| n.id != id);
        self.edges.retain(|e| e.get("fromNode").and_then(Value::as_str) != Some(id) && e.get("toNode").and_then(Value::as_str) != Some(id));
    }
}

// ---------- node templates ----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PortDef {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: PortType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateMeta {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub inputs: Vec<PortDef>,
    #[serde(default, deserialize_with = "null_default")]
    pub outputs: Vec<PortDef>,
    #[serde(default, deserialize_with = "null_default")]
    pub tools: Vec<String>,
    /// Default config merged under each instance's `config`.
    #[serde(default, deserialize_with = "null_default")]
    pub config: Map<String, Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NodeTemplate {
    #[serde(rename = "ref")]
    pub path: String,
    pub title: String,
    #[serde(flatten)]
    pub meta: TemplateMeta,
    /// Markdown body — the prompt for agent nodes (`{{input}}` placeholders).
    pub body: String,
}

impl NodeTemplate {
    pub fn parse(path: &str, src: &str) -> Result<NodeTemplate> {
        let (yaml, body) = split_frontmatter(src);
        let map = match yaml {
            Some(y) => parse_frontmatter(y).map_err(NexusError::Invalid)?,
            None => Map::new(),
        };
        let meta: TemplateMeta = serde_json::from_value(Value::Object(map)).map_err(|e| NexusError::invalid(format!("{path}: {e}")))?;
        let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".md");
        let title = meta.title.clone().or_else(|| meta.name.clone()).unwrap_or_else(|| stem.replace(['-', '_'], " "));
        Ok(NodeTemplate { path: path.to_owned(), title, meta, body: body.trim().to_owned() })
    }

    pub fn input(&self, name: &str) -> Option<&PortDef> {
        self.meta.inputs.iter().find(|p| p.name == name)
    }

    pub fn output(&self, name: &str) -> Option<&PortDef> {
        self.meta.outputs.iter().find(|p| p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOW: &str = "---\nid: 01HQXFLOW\ntype: flow\nname: deploy-pipeline\nentry: n1\nowner: ravi\nnodes:\n  - id: n1\n    ref: templates/nodes/fetch-data.md\n    config: { source: notes/release-notes.md }\n  - id: n2\n    ref: templates/nodes/llm-summarize.md\nedges:\n  - { from: n1.out, to: n2.in, type: document[] }\n---\n\nFlow description in markdown.\n";

    #[test]
    fn flow_roundtrip_preserves_body_and_unknown_keys() {
        let f = FlowFile::parse(FLOW).unwrap();
        assert_eq!(f.meta.nodes.len(), 2);
        assert_eq!(f.meta.edges[0].data_type.as_deref(), Some("document[]"));
        assert_eq!(f.meta.extra["owner"], "ravi");
        let out = f.to_markdown().unwrap();
        let again = FlowFile::parse(&out).unwrap();
        assert_eq!(f, again);
        assert!(out.ends_with("\nFlow description in markdown.\n"));
        assert_eq!(again.unique_node_id(), "n3");
    }

    #[test]
    fn canvas_preserves_unknown_fields() {
        let src = r#"{"nodes":[{"id":"n1","type":"text","x":1,"y":2,"width":3,"height":4,"text":"hi","color":"2"}],"edges":[{"id":"e","fromNode":"n1","toNode":"n2"}]}"#;
        let mut c = Canvas::parse(src).unwrap();
        assert_eq!(c.nodes[0].extra["color"], "2");
        c.upsert("n1", 10.0, 20.0, None, None);
        c.upsert("n9", 0.0, 0.0, None, None);
        let back = Canvas::parse(&c.to_json().unwrap()).unwrap();
        assert_eq!(back.nodes[0].x, 10.0);
        assert_eq!(back.nodes[0].extra["text"], "hi");
        assert_eq!(back.nodes[1].width, DEFAULT_W);
        c.remove("n1");
        assert!(c.edges.is_empty());
    }

    #[test]
    fn port_ref_parse() {
        assert_eq!(PortRef::parse("n1.out").unwrap(), PortRef { node: "n1".into(), port: "out".into() });
        assert_eq!(PortRef::parse("a.b.c").unwrap().node, "a.b");
        assert!(PortRef::parse("n1").is_err());
    }

    #[test]
    fn template_parse() {
        let t = NodeTemplate::parse(
            "templates/nodes/llm-summarize.md",
            "---\nkind: agent\nmodel: claude-sonnet\ninputs:\n  - { name: in, type: document[] }\noutputs:\n  - { name: out, type: string }\ntools: [read_note]\n---\n\nSummarize {{in}}\n",
        )
        .unwrap();
        assert_eq!(t.title, "llm summarize");
        assert_eq!(t.input("in").unwrap().ty, PortType::DocumentList);
        assert_eq!(t.body, "Summarize {{in}}");
    }
}
