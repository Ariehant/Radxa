use super::*;
use crate::state::NullSink;
use std::sync::Arc;

fn vault() -> (tempfile::TempDir, Arc<Vault>) {
    let dir = tempfile::tempdir().unwrap();
    let v = Vault::create(dir.path(), Arc::new(NullSink), false).unwrap();
    (dir, v)
}

const FETCH: &str = "templates/nodes/fetch-data.md";
const LLM: &str = "templates/nodes/llm-summarize.md";
const WRITE: &str = "templates/nodes/write-note.md";

#[test]
fn three_nodes_connected_saved_reopened_preserved() {
    let (_d, v) = vault();
    let f = create(&v, "My Flow").unwrap();
    assert_eq!(f.dir, "flows/my-flow");
    add_node(&v, &f.dir, FETCH, 100.0, 100.0, None).unwrap();
    add_node(&v, &f.dir, LLM, 480.0, 100.0, None).unwrap();
    add_node(&v, &f.dir, WRITE, 860.0, 100.0, None).unwrap();
    connect(&v, &f.dir, "n1.out", "n2.in").unwrap();
    let after = connect(&v, &f.dir, "n2.out", "n3.content").unwrap();
    assert!(after.errors.is_empty(), "{:?}", after.errors);
    assert!(after.edges.iter().all(|e| e.valid));

    // Reopen from disk: nodes, positions and edges preserved.
    let re = view(&load(&v, "flows/my-flow/flow.md").unwrap());
    assert_eq!(re.nodes.iter().map(|n| (n.id.as_str(), n.x)).collect::<Vec<_>>(), vec![("n1", 100.0), ("n2", 480.0), ("n3", 860.0)]);
    assert_eq!(re.edges.len(), 2);
    assert_eq!(re.order.unwrap(), vec!["n1", "n2", "n3"]);
    let md = v.read_text("flows/my-flow/flow.md").unwrap();
    assert!(md.contains("type: document[]"), "{md}");
}

#[test]
fn moving_touches_canvas_only_and_edges_touch_flow_md_only() {
    let (_d, v) = vault();
    let dir = "flows/deploy-pipeline";
    let md_before = v.read_text(&md_path(dir)).unwrap();
    save_layout(&v, dir, &[Position { id: "n2".into(), x: 500.4, y: 260.0, w: None, h: None }]).unwrap();
    assert_eq!(v.read_text(&md_path(dir)).unwrap(), md_before, "flow.md untouched by a move");
    assert_eq!(load(&v, dir).unwrap().canvas.get("n2").unwrap().x, 500.0);

    let canvas_before = v.read_text(&canvas_path(dir)).unwrap();
    disconnect(&v, dir, "n2.out", "n3.content").unwrap();
    assert_eq!(v.read_text(&canvas_path(dir)).unwrap(), canvas_before, "canvas untouched by an edge edit");
    assert_eq!(load(&v, dir).unwrap().flow.meta.edges.len(), 1);
}

#[test]
fn incompatible_edges_are_blocked_and_not_written() {
    let (_d, v) = vault();
    let f = create(&v, "t").unwrap();
    add_node(&v, &f.dir, LLM, 0.0, 0.0, None).unwrap();
    add_node(&v, &f.dir, LLM, 400.0, 0.0, None).unwrap();
    let before = v.read_text(&md_path(&f.dir)).unwrap();
    let err = connect(&v, &f.dir, "n1.out", "n2.in").unwrap_err();
    assert_eq!(err.to_string(), "invalid: type mismatch: string → document[]");
    assert!(connect(&v, &f.dir, "n1.nope", "n2.in").is_err());
    assert_eq!(v.read_text(&md_path(&f.dir)).unwrap(), before);
}

#[test]
fn remove_node_drops_its_edges_and_layout() {
    let (_d, v) = vault();
    let dir = "flows/deploy-pipeline";
    let after = remove_node(&v, dir, "n2").unwrap();
    assert_eq!(after.nodes.len(), 2);
    assert!(after.edges.is_empty());
    assert!(load(&v, dir).unwrap().canvas.get("n2").is_none());
}

#[test]
fn flows_are_indexed_as_queryable_nodes_and_edges() {
    let (_d, v) = vault();
    v.index.flush().unwrap();
    let flows = list(&v).unwrap();
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].nodes, 3);
    let c = v.index.read().unwrap();
    let steps: i64 = c.query_row("SELECT count(*) FROM nodes WHERE kind = 'flow-node'", [], |r| r.get(0)).unwrap();
    assert_eq!(steps, 3);
    let typed: Vec<(String, String)> = c
        .prepare("SELECT port, data_type FROM edges WHERE kind = 'flow' ORDER BY port")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(typed, vec![("out>content".to_string(), "string".to_string()), ("out>in".to_string(), "document[]".to_string())]);
    let layout: i64 = c.query_row("SELECT count(*) FROM canvas_layout", [], |r| r.get(0)).unwrap();
    assert_eq!(layout, 3);
    // Templates show "used by" backlinks.
    let bl = crate::index::query::backlinks(&c, LLM).unwrap();
    assert_eq!(bl[0].path, "flows/deploy-pipeline/flow.md");
}

