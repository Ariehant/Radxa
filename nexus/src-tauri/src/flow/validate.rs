//! Static checks over a flow graph: endpoints exist, port types connect,
//! single-valued inputs have one source, and the graph is a DAG.

use super::model::{FlowFile, NodeTemplate, PortRef};
use super::types::PortType;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Serialize)]
pub struct EdgeCheck {
    pub index: usize,
    pub from: String,
    pub to: String,
    pub from_type: Option<PortType>,
    pub to_type: Option<PortType>,
    pub valid: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Validation {
    pub edges: Vec<EdgeCheck>,
    pub errors: Vec<String>,
    /// Topological order when the graph is acyclic.
    pub order: Option<Vec<String>>,
}

impl Validation {
    pub fn ok(&self) -> bool {
        self.errors.is_empty() && self.edges.iter().all(|e| e.valid)
    }
}

pub fn is_list(t: PortType) -> bool {
    matches!(t, PortType::DocumentList | PortType::NoteRefList | PortType::Any | PortType::Json)
}

/// Type-check a single prospective connection.
pub fn check_types(from: Option<PortType>, to: Option<PortType>) -> Result<(), String> {
    match (from, to) {
        (None, _) => Err("source port does not exist".into()),
        (_, None) => Err("target port does not exist".into()),
        (Some(a), Some(b)) if a.connects_to(b) => Ok(()),
        (Some(a), Some(b)) => Err(format!("type mismatch: {a} → {b}")),
    }
}

pub fn port_types(
    flow: &FlowFile,
    templates: &HashMap<String, NodeTemplate>,
    from: &PortRef,
    to: &PortRef,
) -> (Option<PortType>, Option<PortType>) {
    let out_t = flow
        .node(&from.node)
        .and_then(|n| templates.get(&n.template))
        .and_then(|t| t.output(&from.port))
        .map(|p| p.ty);
    let in_t = flow
        .node(&to.node)
        .and_then(|n| templates.get(&n.template))
        .and_then(|t| t.input(&to.port))
        .map(|p| p.ty);
    (out_t, in_t)
}

pub fn validate(flow: &FlowFile, templates: &HashMap<String, NodeTemplate>) -> Validation {
    let mut v = Validation::default();
    let mut ids = HashSet::new();
    for n in &flow.meta.nodes {
        if !ids.insert(n.id.as_str()) {
            v.errors.push(format!("duplicate node id `{}`", n.id));
        }
        if !templates.contains_key(&n.template) {
            v.errors.push(format!("node `{}`: template `{}` not found", n.id, n.template));
        }
    }
    if let Some(entry) = &flow.meta.entry {
        if !ids.contains(entry.as_str()) {
            v.errors.push(format!("entry node `{entry}` does not exist"));
        }
    }

    let mut fan_in: BTreeMap<String, usize> = BTreeMap::new();
    let mut dag_edges = Vec::new();
    for (index, e) in flow.meta.edges.iter().enumerate() {
        let (from, to) = match (PortRef::parse(&e.from), PortRef::parse(&e.to)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(err), _) | (_, Err(err)) => {
                v.edges.push(EdgeCheck { index, from: e.from.clone(), to: e.to.clone(), from_type: None, to_type: None, valid: false, reason: Some(err.to_string()) });
                continue;
            }
        };
        let (ft, tt) = port_types(flow, templates, &from, &to);
        let mut reason = if !ids.contains(from.node.as_str()) {
            Some(format!("node `{}` does not exist", from.node))
        } else if !ids.contains(to.node.as_str()) {
            Some(format!("node `{}` does not exist", to.node))
        } else if from.node == to.node {
            Some("a node cannot connect to itself".into())
        } else {
            check_types(ft, tt).err()
        };
        let c = fan_in.entry(to.to_string()).or_default();
        *c += 1;
        if reason.is_none() && *c > 1 && !tt.is_some_and(is_list) {
            reason = Some(format!("input `{to}` already has a source"));
        }
        if reason.is_none() {
            dag_edges.push((from.node.clone(), to.node.clone()));
        }
        v.edges.push(EdgeCheck { index, from: e.from.clone(), to: e.to.clone(), from_type: ft, to_type: tt, valid: reason.is_none(), reason });
    }

    let nodes: Vec<String> = flow.meta.nodes.iter().map(|n| n.id.clone()).collect();
    match topo_sort(&nodes, &dag_edges) {
        Ok(order) => v.order = Some(order),
        Err(cycle) => v.errors.push(format!("cycle through: {}", cycle.join(", "))),
    }
    v
}

/// Kahn's algorithm. Stable: ties keep declaration order. On a cycle,
/// returns the nodes that could not be ordered.
pub fn topo_sort(nodes: &[String], edges: &[(String, String)]) -> Result<Vec<String>, Vec<String>> {
    let pos: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    let mut indeg = vec![0usize; nodes.len()];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (a, b) in edges {
        if let (Some(&i), Some(&j)) = (pos.get(a.as_str()), pos.get(b.as_str())) {
            out[i].push(j);
            indeg[j] += 1;
        }
    }
    let mut ready: VecDeque<usize> = (0..nodes.len()).filter(|&i| indeg[i] == 0).collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(i) = ready.pop_front() {
        order.push(nodes[i].clone());
        let mut next: Vec<usize> = Vec::new();
        for &j in &out[i] {
            indeg[j] -= 1;
            if indeg[j] == 0 {
                next.push(j);
            }
        }
        next.sort_unstable();
        ready.extend(next);
    }
    if order.len() == nodes.len() {
        Ok(order)
    } else {
        Err((0..nodes.len()).filter(|&i| indeg[i] > 0).map(|i| nodes[i].clone()).collect())
    }
}

/// All ancestors of `target` (inclusive), in topological order.
pub fn upstream_order(order: &[String], edges: &[(String, String)], target: &str) -> Vec<String> {
    let mut need: HashSet<&str> = HashSet::from([target]);
    let mut stack = vec![target];
    while let Some(n) = stack.pop() {
        for (a, b) in edges {
            if b == n && need.insert(a.as_str()) {
                stack.push(a.as_str());
            }
        }
    }
    order.iter().filter(|n| need.contains(n.as_str())).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tpl(path: &str, inputs: &[(&str, &str)], outputs: &[(&str, &str)]) -> (String, NodeTemplate) {
        let ports = |ps: &[(&str, &str)]| ps.iter().map(|(n, t)| format!("  - {{ name: {n}, type: \"{t}\" }}\n")).collect::<String>();
        let src = format!("---\nkind: test\ninputs:\n{}outputs:\n{}---\n", ports(inputs), ports(outputs));
        (path.to_owned(), NodeTemplate::parse(path, &src).unwrap())
    }

    fn templates() -> HashMap<String, NodeTemplate> {
        HashMap::from([
            tpl("t/fetch.md", &[], &[("out", "document[]")]),
            tpl("t/one.md", &[], &[("out", "document")]),
            tpl("t/llm.md", &[("in", "document[]")], &[("out", "string")]),
            tpl("t/git.md", &[("content", "string")], &[("sha", "string")]),
            tpl("t/num.md", &[("n", "number")], &[]),
        ])
    }

    fn flow(nodes: &[(&str, &str)], edges: &[(&str, &str)]) -> FlowFile {
        let mut f = FlowFile::new("t");
        for (id, t) in nodes {
            f.meta.nodes.push(super::super::model::FlowNodeDef { id: (*id).into(), template: (*t).into(), config: serde_json::Value::Null, title: None });
        }
        for (a, b) in edges {
            f.meta.edges.push(super::super::model::FlowEdgeDef { from: (*a).into(), to: (*b).into(), data_type: None });
        }
        f
    }

    #[test]
    fn valid_pipeline() {
        let f = flow(&[("n1", "t/fetch.md"), ("n2", "t/llm.md"), ("n3", "t/git.md")], &[("n1.out", "n2.in"), ("n2.out", "n3.content")]);
        let v = validate(&f, &templates());
        assert!(v.ok(), "{v:?}");
        assert_eq!(v.order.unwrap(), vec!["n1", "n2", "n3"]);
    }

    #[test]
    fn document_autowraps_and_mismatch_blocks() {
        let f = flow(&[("a", "t/one.md"), ("b", "t/llm.md"), ("c", "t/num.md")], &[("a.out", "b.in"), ("b.out", "c.n")]);
        let v = validate(&f, &templates());
        assert!(v.edges[0].valid);
        assert!(!v.edges[1].valid);
        assert_eq!(v.edges[1].reason.as_deref(), Some("type mismatch: string → number"));
    }

    #[test]
    fn cycles_missing_ports_and_fan_in() {
        let f = flow(&[("a", "t/llm.md"), ("b", "t/llm.md")], &[("a.out", "b.in"), ("b.out", "a.in")]);
        // string → document[] mismatch means no DAG edges, so no cycle either.
        assert!(!validate(&f, &templates()).ok());

        let f = flow(&[("a", "t/git.md"), ("b", "t/git.md")], &[("a.sha", "b.content"), ("b.sha", "a.content")]);
        let v = validate(&f, &templates());
        assert!(v.errors.iter().any(|e| e.starts_with("cycle")));

        let f = flow(&[("a", "t/fetch.md"), ("b", "t/llm.md")], &[("a.nope", "b.in")]);
        assert_eq!(validate(&f, &templates()).edges[0].reason.as_deref(), Some("source port does not exist"));

        let f = flow(&[("a", "t/git.md"), ("b", "t/git.md"), ("c", "t/git.md")], &[("a.sha", "c.content"), ("b.sha", "c.content")]);
        assert!(validate(&f, &templates()).edges[1].reason.as_deref().unwrap().contains("already has a source"));
    }

    #[test]
    fn upstream() {
        let order: Vec<String> = ["a", "b", "c", "d"].map(String::from).to_vec();
        let edges = vec![("a".to_string(), "b".to_string()), ("b".into(), "d".into()), ("c".into(), "d".into())];
        assert_eq!(upstream_order(&order, &edges, "b"), vec!["a", "b"]);
        assert_eq!(upstream_order(&order, &edges, "d"), order);
    }
}