#[test]
fn flow_table_reads_the_index() {
    let (_d, v) = vault();
    let dir = "flows/deploy-pipeline";
    v.index.flush().unwrap();
    save_layout(&v, dir, &[Position { id: "n3".into(), x: 900.0, y: 333.0, w: None, h: None }]).unwrap();
    let c = v.index.read().unwrap();
    let t = crate::index::query::flow_table(&c, "flows/deploy-pipeline/flow.md").unwrap();
    assert_eq!(t.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["n1", "n2", "n3"]);
    assert_eq!(t.nodes[1].kind.as_deref(), Some("agent"));
    assert_eq!(t.nodes[1].template_title.as_deref(), Some("LLM Summarize"));
    assert_eq!((t.nodes[1].inputs, t.nodes[1].outputs), (1, 1));
    assert_eq!(t.nodes[2].y, Some(333.0), "positions come from canvas_layout");
    assert_eq!(t.edges[0].from, "n1.out");
    assert_eq!(t.edges[0].to, "n2.in");
    assert_eq!(t.edges[0].data_type.as_deref(), Some("document[]"));
}

// ---------- execution ----------

fn echo_vault() -> (tempfile::TempDir, Arc<Vault>) {
    let (d, v) = vault();
    let mut c = v.config();
    c.llm.provider = "echo".into();
    v.set_config(c).unwrap();
    v.index.flush().unwrap();
    (d, v)
}

#[test]
fn run_flow_generates_knowledge_and_a_run_log() {
    let (_d, v) = echo_vault();
    let r = v.engine.run(&v, "flows/deploy-pipeline", None, true).unwrap();
    assert!(r.ok, "{:?}", r.nodes);
    assert_eq!(r.nodes.iter().map(|n| n.status.as_str()).collect::<Vec<_>>(), vec!["ok", "ok", "ok"]);
    let summary = r.nodes[1].outputs["out"].as_str().unwrap();
    assert!(summary.starts_with("[echo:llama3.2]"), "{summary}");
    assert!(summary.contains("Release Notes"), "documents rendered into the prompt");

    // The write_note node created a real note…
    assert_eq!(r.created, vec!["notes/generated/release-summary.md"]);
    let note = v.read_text("notes/generated/release-summary.md").unwrap();
    assert!(note.contains("generated_by: '[[flows/deploy-pipeline/flow]]'") || note.contains("generated_by: \"[[flows/deploy-pipeline/flow]]\""), "{note}");
    // …and the run log is indexed knowledge linking flow and outputs.
    assert!(r.log_path.starts_with("runs/deploy-pipeline/"));
    let log = v.read_text(&r.log_path).unwrap();
    assert!(log.contains("| LLM Summarize (n2) | agent | ok |"), "{log}");
    assert!(log.contains("[[notes/generated/release-summary]]"));
    let c = v.index.read().unwrap();
    let bl = crate::index::query::backlinks(&c, "flows/deploy-pipeline/flow.md").unwrap();
    assert!(bl.iter().any(|b| b.path == r.log_path), "run log links back to the flow");
    assert!(bl.iter().any(|b| b.path == "notes/generated/release-summary.md"), "generated note links back to the flow");
}

#[test]
fn run_node_runs_upstream_and_reuses_cache() {
    let (_d, v) = echo_vault();
    let first = v.engine.run(&v, "flows/deploy-pipeline", Some("n2"), true).unwrap();
    assert_eq!(first.nodes.iter().map(|n| n.node.as_str()).collect::<Vec<_>>(), vec!["n1", "n2"], "only upstream of n2");
    let r = v.engine.run(&v, "flows/deploy-pipeline", Some("n3"), true).unwrap();
    assert_eq!(r.nodes.iter().map(|n| n.status.as_str()).collect::<Vec<_>>(), vec!["ok", "cached", "ok"], "fetch re-reads, LLM cached, target runs");
    // Changing the agent's config invalidates its cache entry.
    update_node(&v, "flows/deploy-pipeline", "n2", Some(serde_json::json!({ "model": "other" })), None).unwrap();
    let r = v.engine.run(&v, "flows/deploy-pipeline", Some("n3"), true).unwrap();
    assert_eq!(r.nodes[1].status.as_str(), "ok");
    assert!(r.nodes[1].outputs["out"].as_str().unwrap().starts_with("[echo:other]"));
    assert_eq!(v.engine.last_run("flows/deploy-pipeline").unwrap().run_id, r.run_id);
}

#[test]
fn failures_stop_downstream_and_are_logged() {
    let (_d, v) = vault();
    let mut c = v.config();
    c.llm.provider = "nope".into();
    v.set_config(c).unwrap();
    let r = v.engine.run(&v, "flows/deploy-pipeline", None, false).unwrap();
    assert!(!r.ok);
    assert_eq!(r.nodes.iter().map(|n| n.status.as_str()).collect::<Vec<_>>(), vec!["ok", "error", "skipped"]);
    assert!(r.nodes[1].error.as_deref().unwrap().contains("unknown LLM provider"));
    assert!(v.read_text(&r.log_path).unwrap().contains("**Error:**"));
}

#[test]
fn invalid_subgraph_refuses_to_run() {
    let (_d, v) = echo_vault();
    let dir = "flows/deploy-pipeline";
    let mut l = load(&v, dir).unwrap();
    l.flow.meta.edges.push(model::FlowEdgeDef { from: "n1.out".into(), to: "n3.content".into(), data_type: None });
    v.write_text("flows/deploy-pipeline/flow.md", &l.flow.to_markdown().unwrap(), None).unwrap();
    let e = v.engine.run(&v, dir, Some("n3"), true).unwrap_err().to_string();
    assert!(e.contains("n1.out → n3.content"), "{e}");
    // n2 alone is unaffected by the broken n3 edge.
    assert!(v.engine.run(&v, dir, Some("n2"), true).unwrap().ok);
}
